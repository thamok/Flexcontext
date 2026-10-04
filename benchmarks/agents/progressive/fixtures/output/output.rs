use std::fmt::Write;

use crate::model::SearchResponse;

pub fn render_human(response: &SearchResponse) -> String {
    let mut output = String::new();
    for (index, result) in response.results.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        let _ = writeln!(output, "{:.2}  {}", result.score, result.path);
        let container = result
            .containing_symbol
            .as_deref()
            .map(|name| format!(" in {name}"))
            .unwrap_or_default();
        let _ = writeln!(
            output,
            "{} {}{}\nlines {}–{} · {} bytes{}\n",
            result.kind,
            result.symbol,
            container,
            result.start_line,
            result.end_line,
            result.content_bytes,
            if result.content_truncated {
                " · compacted"
            } else {
                ""
            }
        );
        output.push_str(&result.content);
        if !result.content.ends_with('\n') {
            output.push('\n');
        }
        if !result.relations.is_empty() {
            output.push_str("\nrelated:\n");
            for relation in result.relations.iter().take(12) {
                let _ = writeln!(
                    output,
                    "  {:<12} {:<28} {}:{}",
                    relation.kind, relation.symbol, relation.path, relation.start_line
                );
            }
        }
    }
    if response.results.is_empty() {
        output.push_str("No structurally relevant source units found.\n");
    }
    let stats = &response.stats;
    let _ = writeln!(
        output,
        "\n{} results · {} source bytes (~{} source tokens) · {} human payload bytes · {} files indexed · {} symbols · {} µs",
        stats.returned_symbols,
        stats.returned_bytes,
        stats.approximate_tokens,
        stats.human_payload_bytes,
        stats.files_indexed,
        stats.symbols,
        stats.elapsed_us
    );
    let _ = writeln!(
        output,
        "{} serialized bytes · ~{} estimated context tokens · scanned {} files · skipped {} oversized, {} binary/non-UTF-8 files, {} excluded directories, {} depth-limited directories",
        response.context_cost.serialized_bytes,
        response.context_cost.estimated_tokens,
        stats.files_scanned,
        stats.scan.oversized_files,
        stats.skipped_binary_or_unreadable_files,
        stats.scan.excluded_directories,
        stats.scan.depth_limited_directories
    );
    if !response.trace.is_empty() {
        output.push_str("\nretrieval trace:\n");
        for trace in &response.trace {
            let _ = writeln!(
                output,
                "{}:{} {} · direct rank {:?} score {:.3} · relationship rank {} boost {:.3} · scope boost {:.3} · {}{}",
                trace.path,
                trace.start_line,
                trace.symbol,
                trace.direct_rank,
                trace.direct_score,
                trace.relationship_rank,
                trace.relationship_boost,
                trace.scope_promotion,
                trace.decision,
                if trace.truncated {
                    " · excerpt truncated"
                } else {
                    ""
                }
            );
        }
    }
    output
}

