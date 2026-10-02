#!/usr/bin/env python3
"""Frozen-source CodeGraph/Probe/Flexcontext quality comparison.

Copies manifest-listed source files, indexes the copies, retains all native
responses, and applies the existing comparison harness's evidence/token rules.
Single warm-index CLI samples are diagnostics, not latency benchmarks.
"""
import argparse
import collections
import json
import os
from pathlib import Path
import random
import shutil
import statistics
import subprocess
import time

import tiktoken
import run as h
from codegraph_adapter import normalize_context, normalize_explore


def invoke(argv, root, timeout):
    env = dict(os.environ, CODEGRAPH_TELEMETRY='0', DO_NOT_TRACK='1', CODEGRAPH_NO_DAEMON='1', CODEGRAPH_NO_DOWNLOAD='1')
    started = time.perf_counter()
    result = subprocess.run([str(a) for a in argv], cwd=root, env=env, text=True, capture_output=True, timeout=timeout)
    elapsed = (time.perf_counter() - started) * 1000
    if result.returncode:
        raise RuntimeError(f'{result.returncode}: {result.stderr[:2000]}')
    return result.stdout, result.stderr, elapsed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--frozen-run', type=Path, required=True)
    parser.add_argument('--cases', type=Path, default=h.HERE / 'optimization-cases.json')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--only-repos', nargs='+')
    parser.add_argument('--tools', nargs='+', choices=['codegraph-explore', 'codegraph-context', 'probe', 'flexcontext'], default=['codegraph-explore', 'codegraph-context', 'probe', 'flexcontext'])
    parser.add_argument('--budgets', type=int, nargs='+', default=[2048, 4096])
    parser.add_argument('--codegraph', type=Path, default=h.PROJECT / '.benchmark-tools/codegraph/node_modules/.bin/codegraph')
    parser.add_argument('--probe', type=Path, default=h.PROJECT / '.benchmark-tools/node_modules/@probelabs/probe/bin/probe')
    parser.add_argument('--flexcontext', type=Path, default=h.PROJECT / 'target/release/flexcontext')
    parser.add_argument('--timeout', type=int, default=300)
    parser.add_argument('--query-style', choices=['terms', 'question'], default='terms')
    args = parser.parse_args()
    for name in ['frozen_run', 'cases', 'output', 'codegraph', 'probe', 'flexcontext']:
        setattr(args, name, getattr(args, name).resolve())
    if any(b < 1 for b in args.budgets):
        parser.error('budgets must be positive')
    args.output.mkdir(parents=True, exist_ok=False)
    artifacts = args.output / 'artifacts'
    artifacts.mkdir()
    data = json.loads(args.cases.read_text())
    cases = data['cases'] if isinstance(data, dict) else data
    if args.only_repos:
        cases = [c for c in cases if c['repo'] in args.only_repos]
    roots, manifests = {}, {}
    for repo in sorted({c['repo'] for c in cases}):
        manifest = json.loads((args.frozen_run / f'{repo}.manifest.json').read_text())
        root = args.output / 'snapshots' / repo
        root.mkdir(parents=True)
        for relative, meta in manifest['files'].items():
            origin = args.frozen_run / 'snapshots' / repo / relative
            if h.sha(origin.read_bytes()) != meta['sha256']:
                raise ValueError(f'frozen source changed: {repo}/{relative}')
            dest = root / relative
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(origin, dest)
        roots[repo], manifests[repo] = root, manifest
        h.dump(args.output / f'{repo}.manifest.json', manifest)
    h.validate_cases(cases, roots)
    h.dump(args.output / 'cases.json', cases)
    contract = {'query_style': args.query_style, 'query_policy': 'same question or deterministic terms for every tool; Probe OR joins terms',
        'budgets': args.budgets, 'tools': args.tools, 'cases_sha256': h.sha(args.cases.read_bytes()),
        'source_manifests': {r: m['snapshot_sha256'] for r, m in manifests.items()},
        'versions': {}, 'binary_sha256': {}, 'indexing': {},
        'notes': ['Common source prefix cap and cl100k_base per nonblank line.',
            'CodeGraph native default max-files/max-nodes and native ordering; no label-aware packing.',
            'CodeGraph has no source-byte budget option: uncapped native response reused at both source budgets.',
            'Index build is measured separately. warm_index_cli has one sample per pair; CPU load uncontrolled.',
            'Queries are not agent-planned; CodeGraph graph navigation and follow-up tools are not exercised.']}
    archive = args.output / 'harness'
    archive.mkdir()
    contract['harness_sha256'] = {}
    for path in [Path(__file__), h.HERE / 'codegraph_adapter.py', h.HERE / 'run.py']:
        shutil.copy2(path, archive / path.name)
        contract['harness_sha256'][path.name] = h.sha(path.read_bytes())
    # npm's launcher is tiny; record the lock containing platform-bundle
    # integrity hashes as well, rather than implying the launcher hashes it all.
    lock = args.codegraph.parents[3] / 'package-lock.json'
    if lock.is_file():
        shutil.copy2(lock, args.output / 'codegraph-package-lock.json')
        contract['codegraph_package_lock_sha256'] = h.sha(lock.read_bytes())
    for tool, binary in [('codegraph', args.codegraph), ('probe', args.probe), ('flexcontext', args.flexcontext)]:
        if any(t.startswith(tool) for t in args.tools):
            contract['versions'][tool] = invoke([binary, '--version'], args.output, args.timeout)[0].strip()
            contract['binary_sha256'][tool] = h.sha(binary.read_bytes())
    h.dump(args.output / 'contract.json', contract)
    for repo, root in roots.items():
        if any(t.startswith('codegraph') for t in args.tools):
            raw, err, elapsed = invoke([args.codegraph, 'init', '--yes', root], root, args.timeout)
            (args.output / f'{repo}.index.stdout.txt').write_text(raw)
            (args.output / f'{repo}.index.stderr.txt').write_text(err)
            contract['indexing'][repo] = {'elapsed_ms': elapsed}
            status = invoke([args.codegraph, 'status', root], root, args.timeout)[0]
            (args.output / f'{repo}.status.txt').write_text(status)
            h.dump(args.output / 'contract.json', contract)
            print(f'Indexed {repo}: {elapsed:.0f} ms', flush=True)
    enc = tiktoken.get_encoding('cl100k_base')
    rows, cache = [], {}
    jobs = [(c, t, b) for c in cases for t in args.tools for b in args.budgets]
    random.Random(1729).shuffle(jobs)
    for case, tool, budget in jobs:
        root = roots[case['repo']]
        words = h.terms(case['question'])
        query = ' '.join(words) if args.query_style == 'terms' else case['question']
        row = {'case': case['id'], 'repo': case['repo'], 'split': case.get('split', 'pilot'),
            'tool': tool, 'budget': budget, 'mode': 'warm_index_cli', 'repeat': 0, 'status': 'ok'}
        stem = f"{case['id']}-{tool}-{budget}"
        try:
            key = (case['id'], tool)
            if tool.startswith('codegraph') and key in cache:
                raw, err, elapsed = cache[key]
            else:
                if tool == 'codegraph-explore':
                    argv = [args.codegraph, 'explore', query, '--path', root]
                elif tool == 'codegraph-context':
                    argv = [args.codegraph, 'context', query, '--path', root, '--format', 'json']
                elif tool == 'probe':
                    argv = [args.probe, 'search', ' OR '.join(words) if args.query_style == 'terms' else query, root, '--format', 'json', '--allow-tests', '--max-bytes', budget * 8, '--max-results', 100]
                else:
                    argv = [args.flexcontext, 'search', root, query, '--json', '--detail', 'compact', '--max-bytes', budget * 8, '--max-results', 100]
                raw, err, elapsed = invoke(argv, root, args.timeout)
                if tool.startswith('codegraph'):
                    cache[key] = raw, err, elapsed
            (artifacts / f'{stem}.raw.txt').write_text(raw)
            (artifacts / f'{stem}.stderr.txt').write_text(err)
            source = h.Source(root)
            records, pointers = normalize_explore(raw, source) if tool == 'codegraph-explore' else normalize_context(raw, source) if tool == 'codegraph-context' else h.normalize(tool, raw, source)
            selected, used = h.cap(records, budget, enc)
            h.dump(artifacts / f'{stem}.lines.json', selected)
            context = '\n'.join(f"{r['path']}:{r['line'] or '?'}: {r['text']}" for r in selected)
            (artifacts / f'{stem}.context.txt').write_text(context)
            row.update(h.score(case, selected))
            row.update(source_tokens=used, source_bytes=sum(len(r['text'].encode()) + 1 for r in selected),
                native_tokens=len(enc.encode(raw, disallowed_special=())), native_bytes=len(raw.encode()),
                context_tokens=len(enc.encode(context, disallowed_special=())), latency_ms=elapsed,
                unlocated_source_lines=sum(r['line'] is None for r in selected),
                context_artifact=f'artifacts/{stem}.context.txt')
        except Exception as error:
            row.update(status='error', error=str(error))
        rows.append(row)
        with (args.output / 'rows.jsonl').open('a') as f:
            f.write(json.dumps(row) + '\n')
        print(f"{stem}: {row.get('evidence_recall', row.get('error'))}", flush=True)
    groups = collections.defaultdict(list)
    for row in rows:
        groups[(row['repo'], row['split'], row['budget'], row['tool'])].append(row)
    summary = []
    for key, group in sorted(groups.items()):
        item = dict(zip(['repo', 'split', 'budget', 'tool'], key))
        ok = [r for r in group if r['status'] == 'ok']
        item.update(cases=len(group), errors=len(group) - len(ok))
        if ok:
            for metric in ['evidence_recall', 'evidence_precision', 'source_tokens', 'native_tokens', 'latency_ms']:
                item[metric] = statistics.mean(r[metric] for r in ok)
        summary.append(item)
    h.dump(args.output / 'summary.json', summary)
    # Ensure indexing and querying never changed any frozen source bytes.
    for repo, root in roots.items():
        for relative, meta in manifests[repo]['files'].items():
            if h.sha((root / relative).read_bytes()) != meta['sha256']:
                raise ValueError(f'source mutated during benchmark: {repo}/{relative}')
    return int(any(r['status'] != 'ok' for r in rows))


if __name__ == '__main__':
    raise SystemExit(main())
