"""Independent acceptance checks, materialized only after the agent exits."""
import json
import hashlib
import shutil
import subprocess
import tempfile
from pathlib import Path
from isolation import run as run_isolated

LEXICAL_CHECKS = r'''
#[cfg(test)] mod acceptance {
    use super::identifier_tokens;
    #[test] fn numeric_transitions() {
        for (input, expected) in [
            ("user42Token", vec!["user", "42", "token"]),
            ("2FAEnabled", vec!["2", "fa", "enabled"]),
            ("HTTP2Server", vec!["http", "2", "server"]),
            ("version123", vec!["version", "123"]),
        ] { assert_eq!(identifier_tokens(input), expected); }
    }
    #[test] fn existing_boundaries() {
        assert_eq!(identifier_tokens("XMLParser src/auth-token.rs"),
                   ["xml", "parser", "src", "auth", "token", "rs"]);
        assert_eq!(identifier_tokens("élève2Über"), ["élève", "2", "über"]);
        assert_eq!(identifier_tokens(""), Vec::<String>::new());
    }
}
'''

LIMITER_CHECKS = r'''
import assert from 'node:assert/strict';
import test from 'node:test';
import { createConcurrencyLimiter, withTimeout } from './src/utils/promise.ts';
test('queued work is FIFO at capacity one and two', async () => {
  for (const capacity of [1, 2]) {
    const limit = createConcurrencyLimiter(capacity);
    const starts = [];
    let active = 0, peak = 0;
    await Promise.all(Array.from({ length: 9 }, (_, i) => limit(async () => {
      starts.push(i); active++; peak = Math.max(peak, active);
      await new Promise(resolve => setTimeout(resolve, 2)); active--;
    })));
    assert.deepEqual(starts, [0,1,2,3,4,5,6,7,8]);
    assert.equal(peak, capacity);
  }
});
test('failure and synchronous throw both release capacity', async () => {
  const limit = createConcurrencyLimiter(1);
  const tasks = [limit(() => { throw new Error('sync'); }),
                 limit(async () => { throw new Error('async'); }),
                 limit(async () => 23)];
  const result = await withTimeout(Promise.allSettled(tasks), 1000);
  assert.deepEqual(result.map(r => r.status), ['rejected','rejected','fulfilled']);
  assert.equal(result[2].value, 23);
});
test('invalid capacity still throws', () => {
  for (const n of [0, -1, 1.5, NaN, Infinity]) {
    assert.throws(() => createConcurrencyLimiter(n));
  }
});
'''

JAVA_CHECKS = r'''
import example.paths.PathPolicy;
public class Acceptance {
    public static void main(String[] args) {
        PathPolicy p = new PathPolicy() { public String pathHeading() { return "Paths"; } };
        for (String prefix : new String[] {"src", "src/main", "α"}) {
            if (!p.matchesDirectoryPrefix(prefix, prefix)) throw new AssertionError("self");
            if (!p.matchesDirectoryPrefix(prefix + "/child", prefix)) throw new AssertionError("child");
            for (String suffix : new String[] {"2", "-old", "_copy", ".bak"}) {
                if (p.matchesDirectoryPrefix(prefix + suffix, prefix)) throw new AssertionError("sibling");
            }
        }
        for (String path : new String[] {"/private", "../secret", "src/../secret"}) {
            if (p.allowsRelativePath(path)) throw new AssertionError("unsafe");
        }
        if (!p.allowsRelativePath("src/main")) throw new AssertionError("safe");
    }
}
'''


def execute(command, root):
    try:
        result = run_isolated(command, root)
        return {'command': command, 'exit_code': result.returncode,
                'stdout': result.stdout, 'stderr': result.stderr}
    except subprocess.TimeoutExpired:
        return {'command': command, 'exit_code': None, 'timeout': True}


def grade_public(task, candidate):
    """Re-run frozen public regressions on final source in a separate sandbox."""
    fixture = Path(__file__).parent / task['fixture']
    with tempfile.TemporaryDirectory(prefix='flexcontext-public-grader-') as temporary:
        root = Path(temporary)
        for path, digest in task['files_sha256'].items():
            source = (candidate / path).resolve() if path in task['editable'] else fixture / path
            if path in task['editable'] and not source.is_relative_to(candidate.resolve()):
                return {'passed': False, 'reason': 'escaped editable source'}
            if not source.is_file():
                return {'passed': False, 'reason': 'missing source or public fixture'}
            if path not in task['editable'] and hashlib.sha256(source.read_bytes()).hexdigest() != digest:
                raise RuntimeError('Frozen public fixture changed: ' + path)
            target = root / path
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
        logs = []
        for command in [task['public_command'], task.get('public_run')]:
            if command:
                log = execute(command, root)
                logs.append(log)
                if log['exit_code'] != 0:
                    return {'passed': False, 'logs': logs}
        return {'passed': True, 'logs': logs}


def grade(task, candidate):
    # The trusted grader uses its own tests, command definitions and staging
    # directory. Agent-written tests never define acceptance.
    with tempfile.TemporaryDirectory(prefix='flexcontext-grader-') as temporary:
        root = Path(temporary)
        for path in task['editable']:
            source = (candidate / path).resolve()
            if not source.is_relative_to(candidate.resolve()) or not source.is_file():
                return {'passed': False, 'reason': 'missing or escaped editable source'}
            dest = root / path
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, dest)
        kind = task['grader']
        if kind == 'lexical':
            source = root / 'src/lexical.rs'
            source.write_text(source.read_text() + LEXICAL_CHECKS)
            commands = [['rustc', '--edition=2024', '--test', 'src/lexical.rs', '-o', 'acceptance'], ['./acceptance']]
        elif kind == 'limiter':
            (root / 'package.json').write_text('{"type":"module"}\n')
            (root / 'acceptance.test.ts').write_text(LIMITER_CHECKS)
            commands = [['node', '--test', 'acceptance.test.ts']]
        elif kind == 'java':
            (root / 'Acceptance.java').write_text(JAVA_CHECKS)
            commands = [['javac', '-d', 'classes', 'src/example/paths/PathPolicy.java', 'Acceptance.java'],
                        ['java', '-cp', 'classes', 'Acceptance']]
        else:
            raise ValueError(f'Unknown grader: {kind}')
        logs = []
        for command in commands:
            log = execute(command, root)
            logs.append(log)
            if log['exit_code'] != 0:
                return {'passed': False, 'logs': logs}
        return {'passed': True, 'logs': logs}


if __name__ == '__main__':
    import argparse
    p = argparse.ArgumentParser()
    p.add_argument('task_id')
    p.add_argument('candidate', type=Path)
    args = p.parse_args()
    tasks = json.loads((Path(__file__).parent / 'tasks.json').read_text())['tasks']
    print(json.dumps(grade(next(t for t in tasks if t['id'] == args.task_id), args.candidate), indent=2))