/// The exact representation whose estimated context budget is enforced.
pub enum Representation {
    Human,
    Json,
    Mcp { id: serde_json::Value },
    McpLegacy { id: serde_json::Value },
    CompactJson,
    CompactHuman,
    CompactMcp { id: serde_json::Value, modern: bool },
}
impl Representation {
    pub fn json(detail: crate::model::Detail) -> Self {
        if detail == crate::model::Detail::Compact {
            Self::CompactJson
        } else {
            Self::Json
        }
    }
    pub fn human(detail: crate::model::Detail) -> Self {
        if detail == crate::model::Detail::Compact {
            Self::CompactHuman
        } else {
            Self::Human
        }
    }
    pub fn mcp(detail: crate::model::Detail, id: serde_json::Value, modern: bool) -> Self {
        if detail == crate::model::Detail::Compact {
            Self::CompactMcp { id, modern }
        } else if modern {
            Self::Mcp { id }
        } else {
            Self::McpLegacy { id }
        }
    }
    fn compact(&self) -> bool {
        matches!(
            self,
            Self::CompactJson | Self::CompactHuman | Self::CompactMcp { .. }
        )
    }
    fn name(&self) -> &str {
        match self {
            Self::Human => "human",
            Self::Json => "json-pretty",
            Self::Mcp { .. } => "mcp-jsonrpc",
            Self::McpLegacy { .. } => "mcp-jsonrpc-2025-06-18",
            Self::CompactJson => "json-compact",
            Self::CompactHuman => "human-compact",
            Self::CompactMcp { modern: true, .. } => "mcp-compact",
            Self::CompactMcp { modern: false, .. } => "mcp-compact-2025-06-18",
        }
    }
    pub fn render(&self, response: &SearchResponse) -> anyhow::Result<Vec<u8>> {
        let mut bytes = match self {
            Self::Human => return Ok(render_human(response).into_bytes()),
            Self::Json => serde_json::to_vec_pretty(response)?,
            Self::CompactJson => serde_json::to_vec(&compact_value(response))?,
            Self::CompactHuman => return Ok(render_compact_human(response).into_bytes()),
            Self::CompactMcp { id, modern } => {
                let result = tool_result_detail(response, crate::model::Detail::Compact);
                serde_json::to_vec(
                    &serde_json::json!({"jsonrpc":"2.0","id":id,"result":if *modern { crate::mcp::complete(result) } else { result }}),
                )?
            }
            Self::Mcp { id } => serde_json::to_vec(
                &serde_json::json!({"jsonrpc":"2.0","id":id,"result":crate::mcp::complete(tool_result(response))}),
            )?,
            Self::McpLegacy { id } => serde_json::to_vec(
                &serde_json::json!({"jsonrpc":"2.0","id":id,"result":tool_result(response)}),
            )?,
        };
        bytes.push(b'\n');
        Ok(bytes)
    }
}
pub fn compact_value(response: &SearchResponse) -> serde_json::Value {
    serde_json::json!({"query":response.query,"scope":response.scope,"policy":response.policy,"focused_files":response.focused_files,"context_cost":response.context_cost,
        "results":response.results.iter().map(|r| serde_json::json!({
            "path":r.path,"symbol":r.symbol,"kind":r.kind,"language":r.language,
            "containing_symbol":r.containing_symbol,"start_line":r.start_line,"end_line":r.end_line,
            "content":r.content,"source_spans":r.source_spans,"content_truncated":r.content_truncated
        })).collect::<Vec<_>>()})
}
fn render_compact_human(response: &SearchResponse) -> String {
    let mut out = format!(
        "query: {}\nscope: {}\n",
        response.query,
        serde_json::to_string(&response.scope).unwrap()
    );
    for r in &response.results {
        let _ = writeln!(
            out,
            "\n{}:{}–{} {} {}{}",
            r.path,
            r.start_line,
            r.end_line,
            r.kind,
            r.symbol,
            if r.content_truncated {
                " [excerpt]"
            } else {
                ""
            }
        );
        let _ = writeln!(
            out,
            "spans: {}",
            serde_json::to_string(&r.source_spans).unwrap()
        );
        let _ = writeln!(out, "{}", r.content);
    }
    let _ = writeln!(
        out,
        "\ncost: {}",
        serde_json::to_string(&response.context_cost).unwrap()
    );
    out
}
pub fn tool_result_detail(
    response: &SearchResponse,
    detail: crate::model::Detail,
) -> serde_json::Value {
    if detail == crate::model::Detail::Full {
        return tool_result(response);
    }
    serde_json::json!({"content":[{"type":"text","text":format!("{} relevant code units found.",response.results.len())}],"structuredContent":compact_value(response),"isError":false})
}
pub fn tool_result(response: &SearchResponse) -> serde_json::Value {
    serde_json::json!({"content":[{"type":"text","text":format!("{} relevant code units found.",response.results.len())}],"structuredContent":response,"isError":false})
}
/// Drop the lowest-priority selected units until the full output fits. We do
/// not silently return an over-budget metadata-only response for tiny budgets.
pub fn finalize(
    response: &mut SearchResponse,
    representation: &Representation,
    max_tokens: Option<usize>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        max_tokens != Some(0),
        "--max-tokens must be greater than zero"
    );
    let limit = max_tokens
        .map(|tokens| {
            tokens
                .checked_mul(4)
                .ok_or_else(|| anyhow::anyhow!("token budget is too large"))
        })
        .transpose()?;
    response.context_cost.token_budget = max_tokens;
    response.context_cost.representation = representation.name().into();
    loop {
        response.stats.returned_symbols = response.results.len();
        response.stats.returned_bytes = response.results.iter().map(|r| r.content_bytes).sum();
        response.stats.approximate_tokens =
            response.results.iter().map(|r| r.approximate_tokens).sum();
        response.context_cost.source_bytes = response.stats.returned_bytes;
        // Sizes include their own decimal representations; converge after all
        // other metadata (especially timing) is frozen.
        let mut stable = false;
        for _ in 0..32 {
            let before = (
                response.stats.human_payload_bytes,
                response.stats.json_payload_bytes,
                response.context_cost.clone(),
            );
            if !representation.compact() {
                response.stats.human_payload_bytes = render_human(response).len();
                response.stats.json_payload_bytes = serde_json::to_vec_pretty(response)?.len() + 1;
            }
            response.context_cost.serialized_bytes = representation.render(response)?.len();
            response.context_cost.estimated_tokens =
                response.context_cost.serialized_bytes.div_ceil(4);
            if before
                == (
                    response.stats.human_payload_bytes,
                    response.stats.json_payload_bytes,
                    response.context_cost.clone(),
                )
            {
                stable = true;
                break;
            }
        }
        anyhow::ensure!(stable, "serialized cost accounting failed to converge");
        if limit.is_none_or(|limit| response.context_cost.serialized_bytes <= limit) {
            return Ok(());
        }
        let removed = response.results.pop();
        if let Some(result) = &removed {
            for trace in &mut response.trace {
                if trace.path == result.path
                    && trace.symbol == result.symbol
                    && trace.start_line == result.start_line
                {
                    trace.decision = "serialized payload budget".into();
                }
            }
        }
        anyhow::ensure!(
            removed.is_some(),
            "token budget cannot fit response metadata (requires approximately {} tokens)",
            response.context_cost.estimated_tokens
        );
    }
}
