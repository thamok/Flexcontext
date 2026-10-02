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
