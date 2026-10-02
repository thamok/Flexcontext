#!/usr/bin/env python3
"""Run source-budgeted coding-agent smoke tasks with independent acceptance."""
import argparse
import hashlib
import json
import os
import random
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

from grade import grade, grade_public

HERE = Path(__file__).resolve().parent


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def dump(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')


def audit_trace(events, calls):
    unauthorized = []
    completed = []
    usage = None
    finished = False
    for event in events:
        item = event.get('item', {})
        kind = item.get('type')
        if kind and kind not in ['agent_message', 'reasoning', 'plan', 'error', 'mcp_tool_call']:
            unauthorized.append(item)
        if kind == 'mcp_tool_call' and item.get('server') != 'retrieval_eval':
            unauthorized.append(item)
        if event['type'] == 'item.completed' and kind == 'mcp_tool_call':
            completed.append(item)
        if event['type'] == 'turn.completed':
            usage, finished = event.get('usage'), True
        if event['type'] == 'turn.failed':
            finished = False
    # Fail closed on missing/truncated traces, unbrokered tools or differences
    # between independent broker audit and the agent's completed tool events.
    valid = finished and not unauthorized and len(completed) == len(calls)
    return {'valid': valid, 'turn_completed': finished, 'unauthorized': unauthorized,
            'completed_mcp_calls': len(completed), 'broker_calls': len(calls), 'usage': usage}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--baseline', type=Path, required=True)
    p.add_argument('--candidate', type=Path, required=True)
    p.add_argument('--task', action='append')
    p.add_argument('--arm', choices=['rg', 'baseline', 'candidate'], action='append')
    p.add_argument('--model', required=True)
    p.add_argument('--effort', default='high')
    p.add_argument('--repeats', type=int, default=1)
    p.add_argument('--timeout', type=int, default=180)
    p.add_argument('--source-limit', type=int, default=8000)
    p.add_argument('--call-limit', type=int, default=40)
    p.add_argument('--dry-run', action='store_true')
    args = p.parse_args()
    if min(args.repeats, args.timeout, args.source_limit, args.call_limit) <= 0:
        p.error('Limits and repetitions must be positive')
    for executable in ['codex','rg','rustc','node','javac','java','sandbox-exec']:
        if not shutil.which(executable):
            p.error(f'Missing pilot dependency: {executable}')
    args.output.mkdir(parents=True, exist_ok=False)
    output = args.output.resolve()
    harness = output / 'harness'
    harness.mkdir()
    for source in [*HERE.glob('*.py'), HERE/'tasks.json', HERE/'requirements.txt']:
        shutil.copy2(source, harness/source.name)
    shutil.copytree(HERE/'fixtures', harness/'fixtures', ignore=shutil.ignore_patterns('.DS_Store'))
    tasks = json.loads((HERE / 'tasks.json').read_text())['tasks']
    if args.task:
        unknown = set(args.task) - {t['id'] for t in tasks}
        if unknown:
            p.error(f'Unknown tasks: {sorted(unknown)}')
        tasks = [t for t in tasks if t['id'] in args.task]
    binaries = {}
    for name, path in [('baseline', args.baseline), ('candidate', args.candidate)]:
        copied = output / ('flexcontext-' + name)
        shutil.copy2(path, copied)
        binaries[name] = copied
    arms = args.arm or ['rg','baseline','candidate']
    jobs = [(task, arm, repeat) for task in tasks for arm in arms for repeat in range(args.repeats)]
    random.Random(1729).shuffle(jobs)
    contract = {'tasks_sha256': sha(HERE / 'tasks.json'), 'model': args.model, 'effort': args.effort,
                'seed': 1729, 'repeats': args.repeats, 'arms': arms,
                'timeout_seconds': args.timeout, 'source_token_limit': args.source_limit,
                'call_limit': args.call_limit, 'tokenizer': 'cl100k_base; repeated source and all public-test diagnostics charged',
                'binaries_sha256': {n: sha(b) for n,b in binaries.items()},
                'harness_sha256': {p.name: sha(p) for p in HERE.glob('*.py')},
                'codex_version': subprocess.check_output(['codex','--version'],text=True).strip(),
                'scope': 'Small source-derived/synthetic seeded bug pilot. Not a production task-success benchmark.'}
    dump(output / 'contract.json', contract)
    rows = []
    for task, arm, repeat in jobs:
        trial = output / f"{task['id']}-{arm}-{repeat}"
        trial.mkdir()
        workspace = trial / 'workspace'
        shutil.copytree(HERE / task['fixture'], workspace, ignore=shutil.ignore_patterns('.DS_Store'))
        files = {str(p.relative_to(workspace)): sha(p) for p in workspace.rglob('*') if p.is_file()}
        assert files == task['files_sha256'], f"Fixture changed: {task['id']}"
        spec, audit = trial / 'task.json', trial / 'broker.jsonl'
        dump(spec, task)
        broker_args = [str(harness / 'broker.py'), '--root', str(workspace), '--task', str(spec), '--arm', arm,
                       '--audit', str(audit), '--source-limit', str(args.source_limit), '--call-limit', str(args.call_limit)]
        if arm != 'rg':
            broker_args += ['--binary', str(binaries[arm])]
        config = {
            'approval_policy': 'never', 'web_search': 'disabled',
            'model_reasoning_effort': args.effort,
            'features.shell_tool': False, 'features.multi_agent': False,
            'features.plugins': False, 'features.skill_search': False,
            'features.skip_host_skill_discovery': True,
            'suppress_unstable_features_warning': True,
            'mcp_servers.retrieval_eval.command': sys.executable,
            'mcp_servers.retrieval_eval.args': broker_args,
        }
        command = ['codex','exec','--ignore-user-config','--ignore-rules','--strict-config',
                   '--skip-git-repo-check','--ephemeral','--json','--sandbox','read-only',
                   '--model',args.model,'-C',str(workspace)]
        for key, value in config.items():
            command += ['-c', key + '=' + json.dumps(value)]
        provider = 'ripgrep' if arm == 'rg' else 'Flexcontext'
        prompt = (task['prompt'] + f"\n\nThis is a bounded coding task. Use only retrieval_eval MCP tools. "
                  f"Your search provider is {provider}; {args.source_limit} cumulative source/diagnostic tokens and "
                  f"{args.call_limit} tool calls are available. Start with search, then inspect and edit what is needed. "
                  "Do not use shell, other tools, external resources or agents. Run the public tests and report the outcome.")
        (trial / 'prompt.txt').write_text(prompt)
        dump(trial / 'invocation.json', {'command': command, 'prompt_sha256': hashlib.sha256(prompt.encode()).hexdigest()})
        if args.dry_run:
            rows.append({'task': task['id'], 'arm': arm, 'repeat': repeat, 'status': 'prepared'})
            continue
        started = time.perf_counter()
        timed_out = False
        with (trial/'events.jsonl').open('w') as stdout, (trial/'stderr.txt').open('w') as stderr:
            process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=stdout, stderr=stderr,
                                       text=True, start_new_session=True)
            try:
                process.communicate(prompt, timeout=args.timeout)
            except subprocess.TimeoutExpired:
                timed_out = True
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
        elapsed = time.perf_counter()-started
        try:
            events = [json.loads(line) for line in (trial/'events.jsonl').read_text().splitlines()]
            calls = [json.loads(line) for line in audit.read_text().splitlines()] if audit.exists() else []
            trace = audit_trace(events, calls)
        except json.JSONDecodeError:
            calls = []
            trace = {'valid': False, 'reason': 'malformed JSON trace'}
        unchanged = all((workspace/path).is_file() and sha(workspace/path)==digest
                        for path,digest in files.items() if path not in task['editable'])
        grading_started = time.perf_counter()
        checks = grade(task, workspace)
        public_checks = grade_public(task, workspace)
        grading_seconds = time.perf_counter()-grading_started
        dump(trial/'acceptance.json', checks)
        dump(trial/'public-regressions.json', public_checks)
        budget_compliant = len(calls) <= args.call_limit and sum(c['source_tokens'] for c in calls) <= args.source_limit
        valid = trace['valid'] and unchanged and budget_compliant and not timed_out and process.returncode == 0
        row = {'task': task['id'], 'arm': arm, 'repeat': repeat, 'valid': valid,
               'task_success': valid and checks['passed'] and public_checks['passed'],
               'acceptance_passed': checks['passed'], 'public_regressions_passed': public_checks['passed'],
               'timeout': timed_out, 'exit_code': process.returncode, 'elapsed_seconds': elapsed,
               'grading_seconds': grading_seconds, 'budget_compliant': budget_compliant,
               'protected_files_unchanged': unchanged, 'trace': trace, 'tool_calls': len(calls),
               'source_tokens_delivered': sum(c['source_tokens'] for c in calls),
               'tool_output_tokens': sum(c['output_tokens'] for c in calls),
               'searches_before_first_edit': next((sum(c['tool']=='search' for c in calls[:i]) for i,c in enumerate(calls) if c['tool']=='replace_text' and not c['error']), None),
               'retrieval_ms': sum(c['elapsed_ms'] for c in calls if c['tool']=='search')}
        rows.append(row)
        dump(trial/'result.json', row)
        print(json.dumps(row), flush=True)
        dump(output/'results.json', {'contract': contract, 'rows': rows})
    dump(output/'results.json', {'contract': contract, 'rows': rows})


if __name__ == '__main__':
    main()
