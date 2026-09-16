# Measured milestone results

Measured locally on macOS/Apple silicon with Rust 1.98.1; see [environment.json](environment.json) and the binary hashes/platforms in the scale reports. Raw evidence is checked in. Profiling timings include allocator instrumentation and are single runs, not latency guarantees. CI is configured for Linux/macOS; a Linux run has not been observed locally.

## Retrieval

60 authored queries, K=5, 12,000-byte source selection limit:

| Metric | Original engine | Current static weights |
|---|---:|---:|
| Recall@5 | 0.950 | 0.983 |
| MRR@5 | 0.712 | 0.768 |
| nDCG@5 | 0.780 | 0.833 |
| Expected relationship recall | 0.350 | 0.700 |

[Original baseline](baseline.json), [current query-level report](current.json), [fixed tuning experiment](tuning.json). Development/test split is 36/24 by module. The test subset was not used to choose weights. It achieved Recall@5 1.000, MRR 0.767 and nDCG@5 0.830. The fixtures are small authored examples, so these results do not establish production-repository retrieval quality.

Mean pretty-JSON output grew from approximately 7.03 KB to 8.30 KB because of additional cost/score fields and changed selection/relationships. Source bytes alone would hide that growth. Costs are now reported exactly for the emitted encoding; token estimates remain a bytes/4 heuristic.

## MCP context duplication

The [actual modern MCP sample](mcp-payload.json) returned 7,635 bytes for 560 selected source bytes, including full protocol framing. Duplicating that identical structured payload into text would produce 15,723 bytes: removing duplication saves 8,088 bytes (51.4%). This is a controlled same-payload comparison, not a claim that every query halves its context. [Resident round-trip measurements](mcp-resident.json) use the modern 2026-07-28 protocol.

## Memory

10 MB dense TypeScript fixture, four Rayon workers:

| Measurement | Before | After |
|---|---:|---:|
| Cold index peak live Rust allocations | 54.34 MB | 20.02 MB |
| Cache load peak live Rust allocations | 55.11 MB | 20.41 MB |
| Prepared index peak live Rust allocations | 58.02 MB | 48.13 MB |
| Resident stage RSS (after earlier phases) | 188.83 MB | 95.72 MB |
| Cache JSON size | 24.88 MB | 15.00 MB |

A separate fresh resident process now uses **65.34 MB RSS** for this fixture. The before/after resident-stage row includes allocator state from earlier profile phases; it must not be mistaken for a fresh-process comparison.

Cold indexing allocation calls changed from 11,980,522 to 11,451,200. Cumulative allocated bytes changed from 685.68 MB to 738.70 MB: lower peak memory does **not** imply lower allocation traffic in every phase. Prepared-index construction still allocates substantial normalized/token data. A query on the 10 MB fixture allocates approximately 5.32 MB across 26,892 Rust allocation calls; on 100 MB, approximately 30.14 MB across 89,238 calls. These are measured remaining costs, not optimized-away work.

[Before scaling profile](scaling-before.json), [after scaling profile including fresh resident processes](scaling-after.json), and small mixed-fixture [before](memory-before.json)/[after](memory-after.json) profiles. Rust counters exclude native Tree-sitter allocations; RSS includes native allocations. Process peak RSS is lifetime high water, while the Rust allocation peak resets for each phase.

## Scale bounds

| Eligible source size | Exercise | Profile/guard wall time | Fresh resident idle RSS |
|---|---|---:|---:|
| 10 MB | Full profile | 2.05 s | 65.34 MB |
| 100 MB | Full profile | 16.21 s | 596.59 MB |
| 1 GB | Discovery guard only | 0.23 s | Not indexed |
| 10 GB | Discovery guard only | 0.01 s | Not indexed |

The 1 GB and 10 GB sparse-file probes both stop at 268,544,000 eligible bytes / 1,049 scanned files, just past the 256 MiB cap. They validate bounded refusal only. Full indexing at 1 GB or 10 GB remains unmeasured; raising limits needs a separately resource-bounded run on representative source.

## Validation

30 tests passed, including all 60 corpus queries, hand-calculated metrics, exact emitted-byte budgets, modern protocol lifecycle/errors/cancellation, shared cached source ranges, same-size/mtime replacement, invalid cache ranges/fingerprints, binary transitions, nested AST limits, and scan bounds. Formatting, strict Clippy, release binaries and all eight Criterion smoke cases pass locally. CI will repeat the checks on Linux and macOS.
