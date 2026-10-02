#!/usr/bin/env python3
"""Check compact presentation with baseline retrieval, holding source identical.

This control does not choose or tune retrieval. It verifies the shipped default
wire change separately from the failed focused-policy experiment.
"""
import argparse
import gzip
import json
import random
import statistics
from pathlib import Path

import run as h
import tiktoken


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output',type=Path)
    out=parser.parse_args().output.resolve()
    assert not (out/'presentation.jsonl').exists()
    contract=json.loads((out/'contract.json').read_text())
    assert h.sha((out/'flexcontext-candidate').read_bytes())==contract['candidate_sha256']
    cases=json.loads((out/'cases-frozen.json').read_text())['cases']
    cases=[c for c in cases if c['split']=='heldout']
    baseline=[json.loads(line) for line in (out/'heldout.jsonl').read_text().splitlines()]
    baseline={(r['case'],r['budget']):r for r in baseline if r['variant']=='baseline' and r['adapter']=='native'}
    enc=tiktoken.get_encoding('cl100k_base');rng=random.Random(1042)
    rows=[];timings=[]
    for repo in ['flexcontext','agentx','webkitpy']:
        root=out/'snapshots'/repo
        p=h.Resident([out/'flexcontext-candidate','serve',root],out/f'{repo}-presentation.stderr.txt',180)
        try:
            p.call(h.mcp('server/discover'))
            jobs=[(c,b) for c in cases if c['repo']==repo for b in [2048,4096]]
            def request(case,b):return h.mcp('tools/call',{'query':' '.join(h.terms(case['question'])),'budget':b*2,'max_results':100,'detail':'compact','policy':'baseline'})
            for case,b in jobs:
                raw,_=p.call(request(case,b));data=json.loads(raw)['result']['structuredContent']
                oldrow=baseline[case['id'],b]
                oldpath=(out/oldrow['artifact']).with_name(Path(oldrow['artifact']).name.replace('.lines.json','.raw.json.gz'))
                with gzip.open(oldpath,'rt') as f:old=json.load(f)['result']['structuredContent']
                fields=['path','symbol','kind','containing_symbol','start_line','end_line','content','source_spans','content_truncated']
                extract=lambda value:[{k:r[k] for k in fields} for r in value['results']]
                assert extract(data)==extract(old),(repo,case['id'],b)
                for result in data['results']:result['content']=''
                metadata=len(enc.encode(json.dumps(data,separators=(',',':'),ensure_ascii=False),disallowed_special=()))
                row={'case':case['id'],'repo':repo,'budget':b,'source_identical':True,'baseline_metadata_tokens':oldrow['metadata_tokens'],'compact_metadata_tokens':metadata,'native_bytes':len(raw.encode()),'native_tokens':len(enc.encode(raw,disallowed_special=()))}
                rows.append(row)
                with (out/'presentation.jsonl').open('a') as f:f.write(json.dumps(row)+'\n')
            for repeat in range(20):
                rng.shuffle(jobs)
                for case,b in jobs:
                    raw,ms=p.call(request(case,b));assert not json.loads(raw)['result'].get('isError')
                    timings.append({'repo':repo,'case':case['id'],'budget':b,'repeat':repeat,'latency_ms':ms})
            print('presentation control',repo,flush=True)
        finally:p.close()
    h.dump(out/'presentation-latency.json',timings)
    summary=[]
    for repo in ['flexcontext','agentx','webkitpy']:
        for b in [2048,4096]:
            values=sorted(t['latency_ms'] for t in timings if t['repo']==repo and t['budget']==b)
            summary.append({'repo':repo,'budget':b,'samples':len(values),'median_ms':statistics.median(values),'p95_ms':values[(95*len(values)+99)//100-1]})
    h.dump(out/'presentation-summary.json',{'source_identical_all_36_pairs':True,'metadata_ratio':sum(r['compact_metadata_tokens'] for r in rows)/sum(r['baseline_metadata_tokens'] for r in rows),'timing':summary,'timing_note':'Descriptive separate run block; do not treat as a randomized baseline-vs-compact latency gate.','harness_sha256':h.sha(Path(__file__).read_bytes())})

if __name__=='__main__':main()
