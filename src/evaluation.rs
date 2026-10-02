//! Versioned, path-qualified graded retrieval judgments and reproducible metrics.
use crate::{
    SearchSession,
    model::{SearchResult, Symbol},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct SymbolRef {
    pub path: String,
    pub symbol: String,
    pub kind: String,
}
impl SymbolRef {
    pub fn matches(&self, result: &SearchResult) -> bool {
        self.path == result.path && self.symbol == result.symbol && self.kind == result.kind
    }
    fn exists(&self, symbols: &[Symbol]) -> bool {
        symbols
            .iter()
            .any(|s| s.path == self.path && s.name == self.symbol && s.kind == self.kind)
    }
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedRelation {
    pub source: SymbolRef,
    pub kind: String,
    pub target: SymbolRef,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationQuery {
    pub id: String,
    pub query: String,
    pub fixture: PathBuf,
    pub split: String,
    pub strongly_relevant: Vec<SymbolRef>,
    pub partially_relevant: Vec<SymbolRef>,
    pub distractors: Vec<SymbolRef>,
    pub expected_relationships: Vec<ExpectedRelation>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Corpus {
    pub schema: u32,
    pub queries: Vec<EvaluationQuery>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Measurement {
    pub id: String,
    pub query: String,
    pub split: String,
    pub k: usize,
    pub recall_at_k: f64,
    pub reciprocal_rank: f64,
    pub ndcg_at_k: f64,
    pub distractors_at_k: usize,
    pub relationship_recall: f64,
    pub source_bytes: usize,
    pub serialized_bytes: usize,
    pub estimated_tokens: usize,
    pub results: Vec<SymbolRef>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub queries: usize,
    pub recall_at_k: f64,
    pub mrr: f64,
    pub ndcg_at_k: f64,
    pub relationship_recall: f64,
    pub mean_source_bytes: f64,
    pub mean_serialized_bytes: f64,
    pub mean_estimated_tokens: f64,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub schema: u32,
    pub k: usize,
    pub selection_budget: usize,
    pub summary: Summary,
    pub by_split: BTreeMap<String, Summary>,
    pub measurements: Vec<Measurement>,
}

pub fn metrics(grades: &[u8], relevant: usize, ideal_grades: &[u8], k: usize) -> (f64, f64, f64) {
    let dcg = |values: &[u8]| {
        values
            .iter()
            .take(k)
            .enumerate()
            .map(|(i, grade)| (2f64.powi(*grade as i32) - 1.0) / ((i + 2) as f64).log2())
            .sum::<f64>()
    };
    let hits = grades.iter().take(k).filter(|&&grade| grade == 2).count();
    let rr = grades
        .iter()
        .take(k)
        .position(|&grade| grade == 2)
        .map_or(0.0, |i| 1.0 / (i + 1) as f64);
    let ideal = dcg(ideal_grades);
    (
        hits as f64 / relevant.max(1) as f64,
        rr,
        if ideal > 0.0 {
            dcg(grades) / ideal
        } else {
            0.0
        },
    )
}
fn summarize(rows: &[&Measurement]) -> Summary {
    let mean = |f: fn(&Measurement) -> f64| {
        rows.iter().map(|r| f(r)).sum::<f64>() / rows.len().max(1) as f64
    };
    Summary {
        queries: rows.len(),
        recall_at_k: mean(|r| r.recall_at_k),
        mrr: mean(|r| r.reciprocal_rank),
        ndcg_at_k: mean(|r| r.ndcg_at_k),
        relationship_recall: mean(|r| r.relationship_recall),
        mean_source_bytes: mean(|r| r.source_bytes as f64),
        mean_serialized_bytes: mean(|r| r.serialized_bytes as f64),
        mean_estimated_tokens: mean(|r| r.estimated_tokens as f64),
    }
}
pub fn evaluate(directory: &Path, k: usize, max_bytes: usize) -> Result<Report> {
    evaluate_with_weights(
        directory,
        k,
        max_bytes,
        &crate::ranking::RankingWeights::default(),
    )
}
pub fn evaluate_with_weights(
    directory: &Path,
    k: usize,
    max_bytes: usize,
    weights: &crate::ranking::RankingWeights,
) -> Result<Report> {
    evaluate_suites(
        directory,
        k,
        max_bytes,
        weights,
        &["rust", "typescript", "python", "mixed-monorepo"],
    )
}

/// Explicit suites keep expansion separate from the frozen 60-query baseline.
pub fn evaluate_suites(
    directory: &Path,
    k: usize,
    max_bytes: usize,
    weights: &crate::ranking::RankingWeights,
    suites: &[&str],
) -> Result<Report> {
    weights.validate()?;
    ensure!(
        k > 0 && max_bytes > 0,
        "evaluation budgets must be positive"
    );
    let mut measurements = Vec::new();
    let mut ids = BTreeSet::new();
    let mut sessions = BTreeMap::new();
    ensure!(
        !suites.is_empty(),
        "at least one evaluation suite is required"
    );
    for name in suites {
        ensure!(
            name.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                && !name.is_empty(),
            "invalid suite name"
        );
        let path = directory.join(format!("{name}.json"));
        let corpus: Corpus = serde_json::from_slice(&std::fs::read(&path)?)
            .with_context(|| format!("invalid corpus {}", path.display()))?;
        ensure!(
            corpus.schema == 1 && !corpus.queries.is_empty(),
            "unsupported/empty corpus"
        );
        for case in corpus.queries {
            ensure!(
                ids.insert(case.id.clone()),
                "duplicate query id {}",
                case.id
            );
            ensure!(
                !case.strongly_relevant.is_empty()
                    && !case.partially_relevant.is_empty()
                    && !case.distractors.is_empty(),
                "incomplete judgments {}",
                case.id
            );
            ensure!(
                ["dev", "test"].contains(&case.split.as_str()),
                "invalid split"
            );
            let root = directory.join(&case.fixture).canonicalize()?;
            ensure!(
                root.starts_with(directory.canonicalize()?),
                "fixture escapes corpus"
            );
            if !sessions.contains_key(&root) {
                sessions.insert(root.clone(), SearchSession::open(&root, false)?);
            }
            let session = &sessions[&root];
            let mut labeled = BTreeSet::new();
            for symbol in case
                .strongly_relevant
                .iter()
                .chain(&case.partially_relevant)
                .chain(&case.distractors)
            {
                ensure!(
                    labeled.insert(symbol.clone()),
                    "overlapping judgments in {}",
                    case.id
                );
                ensure!(
                    symbol.exists(session.symbols()),
                    "unresolved judgment in {}: {:?}",
                    case.id,
                    symbol
                );
            }
            for relation in &case.expected_relationships {
                ensure!(
                    relation.source.exists(session.symbols())
                        && relation.target.exists(session.symbols()),
                    "unresolved relationship in {}",
                    case.id
                );
                ensure!(
                    [
                        "calls",
                        "type_reference",
                        "contains",
                        "enclosed_by",
                        "imports",
                        "file_import"
                    ]
                    .contains(&relation.kind.as_str()),
                    "unknown relationship kind"
                );
            }
            let response = session.query_with_weights(&case.query, max_bytes, k, weights)?;
            let mut seen = BTreeSet::new();
            let grades: Vec<_> = response
                .results
                .iter()
                .map(|r| {
                    let key = (r.path.clone(), r.symbol.clone(), r.kind.clone());
                    if !seen.insert(key) {
                        return 0;
                    }
                    if case.strongly_relevant.iter().any(|s| s.matches(r)) {
                        2
                    } else if case.partially_relevant.iter().any(|s| s.matches(r)) {
                        1
                    } else {
                        0
                    }
                })
                .collect();
            let ideal: Vec<_> = std::iter::repeat_n(2, case.strongly_relevant.len())
                .chain(std::iter::repeat_n(1, case.partially_relevant.len()))
                .collect();
            let (recall, rr, ndcg) = metrics(&grades, case.strongly_relevant.len(), &ideal, k);
            let relationships = case
                .expected_relationships
                .iter()
                .filter(|expected| {
                    response.results.iter().any(|r| {
                        expected.source.matches(r)
                            && r.relations.iter().any(|edge| {
                                edge.kind == expected.kind
                                    && edge.path == expected.target.path
                                    && edge.symbol == expected.target.symbol
                            })
                    })
                })
                .count();
            measurements.push(Measurement {
                id: case.id,
                query: case.query,
                split: case.split,
                k,
                recall_at_k: recall,
                reciprocal_rank: rr,
                ndcg_at_k: ndcg,
                distractors_at_k: response
                    .results
                    .iter()
                    .filter(|r| case.distractors.iter().any(|s| s.matches(r)))
                    .count(),
                relationship_recall: relationships as f64
                    / case.expected_relationships.len().max(1) as f64,
                source_bytes: response.stats.returned_bytes,
                serialized_bytes: response.stats.json_payload_bytes,
                estimated_tokens: response.stats.json_payload_bytes.div_ceil(4),
                results: response
                    .results
                    .iter()
                    .map(|r| SymbolRef {
                        path: r.path.clone(),
                        symbol: r.symbol.clone(),
                        kind: r.kind.clone(),
                    })
                    .collect(),
            });
        }
    }
    let summary = summarize(&measurements.iter().collect::<Vec<_>>());
    let by_split = ["dev", "test"]
        .into_iter()
        .map(|split| {
            (
                split.to_owned(),
                summarize(
                    &measurements
                        .iter()
                        .filter(|r| r.split == split)
                        .collect::<Vec<_>>(),
                ),
            )
        })
        .collect();
    Ok(Report {
        schema: 1,
        k,
        selection_budget: max_bytes,
        summary,
        by_split,
        measurements,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hand_calculated_metrics_include_misses_and_partial_credit() {
        let (recall, rr, ndcg) = metrics(&[0, 1, 2], 2, &[2, 2, 1], 3);
        assert_eq!(recall, 0.5);
        assert_eq!(rr, 1.0 / 3.0);
        let expected =
            (1.0 / 3f64.log2() + 3.0 / 4f64.log2()) / (3.0 + 3.0 / 3f64.log2() + 1.0 / 4f64.log2());
        assert!((ndcg - expected).abs() < 1e-12);
        assert_eq!(metrics(&[], 2, &[2, 2, 1], 5), (0.0, 0.0, 0.0));
        assert_eq!(metrics(&[2, 2, 1], 2, &[2, 2, 1], 5), (1.0, 1.0, 1.0));
    }
}

#[derive(Serialize)]
pub struct TuningReport {
    pub method: &'static str,
    pub trials: usize,
    pub weights: crate::ranking::RankingWeights,
    pub baseline: BTreeMap<String, Summary>,
    pub selected: BTreeMap<String, Summary>,
}
/// Two deterministic coordinate passes. Selection reads development metrics
/// only; held-out metrics are reported once for the selected configuration.
pub fn tune(directory: &Path, k: usize, max_bytes: usize) -> Result<TuningReport> {
    use crate::ranking::RankingWeights;
    let mut weights = RankingWeights::tuning_anchor();
    let baseline = evaluate_with_weights(directory, k, max_bytes, &weights)?;
    let mut best = (
        baseline.by_split["dev"].ndcg_at_k,
        baseline.by_split["dev"].mrr,
    );
    let mut trials = 1;
    for _ in 0..2 {
        for field in 0..7 {
            let anchor = weights.clone();
            for factor in [0.5, 1.5] {
                let mut candidate = anchor.clone();
                let value = match field {
                    0 => &mut candidate.exact_name_weight,
                    1 => &mut candidate.normalized_name_weight,
                    2 => &mut candidate.identifier_weight,
                    3 => &mut candidate.body_weight,
                    4 => &mut candidate.call_edge_weight,
                    5 => &mut candidate.container_edge_weight,
                    _ => &mut candidate.diversity_penalty,
                };
                *value *= factor;
                let report = evaluate_with_weights(directory, k, max_bytes, &candidate)?;
                trials += 1;
                let score = (report.by_split["dev"].ndcg_at_k, report.by_split["dev"].mrr);
                if score > best {
                    best = score;
                    weights = candidate;
                }
            }
        }
    }
    let selected = evaluate_with_weights(directory, k, max_bytes, &weights)?.by_split;
    Ok(TuningReport {
        method: "two coordinate passes, factors 0.5 and 1.5; development nDCG then MRR; fixed zero import weight; no automatic default changes",
        trials,
        weights,
        baseline: baseline.by_split,
        selected,
    })
}
