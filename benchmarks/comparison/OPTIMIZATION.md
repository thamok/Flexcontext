# Focused retrieval experiment

No agent-success evaluation or model calls are included. The original
`pilot-20260916` run and its source snapshots remain immutable. The 15 pilot
questions are development diagnostics, not held-out evidence.

`optimization-cases.json` freezes 36 additional source-reviewed questions: 12
each for Flexcontext, AgentX and WebKit's Python tooling. Each repository has six
development and six held-out questions, split by module. None uses the pilot's
modules. The SHA-256 of the authored corpus is
`d202d106427182f9c043c5b7580380d667f2ae22a590da005ec6972df10d2692`.
Queries, evidence atoms and broader relevant-region labels were frozen before
ranking experiments. These are assistant-authored, source-reviewed labels, not
independent human judgments. The WebKit scope is `Tools/Scripts/webkitpy`, not
the browser engine's C++ code.

Run with the Python environment from `setup.sh`, an executable saved before
retrieval changes, and an existing pilot snapshot:

```sh
cargo build --release --locked --bin flexcontext
.benchmark-venv/bin/python benchmarks/comparison/optimize.py prepare --output /tmp/focused-run --pilot /path/to/pilot --baseline /path/to/frozen-flexcontext --candidate target/release/flexcontext
.benchmark-venv/bin/python benchmarks/comparison/optimize.py dev --output /tmp/focused-run
.benchmark-venv/bin/python benchmarks/comparison/optimize.py pilot --output /tmp/focused-run
.benchmark-venv/bin/python benchmarks/comparison/optimize.py heldout --output /tmp/focused-run
.benchmark-venv/bin/python benchmarks/comparison/optimize.py compare --output /tmp/focused-run
.benchmark-venv/bin/python benchmarks/comparison/presentation_control.py /tmp/focused-run
.benchmark-venv/bin/python benchmarks/comparison/report_optimization.py /tmp/focused-run
```

Pass `--cases` to every stage when using another corpus. Stages refuse to
overwrite their result files. `prepare` copies and hash-verifies every source
file against the pilot manifest, preserves both executables, and archives the
implementation and harness. Original repositories and the pilot are read-only.
All artifacts remain local under the selected output directory, including
compressed raw outputs and normalized source records.

The presentation-only control compares compact baseline retrieval against the
frozen baseline responses, asserting identical ordered source text, spans and
truncation flags in every held-out question/budget pair. Its additional resident
timings are descriptive because they run in a separate block; they do not replace
the randomized baseline-versus-focused acceptance gate. The reporting step
verifies source hashes, recomputes source-token counts and scores, checks exact
native Flexcontext byte accounting and validates source-span retention at the
larger budget.

## Controls and acceptance

Development ablations change relationship promotion, quotas, diversity,
file-frequency weighting, path-only evidence, implementation preference, file
focus and stable allocation separately. The stable-only control retains the
baseline quota and diversity rules in its budget-independent planning pass.
The combined focused policy removes those rules. Relative cutoffs 0.25, 0.40
and 0.55 are also tested separately and in the combined policy.

The native adapter uses the pilot's overfetch and prefix cap. A second adapter
queries a fixed 32,768-byte candidate pool for both the 2,048- and 4,096-token
source budgets, keeping the engine's candidate excerpts constant across those
caps. This distinguishes retrieval changes from budget-dependent overfetch.
All tools receive the same authored question terms and source-token ceiling.
The tokenizer is `cl100k_base`; the engine's bytes/4 estimate remains separate.

The highest cutoff preserving development mean evidence recall in every
repository/budget/adapter group is selected. If none qualifies, 0.25 is tested
on held-out questions diagnostically and promotion automatically fails.
The held-out gate additionally requires:

- No recall regression in any repository/budget/adapter group.
- At least 25% fewer irrelevant returned source tokens overall.
- At least 50% fewer non-source payload tokens overall.
- Resident p95 latency no more than 10% above the corresponding baseline in
  each repository/budget group, using 20 repetitions per question and budget.

Irrelevant tokens are the sum of tokenizer costs for normalized returned source
lines outside the frozen relevant regions, including unlocated lines. Non-source
payload tokens are measured by replacing each `content` string with an empty
string in the native structured response and tokenizing its compact JSON form.
Scores, signatures and relationship copies remain non-source overhead. Native
bytes/tokens are also recorded separately without this transformation.

Resident timing excludes startup and normalization. Calls run serially in a
seeded randomized order after warming all distinct queries. Cold CLI means a
new process with the tool's disk cache cleared; OS page caches are uncontrolled.
Warm CLI includes process startup. Probe and ripgrep have no resident adapter
here, so resident latency is unavailable for them. Aider uses its real RepoMap
through a resident Python bridge and receives 20 timing repetitions as well.

Four-tool results include baseline Flexcontext, the focused candidate, Probe,
ripgrep and Aider at both source budgets. Ripgrep retains the pilot's deliberately
simple OR/context-line adapter, including its exhaustive overfetch; conclusions
about that adapter are not claims about the best possible ripgrep workflow.
The acceptance decision is machine-readable in `acceptance.json`. A failed
candidate stays opt-in, and baseline retrieval stays the default.
