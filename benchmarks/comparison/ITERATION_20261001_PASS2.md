# Java, exact context-selection optimization, and agent pilot

This pass adds Java and accepts a performance optimization that preserves retrieval behavior. The first iteration's language support, ranking weights and default baseline policy remain in place. The experimental focused policy stays opt-in.

## Java

`.java` files use [tree-sitter-java](https://github.com/tree-sitter/tree-sitter-java), resolved at 0.23.5. Extraction covers packages/modules, classes, records, interfaces, annotation declarations/elements, enums, fields, methods and ordinary/compact constructors. Tests cover nested ownership, method calls/relations, original source ranges and late statements in oversized methods.

The separately frozen nine-query Java suite has six behavioral queries and three exact-name queries, grouped by fixture across development/test splits. Final Recall@5 is **100%**, MRR@5 **0.7037**, nDCG@5 **0.6947**, and mean selected source **729 bytes**. It is a small synthetic regression suite, not production Java quality evidence. The two queries with expected call relations both recover them; queries without expected edges contribute zero to the all-query relationship mean.

## Optimization and acceptance

Profiling identified exhaustive greedy context selection as the dominant stage on large candidate pools: 270–355 ms in representative AgentX queries compared with 5–30 ms in ranking. After many quota/overlap/budget rejections, the old selector repeatedly recomputed maximum utility over all remaining candidates.

The default selector now keeps candidates in a lazy priority queue. Successful selections only increase diversity counts, so stored utilities are upper bounds. It recalculates stale entries at the front until the maximum is current, retaining the exact penalty arithmetic and original-rank tie breaking. Rejected candidates need no full-pool rescan. Low-level callers with invalid/non-monotone diversity weights retain exhaustive ordering. An exhaustive oracle test covers repeated paths, ties, skipped candidates, zero/large penalties and negative-weight fallback.

Both saved binaries include Java; their only retrieval difference is the selector optimization. On the existing 36 frozen real-repository questions at 2k/4k source-token budgets, **all 72 result objects are exactly identical**: content, symbols, ordering and source spans. All individual and 12 group evidence-recall gates pass. A separate check against the previous iteration's accepted candidate also preserves all 72 contexts. The original 60-query corpus, 43 multilingual queries and nine Java queries retain their retrieval metrics and selected source bytes.

| Held-out repository | Source-token budget | Paired resident p95 before → after, ms | Evidence recall before / after |
|---|---:|---:|---:|
| AgentX | 2,048 | 329.18 → **15.55** | 50.00% / 50.00% |
| AgentX | 4,096 | 329.41 → **19.06** | 66.67% / 66.67% |
| Flexcontext | 2,048 | 1.97 → **1.04** | 83.33% / 83.33% |
| Flexcontext | 4,096 | 2.32 → **1.31** | 100.00% / 100.00% |
| WebKit Python tooling | 2,048 | 319.85 → **16.80** | 66.67% / 66.67% |
| WebKit Python tooling | 4,096 | 332.56 → **17.05** | 66.67% / 66.67% |

These are newly paired, serial, randomized baseline/candidate calls after warming each query, with 20 repetitions and 120 samples per repository/split/budget/variant. Startup is separate, OS cache is uncontrolled, and all source snapshots/questions are unchanged. The largest held-out p95 reduction is approximately 21.2×. This is default selection latency improvement rather than a new relevance-scoring policy or proof of globally optimal retrieval.

Machine-readable results, contracts and samples are in [results/iteration-20261001-pass2](../results/iteration-20261001-pass2/). Detailed responses and executable copies remain in `.benchmark-results/iteration-20261001-pass2-accepted`. The interrupted preliminary performance run was discarded before acceptance measurements.

## Agent completion

The [agent pilot and next-stage design](../agents/README.md) implement controlled discovery/read/edit/test tools, cumulative context accounting, disposable task copies, protected-file checks and independent acceptance grading. Three real CLI agents ran the seeded AgentX FIFO repair once each under ripgrep, prior Flexcontext and candidate Flexcontext conditions. **All three completed the task and passed tool audits.** This verifies the harness, not a comparative success-rate advantage. The small fixture, single repetition, initial provider-label difference and uncontrolled model caching rule out causal timing claims.

The proposed next experiment freezes real maintenance tasks and full testable repository snapshots, keeps the model/workflow limits fixed, splits held-out issues by module family, and measures completed patches before token/time efficiency. It is documented but has not been launched as a broad sweep.

## Reproduction and verification

```sh
target/release/flexcontext-eval --suite java --output /tmp/java.json

.benchmark-venv/bin/python benchmarks/comparison/iteration.py \
  --baseline .benchmark-results/iteration-20261001-pass2-accepted/flexcontext-baseline \
  --candidate .benchmark-results/iteration-20261001-pass2-accepted/flexcontext-candidate \
  --snapshot-root .benchmark-results/optimization-focused-20260916/snapshots \
  --output .benchmark-results/pass2-repeat --repeats 20

.benchmark-venv/bin/python -m unittest discover -s benchmarks/agents -p test_pilot.py
```

All **56 Rust tests**, **eight Criterion smoke checks**, **ten comparison-harness tests**, and **seven agent-pilot tests** pass. Clippy, formatting and diff checks pass. The Java fixtures also compile with the local Java 21 compiler. The release binary is rebuilt and its hash matches the accepted paired candidate; existing resident MCP servers need a restart to load it.
