import json
import tempfile
import unittest
from pathlib import Path

from codegraph_adapter import normalize_context, normalize_explore
from run import Source


class CodeGraphTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        (self.root / 'auth.py').write_text('def login():\n    check_token()\n    return user\n')
        self.source = Source(self.root)

    def tearDown(self):
        self.temp.cleanup()

    def test_explore_attributes_only_visible_verified_numbered_source(self):
        raw = '**`auth.py`** — login\n\n```python\n1\tdef login():\n2\t    check_token()\n... omitted ...\n```\n3\t    return user\n'
        records, pointers = normalize_explore(raw, self.source)
        self.assertEqual(pointers, ['auth.py'])
        self.assertEqual([r['line'] for r in records], [1, 2])

    def test_explore_mismatch_does_not_earn_evidence(self):
        records, _ = normalize_explore('**`auth.py`**\n```python\n2\t    check_to\n99\t    return user\n```', self.source)
        self.assertEqual([r['line'] for r in records], [None, None])

    def test_explore_pointer_is_not_body(self):
        records, _ = normalize_explore('**`auth.py`** — login\n', self.source)
        self.assertEqual(records, [])

    def test_context_never_expands_node_pointer(self):
        records, pointers = normalize_context(json.dumps({'relatedFiles': ['auth.py'], 'nodes': [{'filePath': 'auth.py', 'startLine': 1, 'endLine': 3}]}), self.source)
        self.assertEqual(records, [])
        self.assertEqual(pointers, ['auth.py'])

    def test_context_keeps_truncated_line_unlocated(self):
        records, _ = normalize_context(json.dumps({'codeBlocks': [{'filePath': 'auth.py', 'startLine': 1, 'endLine': 3, 'content': 'def login():\n    check_to\n... (truncated) ...'}]}), self.source)
        self.assertEqual([r['line'] for r in records], [1, None, None])

    def test_explore_rejects_outside_paths(self):
        with self.assertRaisesRegex(ValueError, 'outside snapshot'):
            normalize_explore('**`../outside.py`**', self.source)


if __name__ == '__main__':
    unittest.main()
