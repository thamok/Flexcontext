#!/usr/bin/env python3
"""Jev over a coverage-preserving original-plus-expanded shortlist.

Input artifacts put the original top24 before RRF union candidates, with exact
candidate deduplication. Up to48 candidates are scored, preserving every
original shortlist candidate. Gold evidence is never used to assemble input.
"""
from pathlib import Path

import run as h
import semantic_rerank as reranker

reranker.DESIGN = {
    **reranker.DESIGN,
    "candidate_source": "original top24 followed by Qwen RRF union, exact deduplication",
    "candidate_source_byte_ceiling": 98304,
    "shortlist": 48,
    "adapter_sha256": h.sha(Path(__file__).read_bytes()),
}

if __name__ == "__main__":
    reranker.main()
