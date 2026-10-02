# Documentation

Start with the [README](../README.md) to build the binary and run your first search.

## Guides and reference

| Guide | What you will find |
|---|---|
| [Agent integrations](agents.md) | Codex, Claude Code, Cursor and VS Code setup; skill installation and troubleshooting |
| [Portable discovery skill](../integrations/skills/flexcontext/SKILL.md) | Instructions for discovery, targeted source inspection and index refresh |
| [Retrieval and context budgets](retrieval.md) | Compact/full output, scope filters, experimental policies, source spans and cost accounting |
| [Supported languages](languages.md) | File extensions, extracted structures and parser limitations |
| [Indexing and cache](indexing.md) | Cache invalidation, ignore behavior, cancellation and scan bounds |
| [MCP reference](mcp.md) | Tool arguments, structured results and supported protocol lifecycles |
| [Development and contributing](development.md) | Build checks, CI and evidence expected for retrieval changes |

## Evaluation methods and results

These reports describe specific frozen corpora and local runs. Their scope and limitations are part of the results; they do not establish a general agent task-success or performance advantage.

| Document | Scope |
|---|---|
| [Graded retrieval corpus](../benchmarks/README.md) | Baseline corpus, Java/multilingual suites, metrics and tuning procedure |
| [Recorded baseline results](../benchmarks/results/README.md) | Reproducible regression measurements |
| [Retrieval comparison harness](../benchmarks/comparison/README.md) | Flexcontext, Probe, ripgrep and Aider adapters, frozen evidence and accounting |
| [Initial comparison pilot](../benchmarks/comparison/PILOT.md) | Pilot measurements and limitations |
| [Focused-retrieval procedure](../benchmarks/comparison/OPTIMIZATION.md) | Frozen experiments and acceptance gates |
| [Focused-retrieval results](../benchmarks/comparison/OPTIMIZATION_RESULTS.md) | Gate failures and the decision to keep focused retrieval opt-in |
| [October 1 first iteration](../benchmarks/comparison/ITERATION_20261001.md) | Language and retrieval changes with measured evidence |
| [October 1 second iteration](../benchmarks/comparison/ITERATION_20261001_PASS2.md) | Selection optimization, exact-context comparisons and paired latency |
| [October 1 third iteration](../benchmarks/comparison/ITERATION_20261001_PASS3.md) | Posting-union optimization and fresh confirmation questions |
| [CodeGraph comparison](../benchmarks/comparison/CODEGRAPH_20261001.md) | Tool adapter and observed comparison results |
| [Optional semantic reranking experiment](../benchmarks/comparison/SEMANTIC_20261001.md) | Separate Python prototype with explicit external model use |
| [Coding-agent evaluation](../benchmarks/agents/README.md) | Controlled tools, independent acceptance, token accounting and known incomplete repairs |
| [Historical pipeline measurements](../benches/results/README.md) | Earlier snapshots retained for reference |

The [MIT license](../LICENSE) applies to this project. Third-party tools and benchmark dependencies retain their own licenses.
