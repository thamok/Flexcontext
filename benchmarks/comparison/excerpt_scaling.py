#!/usr/bin/env python3
"""Measure oversized-method excerpts and fail if visible context changes."""
import argparse
import json
import random
import statistics
import tempfile
from pathlib import Path
import run as h


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--before', type=Path, required=True)
    p.add_argument('--after', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--repeats', type=int, default=20)
    args = p.parse_args()
    assert args.repeats > 0
    args.output.mkdir(parents=True, exist_ok=False)
    rng = random.Random(1729)
    binaries = {'before': args.before.resolve(), 'after': args.after.resolve()}
    rows = []
    for count in [1000, 5000, 10000]:
        with tempfile.TemporaryDirectory(prefix='flexcontext-excerpts-') as temporary:
            root = Path(temporary)
            source = 'function authenticateUser(token: string) {\n'
            source += ''.join(f"  const padding{i} = 'irrelevant';\n" for i in range(count))
            source += "  if (!token) { throw new Error('auth denied 🔒'); }\n  return token;\n}\n"
            (root / 'auth.ts').write_text(source)
            processes = {}
            timings = {name: [] for name in binaries}
            contexts = {}
            request = h.mcp('tools/call', {'query': 'authenticateUser denied', 'budget': 256,
                'max_results': 1, 'detail': 'compact', 'policy': 'baseline'})
            try:
                for name, binary in binaries.items():
                    resident = h.Resident([binary, 'serve', root], args.output / f'{count}-{name}.stderr.txt', 180)
                    processes[name] = resident
                    resident.call(h.mcp('server/discover'))
                    raw, _ = resident.call(request)
                    data = json.loads(raw)['result']['structuredContent']
                    assert len(data['results']) == 1 and 'auth denied 🔒' in data['results'][0]['content']
                    assert len(data['results'][0]['content'].encode()) <= 1024
                    contexts[name] = data['results']
                assert contexts['before'] == contexts['after'], f'excerpt changed for {count} statements'
                for _ in range(args.repeats):
                    names = list(binaries); rng.shuffle(names)
                    for name in names:
                        raw, elapsed = processes[name].call(request)
                        assert json.loads(raw)['result']['structuredContent']['results'] == contexts[name]
                        timings[name].append(elapsed)
            finally:
                for process in processes.values():
                    process.close()
            row = {'padding_statements': count, 'source_bytes': len(source.encode()),
                'context_identical': True, 'repeats': args.repeats,
                'timings_ms': timings,
                'median_ms': {n: statistics.median(v) for n, v in timings.items()},
                'p95_ms': {n: sorted(v)[min(len(v)-1, int(len(v)*.95))] for n, v in timings.items()}}
            rows.append(row)
            print(json.dumps({k: row[k] for k in ['padding_statements', 'context_identical', 'median_ms']}), flush=True)
    h.dump(args.output / 'results.json', {'binary_sha256': {n: h.sha(b.read_bytes()) for n, b in binaries.items()},
        'policy': 'baseline', 'detail': 'compact', 'seed': 1729, 'rows': rows,
        'scope': 'Synthetic single TypeScript function. Resident queries after warmup; startup separate. OS cache uncontrolled.'})


if __name__ == '__main__':
    main()
