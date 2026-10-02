#!/usr/bin/env python3
"""Reproducible retrieval comparison. See README.md for the measurement contract."""
import argparse
import collections
import difflib
import hashlib
import importlib.metadata
import json
import os
import platform
import queue
import random
import re
import shutil
import statistics
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from pathlib import Path

import tiktoken

HERE = Path(__file__).resolve().parent
PROJECT = HERE.parent.parent
EXTENSIONS = {".rs", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts",
              ".py", ".pyi", ".swift", ".c", ".h", ".cpp", ".cc", ".hpp", ".m", ".mm",
              ".java", ".cls", ".trigger", ".apex", ".go", ".cs", ".cxx", ".c++", ".hh", ".hxx",
              ".h++", ".ipp", ".tpp", ".metal", ".cu", ".cuh", ".kt", ".kts", ".dart", ".vue", ".lua"}
STOP = set("a an the and or to of in on for from with without is are be by how what which when where why does do it its this that as can into all only after before then if at same their we our than there happens".split())


def dump(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def terms(question):
    return list(dict.fromkeys(w.lower() for w in re.findall(r"[A-Za-z_][A-Za-z_0-9]*", question)
                              if w.lower() not in STOP and len(w) > 1))


def command(argv, cwd=None, timeout=180, input_text=None, allowed=(0,)):
    started = time.perf_counter()
    p = subprocess.run([str(v) for v in argv], cwd=cwd, input=input_text,
                       capture_output=True, text=True, timeout=timeout,
                       env={**os.environ, "RAYON_NUM_THREADS": "4", "NO_COLOR": "1",
                            "TOKENIZERS_PARALLELISM": "false", "PYTHONHASHSEED": "0"})
    elapsed = (time.perf_counter() - started) * 1000
    if p.returncode not in allowed:
        raise RuntimeError(f"exit {p.returncode}: {argv[0]}: {p.stderr[-3000:]}")
    return p.stdout, p.stderr, elapsed


class Resident:
    def __init__(self, argv, stderr, timeout):
        self.timeout = timeout
        self.err = stderr.open("w")
        self.p = subprocess.Popen([str(v) for v in argv], stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE, stderr=self.err, text=True,
                                  env={**os.environ, "RAYON_NUM_THREADS": "4", "NO_COLOR": "1",
                                       "PYTHONHASHSEED": "0"})
        self.lines = queue.Queue()
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()

    def _read(self):
        for line in self.p.stdout:
            self.lines.put(line)
        self.lines.put(None)

    def call(self, request):
        start = time.perf_counter()
        self.p.stdin.write(json.dumps(request) + "\n")
        self.p.stdin.flush()
        try:
            line = self.lines.get(timeout=self.timeout)
        except queue.Empty:
            self.close()
            raise TimeoutError("resident request timed out")
        if line is None:
            raise RuntimeError("resident exited; inspect stderr artifact")
        return line, (time.perf_counter() - start) * 1000

    def close(self):
        if self.p.poll() is None:
            self.p.stdin.close()
            try:
                self.p.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.p.kill()
                self.p.wait()
        self.p.stdout.close()
        self.err.close()


def mcp(method, arguments=None):
    params = {"_meta": {"io.modelcontextprotocol/protocolVersion": "2026-07-28",
                        "io.modelcontextprotocol/clientCapabilities": {}}}
    if arguments is not None:
        params.update(name="code_search", arguments=arguments)
    return {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}


def snapshot(repo_id, spec, output):
    original = Path(spec["path"]).resolve()
    root = output / "snapshots" / repo_id
    root.mkdir(parents=True)
    listed = command(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], original)[0]
    files, skipped = {}, collections.Counter()
    for name in sorted(set(listed.split("\0")) - {""}):
        path = original / name
        if not any(name.startswith(prefix) for prefix in spec.get("include", [""])):
            continue
        if path.suffix.lower() not in EXTENSIONS:
            continue
        if not path.is_file() or path.is_symlink():
            skipped["missing_or_symlink"] += 1
            continue
        data = path.read_bytes()
        if len(data) > 2 * 1024 * 1024 or b"\0" in data:
            skipped["oversize_or_binary"] += 1
            continue
        try:
            data.decode("utf-8")
        except UnicodeDecodeError:
            skipped["non_utf8"] += 1
            continue
        dest = root / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(data)
        files[name] = {"sha256": sha(data), "bytes": len(data)}
    if not files:
        raise ValueError(f"empty snapshot {repo_id}")
    dump(output / f"{repo_id}.files.json", list(files))
    info = {"original": str(original), "include": spec.get("include", [""]),
            "head": command(["git", "rev-parse", "HEAD"], original)[0].strip(),
            "dirty": bool(command(["git", "status", "--porcelain", "--untracked-files=normal"], original)[0]),
            "files": files, "file_count": len(files), "bytes": sum(f["bytes"] for f in files.values()),
            "snapshot_sha256": sha(json.dumps(files, sort_keys=True).encode()), "skipped": dict(skipped),
            "extensions": dict(collections.Counter(Path(f).suffix for f in files))}
    dump(output / f"{repo_id}.manifest.json", info)
    # Deliberately no .git or original ignore files: every adapter sees the same inventory.
    return root, info


