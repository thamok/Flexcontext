# Pilot: 16 September 2026

Completed **540 measurements, 15 questions, two source-token budgets (2,048 and 4,096), four real engines, and three scoped repository snapshots**. All commands succeeded. Ten harness tests pass. The artifact audit verified 4,099 source files, 166,519 returned located lines, every token cap, and every recomputed score. No model calls or coding-agent tasks were run.

This is an assistant-authored pilot with sparse, non-independent judgments. It is not a general leaderboard. See [the measurement contract](README.md) before interpreting the results.

## Retrieval quality

Means below use the two warm CLI repetitions per question. Precision measures returned source lines inside authored relevant implementation regions; unjudged lines count as irrelevant. File recall requires actual selected source from the evidence file.

| Repository | Tool | Evidence recall 2k | Evidence recall 4k | Region precision 4k | Source-file recall 4k |
|---|---|---:|---:|---:|---:|
| flexcontext | flexcontext | 66.7% | 66.7% | 7.1% | 100.0% |
| flexcontext | probe | 66.7% | 100.0% | 12.8% | 100.0% |
| flexcontext | rg | 0.0% | 16.7% | 1.3% | 16.7% |
| flexcontext | aider | 0.0% | 16.7% | 1.1% | 66.7% |
| agentx | flexcontext | 58.3% | 58.3% | 2.1% | 100.0% |
| agentx | probe | 66.7% | 83.3% | 6.5% | 83.3% |
| agentx | rg | 0.0% | 0.0% | 0.0% | 0.0% |
| agentx | aider | 0.0% | 0.0% | 0.0% | 16.7% |
| webkitpy | flexcontext | 100.0% | 66.7% | 2.0% | 100.0% |
| webkitpy | probe | 100.0% | 66.7% | 1.8% | 100.0% |
| webkitpy | rg | 0.0% | 0.0% | 0.0% | 0.0% |
| webkitpy | aider | 0.0% | 0.0% | 0.0% | 0.0% |

The main observed Flexcontext gap is **finding the right file but selecting the wrong units inside it**. At 4k its file recall is 100% in all three groups, while evidence recall is 58–67%. The remote OIDC fallback question returns configuration helpers/types instead of the fallback branches; the cache question returns cache types/write helpers instead of compatibility/reuse logic; the selection-diversity question returns wrappers/tests instead of the selection implementation. These diagnoses come from the saved outputs, not inferred causes inside the ranking algorithm.

Probe recovers more of the authored body evidence on Flexcontext and AgentX; it ties Flexcontext on the three WebKit-tooling questions at 4k. Aider is a signature/navigation map and should also be evaluated with follow-up reads. Its low direct-body recall does not establish poor agent task success. The `rg` policy is fixed OR terms plus path order, not an agent refining precise searches.

The budget curve is not monotonic: both Flexcontext and Probe scored 100% evidence recall on the three webkitpy questions at 2k, falling to 66.7% at 4k. Native byte limits and snippet sizes change with the requested budget, then the adapter applies a token-prefix cap; the selected contexts are therefore not guaranteed to be nested. This result belongs to the documented adapter policy and warrants a fixed-candidate-pool ablation before attributing the regression to an engine alone.

## Native retrieval latency at 4k

Milliseconds, median over the observed questions/repetitions. Cold has one sample per question; warm and resident-repeat have two. These are native response times, **excluding the separately recorded normalization, scoring, and token-accounting overhead**. OS page caches were not flushed. Measurements ran serially on an Apple M4, 10 logical CPUs, 16 GiB RAM, with Rayon limited to four workers; the host was not exclusive.

| Repository | Tool | Tool-cache cold ms | Warm CLI ms | Resident-repeat ms |
|---|---|---:|---:|---:|
| flexcontext | flexcontext | 53.0 | 25.1 | 9.2 |
| flexcontext | probe | 197.6 | 174.5 | N/A |
| flexcontext | rg | 10.7 | 9.9 | N/A |
| flexcontext | aider | 762.1 | 425.1 | 7.5 |
| agentx | flexcontext | 4,317.7 | 1,492.5 | 627.5 |
| agentx | probe | 1,691.2 | 1,680.1 | N/A |
| agentx | rg | 293.3 | 270.9 | N/A |
| agentx | aider | 20,939.6 | 1,065.3 | 457.0 |
| webkitpy | flexcontext | 975.0 | 286.7 | 103.9 |
| webkitpy | probe | 949.2 | 1,003.5 | N/A |
| webkitpy | rg | 85.7 | 46.9 | N/A |
| webkitpy | aider | 2,538.3 | 889.3 | 179.4 |

