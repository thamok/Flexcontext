#!/usr/bin/env python3
"""JSONL bridge to the real Aider RepoMap, without starting a model or coder."""
import contextlib
import json
import re
import sys
from pathlib import Path

import tiktoken
from aider.io import InputOutput
from aider.repomap import RepoMap


class TokenModel:
    def __init__(self, encoding):
        self.encoding = tiktoken.get_encoding(encoding)

    def token_count(self, text):
        return len(self.encoding.encode(text, disallowed_special=()))


def main():
    root = Path(sys.argv[1]).resolve()
    files = [str(root / p) for p in json.loads(Path(sys.argv[2]).read_text())]
    with contextlib.redirect_stdout(sys.stderr):
        mapper = RepoMap(root=str(root), main_model=TokenModel(sys.argv[3]),
                         io=InputOutput(yes=True, pretty=False), map_mul_no_files=1,
                         refresh="always")
    for line in sys.stdin:
        request = json.loads(line)
        if request.get("method") == "ready":
            print(json.dumps({"ready": True}), flush=True)
            continue
        mapper.max_map_tokens = request["budget"]
        # Mention hints come only from the common query, never relevance labels.
        with contextlib.redirect_stdout(sys.stderr):
            result = mapper.get_repo_map([], files, mentioned_fnames=set(),
                                         mentioned_idents=set(request["terms"])) or ""
        print(json.dumps({"map": result}), flush=True)


if __name__ == "__main__":
    main()
