# flexcontext

Structural lexical context retrieval for coding agents, using Tree-sitter and syntactic relationships. Find relevant source units before you know the right files. Supported languages are Rust, TypeScript/TSX, JavaScript/JSX and Python. Relationships are lexical evidence, not compiler semantics.

```console
cargo build --release --locked
./target/release/flexcontext search . "validate token" --json --max-tokens 8000
./target/release/flexcontext index .
./target/release/flexcontext serve /absolute/path/to/repository
./target/release/flexcontext benchmark benchmarks --max-bytes 12000
```

The older `--code-search ROOT QUERY` and `--mcp ROOT` CLI aliases remain available.

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

`--budget N` remains a deprecated estimate of selected source tokens (`4*N` source bytes); it does not bound serialized context. `stats.source_bytes` retains its original meaning of repository source size; use `context_cost.source_bytes` for selected snippets (including excerpt markers). `stats.approximate_tokens` also remains source-only for existing scripts. `--max-results` defaults to 12.

Oversized units may return explicitly incomplete excerpts with `source_spans`. Inspect full source before reasoning about omitted behavior. Each result exposes `lexical_score`, `structural_score`, `diversity_score` and `final_score`. The existing `score` field is the sum before diversity; size penalties are included in lexical score. Diversity is measured at the point that unit was selected.

## MCP

The modern **2026-07-28** protocol uses no initialization handshake. Every request supplies protocol version and capabilities in `params._meta`:

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}},"name":"code_search","arguments":{"query":"auth","max_tokens":8000,"max_results":12}}}
```

`server/discover` is supported but optional. Modern results have `resultType: "complete"` and server identity metadata. Unsupported per-request versions return `-32022`; missing required metadata returns `-32602`.

**2025-06-18** is also supported for clients such as Codex. Send `initialize` with `protocolVersion`, `capabilities`, and `clientInfo`, then `notifications/initialized`. Subsequent requests use the negotiated session without modern metadata; responses omit the modern result wrapper. An unsupported handshake version receives a proposal of `2025-06-18`; clients that cannot use it must disconnect. Explicit modern metadata is always validated, even after a legacy handshake.

Tools:

- `code_search`: `query`, optional `max_tokens`, optional source-only `budget` (default 4096), and `max_results` (1–100).
- `refresh_index`: no arguments; atomically replaces the configured repository snapshot after edits.

Code appears **once**, in `structuredContent`, for both protocols. Text contains only a result-count summary. Clients must consume structured results (supported since 2025-06-18). The spec's recommendation to also serialize structured results into text for older clients is deliberately not followed, to avoid duplicating agent context. Text-only clients are not supported. No client-name detection is used. Serialized byte and token estimates include the actual negotiated response envelope. A reader handles `notifications/cancelled` while the worker is busy, checks cancellation during scanning, and suppresses cancelled responses. There is one query worker; the bounded pending queue applies backpressure.

Repeated queries share an immutable repository snapshot; call `refresh_index` after file edits. Modern protocol identity/capabilities are read independently for each request. The repository is configured by the server command, not inferred from client session history.

For Codex, build with `cargo build --release --locked --bin flexcontext`, then add this to the trusted repository's `.codex/config.toml` (replace both absolute paths):

```toml
[mcp_servers.flexcontext]
command = "/absolute/path/to/flexcontext/target/release/flexcontext"
args = ["serve", "/absolute/path/to/repository"]
enabled = true
startup_timeout_sec = 30
tool_timeout_sec = 30
```

Restart MCP servers from Codex settings after configuring or rebuilding the binary. Use `code_search` for discovery, then inspect the referenced source; use `refresh_index` after edits.

References: [modern per-request metadata](https://modelcontextprotocol.io/specification/2026-07-28/basic/index), [discovery](https://modelcontextprotocol.io/specification/2026-07-28/server/discover), [tool results](https://modelcontextprotocol.io/specification/2026-07-28/server/tools), [2025-06-18 lifecycle](https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle), [Codex MCP configuration](https://learn.chatgpt.com/docs/extend/mcp?surface=cli).

## Scan bounds and cache

Directory and AST traversal use explicit stacks. `.gitignore` is honored even without a `.git` directory; hidden, vendor, dependency, build and generated directories are excluded. Symlinks are not followed. Defaults:

| Limit | Default | Override |
|---|---:|---|
| File size | 2 MiB hard maximum | `--max-file-bytes` can lower it |
| Total eligible source bytes | 256 MiB | `--max-source-bytes` |
| Source files | 100,000 | `--max-source-files` |
| Scanned files | 1,000,000 | Library `ScanLimits` |
| Directory depth | 128 | `--max-depth` |
| Named AST nodes / structural depth per file | 1,000,000 / 256 | Fixed safety bounds |

Oversized files, binary/non-UTF-8 files and excluded/deep directories are counted in statistics. Counts cover encountered entries; contents of ignored or pruned directories are not enumerated. Repository size/count and AST complexity limits return explicit errors rather than silently presenting a complete index. `--progress` writes scan progress to stderr. Ctrl-C requests cancellation; a second Ctrl-C exits immediately. Restrict the root or explicitly raise source limits for larger repositories; there is no blanket flag that removes all bounds.

Each file has one shared source buffer; symbols reference byte ranges and strings are materialized for selected results. AST facts are collected once and assigned to containing units by ranges. `file_import` edges describe only a shared file and receive no ranking boost.

`.flexcontext/index.json` stores source once per file. A SHA-256 build fingerprint includes extraction code, symbol/cache schema, normalization, indexing and the complete dependency lockfile (including grammar versions). Changes automatically invalidate the cache. File checks use size and mtime plus Unix device/inode/ctime where available; this does not prove content identity on every filesystem. `--no-cache` forces a fresh parse. Old `index-v3.json` files are ignored and may be removed manually.

## Reproducible evaluation and development

See [the 60-query graded corpus, metrics and tuning protocol](benchmarks/README.md) and [current measured results](benchmarks/results/README.md). Older measurements remain in [benches/results](benches/results/README.md) as historical snapshots.

```console
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --release --locked --bins
cargo bench --locked --bench pipeline -- --test
python3 scripts/bench_scaling.py --output /tmp/scaling.json
```

CI runs formatting, lint, tests, release build and benchmark smoke tests on Linux and macOS with Rust stable. No separate MSRV promise is made. MIT licensed; see [LICENSE](LICENSE).
