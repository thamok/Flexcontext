"""Restrict task-code test execution on this macOS pilot host."""
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path


def run(command, root, timeout=30):
    if sys.platform != 'darwin' or not shutil.which('sandbox-exec'):
        raise RuntimeError('This pilot requires macOS sandbox-exec; add a container worker for other hosts')
    root = root.resolve()
    temporary = root / '.eval-tmp'
    temporary.mkdir(exist_ok=True)
    read_roots = {str(root), '/System', '/usr', '/bin', '/sbin', '/Library/Developer',
                  '/Library/Java/JavaVirtualMachines', '/Applications/Xcode.app',
                  str(Path.home() / '.rustup'), str(Path.home() / '.cargo/bin')}
    # Prefer installed Command Line Tools for these CLI-only fixtures. Pin
    # the subprocess runtime without changing global xcode-select; an
    # unrelated Xcode update or unaccepted GUI license must not move it.
    developer_root = Path('/Library/Developer/CommandLineTools')
    if not developer_root.is_dir():
        selected = subprocess.run(['/usr/bin/xcode-select', '-p'], capture_output=True, text=True)
        developer_root = Path(selected.stdout.strip()).resolve() if selected.returncode == 0 and selected.stdout.strip() else None
    if developer_root and developer_root.is_dir():
        read_roots.add(str(developer_root))
        if developer_root.name == 'Developer' and developer_root.parent.name == 'Contents':
            read_roots.add(str(developer_root.parent))
    for name in ['rustc', 'node', 'javac', 'java', 'cc']:
        binary = shutil.which(name)
        if binary:
            read_roots.add(str(Path(binary).resolve().parent.parent))
    profile = '\n'.join([
        '(version 1)', '(deny default)', '(allow process*)', '(allow sysctl-read)',
        '(allow mach-lookup)', '(allow file-read-metadata)',
        '(allow file-read* (literal "/"))',
        '(allow file-read* (literal "/private/etc/ssl/openssl.cnf"))',
        '(allow file-read* ' + ' '.join('(subpath ' + json.dumps(p) + ')' for p in sorted(read_roots)) + ')',
        '(allow file-write* (subpath ' + json.dumps(str(root)) + '))',
        '(allow file-read* file-write* (literal "/dev/null") (literal "/dev/urandom") (literal "/dev/random"))',
    ])
    # Do not pass account/API credentials or arbitrary caller environment to
    # agent-authored code. Compiler/runtime configuration remains explicit.
    env = {'PATH': os.environ['PATH'], 'TMPDIR': str(temporary),
           'RUSTUP_HOME': str(Path.home() / '.rustup'), 'LANG': 'en_US.UTF-8'}
    if developer_root and developer_root.is_dir():
        env['DEVELOPER_DIR'] = str(developer_root)
    result = subprocess.run(['sandbox-exec', '-p', profile, *command], cwd=root,
                            env=env, capture_output=True, text=True, timeout=timeout)
    return result
