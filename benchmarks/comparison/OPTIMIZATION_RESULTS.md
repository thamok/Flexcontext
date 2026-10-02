# Focused retrieval results

Decision: **retain baseline retrieval; keep focused policy opt-in**.

36 frozen, source-reviewed questions; 18 development and 18 held out by module. Original 15 pilot questions remain development diagnostics. Local snapshots cover Flexcontext, AgentX and WebKit Python tooling. No agent-success evaluation or model calls.

| Acceptance gate | Result |
|---|---|
| development | FAIL |
| heldout_recall | FAIL |
| irrelevant_source | FAIL |
| compact_metadata | PASS |
| resident_latency | PASS |

Held-out irrelevant source-token change: **+0.3%**. Non-source structured payload-token change: **-65.3%**. These are overall paired native-adapter totals; metadata reduction includes both presentation and changes in returned results.

## Held-out quality

| Repository | Source budget | Baseline recall | Focused recall | Baseline precision | Focused precision | Fixed-pool baseline recall | Fixed-pool focused recall |
|---|---:|---:|---:|---:|---:|---:|---:|
| flexcontext | 2048 | 66.7% | 75.0% | 9.6% | 9.4% | 66.7% | 75.0% |
| flexcontext | 4096 | 100.0% | 75.0% | 7.3% | 4.9% | 100.0% | 75.0% |
| agentx | 2048 | 16.7% | 66.7% | 3.4% | 5.4% | 16.7% | 50.0% |
| agentx | 4096 | 33.3% | 83.3% | 2.1% | 3.5% | 33.3% | 83.3% |
| webkitpy | 2048 | 66.7% | 50.0% | 3.0% | 2.3% | 50.0% | 50.0% |
| webkitpy | 4096 | 66.7% | 50.0% | 1.6% | 1.1% | 66.7% | 50.0% |

The native-adapter regressions are parser fact collection and the legacy MCP wait condition at 4k, and WebKit command-error tail extraction at both budgets. In all these cases the candidate still returns the correct file, but omits the required implementation evidence. This remains an evidence selection/ranking problem within discovered files.


## Resident latency

20 timed repetitions per held-out question and budget, after warming each query; 120 samples per repository/budget/tool. Calls ran serially in randomized order. OS page cache was uncontrolled.

| Repository | Budget | Baseline p95 ms | Focused p95 ms | Ratio |
|---|---:|---:|---:|---:|
| flexcontext | 2048 | 12.35 | 10.19 | 0.825 |
| flexcontext | 4096 | 11.62 | 11.01 | 0.947 |
| agentx | 2048 | 706.09 | 152.79 | 0.216 |
| agentx | 4096 | 713.82 | 162.84 | 0.228 |
| webkitpy | 2048 | 545.03 | 23.13 | 0.042 |
| webkitpy | 4096 | 550.73 | 24.16 | 0.044 |

## Development ablations

Averages across the 18 development questions and both source budgets using the native adapter. Each isolated row changes one retrieval mechanism; the focused rows combine the changes. All experimental rows use compact presentation, while the frozen baseline emits full detail, so the metadata column must not be attributed solely to the retrieval mechanism. Cutoff selection also checks every repository/budget group with the fixed-pool adapter.

| Policy | Recall | Precision | Irrelevant source tokens/query | Metadata tokens/query |
|---|---:|---:|---:|---:|
| baseline | 80.6% | 5.9% | 2911 | 16324 |
| cutoff-0.25 | 80.6% | 5.9% | 2910 | 2512 |
| cutoff-0.4 | 80.6% | 5.9% | 2911 | 2353 |
| cutoff-0.55 | 80.6% | 6.5% | 2677 | 1904 |
| direct | 80.6% | 5.8% | 2911 | 2631 |
| diversity | 83.3% | 6.1% | 2907 | 2260 |
| focus | 77.8% | 5.8% | 2914 | 2659 |
| focused-0.25 | 77.8% | 5.9% | 2861 | 5120 |
| focused-0.4 | 77.8% | 5.9% | 2824 | 4901 |
| focused-0.55 | 63.9% | 6.9% | 2315 | 3720 |
| idf | 66.7% | 5.0% | 2932 | 2886 |
| implementation | 83.3% | 5.9% | 2909 | 2692 |
| quotas | 77.8% | 5.8% | 2914 | 2814 |
| relations | 77.8% | 5.6% | 2922 | 2443 |
| stable | 80.6% | 6.1% | 2856 | 5045 |

## Four-tool comparison

All 36 questions, warm CLI quality, 2k and 4k source-token ceilings. CLI times are means, including startup. This table reports measured adapter behavior, not a general tool leaderboard. Probe and ripgrep resident measurements are unavailable in this harness.

