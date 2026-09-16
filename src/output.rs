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
    output
}

/// The exact representation whose estimated context budget is enforced.
pub enum Representation {
    Human,
    Json,
    Mcp { id: serde_json::Value },
    McpLegacy { id: serde_json::Value },
}
impl Representation {
    fn name(&self) -> &str {
        match self {
            Self::Human => "human",
            Self::Json => "json-pretty",
            Self::Mcp { .. } => "mcp-jsonrpc",
            Self::McpLegacy { .. } => "mcp-jsonrpc-2025-06-18",
        }
    }
    pub fn render(&self, response: &SearchResponse) -> anyhow::Result<Vec<u8>> {
        let mut bytes = match self {
            Self::Human => return Ok(render_human(response).into_bytes()),
            Self::Json => serde_json::to_vec_pretty(response)?,
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
            response.stats.human_payload_bytes = render_human(response).len();
            response.stats.json_payload_bytes = serde_json::to_vec_pretty(response)?.len() + 1;
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
        anyhow::ensure!(
            response.results.pop().is_some(),
            "token budget cannot fit response metadata (requires approximately {} tokens)",
            response.context_cost.estimated_tokens
        );
    }
}
