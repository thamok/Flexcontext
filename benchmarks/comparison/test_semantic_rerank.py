import unittest
import semantic_rerank as s


class SemanticContractTests(unittest.TestCase):
    def test_prompt_allowlist_excludes_labels_and_scores(self):
        hit = {"path": "a.rs", "symbol": "f", "start_line": 1, "end_line": 2,
               "content": "fn f() {}", "evidence": "secret-label", "score": 99}
        body = s.request_body("question", [hit])
        self.assertNotIn(b"secret-label", s.canonical(body))
        self.assertNotIn("score", body["state"]["candidates"][0])

    def test_rerank_preserves_tail_and_stable_ties(self):
        hits = ["a", "b", "c", "d"]
        self.assertEqual(s.rerank(hits, [1, 2, 2]), (["b", "c", "a", "d"], [1, 2, 0]))

    def test_rejects_missing_or_nonfinite_answers(self):
        body = {"questions": {"candidate_0": {}}}
        with self.assertRaises(ValueError):
            s.scores_from_response(body, {"model": s.MODEL, "answers": {}})
        with self.assertRaises(ValueError):
            s.scores_from_response(body, {"model": s.MODEL, "answers": {"candidate_0": {"type": "score", "score": float("nan")}}})
        with self.assertRaises(ValueError):
            s.scores_from_response(body, {"model": s.MODEL, "answers": {"candidate_0": {"type": "score", "score": True}}})


class ExpansionTests(unittest.TestCase):
    def test_rrf_retains_original_candidates_and_duplicate_votes_only_once(self):
        import semantic_expand as e
        def hit(name):
            return {"path": name, "start_line": 1, "end_line": 1, "content": name}
        a, b = hit("a"), hit("b")
        self.assertEqual(e.fuse([[a, a, a], [b]]), [a, b])
        self.assertEqual(e.fuse([[a, b], [b]]), [b, a])

    def test_query_validation_rejects_extra_or_unbounded_queries(self):
        import semantic_expand as e
        def response(content):
            return {"choices": [{"finish_reason": "stop", "message": {"content": content}}]}
        self.assertEqual(e.parse_queries(response('{"queries":["a b","c d"]}')), ["a b", "c d"])
        with self.assertRaises(ValueError):
            e.parse_queries(response('{"queries":["a b"]}'))


