#!/usr/bin/env python3
"""Convert Qwen expansion artifacts into native-shaped reranking inputs."""
import argparse
import json
from pathlib import Path


def prepare(source, output, preserve):
    output.mkdir(parents=True, exist_ok=True)
    for path in sorted(source.glob("*-fused.json")):
        fused = json.loads(path.read_text())["results"]
        hits = fused
        if preserve:
            native = json.loads(path.with_name(path.name.replace("-fused.json", "-q0.json")).read_text())["result"]["structuredContent"]["results"]
            seen, hits = set(), []
            for hit in native[:24] + fused:
                key = (hit["path"], hit["start_line"], hit["end_line"], hit["content"])
                if key not in seen:
                    hits.append(hit)
                    seen.add(key)
        result = {"result": {"structuredContent": {"results": hits}}}
        (output / path.name.replace("-fused.json", "-candidate.json")).write_text(json.dumps(result, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--preserve", action="store_true")
    args = parser.parse_args()
    prepare(args.source, args.output, args.preserve)
