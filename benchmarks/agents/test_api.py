import json
import hashlib
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import Mock

from run_api import API_TOOLS, agent_loop, audit_api_trace, file_key


def response(calls=None, finish='stop'):
    return {'choices': [{'message': {'role': 'assistant', 'content': 'done',
                                     'tool_calls': calls or []}, 'finish_reason': finish}],
            'usage': {'prompt_tokens': 10, 'completion_tokens': 5, 'total_tokens': 15}}


CALL = {'type': 'function', 'id': 'call1',
        'function': {'name': 'search', 'arguments': '{"query":"queue"}'}}


class ApiTests(unittest.TestCase):
    def test_file_key_loads_only_named_value_without_shell_evaluation(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary)/'keys'
            path.write_text('export OTHER_KEY=ignored\nexport SELECTED="literal$(must-not-execute)"\n')
            self.assertEqual(file_key(path, 'SELECTED'), 'literal$(must-not-execute)')
            with self.assertRaises(ValueError):
                file_key(path, 'MISSING')

    def test_audit_requires_completion_correspondence_and_allowed_tools(self):
        events = [{'type': 'response', 'response': response([CALL], 'tool_calls')},
                  {'type': 'tool', 'id': 'call1', 'name': 'search',
                   'result': {'content': [{'type': 'text', 'text': '{}'}], 'isError': False}},
                  {'type': 'response', 'response': response()},
                  {'type': 'finished', 'completed': True}]
        calls = [{'tool': 'search', 'arguments': {'query': 'queue'},
                  'payload_sha256': hashlib.sha256(b'{}').hexdigest(), 'error': False}]
        trace = audit_api_trace(events, calls)
        self.assertTrue(trace['valid'])
        self.assertEqual(trace['usage_totals']['total_tokens'], 30)
        self.assertFalse(audit_api_trace(events[:-1], calls)['valid'])
        self.assertFalse(audit_api_trace(events, [])['valid'])
        wrong = [{'tool': 'search', 'arguments': {'query': 'other'}}]
        self.assertFalse(audit_api_trace(events, wrong)['valid'])
        malformed = [{'type': 'response', 'response': {'choices': []}}]
        self.assertFalse(audit_api_trace(malformed, [])['valid'])
        calls[0]['payload_sha256'] = 'tampered'
        self.assertFalse(audit_api_trace(events, calls)['valid'])

    def test_partial_trace_retains_receipts(self):
        events = [{'type': 'response', 'response': response()}]
        trace = audit_api_trace(events, [])
        self.assertFalse(trace['valid'])
        self.assertEqual(trace['usage_totals']['total_tokens'], 15)

    def test_absent_usage_is_unknown_not_zero(self):
        value = response()
        value.pop('usage')
        trace = audit_api_trace([{'type': 'response', 'response': value}], [])
        self.assertTrue(trace['usage_unknown'])
        self.assertIsNone(trace['usage_totals']['total_tokens'])

    def test_only_broker_functions_exposed(self):
        names = {t['function']['name'] for t in API_TOOLS}
        self.assertEqual(names, {'list_files', 'search', 'read_file', 'replace_text', 'run_public_tests'})
        self.assertTrue(all(not t['function']['parameters']['additionalProperties'] for t in API_TOOLS))

    def test_unfinished_or_surplus_calls_never_execute(self):
        with tempfile.TemporaryDirectory() as temp:
            broker = Mock(calls=0, call_limit=1)
            client = Mock()
            client.complete.return_value = response([CALL, CALL], 'tool_calls')
            finished = agent_loop(client, broker, 'task', Path(temp)/'events.jsonl', time.monotonic()+10, 10)
            self.assertFalse(finished)
            broker.call.assert_not_called()
            client.complete.return_value = response(finish='length')
            self.assertFalse(agent_loop(client, broker, 'task', Path(temp)/'events.jsonl', time.monotonic()+10, 10))


if __name__ == '__main__':
    unittest.main()
