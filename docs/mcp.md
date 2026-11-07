# MCP protocol and tools

For ready-to-use client setup, see [agent integrations](agents.md).

The modern **2026-07-28** protocol uses no initialization handshake. Every request supplies protocol version and capabilities in `params._meta`:

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}},"name":"code_search","arguments":{"query":"auth","max_tokens":8000,"max_results":12}}}
```

`server/discover` is supported but optional. Modern results have `resultType: "complete"` and server identity metadata. Unsupported per-request versions return `-32022`; missing required metadata returns `-32602`.

**2025-06-18** is also supported for clients such as Codex. Send `initialize` with `protocolVersion`, `capabilities`, and `clientInfo`, then `notifications/initialized`. Subsequent requests use the negotiated session without modern metadata; responses omit the modern result wrapper. An unsupported handshake version receives a proposal of `2025-06-18`; clients that cannot use it must disconnect. Explicit modern metadata is always validated, even after a legacy handshake.

Tools:

- `code_search`: `query`, optional `max_tokens`, optional source-only `budget` (default 4096), `max_results` (1–100), `detail`, `scope`, `include_paths`, `exclude_paths`, `policy`, `cutoff`, and `explain`.
- `expand_context`: `reference`, optional source-only `budget`, `max_tokens`, and `detail`; checks for changes before resolving exact source or navigation references. After source changes, search again for current references.
- `refresh_index`: no arguments; reparses every eligible source file regardless of matching metadata, rebuilds the indexes, and atomically replaces the resident snapshot on success. Updates `.flexcontext/index.json` when caching is enabled. Normal searches use inexpensive automatic metadata checks.

Code appears **once**, in `structuredContent`, for both protocols. Text contains only a result-count summary. Clients must consume structured results (supported since 2025-06-18). The spec's recommendation to also serialize structured results into text for older clients is deliberately not followed, to avoid duplicating agent context. Text-only clients are not supported. No client-name detection is used. Serialized byte and token estimates include the actual negotiated response envelope. A reader handles `notifications/cancelled` while the worker is busy, checks cancellation during scanning, and suppresses cancelled responses. There is one query worker; the bounded pending queue applies backpressure.

Before every `code_search` and `expand_context`, the server scans eligible paths and compares lightweight file metadata with the resident snapshot. Modified, created, renamed, and deleted source files trigger an atomic snapshot replacement. The scan uses the existing ignore rules and size/count/depth limits, so changes to ignore rules also take effect. There is no polling interval or background watcher. An unchanged repository reuses the lexical, ranking, and relation indexes without reading source, loading the disk cache, parsing, or rebuilding indexes. With caching enabled, a changed snapshot reparses only changed/new files using `.flexcontext/index.json`; aggregate search indexes are then rebuilt. With `--no-cache`, changed snapshots reparse all eligible source, while unchanged requests still reuse memory.

Change checks cost a bounded directory traversal plus source-file metadata checks per request; they do not hash source contents. They use size/mtime and Unix device/inode/ctime where available, matching [disk-cache validation](indexing.md). This avoids platform-specific watcher dependencies, per-directory watch limits, background event queues, and missed-event recovery. Very large or slow filesystems pay more traversal latency; measure that overhead before adding a watcher optimization. Full-detail `stats.traversal_us` includes the change scan, refresh work appears in cache/parse/index counters, and `stats.elapsed_us` includes the change check and any refresh (before response serialization).

A same-size in-place edit can escape automatic detection when all fingerprint fields remain identical. Call `refresh_index` to recover: explicit refresh bypasses cached source, symbols, and postings even when fingerprints match. It honors the configured root, ignore rules, and scan/parser limits. With `--no-cache`, it reparses without reading or writing the disk index. After success, unchanged requests reuse the refreshed in-memory indexes; new sessions can reuse the refreshed disk cache. This forced-reparse behavior is a follow-up to v0.0.2; installed v0.0.2 binaries still require removing `.flexcontext/index.json` before refreshing for this edge case.

If scanning, parsing, cache writing, or cancellation prevents a refresh, the request returns a tool error (or suppresses the response for a cancelled MCP request). The failed request never falls back to returning old source. The previous complete resident snapshot is retained internally; detected automatic changes are retried on the next source request. After a failed explicit refresh, fix the error and call `refresh_index` again, since matching metadata cannot trigger an automatic retry. Disk-cache replacement uses a complete temporary index and atomic rename, without deleting the existing cache first. Expansion checks freshness too, so references to an earlier indexed-source snapshot fail explicitly; run `code_search` again after edits. Filesystem traversal is not a transaction: concurrent edits during any refresh can require another request to observe them.

Modern protocol identity/capabilities are read independently for each request. The repository is configured by the server command, not inferred from client session history.

For Codex, build with `cargo build --release --locked --bin flexcontext`, then add this to the trusted repository's `.codex/config.toml` (replace both absolute paths):

```toml
[mcp_servers.flexcontext]
command = "/absolute/path/to/flexcontext/target/release/flexcontext"
args = ["serve", "/absolute/path/to/repository"]
enabled = true
startup_timeout_sec = 30
tool_timeout_sec = 30
```

Restart MCP servers from Codex settings after configuring or rebuilding the binary. Use `code_search` for discovery, then inspect the referenced source. No manual refresh is needed after normal edits.

References: [modern per-request metadata](https://modelcontextprotocol.io/specification/2026-07-28/basic/index), [discovery](https://modelcontextprotocol.io/specification/2026-07-28/server/discover), [tool results](https://modelcontextprotocol.io/specification/2026-07-28/server/tools), [2025-06-18 lifecycle](https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle), [Codex MCP configuration](https://learn.chatgpt.com/docs/extend/mcp?surface=cli).
