#!/usr/bin/env python3
"""Summarize all scheduled agent attempts without dropping invalid trials."""
import argparse
import itertools
import json
import statistics
from pathlib import Path


def summarize(data):
    rows, contract = data['rows'], data['contract']
    summaries = {}
    for arm in contract['arms']:
        group = [r for r in rows if r['arm'] == arm]
        successes = sum(r['task_success'] for r in group)
        unknown_usage = sum(r['trace'].get('usage_unknown', False) or r['trace']['usage_totals']['total_tokens'] is None for r in group)
        known_model_tokens = sum(r['trace']['usage_totals']['total_tokens'] or 0 for r in group)
        model_tokens = None if unknown_usage else known_model_tokens
        summaries[arm] = {
            'scheduled': len(contract['tasks'])*contract['repeats'], 'recorded': len(group),
            'successes': successes, 'valid_trials': sum(r['valid'] for r in group),
            'acceptance_passes': sum(r['acceptance_passed'] for r in group),
            'public_regression_passes': sum(r['public_regressions_passed'] for r in group),
            'timeouts': sum(r['timeout'] for r in group),
            'errors': [{'task': r['task'], 'repeat': r['repeat'], 'error': r['error']}
                       for r in group if r['error']],
            'mean_seconds': statistics.mean(r['elapsed_seconds'] for r in group) if group else None,
            'median_seconds': statistics.median(r['elapsed_seconds'] for r in group) if group else None,
            'mean_source_tokens': statistics.mean(r['source_tokens_delivered'] for r in group) if group else None,
            'mean_tool_output_tokens': statistics.mean(r['tool_output_tokens'] for r in group) if group else None,
            'mean_tool_calls': statistics.mean(r['tool_calls'] for r in group) if group else None,
            'total_model_tokens': model_tokens,
            'unknown_usage_trials': unknown_usage, 'known_model_tokens': known_model_tokens,
            'model_tokens_per_success': model_tokens/successes if successes and model_tokens is not None else None,
            'by_task': {task: {'successes': sum(r['task_success'] for r in group if r['task'] == task),
                              'attempts': sum(r['task'] == task for r in group)}
                        for task in contract['tasks']}}
    pairs = {}
    for left, right in itertools.combinations(contract['arms'], 2):
        indexed = {(r['task'], r['repeat'], r['arm']): r for r in rows}
        counts = {'left_only_success': 0, 'right_only_success': 0, 'both_success': 0, 'neither_success': 0,
                  'missing_pairs': 0}
        for task in contract['tasks']:
            for repeat in range(contract['repeats']):
                a, b = indexed.get((task, repeat, left)), indexed.get((task, repeat, right))
                if a is None or b is None:
                    counts['missing_pairs'] += 1
                else:
                    key = ('both_success' if a['task_success'] and b['task_success'] else
                           'left_only_success' if a['task_success'] else
                           'right_only_success' if b['task_success'] else 'neither_success')
                    counts[key] += 1
        pairs[left+' vs '+right] = counts
    return {'contract': contract, 'arms': summaries, 'paired_attempts': pairs,
            'limitation': 'Only three assistant-authored seeded task clusters; repeated attempts are not independent tasks. No completion-rate superiority or non-inferiority claim is supported.'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('results', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    result = summarize(json.loads(args.results.read_text()))
    text = json.dumps(result, indent=2) + '\n'
    if args.output:
        args.output.write_text(text)
    else:
        print(text, end='')


if __name__ == '__main__':
    main()
