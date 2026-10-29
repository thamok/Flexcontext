"""Accounting and source-evidence checks for the bounded progressive pilot."""
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from progressive_eval import Broker, CASES, LIMITS, schemas


class AccountingTests(unittest.TestCase):
    def setUp(self):
        self.case = json.loads(CASES.read_text())['cases'][0]
        self.broker = Broker(Path('/unused'), self.case, 'B')

    def test_navigation_is_charged_but_never_counts_as_source_evidence(self):
        value = {'results': [], 'navigation': {'leads': [dict(e) for e in self.case['required_evidence']]}}
        with patch('progressive_eval.subprocess.run', return_value=SimpleNamespace(returncode=0, stdout=json.dumps(value), stderr='')):
            payload = self.broker.call('search', {'query': 'session'})
        self.assertEqual(self.broker.charged, self.broker.tokens(payload))
        self.assertGreater(self.broker.records[0]['navigation_tokens'], 0)
        self.assertFalse(any(self.broker.coverage().values()))

    def test_repeated_reads_charge_all_output_and_record_source_repeats(self):
        first = self.broker.call('read_file', {'path': 'session.ts'})
        second = self.broker.call('read_file', {'path': 'session.ts'})
        self.assertEqual(self.broker.charged, self.broker.tokens(first)+self.broker.tokens(second))
        self.assertGreater(self.broker.records[1]['repeated_source_line_bytes'], 0)
        self.assertTrue(all(self.broker.coverage().values()))

    def test_budget_errors_are_charged_and_attempts_count(self):
        self.broker.charged = LIMITS['cumulative_payload_tokens']-100
        payload = self.broker.call('read_file', {'path': 'session.ts'})
        self.assertIn('error', json.loads(payload))
        self.assertEqual(self.broker.calls, 1)
        self.assertEqual(self.broker.charged, LIMITS['cumulative_payload_tokens']-100+self.broker.tokens(payload))
        self.assertFalse(any(self.broker.coverage().values()))

    def test_path_inventory_and_arm_tools(self):
        self.assertIn('error', json.loads(self.broker.call('read_file', {'path': '../../cases.json'})))
        self.assertEqual([t['function']['name'] for t in schemas('A')], ['search', 'read_file'])
        self.assertEqual([t['function']['name'] for t in schemas('B')], ['search', 'read_file', 'expand'])


if __name__ == '__main__':
    unittest.main()