def validate_cases(cases, roots):
    seen = set()
    for case in cases:
        if case["id"] in seen:
            raise ValueError(f"duplicate case {case['id']}")
        seen.add(case["id"])
        if not terms(case["question"]) or not case["evidence"]:
            raise ValueError(f"empty query/evidence {case['id']}")
        for unit in [*case["evidence"], {"spans": case.get("relevant_regions", [])}] if case.get("relevant_regions") else case["evidence"]:
            if not unit["spans"]:
                raise ValueError("empty evidence unit")
            for span in unit["spans"]:
                path = roots[case["repo"]] / span["path"]
                content = path.read_text().splitlines(keepends=True)
                if not 1 <= span["start"] <= span["end"] <= len(content):
                    raise ValueError(f"invalid span {case['id']}: {span}")
                actual = "".join(content[span["start"] - 1:span["end"]])
                if sha(actual.encode()) != span["sha256"]:
                    raise ValueError(f"ground truth drift: {case['id']} {span['path']}; re-review labels")


class Source:
    def __init__(self, root):
        self.root = root
        self.cache = {}

    def lines(self, path):
        if path not in self.cache:
            absolute = (self.root / path).resolve()
            if not absolute.is_relative_to(self.root.resolve()):
                raise ValueError(f"path outside snapshot: {path}")
            self.cache[path] = absolute.read_text().splitlines()
        return self.cache[path]

    def excerpt(self, path, text, start=1, end=None):
        """Align only visible, complete lines. Never expand a hit into unseen source."""
        lines = self.lines(path)
        end = len(lines) if end is None else end
        lookup = collections.defaultdict(list)
        for i in range(start - 1, min(end, len(lines))):
            lookup[lines[i].strip()].append(i + 1)
        # Matching blocks resolve duplicate braces in continuous snippets; unique
        # matches recover sliced snippets. Ambiguous lines remain unlocated.
        shown = text.splitlines()
        alignment = {}
        matcher = difflib.SequenceMatcher(None, [s.strip() for s in lines[start - 1:end]],
                                         [s.strip() for s in shown], autojunk=False)
        for block in matcher.get_matching_blocks():
            if block.size >= 2:
                for k in range(block.size):
                    alignment[block.b + k] = start + block.a + k
        result = []
        for i, line in enumerate(shown):
            if not line.strip() or line.startswith(("[…", "[lines ")):
                continue
            matches = lookup.get(line.strip(), [])
            number = alignment.get(i, matches[0] if len(matches) == 1 else None)
            result.append({"path": path, "line": number, "text": line})
        return result


def normalize(tool, raw, source, resident=False):
    records, pointers = [], []
    if tool == "flexcontext":
        data = json.loads(raw)
        if resident:
            if "error" in data or data.get("result", {}).get("isError"):
                raise RuntimeError(str(data)[:2000])
            data = data["result"]["structuredContent"]
        for hit in data["results"]:
            pointers.append(hit["path"])
            records.extend(source.excerpt(hit["path"], hit["content"], hit["start_line"], hit["end_line"]))
    elif tool == "probe":
        for hit in json.loads(raw)["results"]:
            path = str(Path(hit["file"]).relative_to(source.root)) if Path(hit["file"]).is_absolute() else hit["file"]
            pointers.append(path)
            records.extend(source.excerpt(path, hit["code"], *hit["lines"]))
    elif tool == "rg":
        for line in raw.splitlines():
            event = json.loads(line)
            if event["type"] not in ("match", "context"):
                continue
            data = event["data"]
            path = data["path"]["text"].removeprefix("./")
            pointers.append(path)
            for offset, text in enumerate(data["lines"]["text"].splitlines()):
                if text.strip():
                    records.append({"path": path, "line": data["line_number"] + offset, "text": text})
    else:
        raw = json.loads(raw)["map"]
        path, chunk = None, []
        def flush():
            if path is not None:
                records.extend(source.excerpt(path, "\n".join(chunk)))
        for line in raw.splitlines():
            candidate = line.removesuffix(":")
            if line and not line.startswith(("│", "⋮")) and (source.root / candidate).is_file():
                flush()
                path, chunk = candidate, []
                pointers.append(path)
            elif line.startswith("│"):
                chunk.append(line[1:])
        flush()
    return records, list(dict.fromkeys(pointers))