Resident startup is separate: Flexcontext used approximately 19 ms / 1,241 ms / 315 ms for the Flexcontext / AgentX / webkitpy snapshots. Aider bridge startup was approximately 207 ms / 192 ms / 157 ms; graph/map work can remain in the first query. Probe and rg resident engines were not implemented. Do not interpret N/A as zero or compare a resident result to a CLI result without that qualification.

## Payload cost at 4k

Mean over warm CLI requests. Source-token costs use the shared additive line tokenizer. Context tokens include the adapter’s file/line citations. Native bytes/tokens are the complete raw stdout before capping.

| Repository | Tool | Source bytes | Source tokens | Context tokens | Native bytes | Native tokens |
|---|---|---:|---:|---:|---:|---:|
| flexcontext | flexcontext | 18,233 | 4,080 | 7,517 | 108,106 | 29,762 |
| flexcontext | probe | 18,761 | 4,090 | 7,444 | 95,000 | 25,238 |
| flexcontext | rg | 18,124 | 4,090 | 7,646 | 381,890 | 102,130 |
| flexcontext | aider | 13,636 | 3,340 | 5,978 | 17,589 | 5,409 |
| agentx | flexcontext | 17,385 | 4,088 | 11,941 | 125,463 | 34,855 |
| agentx | probe | 17,367 | 4,088 | 11,862 | 192,082 | 52,008 |
| agentx | rg | 17,329 | 4,090 | 10,111 | 34,258,997 | 8,880,581 |
| agentx | aider | 12,133 | 2,874 | 8,334 | 17,787 | 5,380 |
| webkitpy | flexcontext | 18,879 | 4,084 | 10,003 | 143,908 | 36,989 |
| webkitpy | probe | 19,168 | 4,083 | 9,875 | 146,206 | 39,245 |
| webkitpy | rg | 19,191 | 4,089 | 9,612 | 6,075,061 | 1,583,206 |
| webkitpy | aider | 10,953 | 2,342 | 6,262 | 16,494 | 4,737 |

**The rg raw payload is deliberately exhaustive and can be enormous.** This version buffers all JSON matches before the common source cap; the resulting raw-byte/token count measures this adapter policy, not an unavoidable cost of ripgrep. Streaming early termination, better queries, and ranked/agent-driven rg policies are necessary additional baselines before a product comparison. Native candidate overfetch and post-processing are also disclosed for the other engines. Aider often underfills the source budget because its own native map budget includes formatting and file names.

The audit found 21 case/tool/budget groups whose context differed across modes or repeats (Aider and Probe). Evidence recall changed in 0 groups. All individual outputs, scores and timings remain available; context variance is not hidden by one selected run.

## Reproducibility and next stage

The local run is `.benchmark-results/pilot-20260916/`. `REPORT.md` and `summary.json` include both budgets and every measured mode; `rows.jsonl` retains all 540 samples. `verification.json` records the audit. `environment.json`, `dependency-versions.json`, `requirements.lock`, `host.json`, archived harness source, and source manifests identify the run. Raw responses and local code stay in the ignored results directory.

Snapshots: Flexcontext 51 files / 204,858 bytes; AgentX 3,445 files / 24,036,086 bytes; WebKit tooling 603 files / 5,369,240 bytes. These are the scopes in `repos.json`, not complete checkout sizes. WebKit means `Tools/Scripts/webkitpy`, not the C++ browser engine. Current uncommitted source was included and hashed.

Tools: local release Flexcontext binary (SHA-256 recorded), Probe npm 0.6.0-rc339 native binary, local rg (version recorded), Aider 0.86.2, tiktoken 0.12.0/cl100k_base. SciPy 1.16.3 replaces Aider’s pinned 1.15.3 because that wheel failed to load on this macOS host; the RepoMap implementation was not modified.

**120 paired, tool-blinded answer prompts** were exported to `answer-prompts/`. The README includes Codex/OpenCode launch examples and the required trace audit. No answer-quality or task-success scores are claimed yet. Before the agent stage: independently review/expand gold evidence, add held-out questions beyond authentication and these familiar modules, add a realistic query-planning/streaming-rg baseline, and then compare the same model with cumulative read budgets and executable acceptance tests.
