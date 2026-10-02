use std::collections::HashMap;

use crate::language::is_container;
use crate::lexical::{Query, normalize_identifier};
use crate::model::{ScoredSymbol, SearchResult, Symbol};
use crate::relations::{RelationGraph, serializable_relations};

pub fn select_context(
    ranked: &[ScoredSymbol],
    symbols: &[Symbol],
    graph: &RelationGraph,
    max_bytes: usize,
    max_results: usize,
) -> Vec<SearchResult> {
    select_context_for_query(
        ranked,
        symbols,
        graph,
        max_bytes,
        max_results,
        &Query::parse(""),
    )
}

pub fn select_context_for_query(
    ranked: &[ScoredSymbol],
    symbols: &[Symbol],
    graph: &RelationGraph,
    max_bytes: usize,
    max_results: usize,
    query: &Query,
) -> Vec<SearchResult> {
    select_context_with_weights(
        ranked,
        symbols,
        graph,
        max_bytes,
        max_results,
        query,
        &crate::ranking::RankingWeights::default(),
    )
}
#[allow(clippy::too_many_arguments)]
pub fn select_context_with_weights(
    ranked: &[ScoredSymbol],
    symbols: &[Symbol],
    graph: &RelationGraph,
    max_bytes: usize,
    max_results: usize,
    query: &Query,
    weights: &crate::ranking::RankingWeights,
) -> Vec<SearchResult> {
    select_context_config(
        ranked,
        symbols,
        graph,
        max_bytes,
        max_results,
        query,
        weights,
        true,
    )
    .0
}

#[allow(clippy::too_many_arguments)]
pub fn select_context_config(
    ranked: &[ScoredSymbol],
    symbols: &[Symbol],
    graph: &RelationGraph,
    max_bytes: usize,
    max_results: usize,
    query: &Query,
    weights: &crate::ranking::RankingWeights,
    quotas: bool,
) -> (Vec<SearchResult>, HashMap<usize, String>) {
    let mut decisions = HashMap::new();
    let mut results = Vec::new();
    let mut used = 0;
    let mut name_clusters: HashMap<String, usize> = HashMap::new();
    let mut structural_clusters: HashMap<(String, String), usize> = HashMap::new();
    let per_container_limit = (max_bytes / 4).clamp(256, 8 * 1024);
    let mut pending: Vec<_> = ranked
        .iter()
        .take(max_results.saturating_mul(32).clamp(64, 4096))
        .collect();
    let mut paths: HashMap<&str, usize> = HashMap::new();
    let mut kinds: HashMap<&str, usize> = HashMap::new();

    while !pending.is_empty() {
        // Soft diversity: keep the first lexical anchor, then discount repeated paths/kinds.
        // Original rank wins ties; reported lexical scores remain unchanged.
        let next = pending
            .iter()
            .enumerate()
            .max_by(|(ai, a), (bi, b)| {
                let utility = |item: &&ScoredSymbol| {
                    let symbol = &symbols[item.symbol_id];
                    item.score
                        - weights.diversity_penalty
                            * 2.0
                            * paths.get(symbol.path.as_str()).copied().unwrap_or(0) as f64
                        - weights.diversity_penalty
                            * 0.75
                            * kinds.get(symbol.kind.as_str()).copied().unwrap_or(0) as f64
                };
                utility(a).total_cmp(&utility(b)).then_with(|| bi.cmp(ai))
            })
            .map(|(index, _)| index)
            .unwrap();
        let scored = pending.remove(next);
        if results.len() >= max_results || used >= max_bytes {
            break;
        }
        let symbol = &symbols[scored.symbol_id];
        let name_cluster = normalize_identifier(&symbol.name);
        let name_limit = if symbol.kind == "declaration" { 1 } else { 2 };
        if quotas && name_clusters.get(&name_cluster).copied().unwrap_or(0) >= name_limit {
            decisions.insert(scored.symbol_id, "name quota".into());
            continue;
        }
        let structural_cluster = (
            symbol.path.clone(),
            symbol
                .containing_symbol
                .clone()
                .unwrap_or_else(|| "<top-level>".to_owned()),
        );
        let structural_limit = if symbol.containing_symbol.is_some() {
            2
        } else {
            3
        };
        if quotas
            && structural_clusters
                .get(&structural_cluster)
                .copied()
                .unwrap_or(0)
                >= structural_limit
        {
            decisions.insert(scored.symbol_id, "file/container quota".into());
            continue;
        }
        if overlaps_selected(symbol, &results) {
            decisions.insert(scored.symbol_id, "overlapping source".into());
            continue;
        }
        let remaining = (max_bytes - used) / 4 * 4;
        // A complete container would consume and suppress its more precise
        // member hits. Keep its declaration alongside independently ranked members.
        let has_member_candidate = is_container(&symbol.kind)
            && ranked.iter().any(|item| {
                let member = &symbols[item.symbol_id];
                member.id != symbol.id
                    && member.path == symbol.path
                    && member.start_byte >= symbol.start_byte
                    && member.end_byte <= symbol.end_byte
                    && matches!(member.kind.as_str(), "function" | "method")
            });
        let (content, truncated, source_spans) = if has_member_candidate {
            let compact = compact_container(symbol);
            if compact.len() > remaining.min(per_container_limit) {
                (String::new(), false, Vec::new())
            } else {
                (compact, true, compact_spans(symbol))
            }
        } else {
            budgeted_content(symbol, remaining, per_container_limit, query)
        };
        if content.is_empty() {
            decisions.insert(scored.symbol_id, "excerpt does not fit".into());
            continue;
        }
        let content_bytes = content.len();
        used += content_bytes.div_ceil(4) * 4;
        let diversity_score = -weights.diversity_penalty
            * (2.0 * paths.get(symbol.path.as_str()).copied().unwrap_or(0) as f64
                + 0.75 * kinds.get(symbol.kind.as_str()).copied().unwrap_or(0) as f64);
        *paths.entry(&symbol.path).or_default() += 1;
        *kinds.entry(&symbol.kind).or_default() += 1;
        *name_clusters.entry(name_cluster).or_default() += 1;
        *structural_clusters.entry(structural_cluster).or_default() += 1;
        decisions.insert(scored.symbol_id, "selected".into());
        results.push(SearchResult {
            path: symbol.path.clone(),
            language: symbol.language,
            symbol: symbol.name.clone(),
            kind: symbol.kind.clone(),
            containing_symbol: symbol.containing_symbol.clone(),
            start_byte: symbol.start_byte,
            end_byte: symbol.end_byte,
            start_line: symbol.start_line,
            end_line: symbol.end_line,
            signature: symbol.signature().to_owned(),
            score: scored.score,
            lexical_score: scored.signals.total()
                - scored.signals.structural_priority
                - scored.signals.structural_relation,
            structural_score: scored.signals.structural_priority
                + scored.signals.structural_relation,
            diversity_score,
            final_score: scored.score + diversity_score,
            signals: scored.signals.clone(),
            content,
            content_bytes,
            approximate_tokens: content_bytes.div_ceil(4),
            content_truncated: truncated,
            source_spans,
            relations: serializable_relations(symbol.id, graph, symbols),
        });
    }
    for (i, item) in ranked.iter().enumerate() {
        decisions.entry(item.symbol_id).or_insert_with(|| {
            if i >= max_results.saturating_mul(32).clamp(64, 4096) {
                "candidate pool limit"
            } else if results.len() >= max_results {
                "result limit"
            } else {
                "source budget"
            }
            .into()
        });
    }
    (results, decisions)
}

