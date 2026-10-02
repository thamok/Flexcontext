# Frozen-source competitor comparison, 2026-10-01

The preserved pass-2 Flexcontext binary is comparable to Probe on this authored
retrieval corpus: on its 18 historical held-out questions, mean evidence recall
is 66.7% versus Probe's 69.4% at 2,048 source tokens, and 77.8% versus 69.4% at
4,096. CodeGraph scores lower under the same fixed keyword query and common
source-prefix cap. These measurements do not establish coding-agent success or
general product superiority.

## Method

- CodeGraph is the actual npm release `@colbymchenry/codegraph@1.6.1`, installed
  only under ignored `.benchmark-tools/codegraph`. No agent installation or
  application configuration command was used. Usage telemetry was disabled by
  environment variables, and no model API was called.
- `codegraph_eval.py` copies only source files listed in the frozen
  `optimization-focused-20260916` manifests. It verifies every source hash
  before copying, checks all question evidence hashes, and rechecks all copied
  source bytes after execution. Original snapshots remain unchanged.
- The run used all 36 existing optimization questions: 18 development and 18
  historical held-out, six per repository per partition. Questions and labels
  were not rewritten for this comparison. The historical held-out partition has
  already informed prior analysis and must not be presented as newly blind.
- Every engine receives the same deterministic question terms. Probe receives
  their OR expression. CodeGraph `explore` and JSON `context` use native defaults
  for file and node counts. No gold information, expansion, or reranking is
  provided. Flexcontext and Probe use the existing 8-times-source-budget byte
  overfetch allowance and 100-result ceiling.
- CodeGraph has no matching source-byte budget flag on these commands. Its one
  native response per question/command is reused at both common source budgets.
  Actual native transport cost is retained separately.
- All source is scored using `run.py`'s existing line-level `cl100k_base` token
  cap, deduplication, evidence atoms, and relevant regions. Explore's numbered
  lines must match the indicated snapshot line exactly. Context's snippets use
  the existing complete-line alignment. Pointers never receive body evidence.
- Index creation is separate from querying. Single CLI timing samples were
  collected amid other work and are diagnostic only; they are not comparable
  to isolated repeated resident latency measurements.

## Historical held-out results

| Repository | Source tokens | Flexcontext | Probe | CodeGraph explore | CodeGraph context |
|---|---:|---:|---:|---:|---:|
| AgentX | 2,048 | 50.0% | 50.0% | 33.3% | 16.7% |
| AgentX | 4,096 | 66.7% | 50.0% | 33.3% | 16.7% |
| Flexcontext | 2,048 | 83.3% | 100.0% | 33.3% | 16.7% |
| Flexcontext | 4,096 | 100.0% | 100.0% | 33.3% | 16.7% |
| WebKit Python tooling | 2,048 | 66.7% | 58.3% | 16.7% | 16.7% |
| WebKit Python tooling | 4,096 | 66.7% | 58.3% | 33.3% | 16.7% |
| Equal-repository mean | 2,048 | **66.7%** | **69.4%** | **27.8%** | **16.7%** |
| Equal-repository mean | 4,096 | **77.8%** | **69.4%** | **33.3%** | **16.7%** |

At 2,048 tokens, the corresponding mean relevant-region precision is 6.9%,
8.1%, 2.8%, and 4.8%. At 4,096 it is 4.0%, 4.1%, 1.8%, and 4.8%. These are
conservative line-level judgments: unjudged lines count against precision.

Development recall at 2,048 / 4,096 tokens is respectively 77.8% / 83.3% for
Flexcontext, 47.2% / 50.0% for Probe, 44.4% / 44.4% for CodeGraph explore, and
50.0% / 50.0% for CodeGraph context.

## Coverage and limitations

CodeGraph reported indexing all manifest files: 3,445 AgentX files, 51
Flexcontext files, and 603 WebKit tooling files. Its graph contained 47,081,
566, and 12,440 nodes respectively. Thus a missing-file coverage gap does not
explain the observed difference. The snapshots are mostly TypeScript, Rust, and
Python; they do not test full WebKit or broad supported-language quality.

