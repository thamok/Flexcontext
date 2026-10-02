import json
import tempfile
import subprocess
import sys
import unittest
from pathlib import Path

import tiktoken

from run import Source, cap, normalize, score, terms, validate_cases, sha


class HarnessTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        (self.root / "a.py").write_text("def login():\n    check_token()\n    return user\n")
        self.source = Source(self.root)
        self.case = {"id": "one", "repo": "r", "question": "How does login check a token?",
                     "evidence": [{"id": "guard", "spans": [{"path": "a.py", "start": 2, "end": 2,
                         "sha256": sha(b"    check_token()\n")}]}]}

    def tearDown(self):
        self.temp.cleanup()

    def test_signature_is_not_body_evidence(self):
        raw = json.dumps({"results": [{"path": "a.py", "start_line": 1, "end_line": 3,
                                      "content": "def login():\n[… omitted …]"}]})
        records, pointers = normalize("flexcontext", raw, self.source)
        self.assertEqual(pointers, ["a.py"])
        self.assertEqual(score(self.case, records)["evidence_recall"], 0)

    def test_map_pointer_is_not_source_evidence(self):
        records, _ = normalize("aider", json.dumps({"map": "a.py\n"}), self.source)
        self.assertEqual(records, [])
        self.assertEqual(score(self.case, records)["evidence_recall"], 0)

    def test_truncated_map_line_does_not_receive_full_line_credit(self):
        records, _ = normalize("aider", json.dumps({"map": "a.py:\n│    check_to\n"}), self.source)
        self.assertIsNone(records[0]["line"])
        self.assertEqual(score(self.case, records)["evidence_recall"], 0)

    def test_rg_match_and_context_are_both_actual_source(self):
        raw = "\n".join(json.dumps({"type": kind, "data": {"path": {"text": "./a.py"},
                        "line_number": n, "lines": {"text": text}}})
                        for kind, n, text in [("context", 1, "def login():\n"),
                                               ("match", 2, "    check_token()\n")])
        records, _ = normalize("rg", raw, self.source)
        result = score(self.case, records)
        self.assertEqual(result["evidence_recall"], 1)
        self.assertEqual(result["gold_line_precision"], .5)

    def test_probe_body_alignment(self):
        raw = json.dumps({"results": [{"file": str(self.root / "a.py"), "lines": [1, 3],
                                      "code": (self.root / "a.py").read_text()}]})
        records, _ = normalize("probe", raw, self.source)
        self.assertEqual(score(self.case, records)["evidence_recall"], 1)

    def test_budget_is_tokenized_and_deduplicated(self):
        enc = tiktoken.get_encoding("cl100k_base")
        records = self.source.excerpt("a.py", (self.root / "a.py").read_text())
        cost = len(enc.encode(records[0]["text"] + "\n"))
        selected, used = cap([records[0], *records], cost, enc)
        self.assertEqual(selected, [records[0]])
        self.assertEqual(used, cost)
        self.assertEqual(score(self.case, selected)["evidence_recall"], 0)

    def test_multi_span_atom_requires_every_span(self):
        self.case["evidence"][0]["spans"].append({"path": "a.py", "start": 3, "end": 3})
        result = score(self.case, self.source.excerpt("a.py", "    check_token()"))
        self.assertEqual(result["evidence_recall"], 0)
        self.assertEqual(result["gold_line_recall"], .5)

    def test_drift_fails_closed(self):
        validate_cases([self.case], {"r": self.root})
        (self.root / "a.py").write_text("def login():\n    skip_token()\n    return user\n")
        with self.assertRaisesRegex(ValueError, "ground truth drift"):
            validate_cases([self.case], {"r": self.root})

    def test_query_has_no_hidden_expansion(self):
        self.assertEqual(terms("How does the cache reject expired tokens?"), ["cache", "reject", "expired", "tokens"])

    def test_prompt_export_withholds_labels_and_adapter_identity(self):
        (self.root / "cases.json").write_text(json.dumps([self.case]))
        (self.root / "context.txt").write_text("a.py:2: check_token()\n")
        row = {"case": "one", "tool": "probe", "budget": 2048, "source_tokens": 4,
               "status": "ok", "mode": "warm_cli", "repeat": 1, "context_artifact": "context.txt"}
        (self.root / "rows.jsonl").write_text(json.dumps(row) + "\n")
        script = Path(__file__).with_name("export_prompts.py")
        subprocess.run([sys.executable, script, self.root], check=True, capture_output=True)
        prompt = (self.root / "answer-prompts/0000.txt").read_text()
        self.assertIn(self.case["question"], prompt)
        self.assertIn("a.py:2: check_token()", prompt)
        self.assertNotIn("probe", prompt)
        self.assertNotIn("sha256", prompt)


if __name__ == "__main__":
    unittest.main()
