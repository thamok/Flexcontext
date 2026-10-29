# Flexcontext: agent-directed progressive retrieval

Implement and evaluate a bounded extension to Flexcontext that makes useful omitted context discoverable and fetchable by the calling agent. Do not stop at a proposal. Work through a runnable implementation, tests, and a small comparative evaluation.

## Product objective

The coding agent already interprets user intent. Given “gib auth”, it can investigate authentication, authorization, token lifecycle, invalidation, and tests, adapting its search terms to evidence in this repository. Flexcontext should expose useful leads and source; it should not implement another generative reasoning system.

When retrieval finds four distinct relevant methods but selects only two for the first response, the remaining methods must not silently disappear. Return compact, grounded descriptions and an actionable way to fetch the omitted material. The agent decides whether it matters.

Success is a better completed investigation under a fixed cumulative context budget, not merely a smaller first response, more tags, or a larger Recall@K number.

## Boundaries and starting point

Inspect the actual checkout, applicable AGENTS.md instructions, and existing tests first. Record the starting commit and dirty state. Prior discussion used a687d3f935f8ac85d6c87213d5127603d266156d; treat that as a reference, not an instruction to reset or overwrite newer work.

Inspect src/selection.rs, src/search.rs, src/output.rs, src/model.rs, src/mcp.rs, and integrations/skills/flexcontext/SKILL.md. Trace how candidates become selected source and finally serialized output. Verify the rejection reasons, explain-only trace, compact rendering, and final serialized-budget pruning rather than relying on previous reports.

Preserve the parser, index, ranking, cache, and retrieval defaults for this experiment. Add an opt-in behavior for comparison. Fix a directly blocking bug separately and explain it. Do not introduce embeddings, a vector database, a generative reranker, remote inference in the product, a new service, or a broad architecture rewrite. A small local discriminative classifier is an optional experiment, not a predetermined dependency.

Keep the root README unchanged. Reuse the existing test/evaluation infrastructure. Maintain one concise working plan/status file and one final report under appropriate existing directories; do not build another documentation or benchmarking framework.

## 1. Reproduce the information loss

Create a minimal end-to-end case with four distinct lifecycle methods in one container. Make the first search return two bodies while the other two are still eligible candidates. Show where and why they disappear today. Also inspect at least one existing real-source miss.

Distinguish:
- Already-delivered source, including overlap with an emitted excerpt.
- Distinct candidates deferred by quotas, result limits, or source budgets.
- Additional removals during final serialized-payload budgeting.
- Missing portions of a returned, truncated symbol.
- Candidates outside the retained pool, filtered by scope, or never discovered.

Do not equate shared paths, names, or containers with duplicate behavior. Do not classify an entire symbol as already delivered merely because a small excerpt overlaps it. Conversely, do not advertise byte-identical source as new evidence.

Write the failing tests before changing this path. Keep the diagnosis short and tied to concrete functions and outputs.

## 2. Implement a usable continuation contract without ML

Return a bounded navigation summary alongside selected source. Include enough information to choose a follow-up: source-derived symbol/container/path, omission reason, and a reference that actually resolves. Start with names, signatures where useful, and existing structural facts. Do not serialize the complete diagnostic trace or every candidate.

Support exact retrieval of advertised candidates without rediscovering them through the same ranker and quotas. An explicitly requested candidate must not disappear again solely because it belongs to the same class as previous results. Source and serialized-response limits still apply; oversized requests need explicit partial/insufficient-budget behavior.

Use one small, coherent follow-up interface. It may extend the existing tool or add an expansion tool; choose after inspecting the implementation. Support CLI and MCP consistently. Avoid a new session database or a family of overlapping navigation tools.

Required behavior:
- Bind references to the correct repository/index snapshot and originating scope. Never let a stale reference resolve silently to a different symbol after refresh or edits. Define expiration and restart behavior.
- Keep retained state and navigation output bounded. Prefer existing state or self-validating references where practical. Pagination must make progress; retries should be predictable; separate investigations must not share hidden “already seen” state.
- Identify the subset actually summarized. If further candidates remain, provide an honest count/limit indicator and a bounded way to inspect more. Do not imply exhaustive repository coverage.
- Account for navigation metadata within the existing serialized-response budget, including its framing. Record any source displaced to make room. Do not simultaneously claim unchanged source, extra metadata, and an unchanged already-full budget.
- Keep omission information consistent with the final emitted result, including late removals by the output finalizer. Prevent budget/summary recomputation loops.
- Do not automatically replay already-delivered bodies on expansion. Keep enough signatures/enclosing context to make new excerpts intelligible; mark partial source accurately.
- Preserve existing path restrictions, cancellation, error behavior, and supported protocol/representation variants. No scope widening through an expansion reference.

Expose the feature through compact output, not only debug output. Update normal tool descriptions and the portable integration skill so an agent understands: these are leads, source must be fetched to establish behavior, and additional repository searches may still be necessary. Keep those instructions brief.

First deliver a working search → inspect leads → fetch omitted method demonstration. Do not delay this for classifier research.

## 3. Test what classification adds

Separate navigation value from classification value.

