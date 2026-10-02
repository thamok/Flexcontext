# Retrieval and context budgets

## Commands

| Command | Purpose |
|---|---|
| `flexcontext search ROOT QUERY` | Retrieve context from a repository, checking the disk cache for changed source |
| `flexcontext index ROOT` | Build or refresh the disk index and print its summary |
| `flexcontext serve ROOT` | Run a resident stdio MCP server for one repository |
| `flexcontext benchmark benchmarks` | Evaluate the frozen baseline corpus |

The older `--code-search ROOT QUERY` and `--mcp ROOT` aliases remain available. Run `flexcontext --help` for all CLI options.

## Presentation and policies

CLI and MCP responses now default to **compact** presentation. Query, applied
scope, retrieval policy, file/symbol identity, source text, exact source spans,
truncation status and context cost remain available. Scores, ranking signals,
relationship references and detailed timing statistics require `--detail full`
or MCP `"detail":"full"`. This is an intentional default wire-response change;
consumers that read `stats`, `signals`, `signature` or `relations` should explicitly
select full detail. `--explain` (MCP `"explain":true`) implies full detail and adds
per-candidate direct rank, relationship promotion, rejection reason and truncation.

The focused retrieval policy is experimental and explicit until the frozen
acceptance gates pass. `--policy baseline` remains the default. `--policy focused`
combines file-frequency weighting, direct-evidence-gated relationship boosts,
automatic file focus, a relevance cutoff and stable excerpt allocation. All work
is local and deterministic; there is no model or embedding dependency.

```console
flexcontext search . "session expiration" --json --policy focused --include-paths src --exclude-paths src/generated
flexcontext search . "session expiration" --policy focused --scope repository --detail full
flexcontext search . "session expiration" --policy focused --explain --json
```

`--include-paths` and `--exclude-paths` are repeatable search options. Each value
matches a repository-relative exact file or directory subtree, not a glob or
substring. Exclusions win. Absolute paths and traversal components are rejected.
Filters apply before selection and to returned navigation references; automatic
focus never escapes them. They filter retrieval, not the repository index itself.
MCP uses arrays named `include_paths` and `exclude_paths` on each `code_search` call.

`scope` is `auto` by default; the focused policy initially favors the three files
with the strongest direct evidence and distinct query-term coverage. Strong
alternatives remain eligible. `scope: repository` disables this preference while
keeping explicit filters and the relevance cutoff. Responses report the applied
`focused_files`; baseline retrieval leaves this list empty. Tests are never
automatically excluded. A small implementation preference is disabled for test
queries, explicit test paths and exact identifier matches.

`--cutoff` sets the experimental relative direct-score threshold (default 0.25).
The byte budget is a ceiling. Focused excerpts are planned independently of the
budget, retain enclosing signatures and branch/loop conditions, deduplicate
overlapping source, and allocate a stable prefix. Raising the **source byte**
ceiling with other options fixed preserves previously returned source ranges.
This does not promise monotonicity after an external prefix truncation or after
changing the separate serialized-payload `max_tokens` limit.

Library callers can use `SearchSession::query_with_options(&QueryOptions)` with
`SearchScope`, `RetrievalPolicy` and `Detail`. Existing `query` and
`query_with_weights` methods remain compatibility wrappers with baseline retrieval
and full accounting. `SearchOptions` keeps its existing query/budget fields and
adds `retrieval` options; those top-level fields supply the CLI/library query and
source budget. The typed `SearchResponse` retains full fields; serialize with
`output::Representation` and `output::finalize` to account for the actual chosen
presentation and transport envelope.

The isolated experiment policies are `relations`, `quotas`, `diversity`, `idf`,
`direct`, `implementation`, `focus`, `stable`, and `cutoff`. They are diagnostic
controls, not independently promoted defaults. See the
[frozen evaluation procedure](../benchmarks/comparison/OPTIMIZATION.md).
The [completed experiment](../benchmarks/comparison/OPTIMIZATION_RESULTS.md) failed
the recall and irrelevant-source gates, so focused retrieval remains opt-in.
Compact presentation alone preserved all source in the 36 held-out budget pairs
while reducing non-source payload tokens by 83.8%.

## Context budgets

`--max-bytes` is a **source-selection budget**, defaulting to 16 KiB. `--max-tokens` constrains the estimated cost of the **complete emitted representation**, including metadata, JSON escaping and whitespace, trailing newline, and (for MCP) the JSON-RPC envelope and request ID. Both limits apply when supplied together.

Every structured response exposes:

```json
{
  "context_cost": {
    "source_bytes": 8565,
    "serialized_bytes": 42699,
    "estimated_tokens": 10675,
    "selection_budget": 12000,
    "token_budget": null,
    "representation": "json-pretty"
  }
}
```

These are illustrative values. The estimator is `ceil(serialized UTF-8 bytes / 4)`, **not a model tokenizer or a guaranteed actual-token limit**. Client-side rewriting of tool results can change the eventual model context. Lower-priority complete result units are removed until the estimate fits; a limit too small even for response metadata produces an explicit error. An empty result list can fit when no full unit fits. Human and JSON formats are measured separately.

`--budget N` remains a deprecated estimate of selected source tokens (`4*N` source bytes); it does not bound serialized context. In full detail, `stats.source_bytes` retains its original meaning of repository source size; use `context_cost.source_bytes` for selected snippets (including excerpt markers). `stats.approximate_tokens` also remains source-only for existing scripts. `--max-results` defaults to 12.

Oversized units may return explicitly incomplete excerpts with `source_spans`. Inspect full source before reasoning about omitted behavior. Full-detail results expose `lexical_score`, `structural_score`, `diversity_score` and `final_score`. The existing `score` field is the sum before diversity; size penalties are included in lexical score. Diversity is measured at the point that unit was selected. Experimental file-focus promotion is recorded separately in the diagnostic trace.

## Structural excerpts and selection

Complete statement ranges are collected once with the file AST and persisted in the index. The default policy's budgeted method/component excerpts reuse those ranges without reparsing or reading source at query time. When a container has ranked member candidates, context selection retains its compact declaration so its whole body does not suppress the precise member hits. Local closures keep declaration ranking and broad-query suppression. Returned source spans include visible signatures and statements; omission markers are not source. Experimental focused policies keep their separate selection implementation.

Default context selection uses a lazy priority queue to preserve greedy diversity ordering without rescanning every remaining candidate after quota, overlap or budget rejections. The [second iteration report](../benchmarks/comparison/ITERATION_20261001_PASS2.md) records exact-context checks and paired latency results. A separate [coding-agent pilot](../benchmarks/agents/README.md) grades completed patches with independent acceptance checks and cumulative context accounting; its small seeded tasks validate the harness rather than establish production task-success rates.
