#!/usr/bin/env python3
"""Verify final excerpts against a frozen paired run and time the final binary.

Baseline timings come from the earlier paired run; these final timings are
collected subsequently, not interleaved with new baseline calls.
"""
import argparse
import collections
import json
import random
import shutil
import statistics
from pathlib import Path
import run as h


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--reference', type=Path, required=True)
    p.add_argument('--candidate', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--repeats', type=int, default=20)
    args = p.parse_args()
    assert args.repeats > 0
    args.output.mkdir(parents=True, exist_ok=False)
    reference = args.reference.resolve()
    contract = json.loads((reference / 'contract.json').read_text())
    cases = json.loads((reference / 'cases-frozen.json').read_text())['cases']
    roots = {n: Path(v['root']) for n, v in contract['snapshots'].items()}
    h.validate_cases(cases, roots)
    for name, root in roots.items():
        for path, digest in contract['snapshots'][name]['files_sha256'].items():
            assert h.sha((root / path).read_bytes()) == digest, f'snapshot changed: {name}/{path}'
    binary = args.output / 'flexcontext-final'
    shutil.copy2(args.candidate, binary)
    binary = binary.resolve()
    h.dump(args.output / 'contract.json', {'reference': str(reference),
        'reference_contract_sha256': h.sha((reference / 'contract.json').read_bytes()),
        'final_binary_sha256': h.sha(binary.read_bytes()), 'repeats': args.repeats, 'seed': 1729,
        'comparison': 'Every result, content string, source span and order must equal the paired candidate. Final resident timings collected separately after warmup.'})
    rng = random.Random(1729)
    timings = collections.defaultdict(list)
    startups = {}
    verified = []
    for repo, root in roots.items():
        resident = h.Resident([binary, 'serve', root], args.output / f'{repo}.stderr.txt', 180)
        try:
            _, startups[repo] = resident.call(h.mcp('server/discover'))
            jobs = [(c, b) for c in cases if c['repo'] == repo for b in [2048, 4096]]
            expected = {}
            def request(case, budget):
                return h.mcp('tools/call', {'query': ' '.join(h.terms(case['question'])),
                    'budget': budget * 2, 'max_results': 100, 'detail': 'compact', 'policy': 'baseline'})
            for case, budget in jobs:
                raw, _ = resident.call(request(case, budget))
                actual = json.loads(raw)['result']['structuredContent']['results']
                prior = json.loads((reference / 'artifacts' / f"{case['id']}-{budget}-candidate.json").read_text())['result']['structuredContent']['results']
                assert actual == prior, f"context changed: {case['id']} {budget}"
                expected[(case['id'], budget)] = actual
                verified.append({'case': case['id'], 'budget': budget})
            for repeat in range(args.repeats):
                rng.shuffle(jobs)
                for case, budget in jobs:
                    raw, elapsed = resident.call(request(case, budget))
                    assert json.loads(raw)['result']['structuredContent']['results'] == expected[(case['id'], budget)]
                    timings[(repo, case['split'], budget)].append(elapsed)
                if repeat % 5 == 0:
                    print(f'{repo}: equivalent, timed round {repeat+1}/{args.repeats}', flush=True)
        finally:
            resident.close()
    summary = []
    for key, values in sorted(timings.items()):
        ordered = sorted(values)
        summary.append({'repo': key[0], 'split': key[1], 'budget': key[2], 'samples': len(values),
            'resident_median_ms': statistics.median(values),
            'resident_p95_ms': ordered[min(len(ordered)-1, int(len(ordered)*.95))]})
    h.dump(args.output / 'results.json', {'context_equivalence_pass': True, 'verified_pairs': verified,
        'summary': summary, 'startup_ms': startups,
        'timing_samples_ms': {'|'.join(map(str, key)): values for key, values in timings.items()}})
    print(f'Exact context equality: {len(verified)} question/budget pairs', flush=True)


if __name__ == '__main__':
    main()
