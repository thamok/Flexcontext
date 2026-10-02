//! Deterministic, budget-independent excerpt planning. Allocation takes a prefix
//! of this plan: a larger source ceiling can only add original source ranges.
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use crate::lexical::{Query, matching_query_terms};
use crate::model::{ScoredSymbol, SearchResult, SourceSpan, Symbol};
use crate::relations::{RelationGraph, serializable_relations};

pub fn independent(item: &ScoredSymbol) -> bool {
    let s = &item.signals;
    s.exact_symbol_name
        + s.normalized_symbol_name
        + s.symbol_name_tokens
        + s.comments
        + s.identifiers
        + s.signature
        + s.body
        > 0.0
}

pub fn prefer_implementation(
    ranked: &mut [ScoredSymbol],
    symbols: &[Symbol],
    query: &Query,
    scope: &crate::model::SearchScope,
) {
    let is_test = |path: &str| {
        crate::lexical::identifier_tokens(path).iter().any(|t| {
            matches!(
                t.as_str(),
                "test" | "tests" | "spec" | "specs" | "unittest" | "unittests"
            )
        })
    };
    if query.tokens.iter().any(|t| {
        matches!(
            t.as_str(),
            "test" | "tests" | "testing" | "spec" | "unittest"
        )
    }) || scope.include_paths.iter().any(|p| is_test(p))
    {
        return;
    }
    for item in ranked.iter_mut() {
        let symbol = &symbols[item.symbol_id];
        if is_test(&symbol.path)
            && item.signals.exact_symbol_name == 0.0
            && item.signals.normalized_symbol_name == 0.0
        {
            // A preference, never an exclusion. Explicit identifiers are unaffected.
            let penalty = item.score.max(0.0) * 0.1;
            item.signals.structural_priority -= penalty;
            item.score -= penalty;
        }
    }
    crate::ranking::sort_scored(ranked, symbols);
}

/// File rank uses a maximum and distinct term coverage, never declaration count.
/// A small preference for the initial three files leaves strong alternatives eligible.
pub fn concentrate(
    ranked: &mut [ScoredSymbol],
    symbols: &[Symbol],
    query: &Query,
) -> HashMap<usize, f64> {
    let mut files: BTreeMap<&str, (f64, std::collections::BTreeSet<String>)> = BTreeMap::new();
    for item in ranked.iter().filter(|s| independent(s)) {
        let symbol = &symbols[item.symbol_id];
        let entry = files.entry(&symbol.path).or_default();
        entry.0 = entry.0.max(item.score - item.signals.structural_relation);
        let text = format!("{} {}", symbol.name, symbol.signature());
        let tokens = crate::lexical::identifier_tokens(&text);
        for term in &query.tokens {
            if tokens
                .iter()
                .any(|token| crate::lexical::lexically_related(term, token))
            {
                entry.1.insert(term.clone());
            }
        }
    }
    let mut order: Vec<_> = files.into_iter().collect();
    let score = |v: &(f64, std::collections::BTreeSet<String>)| {
        v.0 * (1.0 + 0.1 * v.1.len() as f64 / query.tokens.len().max(1) as f64)
    };
    order.sort_by(|a, b| score(&b.1).total_cmp(&score(&a.1)).then(a.0.cmp(b.0)));
    let focus: Vec<_> = order.iter().take(3).map(|v| v.0).collect();
    let mut boosts = HashMap::new();
    for item in ranked.iter_mut() {
        if focus.contains(&symbols[item.symbol_id].path.as_str()) && independent(item) {
            let boost = (item.score - item.signals.structural_relation).max(0.0) * 0.1;
            item.score += boost;
            boosts.insert(item.symbol_id, boost);
        }
    }
    crate::ranking::sort_scored(ranked, symbols);
    boosts
}

fn merge(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.retain(|r| !r.is_empty());
    ranges.sort_by_key(|r| r.start);
    let mut out: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(last) = out.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
        } else {
            out.push(range);
        }
    }
    out
}

