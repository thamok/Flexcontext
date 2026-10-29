#!/usr/bin/env python3
"""Runnable search -> inspect leads -> exact fetch demonstration; no model calls."""
import argparse
import json
import pathlib
import subprocess
import tempfile

SOURCE = '''class Session {
  loginSession() { return issueToken(); }
  refreshSession() { return rotateToken(); }
  revokeSession() { return deleteToken(); }
  expireSession() { return clearExpired(); }
}
'''

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=pathlib.Path, default=pathlib.Path('target/release/flexcontext'))
    args = parser.parse_args()
    binary = str(args.binary.resolve())
    with tempfile.TemporaryDirectory(prefix='flex-progressive-demo-') as root:
        pathlib.Path(root, 'session.ts').write_text(SOURCE)
        def run(command, *extra):
            return json.loads(subprocess.check_output([binary, command, root, *extra, '--json', '--max-tokens', '3000']))
        before = run('search', 'Session')
        after = run('search', 'Session', '--continuations')
        bodies = lambda response: [r['symbol'] for r in response['results'] if r['kind'] == 'method']
        assert len(bodies(before)) == len(bodies(after)) == 2
        print('Before: two bodies:', bodies(before), '; navigation:', before.get('navigation'))
        print('After: two bodies:', bodies(after))
        omitted = [lead for lead in after['navigation']['leads'] if lead['kind'] == 'method']
        assert len(omitted) == 2
        for lead in omitted:
            print('Lead:', lead['symbol'], '|', lead['reason'])
            fetched = run('expand', lead['reference'])
            assert bodies(fetched) == [lead['symbol']]
            assert not fetched['results'][0]['content_truncated']
            print('Exact fetch:', fetched['results'][0]['content'])
        print('PASS: both omitted methods resolved, independently of their shared-container quota.')

if __name__ == '__main__':
    main()