def cap(records, budget, encoding):
    selected, used, seen = [], 0, set()
    for record in records:
        key = (record["path"], record["line"], record["text"])
        if key in seen:
            continue
        seen.add(key)
        cost = len(encoding.encode(record["text"] + "\n", disallowed_special=()))
        if used + cost > budget:
            break  # common prefix policy, no relevance-aware packing
        selected.append(record)
        used += cost
    return selected, used


def score(case, selected):
    returned = {(r["path"], r["line"]) for r in selected if r["line"] is not None}
    relevant, found = set(), []
    for unit in case["evidence"]:
        required = set()
        for span in unit["spans"]:
            required.update((span["path"], n) for n in range(span["start"], span["end"] + 1))
        relevant.update(required)
        found.append(required <= returned)
    # Unlocated and unjudged nonblank lines count against conservative precision.
    denominator = len(returned) + sum(r["line"] is None for r in selected)
    target_files = {p for p, _ in relevant}
    shown_files = {r["path"] for r in selected}
    regions = {(s["path"], n) for s in case.get("relevant_regions", [])
               for n in range(s["start"], s["end"] + 1)} or relevant
    return {"evidence_recall": sum(found) / len(found),
            "evidence_precision": len(returned & regions) / denominator if denominator else 0.0,
            "evidence_units_found": sum(found), "evidence_units_total": len(found),
            "complete_evidence": all(found),
            "gold_line_recall": len(returned & relevant) / len(relevant),
            "gold_line_precision": len(returned & relevant) / denominator if denominator else 0.0,
            "source_file_recall": len(shown_files & target_files) / len(target_files),
            "missing_evidence": [u["id"] for u, hit in zip(case["evidence"], found) if not hit]}


def clear_cache(tool, root):
    names = [root / ".flexcontext"] if tool == "flexcontext" else list(root.glob(".aider.tags.cache.*")) if tool == "aider" else []
    for path in names:
        if path.is_dir():
            shutil.rmtree(path)
        elif path.exists():
            path.unlink()


def argv_for(tool, case, root, budget, args):
    words = terms(case["question"])
    if tool == "flexcontext":
        return [args.flexcontext, "search", root, " ".join(words), "--json", "--max-bytes", budget * 8, "--max-results", 100], None
    if tool == "probe":
        return [args.probe, "search", " OR ".join(words), root, "--format", "json", "--allow-tests", "--max-bytes", budget * 8, "--max-results", 100], None
    if tool == "rg":
        return ["rg", "--json", "--sort", "path", "-i", "-F", "-C", "3", *[v for w in words for v in ("-e", w)], "."], None
    return [sys.executable, HERE / "aider_worker.py", root, args.output / f"{case['repo']}.files.json", args.encoding], json.dumps({"budget": budget, "terms": words}) + "\n"


