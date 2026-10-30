# Indexing, cache and scan limits

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

Explicit `SearchSession::refresh()` (MCP `refresh_index`) bypasses metadata-based cache reuse and reparses every eligible source file. With caching enabled, it atomically installs a rebuilt `.flexcontext/index.json`; with caching disabled, it leaves the disk index untouched. It replaces the resident snapshot only on success and keeps the session's cache setting for later automatic refreshes. Subsequent unchanged searches reuse the refreshed indexes. Use this operation to recover from same-fingerprint edits without manually deleting the cache; see [MCP freshness](mcp.md) for the v0.0.2 binary limitation and retry behavior.

Resident MCP source requests use the same file fingerprints to detect repository changes automatically. Discovery collects paths and fingerprints in one traversal; that inventory also validates the cache during a refresh, avoiding a second metadata pass. An unchanged request reuses all prepared indexes. A detected change refreshes atomically through the existing cache; errors retain the prior snapshot internally and fail the request, with an automatic retry on the next search. These checks also run with `--no-cache`, which disables disk-cache reuse rather than in-memory reuse. See [MCP freshness](mcp.md) for scan costs and concurrent-edit limitations. Library users can choose `SearchSession::query_current` / `expand_current` for this behavior, or the existing pinned-snapshot query/expansion methods with explicit `refresh`.
