#!/usr/bin/env python3
"""Audit persisted progressive trials and emit small reviewable comparison receipts."""
import argparse
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent


def load(path):
    return json.loads(path.read_text())


def dump(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2)+'\n')


def audit(case, calls, responses):
    requested = [c for r in responses for c in (r['choices'][0]['message'].get('tool_calls') or [])]
    valid = len(requested) == len(calls)
    for req, got in zip(requested, calls):
        valid &= req['function']['name'] == got['tool'] and json.loads(req['function']['arguments']) == got['arguments']
        valid &= hashlib.sha256(got['payload'].encode()).hexdigest() == got['payload_sha256']
    valid &= bool(calls and calls[0]['tool'] == 'search')
    source, enriched = {}, []
    prior = set()
    for call in calls:
        value = json.loads(call['payload'])
        parts = [value] if call['tool'] == 'read_file' and 'content' in value else value.get('results', [])
        for part in parts:
            source.setdefault(part['path'], []).append(part['content'])
        joined = {p: '\n'.join(values) for p, values in source.items()}
        found = {e['id'] for e in case['required_evidence'] if e['snippet'] in joined.get(e['path'], '')}
        enriched.append(dict(call, displaced_source_bytes=value.get('navigation', {}).get('displaced_source_bytes', 0),
                             new_required_evidence=sorted(found-prior),
                             cumulative_required_evidence=sorted(found)))
        prior = found
    return bool(valid), enriched


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--cohort', action='append', required=True, help='NAME=directory')
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args()
    cases = {c['id']: c for c in load(HERE/'progressive/cases.json')['cases']}
    manual = load(args.output/'manual-review.json')
    trials, contracts, partials = [], {}, []
    for item in args.cohort:
        name, path = item.split('=', 1)
        root = Path(path)
        contracts[name] = load(root/'contract.json')
        for case, arm in contracts[name]['order']:
            folder = root/(case+'-'+arm)
            if not (folder/'result.json').exists():
                if folder.exists():
                    responses = load(folder/'responses.json') if (folder/'responses.json').exists() else []
                    partials.append({'cohort': name, 'case': case, 'arm': arm,
                                     'known_usage': {k: sum(r.get('usage', {}).get(k, 0) for r in responses)
                                                     for k in ['prompt_tokens','completion_tokens','total_tokens']},
                                     'in_flight_usage_unknown': True,
                                     'reason': 'Administrative pause for compact references and provider-compatible call-limit message'})
                continue
            result, calls, responses = load(folder/'result.json'), load(folder/'calls.json'), load(folder/'responses.json')
            valid, calls = audit(cases[case], calls, responses)
            review = manual['reviews'].get(name+'/'+case+'-'+arm, {'pass': False, 'note': 'Review pending'})
            trials.append(dict(result, cohort=name, audit_valid=valid, manual_explanation_review=review,
                               attempted_tool_requests=sum(len(r['choices'][0]['message'].get('tool_calls') or []) for r in responses),
                               displaced_source_bytes=sum(c['displaced_source_bytes'] for c in calls),
                               completed_investigation=bool(valid and result['complete_evidence'] and not result['failure'] and review['pass']),
                               expansions_without_new_required_evidence=sum(c['tool']=='expand' and not c['new_required_evidence'] for c in calls),
                               calls_detail=calls))
    summary = {}
    for name in contracts:
        summary[name] = {}
        for arm in 'ABC':
            selected = [t for t in trials if t['cohort']==name and t['arm']==arm]
            if not selected:
                continue
            summary[name][arm] = {'trials': len(selected),
                'complete_evidence': sum(t['complete_evidence'] for t in selected),
                'correct_explanations': sum(t['manual_explanation_review']['pass'] for t in selected),
                'completed_investigations': sum(t['completed_investigation'] for t in selected),
                'errors': sum(t['errors'] for t in selected), 'failures': sum(bool(t['failure']) for t in selected),
                **{key: sum(t[key] for t in selected) for key in ['calls','expansions','expansions_without_new_required_evidence',
                    'payload_tokens','payload_bytes','navigation_tokens','repeated_source_line_bytes','displaced_source_bytes','elapsed_seconds']},
                'usage': {key: sum(t['usage'][key] or 0 for t in selected) for key in ['prompt_tokens','completion_tokens','total_tokens']}}
    dump(args.output/'results.json', {'summary': summary, 'partial_attempts': partials,
        'known_model_tokens': sum(t['usage']['total_tokens'] or 0 for t in trials)+sum(p['known_usage']['total_tokens'] for p in partials),
        'usage_incomplete': bool(partials), 'review_rubric': manual['rubric'], 'reviewer': manual['reviewer'],
        'trials': trials})
    dump(args.output/'contracts.json', contracts)
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
