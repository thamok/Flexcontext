#!/usr/bin/env python3
"""Ablation: identical Jev scoring over original top48, no query model."""
from pathlib import Path

import run as h
import semantic_rerank as reranker

reranker.DESIGN = {
    **reranker.DESIGN,
    "shortlist": 48,
    "adapter_sha256": h.sha(Path(__file__).read_bytes()),
}

if __name__ == "__main__":
    reranker.main()
