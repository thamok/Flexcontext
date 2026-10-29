#!/usr/bin/env python3
"""Replay a frozen progressive pilot through its existing broker; no model calls."""
import argparse
import hashlib
import json
from pathlib import Path
from progressive_eval import Broker, CASES, LIMITS


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cohort', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    contract = json.loads((args.cohort/'contract.json').read_text())
    assert contract['manifest_sha256'] == hashlib.sha256(CASES.read_bytes()).hexdigest()
    assert contract['limits'] == LIMITS
    cases = {case['id']: case for case in json.loads(CASES.read_text())['cases']}
    receipts = json.loads((args.cohort/'results.json').read_text())
    rows = []
    for receipt in receipts:
        case, arm = receipt['case'], receipt['arm']
        broker = Broker(args.binary.resolve(), cases[case], arm)
        for call in json.loads((args.cohort/f'{case}-{arm}'/'calls.json').read_text()):
            payload = broker.call(call['tool'], call['arguments'])
            before, after = json.loads(call['payload']), json.loads(payload)
            def source(value):
                if 'results' in value:
                    return [{k: r[k] for k in ('path', 'symbol', 'content', 'source_spans', 'content_truncated')} for r in value['results']]
                return {k: value[k] for k in ('path', 'start_line', 'content') if k in value}
            rows.append({'case': case, 'arm': arm, 'call': call['call'], 'tool': call['tool'],
                         'payload_exact': before == after, 'source_exact': source(before) == source(after),
                         'error_same': call['error'] == broker.records[-1]['error'],
                         'original_payload_tokens': call['payload_tokens'],
                         'replayed_payload_tokens': broker.records[-1]['payload_tokens']})
    report = {'cohort_binary_sha256': contract['binary_sha256'],
              'final_binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
              'calls': len(rows), 'all_payloads_exact': all(r['payload_exact'] for r in rows),
              'all_source_exact': all(r['source_exact'] for r in rows),
              'all_errors_same': all(r['error_same'] for r in rows), 'rows': rows}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps({k:v for k,v in report.items() if k != 'rows'}))
    if not report['all_source_exact'] or not report['all_errors_same']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
