#!/usr/bin/env python3
"""Verify source measurements and generate the focused-retrieval decision report."""
import argparse
import collections
import functools
import gzip
import json
import importlib.metadata
import platform
import statistics
import sys
from pathlib import Path

import run as h
import tiktoken


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output',type=Path)
    args=parser.parse_args();out=args.output.resolve()
    contract=json.loads((out/'contract.json').read_text())
    probe=h.PROJECT/'.benchmark-tools/node_modules/@probelabs/probe/bin/probe'
    environment={'python':sys.version,'platform':platform.platform(),
                 'versions':{name:importlib.metadata.version(name) for name in ['aider-chat','tiktoken','tree-sitter-language-pack','scipy','numpy']},
                 'probe_version':h.command([probe,'--version'])[0].strip(),'probe_sha256':h.sha(probe.read_bytes()),
                 'probe_package_version':json.loads((probe.parent.parent/'package.json').read_text())['version'],
                 'rg_version':h.command(['rg','--version'])[0].splitlines()[0],
                 'verification_harness_sha256':h.sha(Path(__file__).read_bytes())}
    h.dump(out/'environment-audit.json',environment)
    new=json.loads((out/'cases-frozen.json').read_text())['cases']
    pilot=json.loads((out/'implementation/benchmarks/comparison/cases.json').read_text())['cases']
    cases={c['id']:c for c in [*new,*pilot]}
    roots={r:out/'snapshots'/r for r in ['flexcontext','agentx','webkitpy']}
    h.validate_cases(list(cases.values()),roots)
    for repo,root in roots.items():
        for path,info in json.loads((out/f'{repo}.manifest.json').read_text())['files'].items():
            assert h.sha((root/path).read_bytes())==info['sha256'],path
    assert h.sha((out/'cases-frozen.json').read_bytes())==contract['cases_sha256']
    for name in ['baseline','candidate']:assert h.sha((out/f'flexcontext-{name}').read_bytes())==contract[f'{name}_sha256']
    tables={name:[json.loads(line) for line in (out/f'{name}.jsonl').read_text().splitlines()] for name in ['dev','heldout','pilot-diagnostics','latency','comparison','aider-latency']}
    expected={'dev':684,'heldout':144,'pilot-diagnostics':120,'latency':1440,'comparison':720,'aider-latency':720}
    assert {k:len(v) for k,v in tables.items()}==expected
    sources={repo:h.Source(root) for repo,root in roots.items()}
    enc=tiktoken.get_encoding('cl100k_base')
    @functools.lru_cache(maxsize=200000)
    def cost(text):return len(enc.encode(text+'\n',disallowed_special=()))
    @functools.lru_cache(maxsize=1024)
    def source_bytes(repo,path):return (roots[repo]/path).read_bytes()
    checked=0
    native_spans={}
    fixed_pools={}
    for stage in ['dev','heldout','pilot-diagnostics','comparison']:
        for row in tables[stage]:
            selected=json.loads((out/row['artifact']).read_text());case=cases[row['case']]
            assert sum(cost(r['text']) for r in selected)==row['source_tokens']<=row['budget']
            assert sum(len((r['text']+'\n').encode()) for r in selected)==row['source_bytes']
            regions={(s['path'],n) for s in case['relevant_regions'] for n in range(s['start'],s['end']+1)}
            assert sum(cost(r['text']) for r in selected if (r['path'],r['line']) not in regions)==row['irrelevant_tokens']
            for r in selected:
                if r['line'] is not None:
                    assert sources[row['repo']].lines(r['path'])[r['line']-1].strip()==r['text'].strip()
                    checked+=1
            for key,value in h.score(case,selected).items():assert row[key]==value,(stage,row['case'],key)
            raw_path=(out/row['artifact']).with_name(Path(row['artifact']).name.replace('.lines.json','.raw.json.gz'))
            with gzip.open(raw_path,'rt') as f:raw=f.read()
            assert len(raw.encode())==row['native_bytes']
            if row['tool']=='flexcontext':
                data=json.loads(raw);data=data.get('result',{}).get('structuredContent',data)
                assert data['context_cost']['serialized_bytes']==len(raw.encode())
                assert data['context_cost']['estimated_tokens']==(len(raw.encode())+3)//4
                if row['adapter']=='fixed':
                    fixed_pools[stage,row['case'],row['variant'],row['budget']]=[
                        (r['path'],r['symbol'],r['content'],r['source_spans']) for r in data['results']]
                for result in data['results']:
                    source=source_bytes(row['repo'],result['path'])
                    for span in result['source_spans']:
                        assert source[span['start_byte']:span['end_byte']].decode() in result['content']
                        if row['variant'].startswith('focused'):
                            start=source[:span['start_byte']].count(b'\n')+1
                            shown=source[span['start_byte']:span['end_byte']]
                            assert span['start_line']==start
                            assert span['end_line']==start+shown.removesuffix(b'\n').count(b'\n')
                if row['variant'].startswith('focused') and row['adapter']=='native':
                    spans=collections.defaultdict(list)
                    for result in data['results']:
                        for span in result['source_spans']:spans[result['path']].append((span['start_byte'],span['end_byte']))
                    merged={}
                    for path,ranges in spans.items():
                        combined=[]
                        for start,end in sorted(ranges):
                            if combined and start<=combined[-1][1]:combined[-1]=(combined[-1][0],max(end,combined[-1][1]))
                            else:combined.append((start,end))
                        merged[path]=combined
                    native_spans[stage,row['case'],row['variant'],row['budget']]=merged
    retained_pairs=0
    fixed_pairs=0
    for (stage,case,variant,budget),small in fixed_pools.items():
        if budget==2048:
            assert small==fixed_pools[stage,case,variant,4096],(stage,case,variant)
            fixed_pairs+=1
    for (stage,case,variant,budget),small in native_spans.items():
        if budget!=2048:continue
        large=native_spans[stage,case,variant,4096]
        for path,ranges in small.items():
            for start,end in ranges:assert any(a<=start and end<=b for a,b in large.get(path,[])),(stage,case,variant,path)
        retained_pairs+=1
    for stage in ['latency','aider-latency']:
        seen=collections.defaultdict(set)
        for r in tables[stage]:seen[(r['repo'],r['case'],r['budget'],r.get('variant','aider'))].add(r['repeat'])
        assert all(v==set(range(20)) for v in seen.values())
    verification={'samples':expected,'located_source_lines_checked':checked,'source_and_evidence_hashes_valid':True,'budgets_and_scores_valid':True,'native_byte_accounting_valid':True,'source_span_retention_pairs':retained_pairs,'identical_fixed_candidate_pool_pairs':fixed_pairs,'resident_repetitions_per_query_budget':20}
    h.dump(out/'verification.json',verification)

    def aggregate(rows,keys):
        groups=collections.defaultdict(list)
        for r in rows:groups[tuple(r[k] for k in keys)].append(r)
        result=[]
        for key,group in sorted(groups.items()):
            entry=dict(zip(keys,key));entry['samples']=len(group)
            for metric in ['evidence_recall','evidence_precision','source_tokens','irrelevant_tokens','metadata_tokens','native_bytes','native_tokens','latency_ms']:
                values=[r[metric] for r in group if r.get(metric) is not None]
                if values:entry[metric]=statistics.mean(values)
            result.append(entry)
        return result
    summary={k:aggregate(tables[k],['repo','variant','budget','adapter']) for k in ['dev','heldout','pilot-diagnostics','comparison']}
    acceptance=json.loads((out/'acceptance.json').read_text())
    # Recompute gates independently of the runner's acceptance output.
    held=summary['heldout'];variant=acceptance['variant']
    b={(r['repo'],r['budget'],r['adapter']):r for r in held if r['variant']=='baseline'}
    c={(r['repo'],r['budget'],r['adapter']):r for r in held if r['variant']==variant}
    assert acceptance['gates']['heldout_recall']==all(c[k]['evidence_recall']+1e-12>=v['evidence_recall'] for k,v in b.items())
    native_b=[r for r in tables['heldout'] if r['variant']=='baseline' and r['adapter']=='native']
    native_c=[r for r in tables['heldout'] if r['variant']==variant and r['adapter']=='native']
    for key,metric in [('irrelevant_source_ratio','irrelevant_tokens'),('metadata_ratio','metadata_tokens')]:
        assert abs(acceptance[key]-sum(r[metric] for r in native_c)/sum(r[metric] for r in native_b))<1e-12
    assert acceptance['promote']==all(acceptance['gates'].values())
    extras={}
    for filename,key in [('presentation-summary.json','presentation_control'),('final-workspace-checks.json','local_validation')]:
        if (out/filename).exists():extras[key]=json.loads((out/filename).read_text())
    h.dump(out/'optimization-summary.json',{'contract':contract,'selection':json.loads((out/'selection.json').read_text()),'acceptance':acceptance,'verification':verification,**summary,**extras})
    lines=['# Focused retrieval results','',f"Decision: **{'promote' if acceptance['promote'] else 'retain baseline retrieval; keep focused policy opt-in'}**.",'',
        '36 frozen, source-reviewed questions; 18 development and 18 held out by module. Original 15 pilot questions remain development diagnostics. Local snapshots cover Flexcontext, AgentX and WebKit Python tooling. No agent-success evaluation or model calls.', '',
        '| Acceptance gate | Result |','|---|---|',*[f"| {k} | {'PASS' if v else 'FAIL'} |" for k,v in acceptance['gates'].items()], '',
        f"Held-out irrelevant source-token change: **{(acceptance['irrelevant_source_ratio']-1)*100:+.1f}%**. Non-source structured payload-token change: **{(acceptance['metadata_ratio']-1)*100:+.1f}%**. These are overall paired native-adapter totals; metadata reduction includes both presentation and changes in returned results.",'',
        '## Held-out quality','', '| Repository | Source budget | Baseline recall | Focused recall | Baseline precision | Focused precision | Fixed-pool baseline recall | Fixed-pool focused recall |','|---|---:|---:|---:|---:|---:|---:|---:|']
    for repo in roots:
        for budget in [2048,4096]:
            old=b[(repo,budget,'native')];new=c[(repo,budget,'native')]
            lines.append(f"| {repo} | {budget} | {old['evidence_recall']:.1%} | {new['evidence_recall']:.1%} | {old['evidence_precision']:.1%} | {new['evidence_precision']:.1%} | {b[(repo,budget,'fixed')]['evidence_recall']:.1%} | {c[(repo,budget,'fixed')]['evidence_recall']:.1%} |")
    lines+=['','The native-adapter regressions are parser fact collection and the legacy MCP wait condition at 4k, and WebKit command-error tail extraction at both budgets. In all these cases the candidate still returns the correct file, but omits the required implementation evidence. This remains an evidence selection/ranking problem within discovered files.', '']
    lines+=['','## Resident latency','', '20 timed repetitions per held-out question and budget, after warming each query; 120 samples per repository/budget/tool. Calls ran serially in randomized order. OS page cache was uncontrolled.', '', '| Repository | Budget | Baseline p95 ms | Focused p95 ms | Ratio |','|---|---:|---:|---:|---:|']
    for t in acceptance['timing']:lines.append(f"| {t['repo']} | {t['budget']} | {t['baseline_p95_ms']:.2f} | {t['candidate_p95_ms']:.2f} | {t['ratio']:.3f} |")
    lines+=['','## Development ablations','', 'Averages across the 18 development questions and both source budgets using the native adapter. Each isolated row changes one retrieval mechanism; the focused rows combine the changes. All experimental rows use compact presentation, while the frozen baseline emits full detail, so the metadata column must not be attributed solely to the retrieval mechanism. Cutoff selection also checks every repository/budget group with the fixed-pool adapter.', '', '| Policy | Recall | Precision | Irrelevant source tokens/query | Metadata tokens/query |','|---|---:|---:|---:|---:|']
    for r in aggregate([r for r in tables['dev'] if r['adapter']=='native'],['variant']):
        lines.append(f"| {r['variant']} | {r['evidence_recall']:.1%} | {r['evidence_precision']:.1%} | {r['irrelevant_tokens']:.0f} | {r['metadata_tokens']:.0f} |")
    cold={(r['repo'],r['budget'],r['variant']):r['latency_ms'] for r in summary['comparison'] if r['adapter']=='cold'}
    lines+=['','## Four-tool comparison','', 'All 36 questions, warm CLI quality, 2k and 4k source-token ceilings. CLI times are means, including startup. This table reports measured adapter behavior, not a general tool leaderboard. Probe and ripgrep resident measurements are unavailable in this harness.', '', '| Repository | Budget | Tool/policy | Recall | Precision | Source tokens | Native tokens | Cold CLI ms | Warm CLI ms |','|---|---:|---|---:|---:|---:|---:|---:|---:|']
    for r in summary['comparison']:
        if r['adapter']=='warm_cli':lines.append(f"| {r['repo']} | {r['budget']} | {r['variant']} | {r['evidence_recall']:.1%} | {r['evidence_precision']:.1%} | {r['source_tokens']:.0f} | {r['native_tokens']:.0f} | {cold[(r['repo'],r['budget'],r['variant'])]:.2f} | {r['latency_ms']:.2f} |")
    lines+=['','## Interpretation and audit','',
        'A failed recall gate rules out promotion even if shorter responses improve precision or token counts. No lower-performing repository/budget group is hidden by the overall average. The fixed-pool results isolate the pilot adapter’s changing overfetch ceiling; native source spans have separate monotonicity regression tests.', '',
        'The initial development-only implementation was preserved separately as `optimization-20260916`. Before held-out execution, relationship-only and stable-only ablation controls were corrected to avoid mixing path filtering, quota removal and diversity changes. The authoritative final run is this directory’s frozen contract and executables. No held-out tuning followed.', '',
        'After measurement, the human-readable `--explain` renderer was fixed to display the already-existing trace, and scope/trace regression assertions were added. This diagnostic-only change does not alter retrieval or normal JSON/MCP output. The measured executable hashes remain recorded separately from the final workspace build.', '',
        'Evidence labels are source-reviewed but assistant-authored, and precision counts unjudged lines as irrelevant. Repo coverage is deliberately limited, particularly WebKit Python tooling. Ripgrep’s exhaustive OR/context adapter produces substantial overfetch; these results do not establish an inherent ripgrep cost. Aider RepoMap is primarily navigation and commonly emits signatures rather than behavioral bodies.', '',
        f"Verified {checked:,} located source lines, all source/evidence hashes, all shared source-token ceilings, all scores, native Flexcontext byte accounting and 20 resident repetitions. Compressed raw outputs, normalized source lines, manifests, executable hashes, the implementation archive and machine-readable summaries remain in the local run directory.", '']
    presentation=out/'presentation-summary.json'
    if presentation.exists():
        presentation=json.loads(presentation.read_text())
        lines+=['## Presentation-only control','',
            f"With baseline retrieval held constant, compact output preserves all source text, source spans, ordering and truncation flags in all 36 held-out question/budget pairs. Non-source payload tokens change by **{(presentation['metadata_ratio']-1)*100:+.1f}%**. This isolates the default wire-format benefit from the failed focused retrieval changes.", '',
            'The additional compact-baseline resident timings use 20 repetitions per question/budget in a separate run block. They are descriptive and are not substituted into the randomized acceptance gate. See `presentation-summary.json`.', '']
    lines+=['## Next development targets','',
        'Removing the repeated-file penalty and adding the small implementation preference each improve aggregate development recall from 80.6% to 83.3%. File-frequency weighting alone falls to 66.7%; the weighting experiment should be revisited before combining it with selection changes. A 0.55 cutoff alone keeps aggregate recall at 80.6% but reduces irrelevant source by only about 8%, below the 25% target. These are development observations, not independently accepted production variants.', '',
        'The next retrieval experiment should diagnose term weighting and statement selection on development cases, retain the now-established compact/scoped interfaces, and use a new held-out split for any further tuning-informed promotion claim. No further ranking changes were made after inspecting this held-out run.', '']
    (out/'RESULTS.md').write_text('\n'.join(lines))
    print(json.dumps(verification,indent=2))

if __name__=='__main__':main()