Baseline B uses only structural/source-derived descriptions. Candidate C adds cheap role hints from identifiers, signatures, calls, and comments. Keep hints optional and allow multiple roles or “unknown”. Prefer vocabulary already present in source; use a small role set only where it helps choose follow-ups.

Names such as cancellation, timeout, invalidation, cleanup, authorization, configuration, or tests are examples, not a mandatory ontology. Cover other subsystems too. “Same container” is a fact; “likely cancellation” is a heuristic. Preserve that distinction. Never claim SAML, AD, SSO, or OpenID exists merely because the user asked about auth. Those are possible agent search terms, not repository evidence.

Only prototype one small local learned classifier if inspection or the comparison reveals a concrete weakness that a learned model could plausibly address. A sparse-feature linear classifier or similarly simple model is acceptable; inspect feasibility before choosing. Its task is to describe candidate roles, not to interpret the user's entire request or generate code summaries.

Do not require the user to hand-label a large corpus. Existing annotations and programmatic labels may be explored, but disclose their origin and errors. Training on naming rules and evaluating against those same rules is not evidence of semantic generalization. If the trained model only reproduces a cheaper rule, keep the rule. If credible training/evaluation data is unavailable, report that constraint instead of manufacturing a success claim.

Hints must never be a hard eligibility gate. Untagged or misclassified candidates must remain reachable by reference or structural navigation. Do not print numeric confidence unless it has a defensible interpretation. Measure the classifier's incremental benefit, runtime, memory, artifact size, invalidation behavior, and maintenance cost before proposing production integration.

## 4. Evaluate the agent's actual investigation

Compare:
A. Current compact retrieval plus its existing follow-up searches/reads.
B. The same retrieval with actionable, unlabelled continuations.
C. B plus cheap role hints.
D. B plus the single lightweight learned classifier, only if justified above.

Hold the underlying ranking and selection policies constant initially. Do not improve retrieval weights in one arm and attribute the gain to continuations. Allow ordinary follow-up searches and reads in every arm; charge all delivered output against the same cumulative budget. Each arm gets its appropriate normal tool description, but no benchmark-specific coaching or answer hints.

Use a small development set and a separate confirmation set, with roughly 8–12 distinct cases total for the first pilot. Freeze expected source evidence before evaluating the new behavior. Include existing real-source cases, not just new fixtures with conveniently named methods. Disclose assistant-authored cases and review limitations; do not label them human-adjudicated or universally representative.

Include:
- Multiple required behaviors in one file/container.
- Truncated source or late serialization pruning that hides a needed behavior.
- Related but unnecessary methods, similar names, and misleading/missing role hints.
- An investigation already complete after one call, where expansion wastes context.
- Required code that never entered the candidate pool, where the agent needs a new search rather than an expansion.

Start with one initial search and at most two follow-up retrieval/read calls, with fixed cumulative returned-context and per-response limits. Calibrate feasibility on development cases and freeze the limits before confirmation. Use the same agent/model configuration and randomized arm ordering. A scripted expansion proves API correctness, not that an agent chooses useful follow-ups. Run the actual agent with its normal tool interface and record its choices; do not force it to expand or hand it gold method names.

Measure complete required-evidence coverage across the investigation, correctness of the final explanation, unnecessary expansions, repeated source delivery, navigation overhead, total returned payload, tool calls, and latency. Keep unique evidence coverage separate from charged repeated output. Count errors and unsuccessful attempts. Where receipts exist, report actual model input/output usage separately from emitted tool-output tokens. Do not treat the product's bytes/4 estimate as exact model tokenization.

Use existing source assertions and independent checks where possible. A tag or a file pointer is not proof the agent saw the implementation. Grade from the actual delivered source and final explanation, not only the tool's internal candidate set.

First validate protocol correctness with deterministic tests. Then run a small agent pilot using already-authorized evaluation access. Do not provision accounts, purchase services, or launch a large API sweep. If agent execution is unavailable, finish the implementation and deterministic tests, and explicitly leave agent benefit unmeasured.

## 5. Finish and stop

Test exact candidate retrieval, duplicate/overlap handling, late-budget omissions, metadata caps, partial content, stale/wrong-scope references, pagination/retries, concurrent independent investigations, and behavior with the feature disabled. Reuse existing regression checks; investigate failures rather than weakening them.

Favor the simplest variant supported by the observations. If unlabelled continuations solve the problem and tags add no measurable value in this pilot, retain the simpler variant. If a classifier helps some cases but adds material cost or regressions, keep it experimental and report the trade-off. Do not retune against the confirmation set or keep adding experiments until one wins.

Work autonomously through these milestones. Use bounded parallel help for source inspection or independent review when useful, with clear ownership; do not create recursive research teams. Keep decisions and the next concrete action in the short status file so work can resume after compaction. Ask only when a genuine permission, spending, or non-recoverable product decision blocks progress.

Return:
1. A runnable before/after demonstration showing two initial methods and successful agent-directed access to the other required methods.
2. Reviewable implementation changes, tests, the final tool contract, and reproduction commands.
3. One concise report comparing A/B/C and D if attempted, with actual limits, costs, failure cases, and what remains unproven.

The stopping condition is a tested progressive-retrieval implementation and an honest bounded evaluation—not an impressive token total, a model integration for its own sake, or a larger README.
