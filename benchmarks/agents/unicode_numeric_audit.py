#!/usr/bin/env python3
"""Post-hoc Unicode numeric audit; never replaces the frozen task grader.

Authored after review found all initial lexical repairs used is_ascii_digit.
The task says letter/number boundaries and the original implementation handles
non-ASCII alphanumeric digits. Keep frozen acceptance and this audit separate.
"""
import argparse
import hashlib
import json
import tempfile
from pathlib import Path

from isolation import run as run_isolated


CHECKS = r'''
#[cfg(test)] mod posthoc_unicode_numeric {
    use super::identifier_tokens;
    #[test] fn non_ascii_decimal_boundaries() {
        for (input, expected) in [
            ("user٢Token", vec!["user", "٢", "token"]),
            ("２FAEnabled", vec!["２", "fa", "enabled"]),
            ("élève२Über", vec!["élève", "२", "über"]),
            ("version٣", vec!["version", "٣"]),
            ("a12٢٣B", vec!["a", "12٢٣", "b"]),
        ] { assert_eq!(identifier_tokens(input), expected, "{input}"); }
    }
}
'''


def check(source):
    with tempfile.TemporaryDirectory(prefix='flexcontext-unicode-audit-') as temporary:
        root = Path(temporary)
        (root / 'lexical.rs').write_text(source.read_text() + CHECKS)
        logs = []
        for command in [['rustc', '--edition=2024', '--test', 'lexical.rs', '-o', 'audit'], ['./audit']]:
            result = run_isolated(command, root)
            logs.append({'command': command, 'exit_code': result.returncode,
                         'stdout': result.stdout, 'stderr': result.stderr})
            if result.returncode:
                return {'passed': False, 'logs': logs}
        return {'passed': True, 'logs': logs}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('run', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error('Output must be new')
    project = Path(__file__).resolve().parents[2]
    reference = check(project / 'src/lexical.rs')
    tasks = json.loads((Path(__file__).parent / 'tasks.json').read_text())['tasks']
    task = next(t for t in tasks if t['id'] == 'lexical-digit-boundary')
    seeded = check(Path(__file__).parent / task['fixture'] / 'src/lexical.rs')
    assert reference['passed'] and not seeded['passed'], 'Invalid independent audit controls'
    data = json.loads((args.run / 'results.json').read_text())
    results = []
    for row in data['rows']:
        if row['task'] != 'lexical-digit-boundary':
            continue
        trial = Path(row['trial_directory']) if row.get('trial_directory') else args.run / f"{row['task']}-{row['arm']}-{row['repeat']}"
        source = trial / 'workspace/src/lexical.rs'
        result = check(source)
        results.append({'task': row['task'], 'arm': row['arm'], 'repeat': row['repeat'],
                        'frozen_task_success': row['task_success'],
                        'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(), **result})
    args.output.write_text(json.dumps({
        'classification': 'Post-hoc diagnostic written after observing ASCII-only repairs; not a predeclared held-out gate.',
        'script_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        'checks_sha256': hashlib.sha256(CHECKS.encode()).hexdigest(),
        'reference_passed': reference['passed'], 'seeded_state_passed': seeded['passed'],
        'rows': results,
    }, indent=2) + '\n')
    print(json.dumps({'audited': len(results), 'passed': sum(r['passed'] for r in results)}))


if __name__ == '__main__':
    main()
