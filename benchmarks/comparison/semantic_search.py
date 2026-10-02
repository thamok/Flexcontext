#!/usr/bin/env python3
"""Opt-in Jev semantic search prototype with exact baseline fallback.

Remote calls require --live and JEV_API_KEY. Without --live, matching cached
responses may be replayed; a missing response falls back to native retrieval.
This CLI is separate from the Rust server and is not enabled by default.
"""
import argparse
import http.client
import json
import math
import os
from pathlib import Path
import time
import urllib.error
import urllib.request

import run as h
import semantic_native48 as native48
import tiktoken

s = native48.reranker


class SemanticFailure(RuntimeError):
    """An allowlisted failure code; never a provider or operating-system message."""
    def __init__(self, code):
        super().__init__(code)
        self.code = code


class NativeFailure(RuntimeError):
    def __init__(self, code):
        super().__init__(code)
        self.code = code


def fallback_code(exc):
    if isinstance(exc, SemanticFailure):
        return exc.code
    if isinstance(exc, TimeoutError) or (isinstance(exc, urllib.error.URLError)
                                        and isinstance(exc.reason, TimeoutError)):
        return "api_timeout"
    if isinstance(exc, urllib.error.HTTPError):
        return {401: "api_unauthorized", 403: "api_forbidden", 429: "api_rate_limited"}.get(exc.code, "api_http_error")
    if isinstance(exc, http.client.IncompleteRead):
        return "api_truncated_response"
    if isinstance(exc, (ValueError, KeyError, TypeError)):
        return "invalid_api_response"
    if isinstance(exc, (OSError, http.client.HTTPException)):
        return "api_transport_error"
    return "semantic_unavailable"


def native_result(raw, require_hits=False):
    try:
        data = json.loads(raw)
    except (ValueError, TypeError):
        raise NativeFailure("invalid_native_response") from None
    if not isinstance(data, dict):
        raise NativeFailure("invalid_native_response")
    if "error" in data:
        raise NativeFailure("native_rpc_error")
    result = data.get("result")
    if not isinstance(result, dict):
        raise NativeFailure("invalid_native_response")
    if result.get("isError"):
        raise NativeFailure("native_tool_error")
    if not require_hits:
        return result
    structured = result.get("structuredContent")
    hits = structured.get("results") if isinstance(structured, dict) else None
    if not isinstance(hits, list):
        raise NativeFailure("invalid_native_response")
    for hit in hits:
        if not isinstance(hit, dict) or not all(isinstance(hit.get(k), str) for k in ("path", "content")) or "symbol" not in hit:
            raise NativeFailure("invalid_native_response")
        if any(isinstance(hit.get(k), bool) or not isinstance(hit.get(k), int) for k in ("start_line", "end_line")):
            raise NativeFailure("invalid_native_response")
        if hit["start_line"] < 1 or hit["end_line"] < hit["start_line"]:
            raise NativeFailure("invalid_native_response")
    return hits


def validate_result(body, result):
    if not isinstance(result, dict) or result.get("request") != body:
        raise ValueError("invalid cached request")
    response = result.get("response")
    if not isinstance(response, dict) or not isinstance(response.get("answers"), dict):
        raise ValueError("invalid response shape")
    if any(not isinstance(answer, dict) for answer in response["answers"].values()):
        raise ValueError("invalid answer shape")
    elapsed = result.get("api_wall_ms")
    if isinstance(elapsed, bool) or not isinstance(elapsed, (int, float)) or not math.isfinite(elapsed) or elapsed < 0:
        raise ValueError("invalid latency metadata")
    s.scores_from_response(body, response)