| Repository | Budget | Tool/policy | Recall | Precision | Source tokens | Native tokens | Cold CLI ms | Warm CLI ms |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| agentx | 2048 | aider | 0.0% | 0.0% | 1439 | 2772 | 20200.41 | 973.67 |
| agentx | 4096 | aider | 4.2% | 0.2% | 2870 | 5390 | 20215.92 | 1058.85 |
| agentx | 2048 | baseline | 50.0% | 8.2% | 2041 | 24452 | 3900.15 | 1267.99 |
| agentx | 4096 | baseline | 66.7% | 4.7% | 4090 | 38425 | 3929.53 | 1285.98 |
| agentx | 2048 | focused-0.25 | 75.0% | 9.2% | 2044 | 7140 | 4548.25 | 1830.29 |
| agentx | 4096 | focused-0.25 | 91.7% | 5.5% | 3928 | 13432 | 4493.45 | 1821.02 |
| agentx | 2048 | probe | 37.5% | 3.7% | 2042 | 24352 | 1415.06 | 1360.38 |
| agentx | 4096 | probe | 50.0% | 2.5% | 4089 | 37360 | 1397.79 | 1376.63 |
| agentx | 2048 | rg | 0.0% | 0.0% | 2028 | 4965213 | 212.71 | 207.49 |
| agentx | 4096 | rg | 0.0% | 0.0% | 4088 | 4965212 | 215.57 | 212.51 |
| flexcontext | 2048 | aider | 12.5% | 1.6% | 1617 | 2673 | 724.06 | 446.90 |
| flexcontext | 4096 | aider | 25.0% | 1.4% | 3301 | 5366 | 726.58 | 433.78 |
| flexcontext | 2048 | baseline | 83.3% | 8.2% | 2042 | 19645 | 46.20 | 22.10 |
| flexcontext | 4096 | baseline | 100.0% | 5.4% | 4090 | 34445 | 46.73 | 22.38 |
| flexcontext | 2048 | focused-0.25 | 87.5% | 8.4% | 2039 | 7579 | 56.59 | 31.30 |
| flexcontext | 4096 | focused-0.25 | 87.5% | 4.2% | 4091 | 14859 | 55.26 | 30.82 |
| flexcontext | 2048 | probe | 91.7% | 10.6% | 2043 | 12954 | 147.82 | 147.44 |
| flexcontext | 4096 | probe | 83.3% | 5.2% | 4093 | 23231 | 187.17 | 168.38 |
| flexcontext | 2048 | rg | 11.1% | 1.7% | 2045 | 71538 | 9.11 | 9.66 |
| flexcontext | 4096 | rg | 61.1% | 3.4% | 4009 | 71538 | 12.26 | 8.33 |
| webkitpy | 2048 | aider | 0.0% | 0.2% | 1290 | 2575 | 2440.91 | 774.16 |
| webkitpy | 4096 | aider | 0.0% | 0.1% | 2442 | 4888 | 2686.78 | 837.15 |
| webkitpy | 2048 | baseline | 58.3% | 3.2% | 2042 | 26287 | 1218.91 | 550.75 |
| webkitpy | 4096 | baseline | 58.3% | 1.7% | 4089 | 37221 | 1107.55 | 432.10 |
| webkitpy | 2048 | focused-0.25 | 41.7% | 2.2% | 2040 | 8835 | 1164.21 | 481.98 |
| webkitpy | 4096 | focused-0.25 | 50.0% | 1.5% | 4088 | 15648 | 1213.88 | 495.58 |
| webkitpy | 2048 | probe | 45.8% | 3.1% | 2041 | 27988 | 928.65 | 935.28 |
| webkitpy | 4096 | probe | 45.8% | 1.6% | 4028 | 42431 | 978.05 | 953.73 |
| webkitpy | 2048 | rg | 0.0% | 0.0% | 2045 | 1392714 | 55.59 | 53.55 |
| webkitpy | 4096 | rg | 0.0% | 0.0% | 4086 | 1392714 | 52.61 | 51.70 |

## Interpretation and audit

A failed recall gate rules out promotion even if shorter responses improve precision or token counts. No lower-performing repository/budget group is hidden by the overall average. The fixed-pool results isolate the pilot adapter’s changing overfetch ceiling; native source spans have separate monotonicity regression tests.

The initial development-only implementation was preserved separately as `optimization-20260916`. Before held-out execution, relationship-only and stable-only ablation controls were corrected to avoid mixing path filtering, quota removal and diversity changes. The authoritative final run is this directory’s frozen contract and executables. No held-out tuning followed.

After measurement, the human-readable `--explain` renderer was fixed to display the already-existing trace, and scope/trace regression assertions were added. This diagnostic-only change does not alter retrieval or normal JSON/MCP output. The measured executable hashes remain recorded separately from the final workspace build.

Evidence labels are source-reviewed but assistant-authored, and precision counts unjudged lines as irrelevant. Repo coverage is deliberately limited, particularly WebKit Python tooling. Ripgrep’s exhaustive OR/context adapter produces substantial overfetch; these results do not establish an inherent ripgrep cost. Aider RepoMap is primarily navigation and commonly emits signatures rather than behavioral bodies.

Verified 534,773 located source lines, all source/evidence hashes, all shared source-token ceilings, all scores, native Flexcontext byte accounting and 20 resident repetitions. Compressed raw outputs, normalized source lines, manifests, executable hashes, the implementation archive and machine-readable summaries remain in the local run directory.

## Presentation-only control

With baseline retrieval held constant, compact output preserves all source text, source spans, ordering and truncation flags in all 36 held-out question/budget pairs. Non-source payload tokens change by **-83.8%**. This isolates the default wire-format benefit from the failed focused retrieval changes.

The additional compact-baseline resident timings use 20 repetitions per question/budget in a separate run block. They are descriptive and are not substituted into the randomized acceptance gate. See `presentation-summary.json`.

## Next development targets

Removing the repeated-file penalty and adding the small implementation preference each improve aggregate development recall from 80.6% to 83.3%. File-frequency weighting alone falls to 66.7%; the weighting experiment should be revisited before combining it with selection changes. A 0.55 cutoff alone keeps aggregate recall at 80.6% but reduces irrelevant source by only about 8%, below the 25% target. These are development observations, not independently accepted production variants.

The next retrieval experiment should diagnose term weighting and statement selection on development cases, retain the now-established compact/scoped interfaces, and use a new held-out split for any further tuning-informed promotion claim. No further ranking changes were made after inspecting this held-out run.
