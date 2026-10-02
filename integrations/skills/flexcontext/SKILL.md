---
name: flexcontext
description: Discover relevant implementations in a repository with flexcontext when the files or symbols are unknown, then follow source locations with targeted searches and edits.
---

# Discover code with flexcontext

Use the configured flexcontext MCP server's `code_search` tool for a concise implementation-oriented query. The server searches its configured repository, so confirm that root matches the task. Start with baseline retrieval and compact output; use `max_tokens` when the available context is limited.

If MCP is unavailable but the binary is installed, use the CLI:

```sh
flexcontext search /absolute/path/to/repository "session expiration" --json --max-tokens 8000
```

Read the returned paths, symbols, source locations and source text. Follow promising hits with a targeted text search for callers and usages, and inspect the actual surrounding implementation before editing. When the exact file or identifier is already known, go directly to that source.

Use `include_paths` or `exclude_paths` to narrow known repository subtrees. These values are repository-relative exact files or directory prefixes, not globs. If a query misses, refine its implementation terms or use a targeted text search; avoid repeating broad discovery queries without new evidence.

Treat results as lexical evidence. Relationships are syntactic hints, not compiler resolution. Truncated excerpts omit behavior; inspect the full source when that behavior matters. `max_tokens` estimates serialized bytes divided by four rather than actual model tokens.

After editing, call `refresh_index` before another MCP search. The resident server does not automatically refresh its snapshot. CLI searches recheck the disk cache on each invocation.
