# Four-tool retrieval evaluation

This harness compares the **actual Flexcontext binary, Probe binary, ripgrep, and Aider's `RepoMap` class** against the same immutable source snapshots and authored questions. Included questions and evidence labels are assistant-authored, source-reviewed and frozen before retrieval runs; no model judges returned results. The existing synthetic Flexcontext regression benchmark remains separate.

The focused-retrieval experiment is documented in [OPTIMIZATION.md](OPTIMIZATION.md), with [measured results and the acceptance decision](OPTIMIZATION_RESULTS.md) and a [machine-readable summary](optimization-summary.json). The focused policy failed the recall/noise gates and remains opt-in. Compact baseline output preserves source exactly and reduces non-source payload tokens by 83.8% on the held-out pairs.

Adapter references: [Probe's CLI and query syntax](https://github.com/probelabs/probe), [Aider's repository-map design](https://aider.chat/docs/repomap.html), and the locally installed CLI help. The Aider adapter calls the installed 0.86.2 implementation rather than recreating its ranking algorithm.

## Run

From the Flexcontext repository:

```sh
sh benchmarks/comparison/setup.sh
.benchmark-venv/bin/python benchmarks/comparison/run.py \
  --budgets 2048 4096 --repeats 2 \
  --output .benchmark-results/my-run
```

The output directory must be new. Use `--only-repos flexcontext` for a smoke run, `--tools flexcontext probe rg aider` to select adapters, or `--repos another-config.json --cases another-corpus.json` for another corpus. Commands run serially to avoid competing CPU loads. All requests have timeouts. Errors remain in `rows.jsonl`, are counted separately from retrieval misses, and make the process exit nonzero.

Dependencies live in ignored `.benchmark-*` directories. `requirements.lock` pins the Python environment; `overrides.txt` documents the SciPy 1.16.3 override required because the Aider-pinned 1.15.3 macOS wheel fails dyld validation on the initial machine. Aider's source is unmodified. Probe is pinned to npm release `0.6.0-rc339`, and measurements call its native binary, bypassing Node startup. The setup script downloads dependencies; the retrieval run makes no model calls.

## Pilot corpus and scope

`repos.json` defines explicit directory scopes within local repositories. Its default paths expect sibling AgentX and WebKit checkouts; pass `--repos another-config.json` with your own local paths, or use `--only-repos flexcontext` to evaluate this repository alone:

| Case group | Snapshot scope | Questions |
|---|---|---:|
| Flexcontext | `src`, `tests`, `benches`, synthetic source fixtures | 6 |
| AgentX | `apps`, `lib`, `tools` | 6 |
| WebKit Python tooling | `Tools/Scripts/webkitpy` | 3 |

WebKit's entire local checkout is about 18 GB. **The pilot is not a full-WebKit or C++ engine benchmark.** Read-app was replaced with Flexcontext following the user's updated scope. The original pilot preceded C++ support; the October language expansion adds C++ and other grammars without changing these frozen Python snapshots or judgments. Swift remains unsupported. Language-coverage studies should include new language repositories explicitly and report unsupported targets separately from supported-language retrieval quality.

Snapshots use Git-tracked plus nonignored untracked source files, including current uncommitted edits, with a shared extension allowlist and 2 MiB per-file ceiling. Symlinks, binary, and non-UTF-8 files are omitted. No working repository is changed. Each manifest records the original HEAD, dirty state, paths, bytes, extension counts, per-file SHA-256 and aggregate SHA-256. Source-only snapshots deliberately omit Git metadata, original ignore rules, docs, credentials/config files, and build artifacts outside the declared scopes. Each engine can still apply built-in language/directory exclusions; inspect native statistics when interpreting coverage.

The 15 questions are **a pilot authored by the implementing assistant and reviewed against source, not independently adjudicated ground truth or a random repository sample**. AgentX is concentrated on authentication; several questions revisit previously investigated code. Six questions concern Flexcontext itself. Do not tune ranking on this set and then advertise it as held out. Expand and independently review the labels before drawing broad conclusions.

Each evidence atom consists of one or more required path-qualified source line spans. All spans of an atom must be visible to earn recall credit. Broader `relevant_regions` identify the implementation regions relevant to the question for precision. Every span is protected by a source hash: code changes fail validation, requiring label review. Labels are not passed to adapters. Freeze the questions before a comparison; retain old results when making corpus revisions.

## Equal query and budget policy

Every adapter receives a query deterministically derived from the same question: lowercase lexical words, fixed stopwords removed, stable deduplication. No gold symbol names, relevant files, synonyms, or model-generated expansions are supplied. Flexcontext gets space-separated words; Probe gets their OR expression; `rg` gets case-insensitive fixed-string OR patterns with three context lines, sorted by path; Aider gets the words as mentioned identifiers, no chat files, no mentioned files, and no map-size multiplier. Aider recomputes the map (`refresh="always"`) while retaining its normal tag/tree caches. This is a **fixed keyword-discovery baseline**, not an optimal query planner for each product. A later agent evaluation should allow appropriate query syntax while counting all queries and source tokens.

All engines are offered exactly the same snapshot. Flexcontext and Probe receive an 8×budget source-byte candidate ceiling and 100-result limit. This is a fixed overfetch allowance, not a tokenizer claim. Aider receives the requested map-token budget. `rg` scans all matches in deterministic path order. Save the complete native response before adapting it.

Adapters keep only source lines actually visible in each response. They never read a pointed-to function body into the model context. Aider's filename-only entries count only toward the diagnostic native file-pointer recall. Truncated or ambiguous source lines cannot earn complete-line evidence credit. A common prefix cap then deduplicates identical located lines and counts `tiktoken` **cl100k_base** tokens separately for each nonblank line plus newline. The first line that would exceed the budget stops selection. No label-aware packing or reranking occurs. Per-line accounting makes the budget additive and exact under this declared convention; it differs from tokenizing a whole concatenated file.

The source-token budget is a maximum, not a requirement to fill it. Map headers consume Aider's native map budget, and its signature map is inherently different from a body retriever. The normalized context adds file/line citations outside the source budget. Both normalized context cost and native transport cost are measured, so metadata overhead remains visible. This pilot does not equalize total prompt tokens or CPU time.

## Metrics

- **Evidence recall:** fully recovered required atoms / all required atoms, macro-averaged over questions.
- **Region precision (`evidence_precision`):** unique returned located lines within authored relevant implementation regions / unique returned located lines plus unlocated nonblank lines. Unjudged lines count as irrelevant. This is conservative line-level relevance precision, not precision over model claims.
- **Gold-line precision/recall:** overlap with the smaller atomic evidence line set. Gold-line precision is an evidence-density diagnostic, not an exhaustive semantic precision judgment.
- **Source file recall:** relevant files with actual selected source, not merely a pointer. `native_pointer_file_recall` separately measures pointers before the common cap; it is not comparable to budgeted evidence recall.
- Source bytes and additive source tokens; normalized context bytes/tokens including citations; complete native stdout bytes/tokens including protocol overhead. Stderr is saved separately.
- Latency measured with a monotonic clock, including native transport and process startup where applicable. Normalization/tokenization/scoring time is reported separately. Raw samples, median, and p95 are retained. With only two repeats, p95 describes a small pooled sample and is not a stable tail estimate.

**Cold:** fresh process, relevant tool disk cache deleted before each question/budget combination. Includes parse/index/cache-write work. OS page cache is uncontrolled and often warm from snapshot creation. This is *tool-cache cold*, never a claim of cold disk I/O.

**Warm CLI:** fresh process with the cache created by the cold request, same query. Includes executable/import/cache-load overhead. Probe and `rg` are stateless in this adapter, so their cold/warm difference mainly reflects OS caching and noise.

**Resident first/repeat:** one Flexcontext MCP process or Aider bridge per repository; initial handshake/import startup recorded separately in `resident-startup.json`. Disk caches are warm. Each distinct question/budget is sent once, then repeated in seeded shuffled rounds. First-use work may remain in the first query, especially Aider graph construction. No persistent Probe/rg engine is implemented; resident latency is **N/A**, not a subprocess call relabeled as resident. This is a harness limitation, not a claim about all Probe APIs.

The summary is per repository, budget, tool and mode. Do not let the larger question group dominate a cross-repository average or count repeated timings as independent quality questions. Keep failures and sample counts alongside averages. Larger confirmatory runs should counterbalance run order, collect more cold replicates, and bootstrap paired differences by question, not repetition.

## Artifacts and verification

Results under `.benchmark-results` are ignored because they contain local source excerpts. Nothing is uploaded. Each run retains `environment.json`, source manifests, snapshots, frozen cases, `rows.jsonl`, `summary.json`, `REPORT.md`, native stdout, stderr, selected source records and model-ready contexts. Native output is deliberately uncompressed so response parsing and score attribution can be audited.

```sh
.benchmark-venv/bin/python -m unittest discover -s benchmarks/comparison -p 'test_*.py'
.benchmark-venv/bin/python benchmarks/comparison/verify_run.py .benchmark-results/my-run
```

Tests cover actual evidence versus signatures/pointers, truncated map lines, Probe and `rg` parsing, token caps, deduplication, multi-span atoms, and stale-ground-truth rejection. Adapter smoke runs exercise the real executables. The benchmark makes no ranking or production engine changes.

## Model answers, then agent task success

Export a paired, tool-blinded prompt set from one measured mode/repetition:

```sh
.benchmark-venv/bin/python benchmarks/comparison/export_prompts.py .benchmark-results/my-run
```

Prompts contain only the question and capped context; gold labels and tool identity live in a separate index. The export does not call a model or claim answer correctness. You can use the installed CLIs for an initial answer experiment, keeping the same model, effort, prompt, context budget and repetitions across tools:

```sh
codex exec --ephemeral --skip-git-repo-check --sandbox read-only --json - < /absolute/path/to/answer-prompts/0000.txt
opencode run --pure --format json --model PROVIDER/MODEL --file /absolute/path/to/answer-prompts/0000.txt -- 'Answer the attached question using only its excerpts.'
```

Run these from an isolated empty directory. These generic agent CLIs may inherit tools or MCP configuration; **prompt instructions alone do not enforce closed-context access**. Disable extra tools in the runner configuration and audit JSON traces. Reject closed-context answer samples that perform searches or other source reads. Capture model/version, effort, prompt hash, elapsed time, token usage, tool calls, and final answer; have blinded reviewers score supported/correct claims and citation precision/recall. Retrieval evidence recall alone is not answer accuracy.

For the eventual coding-agent stage, author tasks with executable acceptance tests in independent disposable checkouts. Compare `rg`, Probe, Flexcontext→targeted-search, and Aider-map→targeted-search policies using the same model and task start state. Enforce a cumulative source-token budget across **all** reads/searches, plus equal time/turn limits. Record task success, tests passed, regressions, total/model/source tokens, latency, tool calls, and searches before first edit. Keep hidden tests and gold evidence outside agent access. That agent task-success stage is planned, not measured by this retrieval pilot.