def bounded_ask(body, cache, live, timeout):
    path = cache / f"{s.digest(body)}.json"
    if path.exists():
        try:
            result = json.loads(path.read_text())
            validate_result(body, result)
        except OSError:
            raise SemanticFailure("cache_unavailable") from None
        except (ValueError, KeyError, TypeError):
            raise SemanticFailure("invalid_cache") from None
        return result, True
    if not live:
        raise SemanticFailure("cache_miss")
    key = os.environ.get("JEV_API_KEY") or os.environ.get("TYPESAFE_API_KEY")
    if not key:
        raise SemanticFailure("missing_api_key")
    request = urllib.request.Request(s.ENDPOINT, data=s.canonical(body), headers={
        "Authorization": "Bearer " + key, "Content-Type": "application/json"})
    start = time.perf_counter()
    with urllib.request.urlopen(request, timeout=timeout) as response:
        payload = json.load(response)
    result = {"request": body, "response": payload, "request_sha256": s.digest(body),
              "api_wall_ms": 1000 * (time.perf_counter() - start),
              "request_bytes": len(s.canonical(body)), "retry_attempts": [], "endpoint": s.ENDPOINT}
    validate_result(body, result)
    try:
        h.dump(path, result)
    except OSError:
        raise SemanticFailure("cache_unavailable") from None
    return result, False


def select(question, candidates, baseline, source, budget, encoding, cache, live, timeout):
    # Capture alignment before inference, not atomically with native retrieval.
    # Earlier edits can leave unresolved lines; later disk changes cannot affect
    # these records. Source.excerpt never expands native text.
    try:
        candidate_records = {id(hit): s.records([hit], source) for hit in candidates}
        baseline_records = s.records(baseline, source)
    except (OSError, UnicodeError, ValueError):
        return {"query": question, "budget": budget, "source_tokens": 0, "source_records": [],
                "error": "source_unavailable_after_native_retrieval"}
    body = s.request_body(question, candidates)
    metadata = {"semantic_applied": False, "design_sha256": s.digest(s.DESIGN),
                "model": s.MODEL, "candidate_count": len(body["questions"])}
    selected_records = baseline_records
    if candidates:
        try:
            result, cached = bounded_ask(body, cache, live, timeout)
            validate_result(body, result)
            reranked, _ = s.rerank(candidates, s.scores_from_response(body, result["response"]))
            metadata.update(semantic_applied=True, cache_hit=cached,
                usage=result["response"].get("usage", {}), recorded_api_wall_ms=result["api_wall_ms"])
            selected_records = [record for hit in reranked for record in candidate_records[id(hit)]]
        except (OSError, http.client.HTTPException, ValueError, KeyError, TypeError, RuntimeError) as exc:
            metadata["fallback_reason"] = fallback_code(exc)
            metadata["semantic_applied"] = False
            selected_records = baseline_records
    selected, tokens = h.cap(selected_records, budget, encoding)
    return {"query": question, "budget": budget, "source_tokens": tokens,
            "source_records": selected, "source_alignment_complete": all(r["line"] is not None for r in selected),
            "semantic": metadata}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("query")
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--budget", type=int, choices=[2048, 4096], default=2048)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--live", action="store_true", help="send candidate source to TypeSafe")
    parser.add_argument("--timeout", type=float, default=3.0, help="network socket timeout in seconds; no retries")
    args = parser.parse_args(argv)
    if not 0 < args.timeout <= 30:
        parser.error("timeout must be in (0, 30]")
    root = args.root.resolve()
    start = time.perf_counter()
    process = None
    try:
        args.cache.mkdir(parents=True, exist_ok=True)
        process = h.Resident([args.binary.resolve(), "serve", root], args.cache / "native.stderr.txt", 180)
        raw, _ = process.call(h.mcp("server/discover"))
        native_result(raw)
        def search(budget):
            raw, _ = process.call(h.mcp("tools/call", {"query": " ".join(h.terms(args.query)),
                "budget": budget * 2, "max_results": 100, "detail": "compact", "policy": "baseline"}))
            return native_result(raw, require_hits=True)
        candidates = search(4096)
        baseline = candidates if args.budget == 4096 else search(args.budget)
        result = select(args.query, candidates, baseline, h.Source(root), args.budget,
            tiktoken.get_encoding("cl100k_base"), args.cache, args.live, args.timeout)
        result["binary_sha256"] = h.sha(args.binary.read_bytes())
    except NativeFailure as exc:
        result = {"error": exc.code}
    except TimeoutError:
        result = {"error": "native_timeout"}
    except OSError:
        result = {"error": "native_io_error"}
    except RuntimeError:
        result = {"error": "native_process_error"}
    finally:
        if process is not None:
            process.close()
    result["total_wall_ms"] = 1000 * (time.perf_counter() - start)
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 1 if "error" in result else 0


if __name__ == "__main__":
    raise SystemExit(main())
