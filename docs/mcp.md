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