fn budgeted_content(
    symbol: &Symbol,
    remaining: usize,
    container_limit: usize,
    query: &Query,
) -> (String, bool, Vec<crate::model::SourceSpan>) {
    if symbol.content().len() <= remaining && symbol.content().len() <= container_limit {
        return (
            symbol.content().to_owned(),
            false,
            vec![crate::model::SourceSpan {
                start_byte: symbol.start_byte,
                end_byte: symbol.end_byte,
                start_line: symbol.start_line,
                end_line: symbol.end_line,
            }],
        );
    }
    if (matches!(symbol.kind.as_str(), "function" | "method")
        || (symbol.kind == "declaration" && !symbol.body_range.is_empty()))
        && let Some((content, spans)) =
            crate::slicing::slice_symbol(symbol, query, remaining.min(container_limit))
    {
        return (content, true, spans);
    }
    if is_container(&symbol.kind) {
        let compact = compact_container(symbol);
        if compact.len() <= remaining.min(container_limit) {
            return (compact, true, compact_spans(symbol));
        }
    }
    // Keep small non-containers whole if they fit the overall budget.
    if !is_container(&symbol.kind)
        && !matches!(symbol.kind.as_str(), "function" | "method")
        && symbol.content().len() <= remaining
    {
        return (
            symbol.content().to_owned(),
            false,
            vec![crate::model::SourceSpan {
                start_byte: symbol.start_byte,
                end_byte: symbol.end_byte,
                start_line: symbol.start_line,
                end_line: symbol.end_line,
            }],
        );
    }
    (String::new(), false, Vec::new())
}

fn compact_container(symbol: &Symbol) -> String {
    let mut content = String::new();
    if !symbol.comments().is_empty() {
        content.push_str(&symbol.comments());
        content.push('\n');
    }
    content.push_str(symbol.signature().trim_end());
    if symbol.language == crate::model::Language::Python {
        content.push_str("\n    # … members omitted by context budget");
    } else {
        content.push_str(" { /* … members omitted by context budget */ }");
    }
    content
}

