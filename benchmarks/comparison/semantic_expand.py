#!/usr/bin/env python3
"""Frozen development-only Qwen query expansion and stable RRF experiment."""
import argparse
import collections
import json
import os
from pathlib import Path
import statistics
import time
import urllib.error
import urllib.request

import run as h
import semantic_rerank as s
import tiktoken

MODEL = "Qwen/Qwen3.6-35B-A3B-FP8"
ENDPOINT = "https://inference.hetzner.com/api/v1/chat/completions"
PROMPT = (
    "You rewrite software repository questions into search queries. Given the user's question, "
    "return exactly a JSON object with key queries containing two short search strings. "
    "Each string must contain two to six focused words or likely identifier terms. "
    "Use complementary terminology: the first should name the central operation; the second "
    "should use likely implementation vocabulary or synonyms. Do not answer the question, "
    "invent file paths, or add explanations. Example output format: {\"queries\":[\"term one\",\"term two\"]}."
)
DESIGN = {"version": 1, "model": MODEL, "prompt": PROMPT, "temperature": 0,
          "max_tokens": 256, "enable_thinking": False, "rrf_k": 60,
          "queries": "original lexical terms plus two model rewrites", "shortlist": 100,
          "budgets": [2048, 4096], "native_source_ceiling_per_query": "budget * 8 bytes",
          "selection": "stable RRF over exact source-span candidate identity; unchanged prefix token cap"}


def body_for(question):
    return {"model": MODEL, "messages": [{"role": "system", "content": PROMPT},
        {"role": "user", "content": question}], "temperature": 0, "max_tokens": 256,
        "chat_template_kwargs": {"enable_thinking": False}}


def parse_queries(response):
    choice = response["choices"][0]
    if choice.get("finish_reason") != "stop":
        raise ValueError("completion did not finish normally")
    content = choice["message"]["content"].strip()
    if content.startswith("```json") and content.endswith("```"):
        content = content[7:-3].strip()
    queries = json.loads(content)["queries"]
    if not isinstance(queries, list) or len(queries) != 2 or any(
        not isinstance(q, str) or not 2 <= len(q.split()) <= 6 or len(q) > 160 for q in queries):
        raise ValueError("invalid query rewrite shape")
    return queries


def query_model(question, cache, live):
    body = body_for(question)
    key = s.digest(body)
    path = cache / f"{key}.json"
    if path.exists():
        item = json.loads(path.read_text())
        if item["request"] != body:
            raise ValueError("cache request mismatch")
        parse_queries(item["response"])
        return item, True
    if not live:
        raise RuntimeError(f"missing cache {key}; use --live")
    request = urllib.request.Request(ENDPOINT, data=s.canonical(body), headers={
        "Authorization": "Bearer " + os.environ["HETZNER_INFERENCE_KEY"],
        "Content-Type": "application/json"})
    start = time.perf_counter()
    try:
        with urllib.request.urlopen(request, timeout=90) as response:
            payload = json.load(response)
    except urllib.error.HTTPError as exc:
        raise RuntimeError(f"Hetzner HTTP {exc.code}; response body omitted") from None
    item = {"request": body, "response": payload, "request_sha256": key,
            "api_wall_ms": 1000 * (time.perf_counter() - start)}
    h.dump(path, item)  # retain malformed completions for audit, too
    parse_queries(payload)
    return item, False