CodeGraph explore returned zero mismatching/unlocated selected source lines in
this run. CodeGraph context had 102 unlocated selected lines over 72 budgeted
rows, including native snippet truncation; those lines received no evidence
credit. Native responses averaged about 5,237 tokens for explore and 4,583 for
context, before the common source cap and separate from model-ready context.

CodeGraph's documented primary workflow includes agent-chosen queries and
follow-up graph navigation. This one-shot keyword adapter does not exercise
that workflow. The second `context` command is a separate native adapter, not a
replacement for `explore`, and results are reported separately. Sources:
[CodeGraph repository](https://github.com/colbymchenry/codegraph), installed
1.6.1 CLI help, and retained native output/status artifacts.

## Reproduction and evidence

```sh
npm install --prefix .benchmark-tools/codegraph --save-exact @colbymchenry/codegraph@1.6.1
.benchmark-venv/bin/python -m unittest discover -s benchmarks/comparison -p 'test_*.py'
.benchmark-venv/bin/python benchmarks/comparison/codegraph_eval.py \
  --frozen-run .benchmark-results/optimization-focused-20260916 \
  --flexcontext .benchmark-results/iteration-20261001-pass2/flexcontext-candidate \
  --output .benchmark-results/codegraph-comparison-NEW
```

Measured output: `.benchmark-results/codegraph-comparison-20261001/`, with
`rows.jsonl`, `summary.json`, manifests, source copies, index stdout/stderr,
native status, native responses, selected lines, and normalized contexts.
There are **288 successful rows and zero errors**. Six new adapter tests cover
visible-only extraction, exact line verification, source pointers, truncation,
and path containment. The complete comparison test suite passed 19 tests at
the end of this implementation.

The later-added `confirmation-cases.json` contains nine new source-reviewed
questions and 27 evidence atoms in unused modules, authored by a separate
agent and frozen without retrieval calls for those questions. It is not part
of the results above. Its labels are not independently human adjudicated and
must be used only after candidate policies are locked on development data.

## Fresh confirmation after candidate freeze

The nine-question confirmation corpus was frozen with SHA-256
`a04e4ae6f7a96067c579d22384afb5be6742f877c4bb688c565fe346b5570a34`.
Comparator retrieval ran after that corpus freeze; its quality scores remained
uninspected until the separate semantic candidate was frozen with design hash
`15e6e822087326d25d3265f61c7e50ef12f0c28d8e005f1903f2e756de72c39a`.
These comparator figures use the same preserved pass-2 Flexcontext binary and
adapter configuration as above, not the semantic candidate.

| Repository | Source tokens | Flexcontext | Probe | CodeGraph explore | CodeGraph context |
|---|---:|---:|---:|---:|---:|
| AgentX | 2,048 | 44.4% | 22.2% | 66.7% | 55.6% |
| AgentX | 4,096 | 44.4% | 22.2% | 66.7% | 55.6% |
| Flexcontext | 2,048 | 66.7% | 100.0% | 100.0% | 77.8% |
| Flexcontext | 4,096 | 100.0% | 100.0% | 100.0% | 77.8% |
| WebKit Python tooling | 2,048 | 66.7% | 22.2% | 33.3% | 11.1% |
| WebKit Python tooling | 4,096 | 66.7% | 22.2% | 33.3% | 11.1% |
| Equal-repository mean | 2,048 | **59.3%** | **48.1%** | **66.7%** | **48.1%** |
| Equal-repository mean | 4,096 | **70.4%** | **48.1%** | **66.7%** | **48.1%** |

Mean relevant-region precision at 2,048 tokens is respectively 9.5%, 8.2%,
10.7%, and 24.3%; at 4,096 it is 5.5%, 4.1%, 7.3%, and 24.3%. CodeGraph
context returns much less source, averaging 727 source tokens at either cap.

CodeGraph explore leads at the smaller budget on these new questions, whereas
Flexcontext leads at the larger budget. This reversal from the historical
partition shows why corpus scope matters. There are only three questions per
repository, all separately agent-authored and source-reviewed; this is not
enough to claim general superiority, non-inferiority, or agent task success.

Confirmation artifacts are retained under
`.benchmark-results/codegraph-confirmation-20261001/`. All **72 rows succeeded**,
all original source hashes remained unchanged, and the run archives its
harness files and npm dependency lock. No ranking or prompt change was made
after viewing these confirmation scores.