def report(output, rows):
    groups = collections.defaultdict(list)
    for row in rows:
        groups[(row["repo"], row["tool"], row["budget"], row["mode"])].append(row)
    summary = []
    for key, group in sorted(groups.items()):
        ok = [r for r in group if r["status"] == "ok"]
        result = dict(zip(("repo", "tool", "budget", "mode"), key))
        result.update(samples=len(group), errors=len(group) - len(ok))
        if ok:
            for metric in ("evidence_recall", "evidence_precision", "gold_line_precision", "gold_line_recall", "source_file_recall", "source_tokens", "source_bytes", "native_bytes", "native_tokens", "context_tokens", "latency_ms", "normalization_ms"):
                result[metric] = statistics.mean(r[metric] for r in ok)
            values = sorted(r["latency_ms"] for r in ok)
            result["latency_median_ms"] = statistics.median(values)
            result["latency_p95_ms"] = values[max(0, (95 * len(values) + 99) // 100 - 1)]
        summary.append(result)
    dump(output / "summary.json", summary)
    lines = ["# Retrieval comparison pilot", "", "Gold-line precision is conservative: unjudged source is counted as irrelevant. This is an authored pilot, not a leaderboard.", "",
             "| Repository | Tool | Budget | Mode | Evidence recall | Region precision | Source tokens | Native tokens | Median ms | Errors |",
             "|---|---|---:|---|---:|---:|---:|---:|---:|---:|"]
    for s in summary:
        base = f"| {s['repo']} | {s['tool']} | {s['budget']} | {s['mode']} |"
        if "evidence_recall" in s:
            lines.append(base + f" {s['evidence_recall']:.1%} | {s['evidence_precision']:.1%} | {s['source_tokens']:.0f} | {s['native_tokens']:.0f} | {s['latency_median_ms']:.2f} | {s['errors']} |")
        else:
            lines.append(base + f" — | — | — | — | — | {s['errors']} |")
    (output / "REPORT.md").write_text("\n".join(lines) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repos", type=Path, default=HERE / "repos.json")
    parser.add_argument("--cases", type=Path, default=HERE / "cases.json")
    parser.add_argument("--only-repos", nargs="+")
    parser.add_argument("--tools", nargs="+", choices=["flexcontext", "probe", "rg", "aider"], default=["flexcontext", "probe", "rg", "aider"])
    parser.add_argument("--budgets", nargs="+", type=int, default=[2048, 4096])
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--timeout", type=float, default=180)
    parser.add_argument("--seed", type=int, default=1729)
    parser.add_argument("--encoding", default="cl100k_base")
    parser.add_argument("--flexcontext", type=Path, default=PROJECT / "target/release/flexcontext")
    parser.add_argument("--probe", type=Path, default=PROJECT / ".benchmark-tools/node_modules/@probelabs/probe/bin/probe")
    parser.add_argument("--output", type=Path, default=PROJECT / ".benchmark-results" / datetime.now().strftime("%Y%m%d-%H%M%S"))
    args = parser.parse_args()
    if args.repeats < 1 or any(b < 1 for b in args.budgets):
        parser.error("repeats and budgets must be positive")
    args.output = args.output.resolve()
    args.flexcontext = args.flexcontext.resolve()
    args.probe = args.probe.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    encoding = tiktoken.get_encoding(args.encoding)
    specs = json.loads(args.repos.read_text())
    if args.only_repos:
        specs = {key: specs[key] for key in args.only_repos}
    cases = [c for c in json.loads(args.cases.read_text())["cases"] if c["repo"] in specs]
    roots, manifests = {}, {}
    for name, spec in specs.items():
        print(f"Snapshotting {name}", flush=True)
        roots[name], manifests[name] = snapshot(name, spec, args.output)
    validate_cases(cases, roots)
    dump(args.output / "cases.json", cases)
    environment = {"created_at": datetime.now(timezone.utc).isoformat(), "platform": platform.platform(),
                   "python": sys.version, "encoding": args.encoding, "seed": args.seed,
                   "budgets": args.budgets, "repeats": args.repeats, "tools": args.tools,
                   "versions": {name: importlib.metadata.version(name) for name in ["aider-chat", "tiktoken", "tree-sitter-language-pack", "scipy", "numpy"]},
                   "binary_sha256": {name: sha(path.read_bytes()) for name, path in [("flexcontext", args.flexcontext), ("probe", args.probe)] if name in args.tools},
                   "rg_version": command(["rg", "--version"])[0].splitlines()[0],
                   "cases_sha256": sha(args.cases.read_bytes()),
                   "harness_sha256": {p.name: sha(p.read_bytes()) for p in [Path(__file__), HERE / "aider_worker.py"]},
                   "notes": ["Cold means fresh process with tool disk cache cleared; OS cache is uncontrolled.",
                             "Warm CLI includes process startup; resident excludes startup, recorded separately.",
                             "rg and Probe are stateless CLI adapters; resident mode is not measured."]}
    dump(args.output / "environment.json", environment)
    archive = args.output / "harness"
    archive.mkdir()
    for path in (Path(__file__), HERE / "aider_worker.py"):
        shutil.copy2(path, archive / path.name)
    rows = []
    sources = {repo: Source(root) for repo, root in roots.items()}
    rng = random.Random(args.seed)

    def record(case, tool, budget, mode, repeat, raw=None, elapsed=None, error=None):
        stem = f"{case['id']}-{tool}-{budget}-{mode}-{repeat}"
        row = {"case": case["id"], "repo": case["repo"], "tool": tool, "budget": budget,
               "mode": mode, "repeat": repeat, "query_terms": terms(case["question"]), "status": "error" if error else "ok"}
        if error:
            row["error"] = str(error)
        else:
            artifacts = args.output / "artifacts"
            artifacts.mkdir(exist_ok=True)
            (artifacts / f"{stem}.raw.txt").write_text(raw)
            started = time.perf_counter()
            records, pointers = normalize(tool, raw, sources[case["repo"]], mode.startswith("resident"))
            selected, tokens = cap(records, budget, encoding)
            context = "\n".join(f"{r['path']}:{r['line'] or '?'}: {r['text']}" for r in selected) + ("\n" if selected else "")
            (artifacts / f"{stem}.context.txt").write_text(context)
            dump(artifacts / f"{stem}.lines.json", selected)
            row.update(score(case, selected))
            gold_files = {s["path"] for u in case["evidence"] for s in u["spans"]}
            row.update(source_tokens=tokens, source_bytes=sum(len((r["text"] + "\n").encode()) for r in selected),
                       native_bytes=len(raw.encode()), native_tokens=len(encoding.encode(raw, disallowed_special=())),
                       context_bytes=len(context.encode()), context_tokens=len(encoding.encode(context, disallowed_special=())),
                       native_pointer_file_recall=len(set(pointers) & gold_files) / len(gold_files),
                       unlocated_source_lines=sum(r["line"] is None for r in selected),
                       budget_truncated=len(selected) < len(records), latency_ms=elapsed,
                       normalization_ms=(time.perf_counter() - started) * 1000,
                       context_artifact=f"artifacts/{stem}.context.txt")
        rows.append(row)
        with (args.output / "rows.jsonl").open("a") as out:
            out.write(json.dumps(row) + "\n")
        print(f"{stem}: {row.get('evidence_recall', row.get('error'))}", flush=True)

    jobs = [(c, t, b) for c in cases for t in args.tools for b in args.budgets]
    rng.shuffle(jobs)
    for case, tool, budget in jobs:
        root = roots[case["repo"]]
        clear_cache(tool, root)
        for repeat in range(args.repeats + 1):
            mode = "cold" if repeat == 0 else "warm_cli"
            argv, input_text = argv_for(tool, case, root, budget, args)
            try:
                raw, stderr, elapsed = command(argv, root, args.timeout, input_text, (0, 1) if tool == "rg" else (0,))
                log = args.output / "stderr" / f"{case['id']}-{tool}-{budget}-{mode}-{repeat}.txt"
                log.parent.mkdir(exist_ok=True)
                log.write_text(stderr)
                record(case, tool, budget, mode, repeat, raw, elapsed)
            except Exception as error:
                record(case, tool, budget, mode, repeat, error=error)
        report(args.output, rows)
    startups = []
    for repo, root in roots.items():
        for tool in [t for t in args.tools if t in ("flexcontext", "aider")]:
            process = None
            start = time.perf_counter()
            try:
                argv = [args.flexcontext, "serve", root] if tool == "flexcontext" else [sys.executable, HERE / "aider_worker.py", root, args.output / f"{repo}.files.json", args.encoding]
                process = Resident(argv, args.output / f"{repo}-{tool}-resident.stderr.txt", args.timeout)
                process.call(mcp("server/discover") if tool == "flexcontext" else {"method": "ready"})
                startups.append({"repo": repo, "tool": tool, "startup_ms": (time.perf_counter() - start) * 1000, "disk_cache": "warm"})
                # All distinct queries before repetitions: separate first use from repeated-query hits.
                for repeat in range(args.repeats + 1):
                    queries = [(c, b) for c in cases if c["repo"] == repo for b in args.budgets]
                    rng.shuffle(queries)
                    for case, budget in queries:
                        mode = "resident_first" if repeat == 0 else "resident_repeat"
                        try:
                            words = terms(case["question"])
                            request = mcp("tools/call", {"query": " ".join(words), "budget": budget * 2, "max_results": 100}) if tool == "flexcontext" else {"budget": budget, "terms": words}
                            raw, elapsed = process.call(request)
                            record(case, tool, budget, mode, repeat, raw, elapsed)
                        except Exception as error:
                            record(case, tool, budget, mode, repeat, error=error)
            except Exception as error:
                startups.append({"repo": repo, "tool": tool, "error": str(error)})
            finally:
                if process is not None:
                    process.close()
            dump(args.output / "resident-startup.json", startups)
            report(args.output, rows)
    print(f"Report: {args.output / 'REPORT.md'}", flush=True)
    return int(any(r["status"] != "ok" for r in rows) or any("error" in s for s in startups))


if __name__ == "__main__":
    sys.exit(main())