class SemanticFallbackTests(unittest.TestCase):
    def test_timeout_returns_exact_native_baseline(self):
        from pathlib import Path
        from unittest.mock import patch
        import tempfile
        import semantic_search as cli
        import tiktoken
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "a.rs").write_text("first\nsecond\n")
            first = {"path": "a.rs", "symbol": "a", "start_line": 1, "end_line": 1, "content": "first"}
            second = {"path": "a.rs", "symbol": "b", "start_line": 2, "end_line": 2, "content": "second"}
            with patch.object(cli, "bounded_ask", side_effect=TimeoutError):
                result = cli.select("test", [second, first], [first], s.h.Source(root),
                    2048, tiktoken.get_encoding("cl100k_base"), root, True, .01)
            self.assertEqual(result["source_records"], [{"path": "a.rs", "line": 1, "text": "first"}])
            self.assertFalse(result["semantic"]["semantic_applied"])
            self.assertEqual(result["semantic"]["fallback_reason"], "api_timeout")

    def test_malformed_cache_and_truncated_response_fall_back(self):
        from pathlib import Path
        from unittest.mock import patch
        import tempfile
        import http.client
        import semantic_search as cli
        import tiktoken
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "a.rs").write_text("first\nsecond\n")
            first = {"path": "a.rs", "symbol": "a", "start_line": 1, "end_line": 1, "content": "first"}
            second = {"path": "a.rs", "symbol": "b", "start_line": 2, "end_line": 2, "content": "second"}
            body = cli.s.request_body("test", [second, first])
            invalid = [
                {"request": body, "response": []},
                {"request": body, "response": {"answers": {"candidate_0": None}}, "api_wall_ms": 1},
                {"request": body, "response": {"model": cli.s.MODEL, "answers": {
                    "candidate_0": {"type": "score", "score": 3}, "candidate_1": {"type": "score", "score": 0}}}},
            ]
            for payload in invalid:
                with patch.object(cli, "bounded_ask", return_value=(payload, True)):
                    result = cli.select("test", [second, first], [first], s.h.Source(root),
                        2048, tiktoken.get_encoding("cl100k_base"), root, False, .01)
                self.assertEqual([r["text"] for r in result["source_records"]], ["first"])
                self.assertFalse(result["semantic"]["semantic_applied"])
            with patch.object(cli, "bounded_ask", side_effect=http.client.IncompleteRead(b"", 20)):
                result = cli.select("test", [second, first], [first], s.h.Source(root),
                    2048, tiktoken.get_encoding("cl100k_base"), root, True, .01)
            self.assertEqual([r["text"] for r in result["source_records"]], ["first"])

    def test_source_is_frozen_before_remote_work(self):
        from pathlib import Path
        from unittest.mock import patch
        import tempfile
        import semantic_search as cli
        import tiktoken
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            path = root / "a.rs"
            path.write_text("first\n")
            hit = {"path": "a.rs", "symbol": "a", "start_line": 1, "end_line": 1, "content": "first"}
            def remote(body, *args):
                path.unlink()
                return {"request": body, "api_wall_ms": 1, "response": {"model": cli.s.MODEL,
                    "answers": {"candidate_0": {"type": "score", "score": 3}}}}, False
            with patch.object(cli, "bounded_ask", side_effect=remote):
                result = cli.select("test", [hit], [hit], s.h.Source(root), 2048,
                    tiktoken.get_encoding("cl100k_base"), root, True, .01)
            self.assertEqual(result["source_records"], [{"path": "a.rs", "line": 1, "text": "first"}])
            self.assertTrue(result["semantic"]["semantic_applied"])

    def test_stable_nonsecret_failure_codes(self):
        from pathlib import Path
        from unittest.mock import patch
        import tempfile
        import json
        import urllib.error
        import semantic_search as cli
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            body = cli.s.request_body("question", [])
            with self.assertRaises(cli.SemanticFailure) as missing:
                cli.bounded_ask(body, root, False, 1)
            self.assertEqual(cli.fallback_code(missing.exception), "cache_miss")
            with patch.dict(cli.os.environ, {}, clear=True):
                with self.assertRaises(cli.SemanticFailure) as no_key:
                    cli.bounded_ask(body, root, True, 1)
            self.assertEqual(cli.fallback_code(no_key.exception), "missing_api_key")
            (root / (cli.s.digest(body) + ".json")).write_text(json.dumps({"private": "DO_NOT_LOG"}))
            with self.assertRaises(cli.SemanticFailure) as corrupt:
                cli.bounded_ask(body, root, False, 1)
            self.assertEqual(cli.fallback_code(corrupt.exception), "invalid_cache")
            self.assertEqual(cli.fallback_code(urllib.error.HTTPError("secret-url", 429, "DO_NOT_LOG", {}, None)), "api_rate_limited")
            self.assertEqual(cli.fallback_code(urllib.error.URLError(TimeoutError("DO_NOT_LOG"))), "api_timeout")

    def test_native_errors_are_sanitized_and_exit_nonzero(self):
        from pathlib import Path
        from unittest.mock import patch, Mock
        import contextlib
        import io
        import json
        import tempfile
        import semantic_search as cli
        cases = [
            ({"error": {"message": "DO_NOT_LOG"}}, "native_rpc_error"),
            ({"result": {"isError": True, "content": "DO_NOT_LOG"}}, "native_tool_error"),
            ([], "invalid_native_response"),
        ]
        with tempfile.TemporaryDirectory() as tmp:
            for payload, reason in cases:
                process = Mock()
                process.call.return_value = (json.dumps(payload), 0)
                stream = io.StringIO()
                with patch.object(cli.h, "Resident", return_value=process), contextlib.redirect_stdout(stream):
                    code = cli.main([tmp, "test", "--binary", str(Path(tmp) / "binary"), "--cache", str(Path(tmp) / "cache")])
                self.assertEqual(code, 1)
                self.assertEqual(json.loads(stream.getvalue())["error"], reason)
                self.assertNotIn("DO_NOT_LOG", stream.getvalue())
                process.close.assert_called_once()

    def test_missing_source_exits_nonzero(self):
        from pathlib import Path
        from unittest.mock import patch, Mock
        import contextlib
        import io
        import json
        import tempfile
        import semantic_search as cli
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            binary = root / "binary"
            binary.write_bytes(b"test binary")
            hit = {"path": "deleted.rs", "symbol": "f", "start_line": 1, "end_line": 1, "content": "fn f() {}"}
            process = Mock()
            process.call.side_effect = [(json.dumps({"result": {}}), 0)] + [(json.dumps({"result": {"structuredContent": {"results": [hit]}}}), 0)] * 2
            stream = io.StringIO()
            with patch.object(cli.h, "Resident", return_value=process), contextlib.redirect_stdout(stream):
                code = cli.main([tmp, "test", "--binary", str(binary), "--cache", str(root / "cache")])
            self.assertEqual(code, 1)
            self.assertEqual(json.loads(stream.getvalue())["error"], "source_unavailable_after_native_retrieval")

    def test_edit_before_capture_reports_unresolved_alignment(self):
        from pathlib import Path
        import tempfile
        import semantic_search as cli
        import tiktoken
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "a.rs").write_text("changed after native retrieval\n")
            hit = {"path": "a.rs", "symbol": "a", "start_line": 1, "end_line": 1, "content": "original native source"}
            result = cli.select("test", [hit], [hit], s.h.Source(root), 2048,
                tiktoken.get_encoding("cl100k_base"), root, False, .01)
            self.assertFalse(result["source_alignment_complete"])
            self.assertEqual(result["source_records"], [{"path": "a.rs", "line": None, "text": "original native source"}])


if __name__ == "__main__":
    unittest.main()
