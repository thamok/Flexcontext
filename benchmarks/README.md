# Retrieval and context-cost evaluation

Run `cargo run --release --bin flexcontext-eval -- --output benchmarks/results/current.json`.
The equivalent subcommand is `flexcontext benchmark benchmarks --max-bytes 12000`.
Use `--k 1`, `--k 5`, or `--k 10` to change the cutoff. The standalone evaluator defaults to K=5 and a 12,000-byte selection budget.

The four versioned JSON files contain **60 authored service-maintenance queries** (15 per suite). Fixtures cover authentication, cache expiry, retries, configuration, persistence, HTTP requests, pagination, sessions, uploads, webhook idempotency, jobs, CSV imports, rate limits, path validation, orders, inventory and warehouse export. These are checked-in, small synthetic repositories, not an evaluation of arbitrary production repositories or autonomous coding success.

Each judgment is qualified by repository/fixture, file, symbol and kind. Strong relevance is grade 2; partial relevance is grade 1; plausible distractors and all unjudged results are grade 0. Distractors often share domain vocabulary but implement presentation or reporting instead of the queried behavior. The mixed repository also contains overlapping domain names in API, web and worker code. The evaluator rejects missing symbols, overlapping labels, duplicate query IDs and unresolved relationship endpoints. It never derives judgments from current rankings.

Metrics:

- Recall@K: unique strongly relevant symbols returned / all strongly relevant symbols.
- MRR@K: reciprocal rank of the first strongly relevant result; zero on a miss.
- nDCG@K: gain `2^grade - 1`, logarithmic rank discount, ideal list includes all graded symbols even when fewer than K results are returned.
- Relationship recall: fraction of explicitly expected directed edges present on returned source units. A missed source unit counts as a missed edge. This is output coverage, not compiler-semantic correctness.
- Distractor count@K, selected source bytes, complete pretty-JSON bytes (including trailing newline), and estimated tokens (`ceil(serialized_bytes / 4)`). Timing digit widths and absolute root paths can slightly change byte counts across machines.

The first three modules of each suite are development data (36 queries); the remaining two are held out (24 queries). Queries from the same module stay in the same split. The small size, similar domain vocabulary and intentionally simple fixtures limit generalization. Broader real-repository judgments should precede further ranking changes. All judgments should be reviewed as code changes, including distractors and expected edges.

## Baselines and tuning

`results/baseline.json` records the pre-refactor engine before cost and extraction changes. Keep it immutable. `results/tuning.json` records the fixed tuning experiment; `tuned-weights.json` contains the selected static production weights. No runtime training or embeddings are used.

Reproduce tuning with:

```console
cargo run --release --bin flexcontext-eval -- --tune --output /tmp/tuning.json
cargo run --release --bin flexcontext-eval -- --weights benchmarks/tuned-weights.json
```

Two coordinate passes try factors 0.5 and 1.5 from the original weights, in a fixed field order (29 configurations including the anchor). Selection uses development nDCG, then development MRR as a tie-breaker; held-out results do not select weights. File-import weight stays zero because those edges only describe a file neighborhood. Exact and normalized-name weights participate, although this initial intent-heavy corpus provides limited evidence for changing them. Tuning emits a proposal and never rewrites production defaults. The selected weights in this change were explicitly frozen after reviewing the held-out report.

The integration regression gate compares dev/test Recall, MRR, nDCG and relationship coverage against the original baseline. Metric formula tests use hand-calculated rankings including misses and partial credit. The full query-level ranking is retained in every report to inspect regressions concealed by means.

The modern MCP framing/duplication experiment is reproducible with `python3 scripts/bench_payload.py benchmarks/fixtures/typescript`.

## Memory and scaling

```console
cargo run --release --bin flexcontext-profile -- benchmarks/fixtures/mixed-monorepo
python3 scripts/bench_scaling.py --resident-probe --output benchmarks/results/scaling-after.json
```

The profile binary is the only binary with a counting global allocator. Stages cover discovery, cold indexing, cache population, cache loading, prepared lexical index, relationship index, resident open, idle, and repeated queries. Counters show Rust allocator calls, cumulative allocated bytes, current live allocations, and per-stage peak live allocations. Native Tree-sitter allocations are excluded from these counters. Process RSS includes native allocations; `getrusage` peak RSS is a **process-lifetime high-water mark**, not a resettable stage peak. Idle means immediately after resident construction, not a long soak. Use `flexcontext-profile ROOT --resident-only` for a fresh process, or `--resident-probe` in the scaling harness to record it separately; the full-stage process retains allocator state from earlier phases. The query phase measures the library query including canonical JSON accounting; additional MCP transport serialization is not part of that allocation counter. Allocator instrumentation adds overhead and its timings are not latency benchmarks.

The default scaling harness isolates each case in a temporary directory, limits Rayon to four workers and times out after 300 seconds. Decimal 10 MB and 100 MB use dense synthetic TypeScript and exercise the full pipeline. The 1 GB and 10 GB cases use sparse files solely to test discovery's total-source-byte guard. They deliberately stop before parsing; **they establish bounded refusal, not indexing performance at those sizes**. No 1 GB/10 GB full indexing claim is made. Profile representative production repositories under explicit resource limits before raising the default 256 MiB cap.
