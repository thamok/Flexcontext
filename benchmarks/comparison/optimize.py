#!/usr/bin/env python3
"""Frozen retrieval experiments; no model calls. Run prepare, dev, heldout, compare.

The pilot is read-only. Each stage is append-only and refuses to overwrite its
results. Quality is normalized exactly as in the pilot. Timing repetitions do
not include normalization/tokenization and run serially in randomized order.
"""
import argparse
import collections
import gzip
import json
import platform
import random
import shutil
import statistics
import sys
import time
from pathlib import Path

import run as h
import tiktoken

OUT = h.PROJECT / '.benchmark-results/optimization-focused-20260916'
BASELINE = h.PROJECT / '.benchmark-results/optimization-20260916/flexcontext-baseline'
CANDIDATE = h.PROJECT / 'target/release/flexcontext'
PILOT = h.PROJECT / '.benchmark-results/pilot-20260916'
CASES = h.HERE / 'optimization-cases.json'
BUDGETS = [2048, 4096]
SINGLES = ['relations','quotas','diversity','idf','direct','implementation','focus','stable']
ENC = tiktoken.get_encoding('cl100k_base')

def prepare():
    assert not (OUT/'contract.json').exists(), 'experiment already frozen'
    OUT.mkdir(exist_ok=True)
    if not (OUT/'flexcontext-baseline').exists(): shutil.copy2(BASELINE,OUT/'flexcontext-baseline')
    for repo in ['flexcontext','agentx','webkitpy']:
        manifest=json.loads((PILOT/f'{repo}.manifest.json').read_text())
        for name,info in manifest['files'].items():
            data=(PILOT/'snapshots'/repo/name).read_bytes()
            assert h.sha(data)==info['sha256']
            dest=OUT/'snapshots'/repo/name
            dest.parent.mkdir(parents=True,exist_ok=True); dest.write_bytes(data)
        shutil.copy2(PILOT/f'{repo}.files.json',OUT/f'{repo}.files.json')
        shutil.copy2(PILOT/f'{repo}.manifest.json',OUT/f'{repo}.manifest.json')
    shutil.copy2(CANDIDATE,OUT/'flexcontext-candidate')
    shutil.copy2(CASES,OUT/'cases-frozen.json')
    h.dump(OUT/'contract.json',{
        'cases_sha256':h.sha(CASES.read_bytes()),'baseline_sha256':h.sha((OUT/'flexcontext-baseline').read_bytes()),
        'candidate_sha256':h.sha((OUT/'flexcontext-candidate').read_bytes()),'harness_sha256':h.sha(Path(__file__).read_bytes()),
        'budgets':BUDGETS,'cutoffs':[.25,.40,.55],'single_factor_policies':SINGLES,
        'selection':'Highest cutoff whose dev mean evidence recall is >= baseline in every repo/budget, for both native and fixed candidate adapters. If none qualifies, .25 is tested diagnostically and promotion fails.',
        'heldout_gate':{'recall':'no lower mean in any repository/budget/adapter group','irrelevant_source_token_ratio_max':.75,'metadata_token_ratio_max':.5,'resident_p95_ratio_max':1.1,'resident_repetitions':20},
        'irrelevant_tokens':'Sum shared tokenizer costs of emitted nonblank source lines outside frozen relevant_regions, including unlocated lines. No relevance credit for unjudged source.',
        'metadata_tokens':'Shared tokenizer cost of native JSON after replacing result content strings with empty strings; scores, signatures and relation duplicates remain metadata. Serialized compact JSON is used for this disjoint-field metric for both formats.',
        'fixed_candidate_adapter':'Both 2k and 4k normalization caps query a constant 32768 source-byte ceiling and 100 result slots. No answer-dependent reranking.',
        'native_adapter':'Pilot overfetch policy: native source-byte ceiling = common source token budget * 8; prefix cap is unchanged.',
        'latency':'Serial randomized baseline/candidate MCP calls. Warm each case/budget; 20 repeats each. OS page cache uncontrolled. New-process cold clears only tool cache.',
        'platform':platform.platform(),'no_agent_evaluation':True})
    for folder in ['src','tests','benchmarks/comparison']:
        for source in (h.PROJECT/folder).rglob('*'):
            if source.is_file() and '__pycache__' not in source.parts:
                dest=OUT/'implementation'/source.relative_to(h.PROJECT)
                dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,dest)
    for name in ['Cargo.toml','Cargo.lock','build.rs']:
        shutil.copy2(h.PROJECT/name,OUT/'implementation'/name)
    print('Frozen contract',flush=True)