fn compact_spans(symbol: &Symbol) -> Vec<crate::model::SourceSpan> {
    let raw = &symbol.source[symbol.signature_range.clone()];
    let start = symbol.signature_range.start + raw.len() - raw.trim_start().len();
    let mut spans: Vec<_> = symbol
        .comment_ranges
        .iter()
        .map(|r| {
            let raw = &symbol.source[r.clone()];
            let start = r.start + raw.len() - raw.trim_start().len();
            crate::slicing::span_for_range(symbol, start..start + raw.trim().len())
        })
        .collect();
    spans.push(crate::slicing::span_for_range(
        symbol,
        start..start + symbol.signature().len(),
    ));
    spans
}

fn overlaps_selected(symbol: &Symbol, selected: &[SearchResult]) -> bool {
    selected.iter().any(|result| {
        result.path == symbol.path
            && if result.content_truncated {
                result.source_spans.iter().any(|span| {
                    symbol.start_byte < span.end_byte && span.start_byte < symbol.end_byte
                })
            } else {
                (symbol.start_byte >= result.start_byte && symbol.end_byte <= result.end_byte)
                    || (result.start_byte >= symbol.start_byte
                        && result.end_byte <= symbol.end_byte)
            }
    })
}

#[cfg(test)]
mod tests {
    use crate::model::{Language, ScoreSignals};

    use super::*;

    #[test]
    fn skips_whole_oversized_container_in_favor_of_compact_declaration() {
        let symbol = Symbol {
            id: 0,
            path: "large.rs".to_owned(),
            language: Language::Rust,
            name: "Large".to_owned(),
            normalized_name: "large".to_owned(),
            kind: "struct".to_owned(),
            containing_symbol: None,
            structural_depth: 0,
            start_byte: 0,
            end_byte: 10_000,
            start_line: 1,
            end_line: 500,
            source: std::sync::Arc::from(format!(
                "{}{}",
                "x".repeat(10_000),
                "struct Large".to_owned()
            )),
            signature_range: ("x".repeat(10_000)).len()
                ..("x".repeat(10_000)).len() + ("struct Large".to_owned()).len(),
            body_range: 0..("x".repeat(10_000)).len(),
            comment_ranges: Vec::new(),
            excerpt_ranges: Vec::new(),
            imports: std::sync::Arc::from([]),
            identifiers: Vec::new(),
            type_references: Vec::new(),
            calls: Vec::new(),
        };
        let scored = ScoredSymbol {
            symbol_id: 0,
            score: 1.0,
            signals: ScoreSignals::default(),
        };
        let results = select_context(&[scored], &[symbol], &RelationGraph::default(), 1_024, 2);
        assert_eq!(results.len(), 1);
        assert!(results[0].content_truncated);
        assert!(results[0].content.len() < 1_024);
    }

    #[test]
    fn diversifies_repeated_local_declarations() {
        let base = Symbol {
            id: 0,
            path: "one.rs".to_owned(),
            language: Language::Rust,
            name: "auth".to_owned(),
            normalized_name: "auth".to_owned(),
            kind: "declaration".to_owned(),
            containing_symbol: Some("first".to_owned()),
            structural_depth: 1,
            start_byte: 0,
            end_byte: 18,
            start_line: 1,
            end_line: 1,
            source: std::sync::Arc::from(format!(
                "{}{}",
                "const auth = true;".to_owned(),
                "const auth: bool".to_owned()
            )),
            signature_range: ("const auth = true;".to_owned()).len()
                ..("const auth = true;".to_owned()).len() + ("const auth: bool".to_owned()).len(),
            body_range: 0..(String::new()).len(),
            comment_ranges: Vec::new(),
            excerpt_ranges: Vec::new(),
            imports: std::sync::Arc::from([]),
            identifiers: vec!["auth".to_owned()],
            type_references: Vec::new(),
            calls: Vec::new(),
        };
        let mut duplicate = base.clone();
        duplicate.id = 1;
        duplicate.path = "two.rs".to_owned();
        duplicate.containing_symbol = Some("second".to_owned());
        let scored = [
            ScoredSymbol {
                symbol_id: 0,
                score: 10.0,
                signals: ScoreSignals::default(),
            },
            ScoredSymbol {
                symbol_id: 1,
                score: 9.0,
                signals: ScoreSignals::default(),
            },
        ];
        let results = select_context(
            &scored,
            &[base, duplicate],
            &RelationGraph::default(),
            1_024,
            5,
        );
        assert_eq!(results.len(), 1);
    }
}