fn subtract(ranges: Vec<Range<usize>>, covered: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut out = ranges;
    for c in covered {
        out = out
            .into_iter()
            .flat_map(|r| {
                if r.end <= c.start || r.start >= c.end {
                    return vec![r];
                }
                let mut pieces = Vec::new();
                if r.start < c.start {
                    pieces.push(r.start..c.start);
                }
                if r.end > c.end {
                    pieces.push(c.end..r.end);
                }
                pieces
            })
            .collect();
    }
    out
}

/// Keep ancestor prefixes up to each child block: signatures, loop/branch
/// conditions and match arms remain visible even when a large body is excerpted.
fn plan(symbol: &Symbol, query: &Query) -> Vec<Vec<Range<usize>>> {
    if symbol.content().len() <= 1536 && !crate::language::is_container(&symbol.kind) {
        return vec![vec![symbol.start_byte..symbol.end_byte]];
    }
    let signature = symbol.signature_range.clone();
    if crate::language::is_container(&symbol.kind) {
        return vec![vec![signature]];
    }
    let mut parser = tree_sitter::Parser::new();
    if crate::language::configure_parser(&mut parser, symbol.language).is_err() {
        return vec![vec![signature]];
    }
    let Some(tree) = parser.parse(symbol.content(), None) else {
        return vec![vec![signature]];
    };
    let mut stack = vec![tree.root_node()];
    let mut candidates = Vec::new();
    while let Some(node) = stack.pop() {
        let block = |kind: &str| {
            matches!(
                kind,
                "statement_block" | "block" | "match_block" | "class_body"
            )
        };
        if node.parent().is_some_and(|p| block(p.kind()))
            && !node.has_error()
            && node.byte_range().len() <= 1024
            && !matches!(node.kind(), "comment" | "line_comment")
        {
            let mut ranges = vec![
                signature.clone(),
                symbol.start_byte + node.start_byte()..symbol.start_byte + node.end_byte(),
            ];
            let mut child = node;
            while let Some(parent) = child.parent() {
                if block(child.kind()) && parent.start_byte() < child.start_byte() {
                    ranges.push(
                        symbol.start_byte + parent.start_byte()
                            ..symbol.start_byte + child.start_byte(),
                    );
                }
                // Else excerpts also depend on the preceding condition failing.
                if matches!(parent.kind(), "if_statement" | "if_expression")
                    && let Some(branch) = parent.child_by_field_name("consequence")
                    && parent.start_byte() < branch.start_byte()
                {
                    ranges.push(
                        symbol.start_byte + parent.start_byte()
                            ..symbol.start_byte + branch.start_byte(),
                    );
                }
                child = parent;
            }
            candidates.push((
                matching_query_terms(query, &symbol.content()[node.byte_range()]),
                node.start_byte(),
                merge(ranges),
            ));
        } else {
            let mut cursor = node.walk();
            stack.extend(node.named_children(&mut cursor));
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    if candidates.is_empty() {
        vec![vec![signature]]
    } else {
        candidates.into_iter().map(|v| v.2).collect()
    }
}

fn render(symbol: &Symbol, ranges: &[Range<usize>]) -> (String, Vec<SourceSpan>, bool) {
    let complete = ranges.len() == 1 && ranges[0] == (symbol.start_byte..symbol.end_byte);
    let mut content = String::new();
    let mut spans = Vec::new();
    let mut previous = symbol.start_byte;
    for r in ranges {
        if r.start > previous {
            content.push_str("\n[… omitted …]\n");
        }
        content.push_str(&symbol.source[r.clone()]);
        let start_line = symbol.source[..r.start]
            .bytes()
            .filter(|&b| b == b'\n')
            .count()
            + 1;
        let end_line = start_line
            + symbol.source[r.clone()]
                .strip_suffix('\n')
                .unwrap_or(&symbol.source[r.clone()])
                .bytes()
                .filter(|&b| b == b'\n')
                .count();
        spans.push(SourceSpan {
            start_byte: r.start,
            end_byte: r.end,
            start_line,
            end_line,
        });
        previous = r.end;
    }
    if previous < symbol.end_byte {
        content.push_str("\n[… omitted …]\n");
    }
    (content, spans, !complete)
}

pub fn select(
    ranked: &[ScoredSymbol],
    symbols: &[Symbol],
    graph: &RelationGraph,
    query: &Query,
    max_bytes: usize,
    max_results: usize,
) -> (Vec<SearchResult>, HashMap<usize, String>) {
    let mut units = Vec::new();
    for (rank, item) in ranked.iter().take(256).enumerate() {
        for (part, ranges) in plan(&symbols[item.symbol_id], query)
            .into_iter()
            .enumerate()
        {
            units.push((
                item,
                ranges,
                item.score / (1.0 + part as f64 * 0.35),
                rank,
                part,
            ));
        }
    }
    units.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.3.cmp(&b.3)).then(a.4.cmp(&b.4)));
    let mut selected: Vec<(usize, Vec<Range<usize>>)> = Vec::new();
    let mut covered: HashMap<&str, Vec<Range<usize>>> = HashMap::new();
    let mut used = 0;
    let mut decisions = HashMap::new();
    let mut stopped = "candidate pool limit";
    for (item, ranges, _, _, _) in units {
        let symbol = &symbols[item.symbol_id];
        let visible = covered.entry(&symbol.path).or_default();
        let additions = subtract(ranges, visible);
        if additions.is_empty() {
            decisions
                .entry(symbol.id)
                .or_insert_with(|| "overlapping source".into());
            continue;
        }
        let index = selected.iter().position(|s| s.0 == symbol.id);
        if index.is_none() && selected.len() >= max_results {
            stopped = "result limit";
            break;
        }
        let old = index.map(|i| selected[i].1.clone()).unwrap_or_default();
        let new = merge(
            old.iter()
                .cloned()
                .chain(additions.iter().cloned())
                .collect(),
        );
        let old_cost = if old.is_empty() {
            0
        } else {
            render(symbol, &old).0.len().div_ceil(4) * 4
        };
        let new_cost = render(symbol, &new).0.len().div_ceil(4) * 4;
        let proposed = used - old_cost + new_cost;
        // Do not skip an oversized next unit and backfill with weaker evidence.
        if proposed > max_bytes {
            stopped = "source budget";
            break;
        }
        used = proposed;
        if let Some(i) = index {
            selected[i].1 = new;
        } else {
            selected.push((symbol.id, new));
        }
        *visible = merge(visible.iter().cloned().chain(additions).collect());
        decisions.insert(symbol.id, "selected".into());
    }
    for item in ranked {
        decisions
            .entry(item.symbol_id)
            .or_insert_with(|| stopped.into());
    }
    let scores: HashMap<_, _> = ranked.iter().map(|s| (s.symbol_id, s)).collect();
    let results = selected
        .into_iter()
        .map(|(id, ranges)| {
            let symbol = &symbols[id];
            let item = scores[&id];
            let (content, source_spans, content_truncated) = render(symbol, &ranges);
            SearchResult {
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
                score: item.score,
                lexical_score: item.signals.total()
                    - item.signals.structural_priority
                    - item.signals.structural_relation,
                structural_score: item.signals.structural_priority
                    + item.signals.structural_relation,
                diversity_score: 0.0,
                final_score: item.score,
                signals: item.signals.clone(),
                content_bytes: content.len(),
                approximate_tokens: content.len().div_ceil(4),
                content,
                content_truncated,
                source_spans,
                relations: serializable_relations(id, graph, symbols),
            }
        })
        .collect();
    (results, decisions)
}