def load():
    contract=json.loads((OUT/'contract.json').read_text())
    assert h.sha(CASES.read_bytes())==contract['cases_sha256']
    for name in ['baseline','candidate']:
        assert h.sha((OUT/f'flexcontext-{name}').read_bytes())==contract[f'{name}_sha256']
    cases=json.loads(CASES.read_text())['cases']
    roots={r:OUT/'snapshots'/r for r in ['flexcontext','agentx','webkitpy']}
    h.validate_cases(cases,roots)
    return cases,roots

def write_row(stage,row):
    with (OUT/f'{stage}.jsonl').open('a') as f: f.write(json.dumps(row)+'\n')

def measure(stage,case,variant,budget,adapter,raw,elapsed,source,resident=True,tool='flexcontext'):
    records,_=h.normalize(tool,raw,source,resident)
    selected,tokens=h.cap(records,budget,ENC)
    regions={(s['path'],n) for s in case['relevant_regions'] for n in range(s['start'],s['end']+1)}
    irrelevant=sum(len(ENC.encode(r['text']+'\n',disallowed_special=())) for r in selected if (r['path'],r['line']) not in regions)
    metadata=None
    if tool=='flexcontext':
        data=json.loads(raw)
        data=data['result']['structuredContent'] if resident else data
        for result in data['results']: result['content']=''
        metadata=len(ENC.encode(json.dumps(data,separators=(',',':'),ensure_ascii=False),disallowed_special=()))
    row={'case':case['id'],'repo':case['repo'],'split':case.get('split','pilot'),'variant':variant,'tool':tool,'budget':budget,'adapter':adapter,
         **h.score(case,selected),'source_tokens':tokens,'source_bytes':sum(len((r['text']+'\n').encode()) for r in selected),
         'irrelevant_tokens':irrelevant,'metadata_tokens':metadata,'native_bytes':len(raw.encode()),'native_tokens':len(ENC.encode(raw,disallowed_special=())),
         'latency_ms':elapsed}
    stem=f"{case['id']}-{variant}-{budget}-{adapter}"
    folder=OUT/'artifacts'/stage;folder.mkdir(parents=True,exist_ok=True)
    with gzip.open(folder/f'{stem}.raw.json.gz','wt') as f:f.write(raw)
    h.dump(folder/f'{stem}.lines.json',selected)
    row['artifact']=str((folder/f'{stem}.lines.json').relative_to(OUT))
    write_row(stage,row)
    return row

def start(binary,repo,root,tag):
    p=h.Resident([OUT/f'flexcontext-{binary}','serve',root],OUT/f'{repo}-{tag}-{binary}.stderr.txt',180)
    raw,elapsed=p.call(h.mcp('server/discover'))
    assert 'error' not in json.loads(raw)
    return p

def request(case,budget,variant,adapter='native'):
    args={'query':' '.join(h.terms(case['question'])),'budget':8192 if adapter=='fixed' else budget*2,'max_results':100}
    if variant!='baseline':
        policy,_,cutoff=variant.partition('-')
        args.update(policy=policy,detail='compact')
        if cutoff:args['cutoff']=float(cutoff)
    return h.mcp('tools/call',args)

def groups(rows,variant):
    result=collections.defaultdict(list)
    for r in rows:
        if r['variant']==variant:result[(r['repo'],r['budget'],r['adapter'])].append(r)
    return result

def recall_pass(rows,variant):
    base=groups(rows,'baseline'); new=groups(rows,variant)
    return all(k in new and statistics.mean(r['evidence_recall'] for r in new[k])+1e-12>=statistics.mean(r['evidence_recall'] for r in v) for k,v in base.items())

