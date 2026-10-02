#!/usr/bin/env python3
"""Run the same frozen Jev rubric on Qwen's fused candidate artifacts.

Prepare MCP-shaped candidate artifacts from semantic_expand's fused results,
then invoke this runner with semantic_rerank's arguments. No prompt or rubric
changes are made after observing expansion quality.
"""
from pathlib import Path

import run as h
import semantic_rerank as reranker

reranker.DESIGN = {
    **reranker.DESIGN,
    "candidate_source": "Qwen original-plus-two-query stable RRF union",
    "candidate_source_byte_ceiling": 98304,
    "adapter_sha256": h.sha(Path(__file__).read_bytes()),
}

if __name__ == "__main__":
    reranker.main()
