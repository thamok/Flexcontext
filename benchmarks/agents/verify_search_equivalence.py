#!/usr/bin/env python3
"""Replay completed Flexcontext-arm searches without executing task code.

Only hashes, counts and booleans enter the durable report. Fixture bytes and
successful broker edits recreate each search state in disposable directories.
The original trial artifacts and final workspaces are never modified.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile


HERE = Path(__file__).resolve().parent
PROJECT = HERE.parent.parent


def sha(data):
    return hashlib.sha256(data).hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(',', ':')).encode()


def search(binary, workspace, query):
    result = subprocess.run([str(binary), 'search', str(workspace), query,
                             '--json', '--detail', 'compact', '--max-bytes', '8192',
                             '--max-results', '12'], capture_output=True, text=True, timeout=60)
    if result.returncode:
        # Native errors can include source or paths; retain only numeric status.
        raise RuntimeError(f'Native search failed with exit code {result.returncode}')
    return json.loads(result.stdout)['results']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--results', type=Path, required=True)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, default=PROJECT / 'target/release/flexcontext')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result_bytes = args.results.read_bytes()
    data = json.loads(result_bytes)
    tasks_bytes = (HERE / 'tasks.json').read_bytes()
    tasks = {t['id']: t for t in json.loads(tasks_bytes)['tasks']}
    if sha(tasks_bytes) != data['contract']['tasks_sha256']:
        raise ValueError('Task definitions differ from the frozen agent contract')
    rows = [r for r in data['rows'] if r['arm'] == 'flexcontext']
    identities = [(r['task'], r['repeat']) for r in rows]
    if len(identities) != len(set(identities)):
        raise ValueError('Duplicate Flexcontext trial identity')
    binary_hashes = {name: sha(path.read_bytes()) for name, path in
                     [('baseline', args.baseline), ('candidate', args.candidate)]}
    if binary_hashes['baseline'] != data['contract']['binaries_sha256']['flexcontext']:
        raise ValueError('Baseline binary differs from the one used by the agent')
    report = {
        'schema': 1, 'results_snapshot_sha256': sha(result_bytes),
        'tasks_sha256': sha(tasks_bytes), 'binary_sha256': binary_hashes,
        'script_sha256': sha(Path(__file__).read_bytes()),
        'completed_flexcontext_trials': len(rows),
        'scheduled_flexcontext_trials': len(data['contract']['tasks']) * data['contract']['repeats'],
        'method': 'Replay successful broker edits; compare full selected-result arrays and retained delivered arrays at every recorded search; ignore only top-level timing/stats.',
        'source_or_query_text_retained': False,
        'task_code_executed': False, 'trials': [],
    }
    with tempfile.TemporaryDirectory(prefix='agent-search-equality-') as temporary:
        temporary = Path(temporary)
        binaries = {}
        for name, original in [('baseline', args.baseline), ('candidate', args.candidate)]:
            binaries[name] = temporary / name
            shutil.copy2(original, binaries[name])
            if sha(binaries[name].read_bytes()) != binary_hashes[name]:
                raise ValueError('Binary changed while creating immutable replay copy')
        for trial_number, row in enumerate(rows):
            task = tasks[row['task']]
            trial = Path(row.get('trial_directory') or
                         args.results.parent / f"{row['task']}-flexcontext-{row['repeat']}")
            broker_bytes = (trial / 'broker.jsonl').read_bytes()
            event_bytes = (trial / 'events.jsonl').read_bytes()
            calls = [json.loads(line) for line in broker_bytes.splitlines()]
            deliveries = [json.loads(line) for line in event_bytes.splitlines()
                          if json.loads(line).get('type') == 'tool']
            if len(calls) != len(deliveries):
                raise ValueError('Broker and delivery counts differ')
            workspaces = {}
            for name in binaries:
                workspace = temporary / f'trial-{trial_number}' / name
                shutil.copytree(HERE / task['fixture'], workspace,
                                ignore=shutil.ignore_patterns('.DS_Store'))
                inventory = {str(p.relative_to(workspace)): sha(p.read_bytes())
                             for p in workspace.rglob('*') if p.is_file()}
                if inventory != task['files_sha256']:
                    raise ValueError('Fixture bytes differ from frozen task')
                workspaces[name] = workspace
            trial_report = {'task': row['task'], 'repeat': row['repeat'],
                            'broker_sha256': sha(broker_bytes), 'events_sha256': sha(event_bytes),
                            'searches': [], 'successful_edits_replayed': 0}
            for call, delivery in zip(calls, deliveries):
                payload = delivery['result']['content'][0]['text']
                if (delivery['name'] != call['tool'] or
                        sha(payload.encode()) != call['payload_sha256'] or
                        bool(delivery['result'].get('isError')) != call['error']):
                    raise ValueError('Delivery differs from audited broker payload')
                arguments = call['arguments']
                if call['tool'] == 'replace_text' and not call['error']:
                    path, old, new = arguments['path'], arguments['old'], arguments['new']
                    if path not in task['editable']:
                        raise ValueError('Successful edit was outside the authorized inventory')
                    for workspace in workspaces.values():
                        target = workspace / path
                        original = target.read_text()
                        if not old or original.count(old) != 1:
                            raise ValueError('Recorded successful edit does not replay uniquely')
                        target.write_text(original.replace(old, new, 1))
                    trial_report['successful_edits_replayed'] += 1
                elif call['tool'] == 'search':
                    selected = {name: search(binary, workspaces[name], arguments['query'])
                                for name, binary in binaries.items()}
                    observed = json.loads(payload)
                    retained_match = (selected['baseline'] == observed['results']
                                      if not call['error'] else None)
                    trial_report['searches'].append({
                        'call': call['call'], 'query_sha256': sha(arguments['query'].encode()),
                        'preceding_successful_edits': trial_report['successful_edits_replayed'],
                        'original_broker_error': call['error'],
                        'result_counts': {n: len(hits) for n, hits in selected.items()},
                        'result_sha256': {n: sha(canonical(hits)) for n, hits in selected.items()},
                        'selected_results_exactly_equal': selected['baseline'] == selected['candidate'],
                        'baseline_matches_delivered_results': retained_match,
                    })
            # Task tests are deliberately not executed. Exact final bytes ensure
            # successful edits fully recreate the actual final source state.
            final_hashes = {}
            for relative in task['files_sha256']:
                expected = (trial / 'workspace' / relative).read_bytes()
                if any((workspace / relative).read_bytes() != expected
                       for workspace in workspaces.values()):
                    raise ValueError('Final source differs from replayed successful edits')
                final_hashes[relative] = sha(expected)
            trial_report['final_source_sha256'] = final_hashes
            trial_report['final_source_exactly_recreated'] = True
            report['trials'].append(trial_report)
    for name, original in [('baseline', args.baseline), ('candidate', args.candidate)]:
        if sha(original.read_bytes()) != binary_hashes[name]:
            raise ValueError('Original binary changed during verification')
    searches = [s for t in report['trials'] for s in t['searches']]
    report['search_count'] = len(searches)
    report['all_selected_results_exactly_equal'] = all(s['selected_results_exactly_equal'] for s in searches)
    report['all_delivered_searches_reproduced'] = all(s['baseline_matches_delivered_results'] is not False for s in searches)
    report['all_scheduled_flexcontext_trials_available'] = len(rows) == report['scheduled_flexcontext_trials']
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: report[k] for k in ['completed_flexcontext_trials', 'scheduled_flexcontext_trials',
                                           'search_count', 'all_selected_results_exactly_equal',
                                           'all_delivered_searches_reproduced',
                                           'all_scheduled_flexcontext_trials_available']}))
    return int(not report['all_selected_results_exactly_equal'] or not report['all_delivered_searches_reproduced'])


if __name__ == '__main__':
    raise SystemExit(main())
