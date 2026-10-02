#!/usr/bin/env python3
"""A restricted MCP toolbox with cumulative context accounting for agent pilots."""
import argparse
import hashlib
import json
import subprocess
import sys
import time
from pathlib import Path

import tiktoken
from isolation import run as run_isolated


TOOLS = [
    ('list_files', 'List the available task files.', {}),
    ('search', 'Discover relevant code using a natural-language query or code identifiers. Returns matching source and locations.',
     {'query': {'type': 'string'}}),
    ('read_file', 'Read a file or inclusive line range. Every delivered source token counts again on repeated reads.',
     {'path': {'type': 'string'}, 'start_line': {'type': 'integer', 'minimum': 1}, 'end_line': {'type': 'integer', 'minimum': 1}}),
    ('replace_text', 'Edit an authorized source file by replacing one uniquely matching exact substring. Include sufficient context to make the old text unique.',
     {'path': {'type': 'string'}, 'old': {'type': 'string'}, 'new': {'type': 'string'}}),
    ('run_public_tests', 'Run the fixed public test commands. Independent acceptance checks run after the session.', {}),
]


class Broker:
    def __init__(self, root, task, arm, binary, audit, source_limit=8000, call_limit=40):
        self.root = root.resolve()
        self.task, self.arm, self.binary, self.audit = task, arm, binary, audit
        self.encoder = tiktoken.get_encoding('cl100k_base')
        self.source_limit, self.call_limit = source_limit, call_limit
        self.source_tokens = self.output_tokens = self.calls = 0
        self.files = task['files_sha256']

    def path(self, value):
        if value not in self.files:
            raise ValueError('Path is outside the task file inventory')
        path = (self.root / value).resolve()
        if not path.is_relative_to(self.root) or not path.is_file():
            raise ValueError('Missing or escaped task path')
        return path

    def call(self, name, args):
        started = time.perf_counter()
        self.calls += 1
        source = ''
        error = False
        try:
            if self.calls > self.call_limit:
                raise ValueError('Tool call limit exhausted; finish the task')
            if name == 'list_files':
                value = {'files': sorted(self.files)}
            elif name == 'read_file':
                lines = self.path(args['path']).read_text().splitlines(keepends=True)
                first, last = args.get('start_line', 1), args.get('end_line', len(lines))
                if not isinstance(first, int) or not isinstance(last, int) or first < 1 or last < first:
                    raise ValueError('Invalid inclusive line range')
                source = ''.join(lines[first-1:last])
                value = {'path': args['path'], 'start_line': first, 'content': source}
            elif name == 'search':
                query = args['query']
                if not isinstance(query, str) or not query.strip() or len(query) > 1000:
                    raise ValueError('Provide a nonempty query of at most 1000 characters')
                if self.arm == 'rg':
                    command = ['rg', '--json', '-i', '-F', '-C', '3', '--color', 'never']
                    for term in query.split():
                        command += ['-e', term]
                    command += ['--', *sorted(self.files)]
                    result = subprocess.run(command, cwd=self.root, capture_output=True, text=True, timeout=30)
                    if result.returncode not in [0, 1]:
                        raise ValueError('rg failed: ' + result.stderr[:500])
                    hits = [json.loads(line) for line in result.stdout.splitlines()]
                    matches = [event['data'] for event in hits if event['type'] in ['match', 'context']]
                    blocks = []
                    for hit in matches:
                        path, first, content = hit['path']['text'], hit['line_number'], hit['lines']['text']
                        last = first + len(content.splitlines()) - 1
                        if blocks and blocks[-1]['path'] == path and blocks[-1]['end_line'] + 1 == first:
                            blocks[-1]['content'] += content
                            blocks[-1]['end_line'] = last
                        else:
                            blocks.append({'path': path, 'start_line': first, 'end_line': last, 'content': content})
                    value = {'results': blocks}
                    source = '\n'.join(hit['content'] for hit in value['results'])
                elif self.arm == 'probe':
                    result = subprocess.run([str(self.binary), 'search', ' OR '.join(query.split()),
                        str(self.root), '--format', 'json', '--allow-tests',
                        '--max-bytes', '8192', '--max-results', '12'],
                        capture_output=True, text=True, timeout=30)
                    if result.returncode != 0:
                        raise ValueError('Probe failed: ' + result.stderr[:500])
                    hits = json.loads(result.stdout)['results']
                    # Normalize locations and omit ranking metadata from the agent context.
                    value = {'results': [{'path': str(Path(hit['file']).relative_to(self.root))
                                          if Path(hit['file']).is_absolute() else hit['file'],
                                          'start_line': hit['lines'][0], 'end_line': hit['lines'][1],
                                          'content': hit['code']} for hit in hits]}
                    source = '\n'.join(hit['content'] for hit in value['results'])
                else:
                    result = subprocess.run([str(self.binary), 'search', str(self.root), query,
                        '--json', '--detail', 'compact', '--max-bytes', '8192', '--max-results', '12'],
                        capture_output=True, text=True, timeout=30)
                    if result.returncode != 0:
                        raise ValueError('Flexcontext failed: ' + result.stderr[:500])
                    value = json.loads(result.stdout)
                    source = '\n'.join(r['content'] for r in value['results'])
            elif name == 'replace_text':
                if args['path'] not in self.task['editable']:
                    raise ValueError('This file is not editable')
                path = self.path(args['path'])
                old, new = args['old'], args['new']
                text = path.read_text()
                if not isinstance(old, str) or not old or text.count(old) != 1 or not isinstance(new, str):
                    raise ValueError('The old substring must match exactly once')
                if len(new.encode()) > 100_000:
                    raise ValueError('Replacement is too large')
                path.write_text(text.replace(old, new, 1))
                value = {'edited': args['path']}
            elif name == 'run_public_tests':
                logs = []
                for command in [self.task['public_command'], self.task.get('public_run')]:
                    if command:
                        result = run_isolated(command, self.root)
                        logs.append({'exit_code': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr})
                        if result.returncode != 0:
                            break
                value = {'passed': all(r['exit_code'] == 0 for r in logs), 'logs': logs}
                # Test diagnostics may contain source, including intentional
                # prints. Charge all test output to the same cumulative limit.
                source = json.dumps(logs, ensure_ascii=False)
            else:
                raise ValueError('Unknown tool')
            delivered = len(self.encoder.encode(source, disallowed_special=()))
            if self.source_tokens + delivered > self.source_limit:
                raise ValueError(f'Source budget exhausted or response too large; {self.source_limit-self.source_tokens} source tokens remain. Request a narrower read or search.')
            self.source_tokens += delivered
        except (ValueError, KeyError, OSError, RuntimeError, subprocess.TimeoutExpired) as exc:
            value = {'error': str(exc)}
            delivered = 0
            error = True
        payload = json.dumps(value, ensure_ascii=False)
        output_tokens = len(self.encoder.encode(payload, disallowed_special=()))
        self.output_tokens += output_tokens
        record = {'tool': name, 'arguments': args, 'error': error,
                  'payload_sha256': hashlib.sha256(payload.encode()).hexdigest(),
                  'source_tokens': delivered, 'cumulative_source_tokens': self.source_tokens,
                  'output_tokens': output_tokens, 'cumulative_output_tokens': self.output_tokens,
                  'elapsed_ms': (time.perf_counter()-started)*1000, 'call': self.calls}
        with self.audit.open('a') as stream:
            stream.write(json.dumps(record, ensure_ascii=False) + '\n')
        return {'content': [{'type': 'text', 'text': payload}], 'isError': error}


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--root', type=Path, required=True)
    p.add_argument('--task', type=Path, required=True)
    p.add_argument('--arm', choices=['rg', 'baseline', 'candidate', 'probe'], required=True)
    p.add_argument('--binary', type=Path)
    p.add_argument('--audit', type=Path, required=True)
    p.add_argument('--source-limit', type=int, default=8000)
    p.add_argument('--call-limit', type=int, default=40)
    args = p.parse_args()
    broker = Broker(args.root, json.loads(args.task.read_text()), args.arm, args.binary,
                    args.audit, args.source_limit, args.call_limit)
    required = {'read_file': ['path'], 'search': ['query'], 'replace_text': ['path','old','new']}
    for line in sys.stdin:
        request = json.loads(line)
        if 'id' not in request:
            continue
        method = request['method']
        if method == 'initialize':
            result = {'protocolVersion': request['params']['protocolVersion'], 'capabilities': {'tools': {}},
                      'serverInfo': {'name': 'retrieval-eval', 'version': '1'}}
        elif method == 'tools/list':
            result = {'tools': [{'name': name, 'description': desc,
                'inputSchema': {'type': 'object', 'properties': props, 'required': required.get(name, []), 'additionalProperties': False},
                'annotations': {'readOnlyHint': name in ['list_files','search','read_file'], 'destructiveHint': False, 'openWorldHint': False}}
                for name, desc, props in TOOLS]}
        elif method == 'tools/call':
            result = broker.call(request['params']['name'], request['params'].get('arguments', {}))
        elif method == 'ping':
            result = {}
        else:
            print(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'error': {'code': -32601, 'message': 'Unsupported method'}}), flush=True)
            continue
        print(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': result}), flush=True)


if __name__ == '__main__':
    main()