def fuse(ranked_lists):
    scores, hits, first = {}, {}, {}
    for ranked in ranked_lists:
        seen = set()
        for rank, hit in enumerate(ranked, 1):
            key = (hit["path"], hit["start_line"], hit["end_line"], hit["content"])
            if key in seen:
                continue
            seen.add(key)
            if key not in first:
                first[key] = len(first)
                hits[key] = hit
            scores[key] = scores.get(key, 0) + 1 / (DESIGN["rrf_k"] + rank)
    return [hits[key] for key in sorted(scores, key=lambda key: (-scores[key], first[key]))]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--snapshot-root", type=Path, required=True)
    parser.add_argument("--cases", type=Path, default=h.HERE / "optimization-cases.json")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--live", action="store_true")
    args = parser.parse_args()
    cases = [c for c in json.loads(args.cases.read_text())["cases"] if c["split"] == "dev"]
    roots = {c["repo"]: (args.snapshot_root / c["repo"]).resolve() for c in cases}
    h.validate_cases(cases, roots)
    enc = tiktoken.get_encoding("cl100k_base")
    contract = {"design": DESIGN, "design_sha256": s.digest(DESIGN), "split": "dev",
        "binary_sha256": h.sha(args.binary.read_bytes()), "cases_sha256": h.sha(args.cases.read_bytes()),
        "script_sha256": h.sha(Path(__file__).read_bytes())}
    path = args.output / "contract.json"
    if path.exists() and json.loads(path.read_text()) != contract:
        raise ValueError("contract changed; use a new output directory")
    h.dump(path, contract)
    rows, calls = [], []
    next_call = 0
    for repo, root in sorted(roots.items()):
        source = h.Source(root)
        process = h.Resident([args.binary.resolve(), "serve", root], args.output / f"{repo}.stderr.txt", 180)
        try:
            process.call(h.mcp("server/discover"))
            for case in [c for c in cases if c["repo"] == repo]:
                time.sleep(max(0, next_call - time.monotonic()))
                item, cached = query_model(case["question"], args.cache, args.live)
                next_call = time.monotonic() + (0 if cached else 6.2)
                queries = [" ".join(h.terms(case["question"])), *parse_queries(item["response"])]
                calls.append({"case": case["id"], "repo": repo, "queries": queries,
                    "api_wall_ms": item["api_wall_ms"], "usage": item["response"].get("usage", {}),
                    "cache_hit": cached, "request_sha256": item["request_sha256"]})
                for budget in DESIGN["budgets"]:
                    ranked, retrieval_ms = [], []
                    for i, query in enumerate(queries):
                        raw, elapsed = process.call(h.mcp("tools/call", {"query": query,
                            "budget": budget * 2, "max_results": 100, "detail": "compact", "policy": "baseline"}))
                        data = json.loads(raw)
                        hits = data["result"]["structuredContent"]["results"]
                        ranked.append(hits)
                        retrieval_ms.append(elapsed)
                        h.dump(args.output / "artifacts" / f"{case['id']}-{budget}-q{i}.json", data)
                    fused = fuse(ranked)
                    source_tokens_all = sum(len(enc.encode(hit["content"], disallowed_special=())) for hits in ranked for hit in hits)
                    source_chars_all = sum(len(hit["content"]) for hits in ranked for hit in hits)
                    for variant, hits in [("original", ranked[0]), ("qwen_rrf", fused)]:
                        selected, tokens = h.cap(s.records(hits, source), budget, enc)
                        rows.append({"case": case["id"], "repo": repo, "budget": budget, "variant": variant,
                            "source_tokens": tokens, **h.score(case, selected),
                            "candidate_coverage": h.score(case, s.records(hits, source))["evidence_recall"],
                            "retrieved_source_tokens": source_tokens_all if variant == "qwen_rrf" else sum(len(enc.encode(hit["content"], disallowed_special=())) for hit in ranked[0]),
                            "retrieved_source_chars": source_chars_all if variant == "qwen_rrf" else sum(len(hit["content"]) for hit in ranked[0]),
                            "retrieval_ms": sum(retrieval_ms) if variant == "qwen_rrf" else retrieval_ms[0]})
                        h.dump(args.output / "selected" / f"{case['id']}-{budget}-{variant}.json", selected)
                    h.dump(args.output / "artifacts" / f"{case['id']}-{budget}-fused.json", {"results": fused})
                print(f"{case['id']}: {queries[1:]}, API {item['api_wall_ms']:.0f} ms", flush=True)
                h.dump(args.output / "partial.json", {"rows": rows, "calls": calls})
        finally:
            process.close()
    groups = collections.defaultdict(list)
    for row in rows:
        groups[(row["repo"], row["budget"], row["variant"])].append(row)
    summary = [{"repo": key[0], "budget": key[1], "variant": key[2], **{
        metric: statistics.mean(r[metric] for r in group) for metric in ["evidence_recall", "evidence_precision", "candidate_coverage", "source_tokens", "retrieved_source_tokens", "retrieval_ms"]}}
        for key, group in sorted(groups.items())]
    h.dump(args.output / "results.json", {"contract": contract, "rows": rows, "calls": calls, "summary": summary})
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
