# Retrieval and language expansion, 2026-10-01

This pass adds Apex, Go, C, C#, C++, Objective-C, Metal, CUDA, Kotlin, Dart, Vue and Lua extraction, and improves React JSX/TSX components and hooks. It retains the baseline policy and ranking weights. The earlier experimental focused policy remains opt-in.

The implementation caches complete statement ranges during full-file extraction, so default budgeted excerpts do not create a Tree-sitter parser per result/query. This preserves class-method syntax, original UTF-8 offsets and late decisive statements. Short containers retain their declarations when their members are candidates, allowing precise method results instead of suppressing them through whole-container overlap. Visible signatures have source spans, and nested excerpts cannot duplicate selected source. Local arrow bindings retain declaration ranking and broad-query suppression.

Language coverage and adapter limitations are documented in [the main README](../../README.md). Vue uses an HTML shell with inline JS/TS/JSX/TSX parsing and searchable template units. Metal uses a C++ dialect adapter that masks reserved qualifiers in parser input while retaining original source. C++/Objective-C header detection distinguishes guarded Objective-C forward declarations from actual interfaces. Relations reject noncallable targets and incompatible language families; they remain syntactic evidence rather than compiler resolution.

## Synthetic regression results

The original 60-query corpus is unchanged. Every query retains identical returned symbol order, Recall@5, reciprocal rank, nDCG@5, relationship coverage and selected source bytes against the dirty checkout captured at the start of this pass.

| Original 60 queries, 12,000 source bytes | Before | After |
|---|---:|---:|
| Recall@5 | 98.33% | 98.33% |
| MRR@5 | 0.7681 | 0.7681 |
| nDCG@5 | 0.8334 | 0.8334 |
| Relationship coverage | 70.00% | 70.00% |
| Mean selected source bytes | 527.7 | 527.7 |

The new 43-query multilingual corpus was authored with explicit symbol judgments, partial relevance, vocabulary distractors and resolvable relationships before its first retrieval run. Fixtures stay in one split each. This is a small synthetic regression suite with 14 exact-name queries; it does not establish general production-language quality. Its before column measures the expanded extractor before container/member selection changes, rather than treating unsupported languages in the original binary as retrieval misses.

| Expanded 43 queries, 12,000 source bytes | Before selection optimization | Final |
|---|---:|---:|
| Recall@5 | 79.07% | 95.35% |
| MRR@5 | 0.6453 | 0.7229 |
| nDCG@5 | 0.6625 | 0.7629 |
| Mean selected source bytes | 720.1 | 568.9 |
| Mean full JSON estimated tokens, bytes/4 | 2234.6 | 2238.4 |

Selected source bytes fall **21.0%**; full JSON token estimates rise approximately **0.2%**. Source savings and total response cost are separate measurements. The 2,048-source-byte run has the same final quality metrics and mean selected source bytes, because these fixtures are small. Oversized-method tests separately exercise late behavior under 1k/2k byte budgets across every requested language/frontend.

Two authored queries still miss their strongly relevant symbols at K=5: `multilingual-02-2` (expiry/current-time wording) and `multilingual-12-1` (job retry behavior displaced by other retry implementations). These remain explicit failures in the reports. No synonyms, embeddings or scoring-weight changes were added to fit them.

Machine-readable synthetic reports are in [results/iteration-20261001](../results/iteration-20261001/). The before-selection report normalizes earlier non-finite relationship values for cases with no expected edges to zero, matching the corrected evaluator; rankings and judgments are preserved.

## Frozen real-repository gate

The paired comparison uses the same 36 previously frozen questions and snapshots for Flexcontext, AgentX and WebKit Python tooling. Both binaries use baseline policy, compact output, the same queries, and the pilot's shared tokenizer/normalization at 2k/4k source-token budgets. The candidate must preserve mean decisive-statement evidence recall in every repository/split/budget group. Correct-file discovery alone does not earn credit.

The first candidate failed the AgentX development gate: a promoted local `release` arrow binding displaced the concurrency limiter containing capacity validation and queue behavior. It was rejected. Preserving local declaration ranking corrected this regression. The accepted 20-repeat paired run passes all 12 repository/split/budget groups, with **no evidence-recall regression in any individual question/budget pair**. All development groups retain their baseline recall.

| Held-out repository | Source-token budget | Evidence recall before → final | Resident p95 before → final, ms |
|---|---:|---:|---:|
| AgentX | 2,048 | 16.67% → 50.00% | 539.47 → 332.49 |
| AgentX | 4,096 | 33.33% → 66.67% | 554.98 → 330.43 |
| Flexcontext | 2,048 | 66.67% → 83.33% | 9.48 → 1.95 |
| Flexcontext | 4,096 | 100.00% → 100.00% | 8.37 → 2.36 |
| WebKit Python tooling | 2,048 | 66.67% → 66.67% | 420.87 → 330.85 |
| WebKit Python tooling | 4,096 | 66.67% → 66.67% | 417.82 → 337.30 |