def dev():
    assert not (OUT/'dev.jsonl').exists()
    cases,roots=load();cases=[c for c in cases if c['split']=='dev'];rows=[]
    rng=random.Random(1729)
    for repo,root in roots.items():
        processes={name:start(name,repo,root,'dev') for name in ['baseline','candidate']}
        try:
            source=h.Source(root)
            jobs=[(c,b,v,a) for c in cases if c['repo']==repo for b in BUDGETS
                  for v in ['baseline',*SINGLES,'cutoff-0.25','cutoff-0.4','cutoff-0.55','focused-0.25','focused-0.4','focused-0.55']
                  for a in (['native','fixed'] if v=='baseline' or v.startswith('focused') else ['native'])]
            rng.shuffle(jobs)
            for i,(case,budget,variant,adapter) in enumerate(jobs):
                raw,elapsed=processes['baseline' if variant=='baseline' else 'candidate'].call(request(case,budget,variant,adapter))
                rows.append(measure('dev',case,variant,budget,adapter,raw,elapsed,source))
                if i%20==0:print(repo,i,len(jobs),flush=True)
        finally:
            for p in processes.values():p.close()
    eligible=[c for c in [.25,.4,.55] if recall_pass(rows,f'focused-{c}')]
    selection={'eligible_cutoffs':eligible,'selected_cutoff':max(eligible) if eligible else .25,'dev_recall_gate_pass':bool(eligible)}
    h.dump(OUT/'selection.json',selection);print(selection,flush=True)

def pilot():
    """Use the original questions as development diagnostics, without writing
    into the original pilot directory or changing its labels."""
    assert not (OUT/'pilot-diagnostics.jsonl').exists()
    _,roots=load();cases=json.loads((h.HERE/'cases.json').read_text())['cases']
    h.validate_cases(cases,roots)
    selection=json.loads((OUT/'selection.json').read_text());variant=f"focused-{selection['selected_cutoff']}"
    for repo,root in roots.items():
        processes={name:start(name,repo,root,'pilot') for name in ['baseline','candidate']}
        try:
            for case in [c for c in cases if c['repo']==repo]:
                for budget in BUDGETS:
                    for v in ['baseline',variant]:
                        for adapter in ['native','fixed']:
                            raw,elapsed=processes['baseline' if v=='baseline' else 'candidate'].call(request(case,budget,v,adapter))
                            measure('pilot-diagnostics',case,v,budget,adapter,raw,elapsed,h.Source(root))
            print('pilot diagnostics',repo,flush=True)
        finally:
            for p in processes.values():p.close()

