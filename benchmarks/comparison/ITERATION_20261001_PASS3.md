# Competitor confirmation, optional semantic retrieval, and index optimization

This pass keeps native retrieval deterministic, reduces repeated posting-union
work, and adds an explicit opt-in Jev reranking prototype. The semantic policy
was selected on development questions and then evaluated once on nine newly
authored questions from unused modules. The ordinary Rust CLI/MCP does not call
an inference service.

## Fresh confirmation

A separate agent authored and source-reviewed nine questions with 27 evidence
atoms, three questions each from Flexcontext, AgentX and WebKit Python tooling.
The questions, required source spans and hashes were frozen before retrieval.
They do not overlap the earlier corpus's module families. They are not human
adjudicated or a random sample of repository maintenance work.

The semantic design and executable were frozen before the new comparator
results were inspected. All tools used the same source snapshots, question
terms and common `cl100k_base` source cap. CodeGraph's native `explore` and
`context` commands are separate adapters. Actual visible source is required for
evidence credit; file pointers receive no body-evidence credit.

| Workflow | Evidence recall, 2,048 source tokens | Evidence recall, 4,096 |
|---|---:|---:|
| Native Flexcontext | 59.26% | 70.37% |
| Flexcontext + Jev top-48 | **70.37%** | **70.37%** |
| Probe | 48.15% | 48.15% |
| CodeGraph explore | 66.67% | 66.67% |
| CodeGraph context | 48.15% | 48.15% |

Jev improves one question at 2k and preserves the other 17 question/budget
results. There is no recall loss in any repository/budget group. On development,
it also improves exactly one of 36 pairs without a loss. These small counts
support an opt-in experiment; they do not establish statistical superiority or
broad task-completion non-inferiority.

The 18 historical held-out questions give a different competitor ordering:
native Flexcontext scores 66.67% / 77.78%, Probe 69.44% / 69.44%, CodeGraph
explore 27.78% / 33.33%, and CodeGraph context 16.67% / 16.67%. Both partitions
are retained in [the competitor report](CODEGRAPH_20261001.md). The earlier
partition has informed previous analysis and is not newly blind.

## Semantic policy and rejected alternatives

The accepted experimental recipe takes the original native 4k-budget candidate
response, asks pinned `jev-1.13.0` one independent relevance judgment per
candidate for at most 48 candidates, sorts by score with stable original-rank
ties, and retains the untouched tail. The common output cap then selects only
visible source. Gold annotations and answer notes never enter model requests;
candidates include their native paths. Cached requests are content-addressed;
remote use is explicit.

Nine confirmation requests took approximately 0.33–0.42 seconds each at the API
boundary. This is additional inference time, not total CLI or resident search
latency. Input, output, retrieved-source and returned-source consumption are
reported separately in [the semantic report](SEMANTIC_20261001.md).

The experiment retained negative results:

- Original top-24 Jev reranking changed ordering but did not improve recall.
- Qwen generated two additional queries, combined with the original through
  reciprocal-rank fusion. It improved ordering for one question but displaced
  previously correct results, failing the no-regression gate.
- Jev over that fusion's top 24 did not repair omitted-candidate coverage.
- A larger union preserved coverage and passed development. An ablation then
  achieved the same recall with the original top 48 alone, so the extra Qwen
  calls and searches were not retained in the selected recipe.
- A fixed BM25 index regressed on every development repository/budget group.
  It remains only as a reproducible research worker, not a production policy.
  Its contract records document construction, term frequency, IDF, length
  normalization, declaration exclusion and lack of relationship score boosts.

## Native index change

`LexicalIndex::candidates` now visits each distinct posting key once, unions IDs
in a hash table, and sorts the unique IDs. This retains the previous ascending
tree-union order. The hash table replaces repeated tree lookups;
deduplicating before sorting keeps temporary candidate storage proportional to
distinct IDs rather than the sum of overlapping posting-list lengths.

A reference tree-union test covers overlapping aliases, duplicated terms,
missing terms, empty queries, sparse IDs and queries longer than 64 terms. All
72 historical and all 18 fresh confirmation selected-result arrays are exactly
identical to the accepted pass-2 binary: symbols, order, source text and spans.
The final binary also preserves all per-query retrieval metrics, symbol order
and source bytes in the original 60-query, 43-query multilingual and nine-query
Java suites. Full diagnostic JSON sizes can vary with timing counters.

New paired measurements used serial randomized calls, query warmup, 20 repeats,
and 120 samples per repository/split/budget/variant. Startup is separate, OS
cache is uncontrolled, and other host work was not fully isolated. These are
small descriptive improvements, with no claim that every query becomes faster.

| Historical held-out repository | Source tokens | Resident p95 before | After |
|---|---:|---:|---:|
| AgentX | 2,048 | 15.65 ms | 15.47 ms |
| AgentX | 4,096 | 19.16 ms | 18.74 ms |
| Flexcontext | 2,048 | 1.01 ms | 1.04 ms |
| Flexcontext | 4,096 | 1.36 ms | 1.39 ms |
| WebKit Python tooling | 2,048 | 16.67 ms | 16.47 ms |
| WebKit Python tooling | 4,096 | 17.28 ms | 16.69 ms |

