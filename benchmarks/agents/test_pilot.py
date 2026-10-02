import json
import shutil
import tempfile
import unittest
from pathlib import Path

from broker import Broker
from grade import grade, grade_public
from isolation import run as run_isolated
from run import audit_trace, sha

HERE = Path(__file__).resolve().parent
TASKS = json.loads((HERE/'tasks.json').read_text())['tasks']


class PilotTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='flexcontext-pilot-test-')
        self.root = Path(self.temp.name)
        self.task = TASKS[1]
        self.workspace = self.root/'workspace'
        shutil.copytree(HERE/self.task['fixture'], self.workspace)
        self.broker = Broker(self.workspace, self.task, 'rg', None, self.root/'audit.jsonl', 8000, 40)

    def tearDown(self):
        self.temp.cleanup()

    def test_fixture_hashes_are_frozen(self):
        for task in TASKS:
            root = HERE/task['fixture']
            self.assertEqual(task['files_sha256'],
                {str(p.relative_to(root)):sha(p) for p in root.rglob('*') if p.is_file() and p.name != '.DS_Store'})

    def test_repeated_reads_charge_and_exhaust_cumulative_budget(self):
        args = {'path':'src/utils/promise.ts','start_line':1,'end_line':10}
        self.assertFalse(self.broker.call('read_file', args)['isError'])
        first = self.broker.source_tokens
        self.assertFalse(self.broker.call('read_file', args)['isError'])
        self.assertEqual(self.broker.source_tokens, first*2)
        self.broker.source_limit = first*2
        self.assertTrue(self.broker.call('read_file', args)['isError'])
        self.assertEqual(self.broker.source_tokens, first*2)

    def test_traversal_symlinks_and_protected_edits_are_rejected(self):
        self.assertTrue(self.broker.call('read_file', {'path':'../task.json'})['isError'])
        self.assertTrue(self.broker.call('replace_text', {'path':'tests/public.test.ts','old':'test','new':'skip'})['isError'])
        path = self.workspace/'src/utils/promise.ts'
        path.unlink()
        path.symlink_to(HERE/'grade.py')
        self.assertTrue(self.broker.call('read_file', {'path':'src/utils/promise.ts'})['isError'])

    def test_unique_replacement_and_call_limit(self):
        args = {'path':'src/utils/promise.ts','old':'queue.pop()','new':'queue.shift()'}
        self.assertFalse(self.broker.call('replace_text', args)['isError'])
        self.assertTrue(self.broker.call('replace_text', args)['isError'])
        self.broker.call_limit = self.broker.calls
        self.assertTrue(self.broker.call('list_files', {})['isError'])

    def test_rg_provides_paths_and_charges_only_delivered_source(self):
        result = self.broker.call('search', {'query': 'queue'})
        payload = json.loads(result['content'][0]['text'])
        self.assertTrue(payload['results'])
        self.assertTrue(all(hit['path'] in self.task['files_sha256'] for hit in payload['results']))
        source = '\n'.join(hit['content'] for hit in payload['results'])
        self.assertEqual(self.broker.source_tokens, len(self.broker.encoder.encode(source)))
        lexical = TASKS[0]
        single = Broker(HERE/lexical['fixture'], lexical, 'rg', None, self.root/'single.jsonl')
        one = json.loads(single.call('search', {'query': 'identifier'})['content'][0]['text'])
        self.assertTrue(one['results'])
        self.assertTrue(all(hit['path'] == 'src/lexical.rs' for hit in one['results']))

    def test_trace_requires_completed_turn_and_only_broker_tools(self):
        events = [{'type':'item.completed','item':{'type':'mcp_tool_call','server':'retrieval_eval'}},
                  {'type':'turn.completed','usage':{'input_tokens':12}}]
        self.assertTrue(audit_trace(events, [{}])['valid'])
        self.assertFalse(audit_trace(events[:-1], [{}])['valid'])
        self.assertFalse(audit_trace(events, [])['valid'])
        self.assertFalse(audit_trace(events+[{'type':'item.completed','item':{'type':'command_execution'}}], [{}])['valid'])

    def test_task_code_cannot_read_host_grader_or_write_outside_workspace(self):
        target = self.root/'outside-secret'
        target.write_text('secret')
        result = run_isolated(['cat',str(target)],self.workspace)
        self.assertNotEqual(result.returncode,0)
        self.assertNotIn('secret',result.stdout)
        result = run_isolated(['touch',str(self.root/'outside-write')],self.workspace)
        self.assertNotEqual(result.returncode,0)
        self.assertFalse((self.root/'outside-write').exists())

    def test_seeded_tasks_fail_and_reference_repairs_pass(self):
        repairs = {
            'lexical': ('                || (previous.is_some_and(char::is_uppercase)',
                        '                || previous.is_some_and(|prev| prev.is_alphabetic() != ch.is_alphabetic())\n                || (previous.is_some_and(char::is_uppercase)'),
            'limiter': ('queue.pop()', 'queue.shift()'),
            'java': ('path.startsWith(prefix)', 'path.startsWith(prefix + "/")'),
        }
        for task in TASKS:
            candidate = self.root/task['id']
            shutil.copytree(HERE/task['fixture'], candidate)
            before = grade(task,candidate)
            self.assertFalse(before['passed'],task['id'])
            path = candidate/task['editable'][0]
            old,new = repairs[task['grader']]
            source = path.read_text()
            self.assertEqual(source.count(old),1)
            path.write_text(source.replace(old,new))
            after = grade(task,candidate)
            self.assertTrue(after['passed'], f"{task['id']}: {after}")
            public = grade_public(task, candidate)
            self.assertTrue(public['passed'], f"{task['id']}: {public}")

    def test_final_public_regression_catches_hidden_only_pass(self):
        source = self.workspace/'src/utils/promise.ts'
        source.write_text(source.read_text().replace('queue.pop()', 'queue.shift()')
                          .replace('return await Promise.race([promise, timeoutPromise]);',
                                   'return (await Promise.race([promise, timeoutPromise])) === 17 ? undefined : await promise;'))
        # The successful timeout wrapper value is an existing public contract.
        # Make an explicit regression which the hidden limiter checks do not cover.
        self.assertTrue(grade(self.task, self.workspace)['passed'])
        self.assertFalse(grade_public(self.task, self.workspace)['passed'])


if __name__ == '__main__':
    unittest.main()
