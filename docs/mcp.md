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
- `refresh_index`: no arguments; forces an atomic reload of the configured repository snapshot. Normal searches refresh automatically.

Code appears **once**, in `structuredContent`, for both protocols. Text contains only a result-count summary. Clients must consume structured results (supported since 2025-06-18). The spec's recommendation to also serialize structured results into text for older clients is deliberately not followed, to avoid duplicating agent context. Text-only clients are not supported. No client-name detection is used. Serialized byte and token estimates include the actual negotiated response envelope. A reader handles `notifications/cancelled` while the worker is busy, checks cancellation during scanning, and suppresses cancelled responses. There is one query worker; the bounded pending queue applies backpressure.

Before every `code_search` and `expand_context`, the server scans eligible paths and compares lightweight file metadata with the resident snapshot. Modified, created, renamed, and deleted source files trigger an atomic snapshot replacement. The scan uses the existing ignore rules and size/count/depth limits, so changes to ignore rules also take effect. There is no polling interval or background watcher. An unchanged repository reuses the lexical, ranking, and relation indexes without reading source, loading the disk cache, parsing, or rebuilding indexes. With caching enabled, a changed snapshot reparses only changed/new files using `.flexcontext/index.json`; aggregate search indexes are then rebuilt. With `--no-cache`, changed snapshots reparse all eligible source, while unchanged requests still reuse memory.

Change checks cost a bounded directory traversal plus source-file metadata checks per request; they do not hash source contents. They use size/mtime and Unix device/inode/ctime where available, matching [disk-cache validation](indexing.md). This avoids platform-specific watcher dependencies, per-directory watch limits, background event queues, and missed-event recovery. Very large or slow filesystems pay more traversal latency; measure that overhead before adding a watcher optimization. Full-detail `stats.traversal_us` includes the change scan, refresh work appears in cache/parse/index counters, and `stats.elapsed_us` includes the change check and any refresh (before response serialization).

If scanning, parsing, cache writing, or cancellation prevents a refresh, the request returns a tool error (or suppresses the response for a cancelled MCP request). It never falls back to returning old source. The previous complete snapshot is retained internally; the next request retries automatically. `refresh_index` remains available to force a reload or diagnose an error. Expansion checks freshness too, so references to an earlier indexed-source snapshot fail explicitly; run `code_search` again after edits. Filesystem traversal is not a transaction: concurrent edits during a request can require the next request to observe them, and metadata cannot establish content identity on every filesystem.

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
