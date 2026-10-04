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

For experimental progressive retrieval, opt in with `continuations: true` (CLI `--continuations`). Compact output then includes bounded `navigation.leads` for omitted or partial source. Choose relevant leads and pass their opaque `reference` unchanged to `expand_context` (CLI `flexcontext expand ROOT REFERENCE --json`). Exact source expansion bypasses ranking and selection quotas. `navigation.next` uses the same interface to inspect another page; it returns leads, not source. Leads and optional `role_hints` are navigation cues, never proof of behavior. Further searches may be necessary for code outside the retained pool. Read source before drawing conclusions.

Expansion uses the originating scope. References require the identical repository/index snapshot and signing key; after edits refresh and search again. CLI restart is supported. Opt-in search creates `.flexcontext/progressive-key-v1`, including with `--no-cache`; deleting this key expires references. There is no shared seen-state: retries repeat the same request, and partial-source follow-ups should use the new reference returned with the excerpt. Source and complete-output budgets still apply; on an insufficient-budget error, request less source or increase the response budget. Source spans and `content_truncated` describe partial excerpts, which may split a statement.

After editing, call `refresh_index` before another MCP search. The resident server does not automatically refresh its snapshot. CLI searches recheck the disk cache on each invocation.