Each held-out group contains six questions. After the paired acceptance run, a final performance-only change removed repeated overlap scans and repeated newline-prefix scans during excerpt construction. The final binary returns exactly the same result objects, content, source spans and order for **all 72 question/budget pairs** against the accepted candidate. Its timings were collected subsequently, with 20 repetitions per question after warmup; the table reuses baseline timings from the earlier paired run and does not represent a newly interleaved baseline/final experiment. Each timing group has 120 samples. Source evidence and quality scores therefore carry forward by exact context equality.

The complete rows, group summaries, raw timing samples and provenance contracts are saved in [real-repositories.json](../results/iteration-20261001/real-repositories.json). Source-token budgets and synthetic source-byte budgets are distinct. Compact JSON payload sizes are also reported separately: these changes improve source selection but do not establish lower total protocol cost across the real repositories.

## Large-method scaling

The final change checks only the latest enclosing range in ordered AST statements and builds one line-start index per excerpt. Relevance is scored once per candidate. This removes quadratic overlap and line-number work while retaining the existing relevance ordering; sorting remains O(n log n).

| Synthetic TypeScript padding statements | Resident median before → after, ms | Speedup |
|---|---:|---:|
| 1,000 | 3.785 → 0.485 | 7.8× |
| 5,000 | 85.477 → 2.459 | 34.8× |
| 10,000 | 335.114 → 5.028 | 66.7× |

This compares the accepted candidate before the final loop optimization against the final binary, with 20 paired resident repetitions, a 1,024-source-byte excerpt limit and a decisive guard late in the method. All returned result objects and spans are identical, including the Unicode guard. It measures one deliberately oversized method, not an overall repository speedup. Raw samples and binary hashes are in [excerpt-scaling.json](../results/iteration-20261001/excerpt-scaling.json).

## Verification and scope

- `cargo test --all-targets`: 53 Rust tests and eight Criterion smoke checks pass.
- `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all -- --check`, and `git diff --check` pass.
- Python comparison harness: ten tests pass.
- All requested grammars load and extract behavior; tests cover Vue script coordinates, React variable bindings, full Objective-C selectors/header signatures, Go receivers, Lua assignments, C++ qualified methods, persistent excerpt cache reuse and conservative relations.
- An additional C++ smoke check indexes 102 files / 812,251 source bytes in `WebKit/Source/WTF/wtf/text`, extracting 5,430 units and returning the `StringImpl` class plus UTF-8 methods. This is a scoped indexing/search smoke check; it has no independently authored recall judgments and is separate from the frozen WebKit Python comparison.
- Fresh release binaries are built. Existing resident servers must restart to load the changed binary; `refresh_index` updates their source snapshot but does not replace their running executable.

Reproduce the synthetic suite:

```sh
target/release/flexcontext-eval --output /tmp/original.json
target/release/flexcontext-eval --suite multilingual --output /tmp/multilingual.json
target/release/flexcontext-eval --suite multilingual --max-bytes 2048 --output /tmp/multilingual-2k.json
```

Reproduce a paired run with the binaries saved by this iteration and a fresh output directory:

```sh
.benchmark-venv/bin/python benchmarks/comparison/iteration.py \
  --baseline .benchmark-results/iteration-20261001-accepted/flexcontext-baseline \
  --candidate .benchmark-results/iteration-20261001-accepted/flexcontext-candidate \
  --snapshot-root .benchmark-results/optimization-focused-20260916/snapshots \
  --output .benchmark-results/iteration-repeat --repeats 20
```

Contracts retain binary hashes, question hashes, source-file hashes, fixed budgets and RNG seed. Raw responses, selected lines, startup timings and resident timing samples remain in the ignored local experiment directory. Timing repetitions run serially after warming distinct queries; startup is separate and the OS page cache is uncontrolled. This pass measures retrieval and latency, not autonomous coding-task success or global optimality.

Reproduce final context equivalence and timing against the accepted paired run:

```sh
.benchmark-venv/bin/python benchmarks/comparison/verify_equivalent.py \
  --reference .benchmark-results/iteration-20261001-accepted \
  --candidate .benchmark-results/iteration-20261001-linear-final/flexcontext-final \
  --output .benchmark-results/equivalence-repeat --repeats 20

.benchmark-venv/bin/python benchmarks/comparison/excerpt_scaling.py \
  --before .benchmark-results/iteration-20261001-accepted/flexcontext-candidate \
  --after .benchmark-results/iteration-20261001-linear-final/flexcontext-final \
  --output .benchmark-results/excerpt-repeat --repeats 20
```

Final release SHA-256: `56d2ecd636fd7594ca5910706d9eced6fb66277b4efd6f37ec545d9f89936ed7`.
