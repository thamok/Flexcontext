"""Dummy-only check of startup-environment versus in-memory credentials.

This demonstrates a narrow credential-handling property, not hostile-code
containment: this macOS sandbox can still read another same-UID process's
startup environment through KERN_PROCARGS2. Never target a real API worker.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import uuid

from isolation import run as run_isolated


PROBE = r'''
#include <sys/types.h>
#include <sys/sysctl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
int main(int argc, char **argv) {
    int mib[3] = {CTL_KERN, KERN_PROCARGS2, atoi(argv[1])};
    size_t size = 1024 * 1024;
    char *data = calloc(1, size + 1);
    int result = sysctl(mib, 3, data, &size, NULL, 0);
    int saved_errno = errno;
    size_t length = strlen(argv[2]);
    int found = 0;
    if (result == 0) {
        for (size_t i = 0; i + length <= size; ++i) {
            if (!memcmp(data + i, argv[2], length)) { found = 1; break; }
        }
    }
    printf("{\"return_code\":%d,\"errno\":%d,\"sentinel_visible\":%s}\n",
           result, saved_errno, found ? "true" : "false");
    free(data);
    return 0;
}
'''

# The sentinel value is not embedded in argv or the script. Each child reads
# only the dummy file after exec and retains its value in Python memory.
WORKER = ('import pathlib,sys,time; '
          'secret=pathlib.Path(sys.argv[1]).read_text(); '
          'print(bool(secret),flush=True); time.sleep(60)')


@unittest.skipUnless(sys.platform == 'darwin' and shutil.which('sandbox-exec')
                     and shutil.which('clang'), 'macOS sandbox and clang required')
class ProcessCredentialTests(unittest.TestCase):
    def test_clean_initial_environment_keeps_memory_only_sentinel_out_of_procargs(self):
        with tempfile.TemporaryDirectory(prefix='dummy-process-privacy-') as temporary:
            base = Path(temporary)
            root = base / 'sandbox'
            root.mkdir()
            # Outside the task's permitted filesystem subtree.
            secret_file = base / 'dummy-secret.txt'
            sentinel = 'dummy_only_' + uuid.uuid4().hex
            secret_file.write_text(sentinel)
            (root / 'probe.c').write_text(PROBE)
            subprocess.run(['clang', str(root / 'probe.c'), '-o', str(root / 'probe')],
                           check=True, capture_output=True, timeout=30)
            clean_env = {'PATH': os.environ['PATH']}
            children = []
            try:
                for environment in [dict(clean_env, DUMMY_CREDENTIAL=sentinel), clean_env]:
                    child = subprocess.Popen([sys.executable, '-c', WORKER, str(secret_file)],
                                             env=environment, stdout=subprocess.PIPE,
                                             stderr=subprocess.PIPE, text=True)
                    children.append(child)
                    self.assertEqual(child.stdout.readline().strip(), 'True')
                def probe(child, sandboxed):
                    command = [str(root / 'probe'), str(child.pid), sentinel]
                    result = (run_isolated(command, root) if sandboxed else
                              subprocess.run(command, check=True, capture_output=True,
                                             text=True, timeout=30))
                    self.assertEqual(result.returncode, 0)
                    return json.loads(result.stdout)
                control = probe(children[0], False)
                self.assertEqual(control['return_code'], 0)
                self.assertTrue(control['sentinel_visible'],
                                'Positive control must demonstrate startup-environment access')
                sandbox_control = probe(children[0], True)
                self.assertEqual(sandbox_control['return_code'], 0)
                self.assertTrue(sandbox_control['sentinel_visible'],
                                'Revisit the documented same-UID boundary if the OS now blocks this')
                for sandboxed in [False, True]:
                    memory_only = probe(children[1], sandboxed)
                    self.assertEqual(memory_only['return_code'], 0)
                    self.assertFalse(memory_only['sentinel_visible'])
            finally:
                for child in children:
                    child.terminate()
                    child.wait(timeout=5)
                    child.stdout.close()
                    child.stderr.close()


if __name__ == '__main__':
    unittest.main()
