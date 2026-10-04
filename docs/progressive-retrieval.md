# Progressive retrieval: implementation and bounded agent pilot

**Result:** exact continuations work, including real agent-chosen expansion, but this pilot did not show better required-evidence coverage or lower context cost. Cheap role hints added no established investigation benefit. Keep continuations and hints opt-in; no learned classifier was justified or added.

## What was compared

Eight assistant-authored investigations were frozen before evaluation in `benchmarks/agents/progressive/cases.json`, with exact source assertions and SHA256 inventories: four development cases (lifecycle, real cache handling, real lexical matching, misleading/opaque worker names) and four confirmation cases (real output budgeting, the existing AgentX-derived limiter fixture, one-call arithmetic, and cleanup outside a session-focused candidate pool). Source copies preserve the starting checkout. The limiter deliberately retains its existing seeded LIFO regression; it does not represent current AgentX behavior. These cases and reviews are not human-adjudicated or broadly representative.

A uses ordinary compact search and follow-up searches/reads; B adds unlabelled actionable leads/expansion; C adds cheap optional role hints. Ranking and selection settings are identical. The agent chooses queries and tools; prompts contain no gold method names or expansion coaching. Each arm receives its normal tool descriptions.

Fixed limits: **one initial search plus at most two follow-ups; 6,000 cumulative and 2,400 per-response `cl100k_base` tokens of complete delivered JSON; 4,800 selected-source bytes; six search results; product serialized estimate 1,800 tokens (bytes/4).** Errors, navigation, wrappers and repeated source count. Oversized responses produce charged errors. API framing/model input is accounted separately by provider receipts. The existing `run_api.Client` uses Qwen/Qwen3.6-35B-A3B-FP8, temperature 0, thinking disabled, 1,400 output tokens per response, 150 seconds/trial and 6.1-second request pacing. Arm order is randomized with seed 1729; one attempt per case/arm.

## Stable confirmation results

All twelve confirmation trials used the same frozen binary (`cf693fff…fb8fb8`) and unchanged limits. Coverage requires delivered implementation text; names, hints and candidate lists never count. Final explanations were manually compared with source separately.

| Four confirmation cases per arm | A: current | B: leads | C: hints |
|---|---:|---:|---:|
| Complete required-source coverage | 4/4 | 4/4 | 4/4 |
| Complete, correct final investigations | 4/4 | 3/4 | 4/4 |
| Retrieval calls | 9 | 9 | 10 |
| Charged tool errors | 1 | 0 | 0 |
| Exact expansions | 0 | 1 | 1 |
| Expansions adding required evidence | 0 | 0 | 0 |
| Total delivered payload tokens | 4,950 | 6,213 | 6,992 |
| Total delivered payload bytes | 19,503 | 23,467 | 26,327 |
| Navigation-object tokens | 0 | 1,225 | 1,423 |
| Repeated source-line bytes | 2,665 | 2,650 | 3,729 |
| Source bytes displaced by navigation | 0 | 1,609 | 1,609 |
| Mean paced wall time | 39.1 s | 33.5 s | 37.0 s |
| Reported model input tokens | 18,718 | 24,198 | 28,505 |
| Reported model output tokens | 2,979 | 2,995 | 2,627 |

B's limiter explanation reached the model output cap and ended mid-sentence, despite complete source and correct required facts before truncation. It remains a failed bounded completion; this single stochastic difference does not establish a classifier advantage. A recovered from an oversized full-file read. C made an extra test-oriented search without establishing tests. Limiter A's eventual-drain wording needs the usual finite-arrivals/settling-tasks qualification; no fairness guarantee was established.

On the output case, initial truncated source hid `results.pop()` in every arm. Targeted reads recovered it. B/C navigation additionally displaced 1,609 bytes, including `Representation::render`, which A initially received. Both agents then independently chose to expand `render`: the references worked, but this corroboration added no frozen required evidence and partly recovered source displaced by navigation itself. This is an actual agent-choice result, distinct from the scripted API demonstration.

All arms stopped after one call for the complete arithmetic case. For storage cleanup, agents followed the discovered storage path with ordinary reads; they did not assume leads were exhaustive or invent a scheduler. `outside-pool.json` separately demonstrates that a session query omits `sweep` from the retained pool and a new `records delete` query finds it. The recorded agents chose reads, not that scripted query.

## Calibration, hints and limitations

Ten earlier completed development trials used the initial larger reference encoding. Four B/C trials suffered payload-cap errors and a provider HTTP400 when the old runner appended a late system message. One further trial was administratively interrupted. References were compacted, the control message changed to a user message, and the two unfinished development A/B slots ran on the confirmation binary. **Do not aggregate those mixed-binary development arms as a controlled comparison.** All failed and partial receipts are retained. In the cache probe, compact references reduced full output from 3,609 to 1,390 measured tokens without ranking changes. Development also exposed correct source coverage followed by incorrect agent explanations (invented lexical stems; overstated cache-flush durability).

Hints remain fallible source-word cues. In `hint-check.json`, read-only `cancelMetrics` receives `cancel`, while `pump` has no hint. The actual agent correctly understood the opaque worker by reading source. For the identical `worker` query, selected source was identical across A/B/C; B/C emitted 785/788 measured tokens. Five warm-cache CLI timing samples are retained but too noisy to attribute tiny runtime differences. There is neither an observed role-caused investigation failure nor credible independent role labels here to justify training a local classifier. D was therefore not attempted; learned-model runtime/memory/artifact metrics are inapplicable. No extra generative model, embeddings, or indexing rewrite was introduced into the product.

