# Development and evaluation

Build with the current stable Rust toolchain and a C/C++ toolchain for the Tree-sitter grammars. CI covers Linux and macOS. See the [README](../README.md) for installation.

The [third optimization report](../benchmarks/comparison/ITERATION_20261001_PASS3.md)
compares actual Probe and CodeGraph tools on frozen source, documents a small
fresh confirmation set, and records the exact-output posting-union optimization.
An [optional Jev semantic reranking prototype](../benchmarks/comparison/SEMANTIC_20261001.md)
improves one confirmation question without a measured recall regression. It is
a separate, explicitly enabled Python command; the Rust CLI and MCP remain local.

See [the 60-query graded corpus, metrics and tuning protocol](../benchmarks/README.md) and [current measured results](../benchmarks/results/README.md). Older measurements remain in [benches/results](../benches/results/README.md) as historical snapshots.

```console
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --release --locked --bins
cargo bench --locked --bench pipeline -- --test
python3 scripts/bench_scaling.py --output /tmp/scaling.json
```

CI runs formatting, lint, tests, release build and benchmark smoke tests on Linux and macOS with Rust stable. No separate MSRV promise is made. MIT licensed; see [LICENSE](../LICENSE).

## Contributing

Keep changes scoped and explain the behavior they improve. Add regression coverage for behavior changes and run the checks above before opening a pull request. Retrieval changes should include reproducible comparisons against the frozen corpora; report quality, source and payload cost, and latency separately. Experimental policies stay opt-in until their documented acceptance gates pass.

Never commit local source snapshots, model credentials or full agent traces. Benchmark environments and raw runs belong in the ignored `.benchmark-*` directories. The documentation index links the sanitized reports and their limitations. Published October 1 result summaries use repository-relative artifact paths in place of the original machine's absolute checkout prefix; measurements and source hashes are preserved.
