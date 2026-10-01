#!/usr/bin/env python3
"""Compare actual modern MCP framing with the same payload duplicated into text."""
import argparse
import copy
import hashlib
import json
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('root')
parser.add_argument('--query', default='parse HTTP request')
parser.add_argument('--max-tokens', type=int, default=2048)
parser.add_argument('--binary', default='./target/release/flexcontext')
parser.add_argument('--output')
args = parser.parse_args()
request = {'jsonrpc': '2.0', 'id': 'cost-sample', 'method': 'tools/call', 'params': {
    '_meta': {'io.modelcontextprotocol/protocolVersion': '2026-07-28', 'io.modelcontextprotocol/clientCapabilities': {}},
    'name': 'code_search', 'arguments': {'query': args.query, 'max_tokens': args.max_tokens}}}
process = subprocess.run([args.binary, 'serve', args.root, '--no-cache'], input=json.dumps(request)+'\n', text=True, capture_output=True, check=True, timeout=60)
response = json.loads(process.stdout)
if 'error' in response or response['result'].get('isError'):
    raise RuntimeError(response)
actual = len(process.stdout.encode())
cost = response['result']['structuredContent']['context_cost']
assert actual == cost['serialized_bytes']
assert actual <= args.max_tokens * 4
duplicate = copy.deepcopy(response)
duplicate['result']['content'][0]['text'] = json.dumps(response['result']['structuredContent'], ensure_ascii=False, separators=(',', ':'))
duplicate_bytes = len((json.dumps(duplicate, ensure_ascii=False, separators=(',', ':'))+'\n').encode())
report = {'protocol': '2026-07-28', 'query': args.query, 'actual_mcp_bytes': actual,
          'same_payload_with_duplicate_text_bytes': duplicate_bytes, 'bytes_removed_by_single_payload': duplicate_bytes-actual,
          'reduction_fraction': (duplicate_bytes-actual)/duplicate_bytes, 'context_cost': cost,
          'binary_sha256': hashlib.sha256(Path(args.binary).read_bytes()).hexdigest(),
          'method': 'Actual modern stdio response compared to the identical payload with serialized structuredContent substituted into content[0].text. This isolates duplication; it is not an old binary measurement.'}
text = json.dumps(report, indent=2)+'\n'
if args.output:
    Path(args.output).write_text(text)
else:
    print(text, end='')