## Agent completion

The [agent harness](../agents/README.md) now supports a direct OpenAI-compatible
tool loop, Probe, explicit first-search control, uniform source-only charging,
independent final public regressions, hidden acceptance, protected files and
payload-hash trace auditing. It pins Qwen on the documented Hetzner endpoint.
The three assistant-authored seeded repairs remain smoke tasks; repeated
attempts do not turn them into representative full-repository issues.

An initial preflight was stopped after audit found asymmetric ripgrep metadata
charging and missing filenames on single-file searches. That preflight is
retained separately. The corrected frozen repeated sweep and its final
acceptance results are documented in the agent report. All 27 scheduled
attempts completed with valid traces, within the limits, and passed both the
frozen hidden checks and final public regressions. There were no provider
errors, timeouts or unknown-usage trials in this cohort.

| Search workflow | Frozen-suite passes | Mean agent seconds | Mean source/diagnostic tokens | Mean broker-output tokens | Mean provider tokens |
|---|---:|---:|---:|---:|---:|
| ripgrep | 9/9 | 63.96 | 1,794 | 2,318 | 28,056 |
| Native Flexcontext | 9/9 | 67.41 | 1,865 | 3,065 | 34,410 |
| Probe | 9/9 | 70.69 | 1,964 | 2,480 | 30,472 |

Provider totals include input and output across all model turns. The cohort
used 836,444 provider tokens; excluded calibration/preflight/interrupted work
used at least another 236,644, with incomplete usage for in-flight requests.
All paired frozen outcomes are ties. This tiny, three-task experiment supports
neither a completion advantage nor a broad efficiency claim. Flexcontext did
not minimize model consumption. Jev was not an agent-evaluation condition.

The agent used the accepted pass-2 binary. An independent offline replay of all
11 searches across its nine Flexcontext trials recreated preceding edits and
verified exact selected-result equality with the final release binary. Every
baseline replay also matched the retained delivered result. All 27 final
patches were independently reviewed; changes were confined to the intended
boundary/FIFO expressions, with protected inventory files unchanged.

Review of all nine lexical patches found a separate acceptance gap:
every repair handles only ASCII digits. They pass the frozen tests but fail a
post-hoc Unicode-decimal audit, while the production reference passes it.
`unicode_numeric_audit.py` retains this supplementary test separately; frozen
scores must not be presented as proof of complete Unicode behavior.

A dummy-only privacy test also established that the current macOS sandbox can
read another same-user process's startup environment via `KERN_PROCARGS2`, even
with narrower profile rules. A clean-startup-environment worker that loads its
key into Python memory after launch prevents that specific credential read;
the broader host-process limitation remains. The benchmark is restricted to
trusted, authored fixtures and does not claim containment of hostile code.
No real credentials or unrelated process environments were inspected by the
test. Completed source patches were independently reviewed for unrelated
process, environment or network behavior before continuation.

The host's selected Xcode changed during the final Java trials. A later harness
self-test therefore could not compile its Rust reference under the old runtime
allowlist. Future test subprocesses now explicitly select the installed Command
Line Tools; the global Xcode selection is untouched. The active benchmark kept
its frozen, already-loaded harness, and its Java trials were unaffected.

## Evidence and reproduction

Sanitized numeric results and binary/corpus hashes are in
[iteration-20261001-pass3](../results/iteration-20261001-pass3/). Native source,
API response caches, disposable task workspaces and detailed traces remain in
ignored `.benchmark-results` directories. Existing dirty repository work was
preserved, and no credentials were written to result files.

```sh
cargo build --release --locked --bins
cargo build --release --example bm25_experiment
.benchmark-venv/bin/python benchmarks/comparison/local_index.py \
  --split dev --output .benchmark-results/bm25-NEW

.benchmark-venv/bin/python benchmarks/comparison/iteration.py \
  --baseline .benchmark-results/iteration-20261001-pass2-accepted/flexcontext-candidate \
  --candidate target/release/flexcontext \
  --snapshot-root .benchmark-results/optimization-focused-20260916/snapshots \
  --output .benchmark-results/posting-union-NEW --repeats 20
```

The BM25 experiment intentionally does not run on the fresh confirmation set:
it failed development. Its saved worker reproduces all 72 original development
quality rows exactly after removal from production.

Final native verification passed 58 Rust tests, eight Criterion smoke checks,
formatting, Clippy with warnings denied, and a locked release build. All 28
retrieval-harness tests and all 16 agent-harness tests passed. A separate uncached Jev CLI smoke on the current
checkout returned 2,047 source tokens under a 2,048 cap, with 555 ms observed API
time and 710 ms total command time. It used 17,281 provider input tokens and 762
output tokens. This is one integration check, not a latency benchmark; its usage
is additional to the experimental totals in the semantic report. Unlocated
synthetic omission markers are explicitly reported through source alignment
metadata and do not receive evidence credit.
