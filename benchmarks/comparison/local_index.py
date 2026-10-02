#!/usr/bin/env python3
"""Compare the frozen local BM25 experiment with baseline on existing cases.

Defaults to development only. Heldout is a historical regression partition,
not a new blind holdout. Retrieval never receives evidence labels.
"""
import argparse
import collections
import json
import statistics
import shutil
from pathlib import Path

import run as h
import tiktoken


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=h.PROJECT / 'target/release/examples/bm25_experiment')
    parser.add_argument('--snapshot-root', type=Path, default=h.PROJECT / '.benchmark-results/optimization-focused-20260916/snapshots')
    parser.add_argument('--cases', type=Path, default=h.HERE / 'optimization-cases.json')
    parser.add_argument('--split', choices=['dev', 'heldout', 'all'], default='dev')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    shutil.copy2(args.binary, args.output / 'bm25-worker')
    args.binary = (args.output / 'bm25-worker').resolve()
    cases = json.loads(args.cases.read_text())['cases']
    cases = [c for c in cases if args.split == 'all' or c['split'] == args.split]
    roots = {c['repo']: (args.snapshot_root / c['repo']).resolve() for c in cases}
    h.validate_cases(cases, roots)
    h.dump(args.output / 'contract.json', {
        'binary_sha256': h.sha(args.binary.read_bytes()),
        'cases_sha256': h.sha(args.cases.read_bytes()),
        'harness_sha256': h.sha(Path(__file__).read_bytes()),
        'split': args.split, 'budgets': [2048, 4096],
        'policies': ['baseline', 'bm25'],
        'bm25': {'k1': 1.2, 'b': .75, 'document': 'original symbol source + path + containing symbol',
                 'idf': 'log(1 + (N - df + .5)/(df + .5))',
                 'morphology': 'existing symmetric prefix/stem match; merged term frequencies',
                 'relations': 'metadata only; no score propagation',
                 'nested_declarations': 'Excluded when at least 8 symbols have positive BM25 score; baseline instead applies its structural penalty.',
                 'selection': 'unchanged quota, diversity and AST source selection'},
        'promotion_gate': 'No recall loss in any repo/budget group; meaningful aggregate gain; new unseen confirmation required.',
        'timing': 'First call only, includes lazy BM25 construction; not a latency acceptance measurement.',
        'shared_harness_sha256': h.sha(Path(h.__file__).read_bytes()),
        'snapshots': {name: {str(p.relative_to(root)): h.sha(p.read_bytes()) for p in sorted(root.rglob('*'))
                           if p.is_file() and '.flexcontext' not in p.parts} for name, root in roots.items()},
    })
    enc = tiktoken.get_encoding('cl100k_base')
    rows = []
    for repo, root in sorted(roots.items()):
        source = h.Source(root)
        process = h.Resident([args.binary.resolve(), 'serve', root], args.output / f'{repo}.stderr.txt', 180)
        try:
            raw, _ = process.call(h.mcp('server/discover'))
            assert 'error' not in json.loads(raw)
            for case in [c for c in cases if c['repo'] == repo]:
                for budget in [2048, 4096]:
                    for policy in ['baseline', 'bm25']:
                        raw, elapsed = process.call(h.mcp('tools/call', {
                            'query': ' '.join(h.terms(case['question'])), 'budget': budget * 2,
                            'max_results': 100, 'detail': 'compact', 'policy': policy}))
                        data = json.loads(raw)
                        assert 'error' not in data and not data['result'].get('isError')
                        records, _ = h.normalize('flexcontext', raw, source, resident=True)
                        selected, tokens = h.cap(records, budget, enc)
                        regions = {(s['path'], n) for s in case['relevant_regions'] for n in range(s['start'], s['end'] + 1)}
                        row = {'case': case['id'], 'repo': repo, 'split': case['split'], 'budget': budget,
                               'policy': policy, **h.score(case, selected), 'source_tokens': tokens,
                               'irrelevant_tokens': sum(len(enc.encode(r['text'] + '\n', disallowed_special=()))
                                                        for r in selected if (r['path'], r['line']) not in regions),
                               'first_ms': elapsed}
                        rows.append(row)
                        stem = f"{case['id']}-{budget}-{policy}"
                        h.dump(args.output / 'artifacts' / f'{stem}.json', data)
                        h.dump(args.output / 'artifacts' / f'{stem}.lines.json', selected)
                        print(json.dumps(row), flush=True)
        finally:
            process.close()
    groups = collections.defaultdict(list)
    for row in rows:
        groups[(row['repo'], row['split'], row['budget'], row['policy'])].append(row)
    summary = [{'repo': key[0], 'split': key[1], 'budget': key[2], 'policy': key[3], 'questions': len(group),
                **{metric: statistics.mean(r[metric] for r in group)
                   for metric in ['evidence_recall', 'evidence_precision', 'source_tokens', 'irrelevant_tokens']}}
               for key, group in sorted(groups.items())]
    h.dump(args.output / 'results.json', {'rows': rows, 'summary': summary})
    contract = json.loads((args.output / 'contract.json').read_text())
    for name, root in roots.items():
        for path, digest in contract['snapshots'][name].items():
            assert h.sha((root / path).read_bytes()) == digest, f'snapshot changed: {name}/{path}'
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
