//! Budgeted excerpts preserve complete syntax nodes and explicit original-file ranges.
use crate::{
    lexical::{Query, matching_query_terms},
    model::{SourceSpan, Symbol},
};

pub fn slice_symbol(
    symbol: &Symbol,
    query: &Query,
    limit: usize,
) -> Option<(String, Vec<SourceSpan>)> {
    let mut content = format!(
        "{}\n[… body excerpts; omitted code is not shown …]\n",
        symbol.signature()
    );
    if content.len() > limit {
        return None;
    }
    let available = limit.saturating_sub(content.len());
    let line_starts: Vec<_> = std::iter::once(symbol.start_byte)
        .chain(
            symbol
                .content()
                .bytes()
                .enumerate()
                .filter_map(|(i, b)| (b == b'\n').then_some(symbol.start_byte + i + 1)),
        )
        .collect();
    let span_for = |range: std::ops::Range<usize>| SourceSpan {
        start_byte: range.start,
        end_byte: range.end,
        start_line: symbol.start_line + line_starts.partition_point(|&p| p <= range.start) - 1,
        end_line: symbol.start_line + line_starts.partition_point(|&p| p <= range.end) - 1,
    };
    let mut candidates: Vec<(std::ops::Range<usize>, usize)> = Vec::new();
    // Reuse complete statements collected in the full-file AST. This also
    // handles methods that cannot be parsed correctly outside their container.
    for range in &symbol.excerpt_ranges {
        if range.len() + 48 > available
            // Ranges arrive in source order. Selected enclosing ranges suppress
            // their descendants, so only the most recent range can contain this one.
            || candidates.last().is_some_and(|(parent, _)|
                parent.start <= range.start && parent.end >= range.end)
        {
            continue;
        }
        let source = symbol.source.get(range.clone())?;
        candidates.push((range.clone(), matching_query_terms(query, source)));
    }
    if candidates.is_empty()
        && !symbol.body_range.is_empty()
        && symbol.body_range.len() + 48 <= available
    {
        candidates.push((
            symbol.body_range.clone(),
            matching_query_terms(query, symbol.body()),
        ));
    }
    candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.start.cmp(&b.0.start)));
    let mut chosen = Vec::new();
    let mut used = content.len();
    for (range, _) in candidates {
        let span = span_for(range.clone());
        let header = format!("\n[lines {}–{}]\n", span.start_line, span.end_line);
        let cost = header.len() + range.len() + 1;
        if cost + used <= limit {
            chosen.push(range);
            used += cost;
        }
    }
    chosen.sort_by_key(|r| r.start);
    let signature = symbol.signature();
    let raw_signature = &symbol.source[symbol.signature_range.clone()];
    let start =
        symbol.signature_range.start + raw_signature.len() - raw_signature.trim_start().len();
    let mut spans = vec![span_for(start..start + signature.len())];
    for range in chosen {
        let span = span_for(range.clone());
        let start_line = span.start_line;
        let end_line = span.end_line;
        content.push_str(&format!("\n[lines {start_line}–{end_line}]\n"));
        content.push_str(&symbol.source[range]);
        content.push('\n');
        spans.push(span);
    }
    Some((content, spans))
}

pub(crate) fn span_for_range(symbol: &Symbol, range: std::ops::Range<usize>) -> SourceSpan {
    let line = |offset| {
        symbol.start_line
            + symbol.source.as_bytes()[symbol.start_byte..offset]
                .iter()
                .filter(|&&b| b == b'\n')
                .count()
    };
    SourceSpan {
        start_byte: range.start,
        end_byte: range.end,
        start_line: line(range.start),
        end_line: line(range.end),
    }
}
