#!/usr/bin/env python3
"""Paired baseline/candidate gate on already frozen real-repository evidence.

Both use baseline policy and compact output. Reuses the pilot's tokenizer,
normalization and exact evidence labels; never changes or derives judgments.
"""
import argparse
import collections
import json
import random
import shutil
import statistics
from pathlib import Path

import run as h
import tiktoken


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--snapshot-root', type=Path, required=True)
    parser.add_argument('--cases', type=Path, default=h.HERE / 'optimization-cases.json')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--repeats', type=int, default=20)
    args = parser.parse_args()
    assert args.repeats > 0
    args.output.mkdir(parents=True, exist_ok=False)
    cases = json.loads(args.cases.read_text())['cases']
    roots = {name: (args.snapshot_root / name).resolve() for name in sorted({c['repo'] for c in cases})}
    h.validate_cases(cases, roots)
    shutil.copy2(args.cases, args.output / 'cases-frozen.json')
    binaries = {}
    for name, path in [('baseline', args.baseline), ('candidate', args.candidate)]:
        dest = args.output / f'flexcontext-{name}'
        shutil.copy2(path, dest)
        binaries[name] = dest.resolve()
    manifests = {}
    for name, root in roots.items():
        files = {str(p.relative_to(root)): h.sha(p.read_bytes()) for p in sorted(root.rglob('*'))
                 if p.is_file() and '.flexcontext' not in p.parts}
        manifests[name] = {'root': str(root), 'files_sha256': files}
    h.dump(args.output / 'contract.json', {
        'cases_sha256': h.sha(args.cases.read_bytes()),
        'binaries_sha256': {n: h.sha(p.read_bytes()) for n, p in binaries.items()},
        'snapshots': manifests, 'source_budgets': [2048, 4096],
        'repeats': args.repeats, 'seed': 1729, 'policy': 'baseline', 'detail': 'compact',
        'tokenizer': 'cl100k_base per nonblank line plus newline',
        'gate': 'No evidence recall loss in any repository/split/budget group.',
        'resident_timing': 'Serial randomized calls after each distinct query is warmed; startup separate.',
    })
    enc = tiktoken.get_encoding('cl100k_base')
    rng = random.Random(1729)
    rows, timings, startups = [], collections.defaultdict(list), {}
    for repo, root in roots.items():
        source = h.Source(root)
        processes = {}
        try:
            for name, binary in binaries.items():
                process = h.Resident([binary, 'serve', root], args.output / f'{repo}-{name}.stderr.txt', 180)
                processes[name] = process
                raw, elapsed = process.call(h.mcp('server/discover'))
                assert 'error' not in json.loads(raw)
                startups[f'{repo}-{name}'] = elapsed
            jobs = [(c, b, n) for c in cases if c['repo'] == repo for b in [2048, 4096] for n in binaries]
            rng.shuffle(jobs)
            for case, budget, name in jobs:
                request = h.mcp('tools/call', {'query': ' '.join(h.terms(case['question'])),
                    'budget': budget * 2, 'max_results': 100, 'detail': 'compact', 'policy': 'baseline'})
                raw, elapsed = processes[name].call(request)
                records, _ = h.normalize('flexcontext', raw, source, resident=True)
                selected, tokens = h.cap(records, budget, enc)
                regions = {(s['path'], n) for s in case['relevant_regions'] for n in range(s['start'], s['end'] + 1)}
                irrelevant = sum(len(enc.encode(r['text'] + '\n', disallowed_special=())) for r in selected if (r['path'], r['line']) not in regions)
                stem = f"{case['id']}-{budget}-{name}"
                h.dump(args.output / 'artifacts' / f'{stem}.json', json.loads(raw))
                h.dump(args.output / 'artifacts' / f'{stem}.lines.json', selected)
                rows.append({'case': case['id'], 'split': case['split'], 'repo': repo,
                    'budget': budget, 'variant': name, **h.score(case, selected),
                    'source_tokens': tokens, 'irrelevant_tokens': irrelevant,
                    'native_tokens': len(enc.encode(raw, disallowed_special=())), 'first_ms': elapsed})
            for repeat in range(args.repeats):
                rng.shuffle(jobs)
                for case, budget, name in jobs:
                    request = h.mcp('tools/call', {'query': ' '.join(h.terms(case['question'])),
                        'budget': budget * 2, 'max_results': 100, 'detail': 'compact', 'policy': 'baseline'})
                    raw, elapsed = processes[name].call(request)
                    data = json.loads(raw)
                    assert 'error' not in data and not data['result'].get('isError')
                    timings[(repo, case['split'], budget, name)].append(elapsed)
                if repeat % 5 == 0:
                    print(f'{repo}: timed round {repeat + 1}/{args.repeats}', flush=True)
        finally:
            for process in processes.values():
                process.close()
    grouped = collections.defaultdict(list)
    for row in rows:
        grouped[(row['repo'], row['split'], row['budget'], row['variant'])].append(row)
    summary = []
    for key, group in sorted(grouped.items()):
        samples = sorted(timings[key])
        summary.append({'repo': key[0], 'split': key[1], 'budget': key[2], 'variant': key[3],
            'questions': len(group), 'samples': len(samples),
            **{metric: statistics.mean(r[metric] for r in group) for metric in ['evidence_recall', 'evidence_precision', 'source_tokens', 'irrelevant_tokens', 'native_tokens']},
            'resident_median_ms': statistics.median(samples),
            'resident_p95_ms': samples[min(len(samples) - 1, int(len(samples) * .95))]})
    paired = {(r['repo'], r['split'], r['budget'], r['variant']): r for r in summary}
    failures = []
    for key, baseline in paired.items():
        if key[3] == 'baseline':
            candidate = paired[(*key[:3], 'candidate')]
            if candidate['evidence_recall'] + 1e-12 < baseline['evidence_recall']:
                failures.append({'repo': key[0], 'split': key[1], 'budget': key[2]})
    h.dump(args.output / 'results.json', {'rows': rows, 'summary': summary,
        'startup_ms': startups, 'evidence_recall_gate_pass': not failures, 'failures': failures,
        'timing_samples_ms': {'|'.join(map(str, key)): value for key, value in timings.items()}})
    print(json.dumps({'evidence_recall_gate_pass': not failures, 'failures': failures}), flush=True)


if __name__ == '__main__':
    main()
