#!/usr/bin/env python3
"""Audit saved source provenance, token caps, scores and repetition stability."""
import argparse
import collections
import json
from pathlib import Path

import tiktoken

from run import Source, dump, score, sha, validate_cases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    args = parser.parse_args()
    env = json.loads((args.run / "environment.json").read_text())
    encoding = tiktoken.get_encoding(env["encoding"])
    for name, digest in env["harness_sha256"].items():
        assert sha((args.run / "harness" / name).read_bytes()) == digest, name
    cases = {c["id"]: c for c in json.loads((args.run / "cases.json").read_text())}
    sources = {c["repo"]: Source(args.run / "snapshots" / c["repo"]) for c in cases.values()}
    validate_cases(list(cases.values()), {k: s.root for k, s in sources.items()})
    checked_files = 0
    for repo, source in sources.items():
        manifest = json.loads((args.run / f"{repo}.manifest.json").read_text())
        for path, info in manifest["files"].items():
            assert sha((source.root / path).read_bytes()) == info["sha256"], path
            checked_files += 1
    rows = [json.loads(line) for line in (args.run / "rows.jsonl").read_text().splitlines()]
    tools = env.get("tools", sorted({r["tool"] for r in rows}))
    expected = len(cases) * len(env["budgets"]) * (env["repeats"] + 1) * (len(tools) + sum(t in ("flexcontext", "aider") for t in tools))
    seen, hashes, errors = set(), collections.defaultdict(set), []
    checked_lines = 0
    for row in rows:
        key = tuple(row[k] for k in ("case", "tool", "budget", "mode", "repeat"))
        assert key not in seen, f"duplicate row {key}"
        seen.add(key)
        if row["status"] != "ok":
            errors.append(key)
            continue
        artifact = args.run / row["context_artifact"]
        selected = json.loads(Path(str(artifact).replace(".context.txt", ".lines.json")).read_text())
        context = "\n".join(f"{r['path']}:{r['line'] or '?'}: {r['text']}" for r in selected) + ("\n" if selected else "")
        assert artifact.read_text() == context, key
        raw = Path(str(artifact).replace(".context.txt", ".raw.txt")).read_text()
        assert len(raw.encode()) == row["native_bytes"], key
        assert len(encoding.encode(raw, disallowed_special=())) == row["native_tokens"], key
        used = sum(len(encoding.encode(r["text"] + "\n", disallowed_special=())) for r in selected)
        assert used == row["source_tokens"] <= row["budget"], key
        for record in selected:
            if record["line"] is not None:
                actual = sources[row["repo"]].lines(record["path"])[record["line"] - 1]
                assert actual.strip() == record["text"].strip(), (key, record)
                checked_lines += 1
        for metric, value in score(cases[row["case"]], selected).items():
            assert row[metric] == value, (key, metric)
        hashes[key[:3]].add(sha(artifact.read_bytes()))
    unstable = [list(key) for key, values in hashes.items() if len(values) > 1]
    result = {"samples": len(rows), "expected_samples": expected, "complete": len(rows) == expected,
              "error_samples": errors, "snapshot_files_verified": checked_files,
              "located_lines_verified": checked_lines, "contexts_varying_across_modes_or_repeats": unstable,
              "all_token_caps_and_recomputed_scores_valid": True}
    dump(args.run / "verification.json", result)
    print(json.dumps(result, indent=2))
    return bool(errors) or len(rows) != expected


if __name__ == "__main__":
    raise SystemExit(main())
