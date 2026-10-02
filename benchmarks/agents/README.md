# Coding-agent task completion evaluation

The primary KPI is **a completed, independently verified task under fixed resource limits**. Retrieval Recall@K, correct-file discovery, the agent's final message and running a test command are diagnostics, not success criteria.

This directory implements the first runnable pilot. It uses Codex CLI JSONL events, a restricted MCP tool broker, disposable task copies, and acceptance checks materialized after the agent exits. The runner follows the trace-based approach in [OpenAI's evaluation guidance](https://developers.openai.com/blog/eval-skills) and [non-interactive CLI documentation](https://learn.chatgpt.com/docs/non-interactive-mode).

## Current pilot

`tasks.json` freezes three small seeded regressions, each with an explicit file inventory, editable paths, origin and source hashes:

| Task | Origin | Independent acceptance |
|---|---|---|
| Identifier digit boundaries | Flexcontext's actual lexical implementation, with one condition removed | ASCII numeric/alphabetic transitions, acronym/camel-case behavior, punctuation and accented Unicode letters; non-ASCII numeric transitions were missing |
| Queued jobs run in reverse order | Frozen AgentX promise utility, with FIFO dequeue changed to LIFO | FIFO at capacities one/two, peak concurrency, rejection/synchronous-throw release and invalid capacities |
| Directory prefixes admit sibling paths | Synthetic Java retrieval fixture, with the separator boundary removed | Exact directory, descendants, sibling prefixes, Unicode and existing traversal rejection |

These are source-derived/synthetic smoke tasks, not representative issue-resolution tasks in full repositories. The grader tests show every seeded state fails and its reference repair passes. Seven harness tests also verify cumulative charging, protected files, path/symlink rejection, tool-call limits, trace validity, and host read/write restrictions.

The initial live smoke ran the FIFO task once in each of three conditions: ripgrep, the previous Flexcontext binary and the second-pass binary, all with `gpt-6.1-sol`, high effort and Codex CLI 0.159.3. **All three patches pass independent acceptance and tool audits.** They took 37.5, 42.3 and 36.2 seconds respectively; this sample does not establish a completion-rate or latency advantage. The initial smoke exposed baseline/candidate labels in their prompts; the current runner uses the same `Flexcontext` provider label and task prompt for both versions. Recorded data is [agent-pilot.json](../results/iteration-20261001-pass2/agent-pilot.json).

## Controlled tools and accounting

The broker exposes only `list_files`, `search`, `read_file`, `replace_text` and `run_public_tests`. Each trial starts with an independent copy of identical fixture bytes. Edits are limited to explicit source paths; public tests and configuration files are protected. Search operates on the current edited state: Flexcontext CLI performs an incremental refresh on each call. It is a CLI integration pilot, not a measurement of resident MCP search speed.

Repeated source delivery counts repeatedly toward the limit. Every public-test diagnostic also counts, preventing source printed by task code from becoming an unmetered read. Default limits are 8,000 cumulative source/diagnostic tokens, 40 attempted tool calls and 180 seconds per trial. The tokenizer is pinned `cl100k_base`; model tokens remain the CLI's reported usage rather than this local source estimate. Total broker JSON payload tokens are reported separately and exclude tool definitions/protocol wrappers. No USD cost is inferred from token counts.

The CLI ignores user configuration/rules, disables shell, plugins, skill discovery, web search and multi-agent tools, and uses read-only sandbox mode. The broker performs authorized edits. Trace auditing fails closed on native command/file/web events, other MCP servers, missing turn completion, mismatched broker/tool-call counts, budget excess or modified protected files. This is an access-controlled pilot with an audit; it does not assert that generic CLI prompt instructions enforce isolation.

On macOS, task-code test commands execute under a separate `sandbox-exec` profile with network denied, credentials omitted from the child environment, source/runtime reads restricted, and writes restricted to the trial directory. Acceptance code is created in a different staging directory only after the session ends. The tests verify that test processes cannot read the host grader or write outside their workspace. The worker currently requires macOS; a Linux/container worker remains to be implemented. Tool-broker code and approved compiler/runtime paths remain trusted infrastructure.

Contracts pin task, source, prompt, harness and binary hashes; actual runs retain harness copies, invocations, JSONL traces, independent broker audits, patches through final workspace state, acceptance logs, token usage and timings in ignored experiment directories. The pilot model/effort are explicit rather than inherited from personal CLI settings. Model/account prompt caching and nondeterminism remain uncontrolled and must not be confused with source-budget equality.

```sh
.benchmark-venv/bin/python -m unittest discover -s benchmarks/agents -p test_pilot.py

.benchmark-venv/bin/python benchmarks/agents/run.py \
  --baseline .benchmark-results/iteration-20261001-pass2-accepted/flexcontext-baseline \
  --candidate .benchmark-results/iteration-20261001-pass2-accepted/flexcontext-candidate \
  --model gpt-6.1-sol --effort high --task limiter-fifo --repeats 1 \
  --output .benchmark-results/agent-pilot-repeat
```

Add `--dry-run` to prepare the frozen contract, fixtures and invocations without model calls. Omit `--task` to include all three smoke tasks. Output directories must be fresh. A separate environment can install `requirements.txt`; the existing comparison environment already has the pinned tokenizer.

## Repeated direct-API smoke comparison

`run_api.py` runs the same frozen tasks with an OpenAI-compatible function-calling model and the same broker. It compares ripgrep, Probe, and the accepted Flexcontext binary. The system prompt, user prompt, tool schema, model settings, source budget, and call budget are identical across conditions; the provider name is omitted. Every trial requires `search` as its first function call through the API's `tool_choice`, because calibration showed the model sometimes ignored a prompt-only search instruction. Subsequent tool selection is autonomous. Condition order is randomized within each task/repetition using seed 1729.

This runner sends only prompts, broker source/results, and the model's own prior messages to the provider. The model has no general shell, filesystem, or HTTP tool. Public tests retain the separate macOS sandbox and credential-free child environment; acceptance code is materialized after the trial. Final source must pass both hidden acceptance and the frozen public regressions, independently re-run in separate grading workspaces. API response transcripts and provider usage receipts are retained and cross-checked against broker arguments, call IDs, error flags, and SHA256 hashes of delivered payloads. Missing usage is reported as unknown rather than zero. Truncated outputs, unknown tools, mismatched calls, wall-clock exhaustion, and provider failures cannot count as successful tasks. Failed attempts remain in the summary denominator. The recommended worker reads the selected key from its dotenv file into process memory after a clean startup; credential values are never written to transcripts or contracts. The macOS limitation below explains why startup environment keys are unsuitable here.

All three search adapters charge only delivered source `content` fields. Ripgrep uses JSON events, coalesces adjacent lines, and supplies explicit relative paths even when the inventory has a single file. Path/line metadata remains included in total tool-output and provider input tokens. An initial three-completed-trial preflight was stopped after independent audit found that the older ripgrep text adapter omitted single-file paths and charged its line labels as source. Those three successes and the interrupted fourth trial remain preserved in `.benchmark-results/agent-qwen-20261001/PREFLIGHT_STATUS.json`; they are excluded from the separately frozen corrected sweep, not relabeled as task failures.

The defaults are temperature 0, thinking disabled, 4,096 output tokens per response, 8,000 cumulative source/diagnostic tokens, 40 tool calls, and 180 seconds per trial. Provider request pacing counts toward wall time. `--request-interval 6.1` follows Hetzner's documented 10-requests-per-minute limit; avoid sharing that quota with other experiments. This measures an end-to-end paced API workflow, not just retrieval speed. The fixed model identifier does not freeze provider weights, server hardware, caching, or load.

```sh
.benchmark-venv/bin/python -m unittest discover -s benchmarks/agents -p 'test_*.py'

# Keep credentials out of the initial process environment and arguments.
env -i PATH="$PATH" HOME="$HOME" LANG=en_US.UTF-8 \
  .benchmark-venv/bin/python benchmarks/agents/run_api.py \
  --output .benchmark-results/agent-qwen-controlled-NEW \
  --flexcontext .benchmark-results/iteration-20261001-pass2-accepted/flexcontext-candidate \
  --probe .benchmark-tools/node_modules/@probelabs/probe/bin/probe-binary \
  --model Qwen/Qwen3.6-35B-A3B-FP8 \
  --key-file "$HOME/.env" \
  --base-url https://inference.hetzner.com/api/v1 --repeats 3

.benchmark-venv/bin/python benchmarks/agents/summarize_api.py \
  .benchmark-results/agent-qwen-controlled-NEW/results.json
```

Use the actual Probe executable, `probe-binary`, rather than its npm launcher: the runner copies executables into the frozen experiment directory. Fixture inventory checks ignore Finder's `.DS_Store` metadata only; source hashes remain mandatory. The three tasks remain assistant-authored seeded smoke cases. Repeating them does not create new independent tasks, representative maintenance coverage, independent author review, or evidence of held-out task-success parity. Official provider documentation: [Hetzner Inference](https://docs.hetzner.com/general/company-and-policy/experiments/inference/).

### macOS credential boundary and versioned continuation

A dummy-only privacy audit found that macOS 27.2 permits a direct `KERN_PROCARGS2` call to read another process's initial environment under the same user, even when the sandbox blocks launching `ps`. Removing broad process/sysctl allowances did not mediate that kernel path on this host. This sandbox therefore does **not** establish hostile-code containment or general isolation from host process environments. These runs use the trusted authored fixtures above; completed source patches were inspected separately.

The API worker now launches with `env -i`, accepts only a short list of nonsecret environment names, and uses `--key-file` to load the selected dotenv assignment into trusted Python memory after startup. It neither evaluates shell syntax nor exports the selected value. A regression test supplies a fake sentinel to two dedicated dummy workers: a startup-environment positive control exposes it through the direct syscall, while a clean-environment worker reading it into memory after startup does not. No real worker environment or real credential was queried in that test. Public-test subprocesses still receive minimal environments, and filesystem/network checks remain in place. This protects the benchmark's API credential from the observed process-arguments channel; it does not claim to protect arbitrary other host processes' environments.

The corrected sweep was paused after eight completed trials to make this change. All eight final patches were independently inspected and regraded; their frozen hidden and public checks still pass. `--continue-from <previous results.json>` verifies the model, prompts, schemas, source/task hashes, binaries, pacing, and budgets match, preserves every completed trial (including any failures), and schedules only the remaining task/arm/repetition keys. The continuation contract records both harness versions and administrative interruptions, including partial reported usage and an explicit incomplete-usage flag. Final combined data is in `.benchmark-results/agent-qwen-memory-key-20261001`; the earlier eight workspaces and interrupted ninth trace remain in `.benchmark-results/agent-qwen-controlled-20261001`. This infrastructure amendment is a comparability limitation and is disclosed with the results.

The frozen lexical acceptance suite also missed non-ASCII numeric boundaries. `unicode_numeric_audit.py` provides a separately labelled post-hoc check across every final lexical patch. It must not overwrite the frozen grading outcome: frozen-check passes and this broader correctness audit are reported separately. All nine final lexical patches passed the frozen suite but failed the expanded Unicode-number checks; the production reference passes them and the seeded state fails. These known incomplete repairs demonstrate why these smoke results cannot support a general task-completion claim.

### Completed 27-trial results

The fixed `Qwen/Qwen3.6-35B-A3B-FP8` agent completed three repetitions of each task with each provider. All 27 traces, source/call/time limits, protected files, frozen hidden tests, and independent final public regressions pass. **These are frozen-suite passes, not 27 complete repairs:** every lexical repair fails the separate Unicode-number diagnostic, such as splitting `user٢Token` into `user`, `٢`, and `token`.

| Search workflow | Frozen suite | Post-hoc Unicode numeric checks | Mean wall time | Mean source/diagnostic tokens | Mean tool-output tokens | Mean tool calls | Mean reported model tokens |
|---|---:|---:|---:|---:|---:|---:|---:|
| Ripgrep | 9/9 | 0/3 | 63.96 s | 1,794 | 2,318 | 8.67 | 28,056 |
| Accepted Flexcontext | 9/9 | 0/3 | 67.41 s | 1,865 | 3,065 | 8.22 | 34,410 |
| Probe | 9/9 | 0/3 | 70.69 s | 1,964 | 2,480 | 8.78 | 30,472 |

Each pair of workflows has nine paired frozen-suite passes and zero one-sided wins. There are no scheduled-trial API errors, timeouts, missing usage receipts, or invalid traces. These three task clusters do not establish general completion parity, non-inferiority, or a reliable timing advantage. Provider pacing and server caching/load affect elapsed time. Reported model tokens include cached input according to the provider's receipts and do not imply a dollar cost.

The primary cohort used **836,444 reported model tokens**. Calibration, the aborted adapter preflight, and administrative interruptions used **at least another 236,644**; interrupted in-flight usage is incomplete and remains explicitly unknown. The eight trials before the credential amendment are preserved and independently regraded. The agent runs used accepted Flexcontext binary `06566aee…a324`; a separate replay of all 11 recorded Flexcontext searches across its nine trials returned identical result arrays with the final production binary. Independent patch review found only the intended edits across all 27 final workspaces and no protected-file changes.

An external `xcode-select` change to Xcode-beta occurred after the lexical trials. It caused a later harness test to fail on SDK access/license setup. Future CLI fixture subprocesses now explicitly use the installed Command Line Tools, with selected-Xcode fallback when unavailable; the user's global selection and the active frozen sweep were not changed. All **16 harness tests pass** with that runtime choice.

Durable evidence: [full sanitized agent summary](../results/iteration-20261001-pass3/agent-completion.json), [post-hoc Unicode audit](../results/iteration-20261001-pass3/agent-unicode-audit.json), [final-binary search replay](../results/iteration-20261001-pass3/agent-search-equivalence.json), and [independent patch review](../results/iteration-20261001-pass3/agent-patch-review.json). Full traces, provider receipts, source workspaces, acceptance logs, and both harness snapshots remain in the ignored experiment directories described above.

## Next experiment: real maintenance tasks

1. **Author and freeze tasks before retrieval tuning.** Start with 12 development tasks across Flexcontext/Rust, AgentX/TypeScript and a Java project with a working local test suite. Use actual bug reports or feature requirements and full start snapshots, including explicitly captured dirty changes where applicable. Include multi-file repairs, regression-sensitive behavior and plausible distractors. Require the initial snapshot to fail task acceptance and a reviewed reference implementation to pass it. Keep gold patches and hidden tests outside the worker. Record task authorship; the current smoke tasks are assistant-authored.
2. **Compare workflows while fixing the agent.** Begin with ripgrep plus targeted reads, prior Flexcontext plus targeted reads, and candidate Flexcontext plus targeted reads. Use identical prompts for the two Flexcontext versions, the same model/effort, time/call/context limits, dependencies and public test commands. Allow the agent to choose queries, follow references and test its patch. Add Probe/Aider later using the same broker rather than mixing different agents, prompts and tools in the first comparison.
3. **Calibrate on development tasks.** Run three independent attempts per condition/task, randomizing execution order within each task/repetition. Tune budgets and task difficulty here, not on held-out tasks. Log failures separately as agent failure, resource exhaustion, invalid access, infrastructure failure or failed regression checks. Do not optimize retrieval against test names or gold paths.
4. **Freeze a held-out set by module/issue family.** Start with at least 30 previously unseen tasks, then expand language/repository coverage. Reserve entire dependency clusters to avoid near-duplicate issues crossing splits. Run five attempts per condition/task to expose variance. A 30-task × three-condition × five-attempt sweep is 450 agent trials; it is a proposed later experiment, not an automation or a run launched by this pass.
5. **Grade artifacts independently.** Require all hidden task tests, specified regression tests, unchanged protected files, valid tool access and resource compliance. A compiler error in the patch is a task failure. A missing compiler/service is an infrastructure failure. Track both scheduled-trial completion (invalid/failed trials remain in the denominator) and valid-trial success, with infrastructure counts visible; never silently drop inconvenient runs.
6. **Promote on completion and efficiency.** Report pass@1 over independent attempts, paired wins/losses by task, and success per repository/task family. Use task-cluster bootstrap confidence intervals, keeping repeated attempts within their task cluster. Predeclare a practical non-inferiority margin and stop/go rule before held-out execution. Require no meaningful completion loss; seek lower time/context/total model consumption at equal success. Thirty tasks calibrate the process and may still be underpowered for narrow differences.

The dashboard should keep these measures separate:

| Measure | Purpose |
|---|---|
| Independently verified task completion | Primary effectiveness KPI |
| Regression test outcome and patch scope | Correctness and unintended-change detection |
| Agent wall time, grading time, timeout rate | End-to-end workflow efficiency |
| Input/cached-input/output/reasoning usage | Model consumption; preserve the receipt's semantics |
| Cumulative source/diagnostic and broker payload tokens | Retrieval/read amplification and tool overhead |
| Searches, reads, edits, tests; searches before first edit | Explain the workflow and failure mode |
| Aggregate consumption across all attempts / completed tasks | Efficiency including failed attempts; undefined when no task completes |
| Infrastructure/invalid-trace rates | Reliability of the evaluation itself |

Retrieval evidence can diagnose *why* completion changed, but cannot replace the completion result. Dollar cost needs verified model pricing and receipt semantics before calculation. A complementary unrestricted native-tool experiment can measure normal developer workflow benefit later; its context costs require independent measurement and should not be mixed with this controlled comparison.