def percentile(values):
    values=sorted(values);return values[max(0,(95*len(values)+99)//100-1)]

def heldout():
    assert not (OUT/'heldout.jsonl').exists()
    cases,roots=load();cases=[c for c in cases if c['split']=='heldout']
    selection=json.loads((OUT/'selection.json').read_text());variant=f"focused-{selection['selected_cutoff']}"
    rows=[];times=[];rng=random.Random(20260916)
    for repo,root in roots.items():
        processes={name:start(name,repo,root,'heldout') for name in ['baseline','candidate']}
        try:
            source=h.Source(root)
            queries=[(c,b,v,a) for c in cases if c['repo']==repo for b in BUDGETS for v in ['baseline',variant] for a in ['native','fixed']]
            rng.shuffle(queries)
            for case,budget,v,adapter in queries:
                raw,elapsed=processes['baseline' if v=='baseline' else 'candidate'].call(request(case,budget,v,adapter))
                rows.append(measure('heldout',case,v,budget,adapter,raw,elapsed,source))
            for repeat in range(20):
                # Native adapter latency is the acceptance gate. Fixed pool quality
                # isolates adapter effects but is not a separate latency workload.
                jobs=[q for q in queries if q[3]=='native'];rng.shuffle(jobs)
                for case,budget,v,adapter in jobs:
                    raw,elapsed=processes['baseline' if v=='baseline' else 'candidate'].call(request(case,budget,v,adapter))
                    data=json.loads(raw);assert not data.get('error') and not data['result'].get('isError')
                    row={'repo':repo,'case':case['id'],'budget':budget,'variant':v,'repeat':repeat,'latency_ms':elapsed}
                    times.append(row);write_row('latency',row)
                print('timing',repo,repeat+1,flush=True)
        finally:
            for p in processes.values():p.close()
    base=[r for r in rows if r['variant']=='baseline' and r['adapter']=='native']
    candidate=[r for r in rows if r['variant']==variant and r['adapter']=='native']
    ratio=lambda key:sum(r[key] for r in candidate)/max(1,sum(r[key] for r in base))
    timing=[]
    for repo in roots:
        for b in BUDGETS:
            p95={v:percentile([r['latency_ms'] for r in times if r['repo']==repo and r['budget']==b and r['variant']==v]) for v in ['baseline',variant]}
            timing.append({'repo':repo,'budget':b,'baseline_p95_ms':p95['baseline'],'candidate_p95_ms':p95[variant],'ratio':p95[variant]/p95['baseline']})
    gates={'development':selection['dev_recall_gate_pass'],'heldout_recall':recall_pass(rows,variant),'irrelevant_source':ratio('irrelevant_tokens')<=.75,'compact_metadata':ratio('metadata_tokens')<=.5,'resident_latency':all(t['ratio']<=1.1 for t in timing)}
    h.dump(OUT/'acceptance.json',{'variant':variant,'promote':all(gates.values()),'gates':gates,'irrelevant_source_ratio':ratio('irrelevant_tokens'),'metadata_ratio':ratio('metadata_tokens'),'timing':timing})
    print(json.dumps(gates),flush=True)

def compare():
    """Four-tool comparison on all 36 questions. Cold+warm CLI, 20 Aider
    resident samples; baseline/candidate MCP repetitions are in heldout()."""
    assert not (OUT/'comparison.jsonl').exists()
    cases,roots=load();selection=json.loads((OUT/'selection.json').read_text());variant=f"focused-{selection['selected_cutoff']}"
    args=argparse.Namespace(flexcontext=OUT/'flexcontext-baseline',probe=h.PROJECT/'.benchmark-tools/node_modules/@probelabs/probe/bin/probe',output=OUT,encoding='cl100k_base')
    rng=random.Random(804)
    for repo,root in roots.items():
        source=h.Source(root)
        jobs=[(c,b,t) for c in cases if c['repo']==repo for b in BUDGETS for t in ['flexcontext','probe','rg','aider','candidate']];rng.shuffle(jobs)
        for i,(case,budget,tool) in enumerate(jobs):
            actual='flexcontext' if tool=='candidate' else tool
            argv,inp=h.argv_for(actual,case,root,budget,args)
            if tool=='candidate':argv[0]=OUT/'flexcontext-candidate';argv += ['--policy','focused','--cutoff',str(selection['selected_cutoff'])]
            h.clear_cache(actual,root)
            for mode in ['cold','warm_cli']:
                raw,err,elapsed=h.command(argv,root,180,inp,(0,1) if tool=='rg' else (0,))
                # Native rg output can be very large; still account all bytes and
                # tokens and keep a compressed artifact, with no silent overfetch cap.
                measure('comparison',case,variant if tool=='candidate' else 'baseline' if tool=='flexcontext' else tool,budget,mode,raw,elapsed,source,False,actual)
            print('compare',repo,i+1,len(jobs),tool,flush=True)
        p=h.Resident([sys.executable,h.HERE/'aider_worker.py',root,OUT/f'{repo}.files.json','cl100k_base'],OUT/f'{repo}-aider-timing.stderr.txt',180)
        try:
            p.call({'method':'ready'})
            jobs=[(c,b) for c in cases if c['repo']==repo and c['split']=='heldout' for b in BUDGETS]
            for case,b in jobs:p.call({'budget':b,'terms':h.terms(case['question'])})
            for repeat in range(20):
                rng.shuffle(jobs)
                for case,b in jobs:
                    _,ms=p.call({'budget':b,'terms':h.terms(case['question'])})
                    write_row('aider-latency',{'repo':repo,'case':case['id'],'budget':b,'repeat':repeat,'latency_ms':ms})
        finally:p.close()

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('stage',choices=['prepare','dev','pilot','heldout','compare'])
    parser.add_argument('--output',type=Path,default=OUT)
    parser.add_argument('--pilot',type=Path,default=PILOT)
    parser.add_argument('--cases',type=Path,default=CASES)
    parser.add_argument('--baseline',type=Path,default=BASELINE)
    parser.add_argument('--candidate',type=Path,default=CANDIDATE)
    args=parser.parse_args()
    OUT=args.output.resolve();PILOT=args.pilot.resolve();CASES=args.cases.resolve()
    BASELINE=args.baseline.resolve();CANDIDATE=args.candidate.resolve()
    globals()[args.stage]()