The confirmation consumed **80,022 reported model tokens**. Across calibration, the resumed development slots and confirmation, receipts establish **at least 133,466 model tokens**; interrupted in-flight usage is unknown. No monetary cost is inferred. All 24 completed trial traces match their broker request arguments and payload hashes. Repeated-source bytes use exact nonblank line equality within a path; this includes shared syntax and is not semantic duplicate detection. Small authored corpora, one attempt per arm, provider caching/load, explanation-review judgment and model nondeterminism limit generalization.

Reviewable tool payloads, explanations, coverage, usage, errors and hashes are in `benchmarks/results/progressive-20261002/results.json`; contracts, manual judgments and deterministic probes sit beside it. Raw API responses remain in `.benchmark-results/progressive-*`. Product duplicate/overlap hardening landed after the live binary was frozen. Replaying all 28 confirmation calls against the final `06811e36…` binary returned exactly equal full payloads, source, navigation, errors and token charges (`final-binary-replay.json`). The final 72 frozen default-retrieval arrays also remained identical (`default-equivalence.json`). Replay validates delivered-context equivalence; it is not a new live agent run.

## Delivered interface and verification

`search --continuations` / MCP `code_search` with `continuations: true` adds `navigation` to compact or full output. `--role-hints` / `role_hints: true` adds optional source-word cues. The normal experimental variant is unlabelled. Neither option changes ranking or selection. Disabled searches omit navigation and preserve existing result arrays. Root `README.md` is unchanged.

Each lead has a source path, symbol, kind, container, starting line, omission reason and opaque reference. `expand ROOT REFERENCE` / MCP `expand_context` with `reference` resolves one source candidate directly, without ranking or quotas. Passing `navigation.next` to the same operation returns another navigation page, without replaying source. Both MCP protocol versions and compact/full JSON/human representations are supported.

Navigation retains the first 64 post-policy candidates, displays at most four leads per page, and caps missing ranges at 256. `remaining` counts omitted candidates in the current retained page sequence, including the displayed leads; `outside_retained` counts discovered candidates excluded by retention/fragmentation limits. It does not count filtered or undiscovered repository code. Those need another search. Duplicate-source mappings are capped at 4,096; excessive mappings fail explicitly. A scope/reference exceeding 32 KiB also fails before advertising unusable references.

References authenticate repository/index snapshot, original scope, candidate identity, and remaining source ranges with HMAC. Opt-in search creates one private `.flexcontext/progressive-key-v1` signing key, including with `--no-cache`. This is key material, not a session database. References survive CLI/server restart only with the same canonical repository, indexed source, cache compatibility fingerprint and key. Changed snapshots or deleted/replaced keys invalidate references; refresh and search again. Resident searches continue to use their immutable snapshot until `refresh_index`, as before. Expansion cannot override scope.

Delivered byte spans are subtracted from source, including translated spans of indexed byte-identical copies and nested symbols inside delivered parents. Partial overlap does not erase the rest of a symbol. Partial expansion can split a statement; `content_truncated`, exact `source_spans`, signature and container identify what was emitted. Follow the new partial reference for remaining bytes. There is no hidden shared seen-state: retrying a reference repeats that request, and separate old/sibling references do not acquire knowledge of subsequent calls.

Navigation, signatures, cost fields and protocol framing count toward the serialized budget. Finalization removes source monotonically and recomputes omissions, with `displaced_source_bytes` recording source removed during this stage. It does not rerun selection. If metadata cannot fit, search fails explicitly. Exact expansion instead reports insufficient serialized budget rather than silently dropping its requested source; lower the source budget (`--max-bytes`, MCP `budget` × 4) or raise `max_tokens`.

The failing regression preceded implementation. The runnable demonstration returns exactly two initial method bodies (`loginSession`, `refreshSession`), advertises `revokeSession` and `expireSession` as container-quota omissions, and fetches each omitted body successfully. A real `src/lexical.rs` search for `tokenize identifier` also reproduced quota omissions of `matching_query_terms`, `lexically_related`, and `light_stem`.

Final local validation: **71 Rust tests**, including **13 progressive-retrieval tests**, plus eight existing benchmark smoke checks, four evaluation-harness tests, strict Clippy, and clean diff checks. Tests cover exact fetches, result/source/serialized omissions, duplicate/partial overlap, Unicode/punctuation tails, bounds, authenticated scope, stale references, restart/key expiration, pagination/retries, concurrency, and CLI/MCP output agreement. The 72 frozen source-array comparisons and 28 exact final-payload replays are separately recorded above.

## Commands

```sh
cargo build --release
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
.benchmark-venv/bin/python benchmarks/agents/progressive_replay.py \
  --cohort .benchmark-results/progressive-confirmation-20261002 \
  --binary target/release/flexcontext --output /tmp/progressive-replay.json
python3 benchmarks/agents/progressive_demo.py --binary target/release/flexcontext
.benchmark-venv/bin/python -m unittest discover -s benchmarks/agents/progressive -p 'test_*.py'

# Existing authorized evaluation access; use a fresh output directory.
env -i PATH="$PATH" HOME="$HOME" LANG=en_US.UTF-8 \
  .benchmark-venv/bin/python benchmarks/agents/progressive_eval.py \
  --binary target/release/flexcontext --output .benchmark-results/progressive-dev-NEW \
  --split development --key-file "$HOME/.env"
# Use --split confirmation for the frozen confirmation cases.
# Add --dry-run to check source hashes and save the contract without model calls.

.benchmark-venv/bin/python benchmarks/agents/progressive_summarize.py \
  --cohort calibration=.benchmark-results/progressive-development-20261002 \
  --cohort final-development=.benchmark-results/progressive-development-final-20261002 \
  --cohort confirmation=.benchmark-results/progressive-confirmation-20261002 \
  --output benchmarks/results/progressive-20261002
```
