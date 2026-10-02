This is `flexcontext`, the fast lexical context generator for agents.

Find useful code before you know the right files or symbol names. `flexcontext` searches a repository, ranks functions and other source units, and returns relevant code with file locations within a context budget. Use it from your terminal, a Rust application, or a coding agent through MCP.

## Why use it?

- **Start with a question.** Search for `session expiration` or `queued job cancellation` to discover likely implementations, then follow up with precise searches and edits.
- **Keep context useful.** Structural extraction preserves functions, declarations and relevant statements. Compact responses avoid unnecessary ranking metadata, and configurable budgets limit returned context.
- **Work locally.** The CLI and MCP server use deterministic lexical retrieval and Tree-sitter parsing. No embeddings, model calls, API keys or hosted index are required.
- **Reuse indexing work.** A persistent cache reparses changed files; the MCP server keeps a repository snapshot in memory for repeated queries.
- **Inspect the evidence.** Results include paths, source locations and truncation information. Full detail and explanation options expose ranking and selection decisions.

Supports Rust, TypeScript, JavaScript, Python, Java, Apex, Go, C, C#, C++, Objective-C, Metal, CUDA, Kotlin, Dart, Vue and Lua, including React in JS/TS. See [language coverage and parser limitations](docs/languages.md).

## Build and install

You need [Rust stable](https://rustup.rs/) and a C/C++ build toolchain for the parser grammars. On macOS, install the Xcode Command Line Tools; on Linux, install your distribution's development tools. CI checks Linux and macOS.

```sh
git clone https://github.com/thamok/flexcontext.git
cd flexcontext
cargo build --release --locked --bin flexcontext
./target/release/flexcontext --help
```

The binary is `target/release/flexcontext`. To install it on your Cargo executable path:

```sh
cargo install --path . --locked --bin flexcontext
flexcontext --version
```

Or build and install directly from Git in one command:

```sh
cargo install --git https://github.com/thamok/flexcontext.git --branch main --locked --bin flexcontext
```

Ensure Cargo's executable directory (usually `~/.cargo/bin`) is on your `PATH`. Installing from Git builds from source; you do not need a separate runtime after compilation.

## Search a repository

```sh
# Discover the implementation behind a concept.
flexcontext search /path/to/repository "session expiration"

# Return structured context for an agent or script.
flexcontext search . "validate token" --json --max-tokens 8000

# Narrow discovery to a source subtree and exclude generated code.
flexcontext search . "queued job cancellation" --include-paths src --exclude-paths src/generated

# Inspect scores and selection decisions.
flexcontext search . "cache invalidation" --json --explain
```

CLI and MCP responses default to compact output and baseline retrieval. `--max-bytes` limits selected source (16 KiB by default); `--max-tokens` limits the estimated cost of the complete response using UTF-8 bytes divided by four. It is an estimate, not a model tokenizer. Use `--detail full` for ranking metadata and timings. Path filters match exact repository-relative files or directory subtrees.

Use the results to find relevant code, then inspect the surrounding source before making a change. Excerpts can omit behavior and are marked as truncated. [Retrieval, budgets and compatibility](docs/retrieval.md) explains the options, including experimental policies.

## Connect your coding agent

After installing the binary, run the appropriate command **from the repository you want to search**:

```sh
# Codex: register this repository in your user configuration.
codex mcp add flexcontext -- "$(command -v flexcontext)" serve "$PWD"

# Claude Code: register this repository in project configuration.
claude mcp add --transport stdio --scope project flexcontext -- "$(command -v flexcontext)" serve "$PWD"
```

Start a new agent session, then ask: “Use flexcontext to find how session expiration works, inspect the relevant source, and explain it.” The server exposes `code_search` and `refresh_index`; refresh after editing because the resident snapshot does not update automatically.

[Agent integrations](docs/agents.md) includes Cursor and VS Code/GitHub Copilot configuration, setup checks, troubleshooting, and an optional [portable discovery skill](integrations/skills/flexcontext/SKILL.md) for Codex and Claude Code. Setup follows the official [Codex](https://learn.chatgpt.com/docs/extend/mcp?surface=cli) and [Claude Code](https://code.claude.com/docs/en/mcp) MCP instructions.

## Documentation

- [Documentation index](docs/README.md): all guides, evaluation methods and experiment reports.
- [Agent integrations and skill installation](docs/agents.md).
- [Retrieval, context budgets and response compatibility](docs/retrieval.md).
- [Supported languages and parser limitations](docs/languages.md).
- [Indexing, cache and scan limits](docs/indexing.md).
- [MCP tools and protocol reference](docs/mcp.md).
- [Development, validation and contributing](docs/development.md).
- [Evaluation corpus](benchmarks/README.md), [retrieval comparisons](benchmarks/comparison/README.md), and [coding-agent evaluation](benchmarks/agents/README.md).

MIT licensed. See [LICENSE](LICENSE).
