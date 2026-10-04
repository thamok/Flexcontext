//! MCP stdio: modern stateless requests or a negotiated 2025-06-18 session.
use crate::SearchSession;
use anyhow::Result;
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{BufRead, Write};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryArgs {
    query: String,
    #[serde(default = "default_budget")]
    budget: usize,
    #[serde(default = "default_results")]
    max_results: usize,
    #[serde(default)]
    max_tokens: Option<usize>,
    #[serde(default)]
    detail: crate::model::Detail,
    #[serde(default)]
    scope: crate::model::ScopeMode,
    #[serde(default)]
    include_paths: Vec<String>,
    #[serde(default)]
    exclude_paths: Vec<String>,
    #[serde(default)]
    policy: crate::model::RetrievalPolicy,
    #[serde(default = "default_cutoff")]
    cutoff: f64,
    #[serde(default)]
    explain: bool,
    #[serde(default)]
    continuations: bool,
    #[serde(default)]
    role_hints: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpandArgs {
    reference: String,
    #[serde(default = "default_budget")]
    budget: usize,
    #[serde(default)]
    max_tokens: Option<usize>,
    #[serde(default)]
    detail: crate::model::Detail,
}
fn default_cutoff() -> f64 {
    0.25
}
fn default_budget() -> usize {
    4096
}
fn default_results() -> usize {
    12
}

type Cancellations = std::sync::Arc<
    std::sync::Mutex<
        std::collections::HashMap<String, std::sync::Arc<std::sync::atomic::AtomicBool>>,
    >,
>;
struct Pending {
    value: Value,
    parse_error: bool,
    key: Option<String>,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    registry: Cancellations,
}
impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(key) = &self.key {
            self.registry.lock().unwrap().remove(key);
        }
    }
}

pub fn serve(
    session: &mut SearchSession,
    input: impl BufRead + Send,
    mut output: impl Write,
) -> Result<()> {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    let registry: Cancellations = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let (sender, receiver) = mpsc::sync_channel(128);
    std::thread::scope(|scope| {
        scope.spawn(move || {
            for line in input.lines() {
                let value = line.map(|line| match serde_json::from_str::<Value>(&line) {
                    Ok(value) => (value, false),
                    Err(_) => (Value::Null, true),
                });
                match value {
                    Ok((value, parse_error)) => {
                        if value["jsonrpc"] == "2.0"
                            && value["method"] == "notifications/cancelled"
                            && value.get("id").is_none()
                        {
                            let key = value["params"]["requestId"].to_string();
                            if let Some(flag) = registry.lock().unwrap().get(&key) {
                                flag.store(true, Ordering::Relaxed);
                            }
                            continue;
                        }
                        let key = value.get("id").map(Value::to_string);
                        let flag = Arc::new(AtomicBool::new(false));
                        if let Some(key) = &key {
                            registry.lock().unwrap().insert(key.clone(), flag.clone());
                        }
                        if sender
                            .send(Ok(Pending {
                                value,
                                parse_error,
                                key,
                                cancelled: flag,
                                registry: registry.clone(),
                            }))
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(err) => {
                        let _ = sender.send(Err(err));
                        return;
                    }
                }
            }
        });
        process_requests(session, receiver, &mut output)
    })
}
fn process_requests(
    session: &mut SearchSession,
    requests: impl IntoIterator<Item = std::io::Result<Pending>>,
    mut output: impl Write,
) -> Result<()> {
    // None: no legacy handshake; Some(false): awaiting initialized notification.
    // Modern requests always validate their own metadata, independently of this state.
    let mut legacy_ready = None;
    for pending in requests {
        let pending = pending?;
        let _scope = crate::repository::RequestCancellation::enter(pending.cancelled.clone());
        if crate::repository::request_cancelled() {
            continue;
        }
        let request = &pending.value;
        if pending.parse_error {
            write_response(&mut output, &error(Value::Null, -32700, "Parse error"))?;
            continue;
        }
        let id = request.get("id").cloned();
        let method = request.get("method").and_then(Value::as_str);
        if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
            || method.is_none()
            || id
                .as_ref()
                .is_some_and(|id| !id.is_string() && !id.is_i64() && !id.is_u64())
        {
            write_response(&mut output, &error(Value::Null, -32600, "Invalid Request"))?;
            continue;
        }
        let method = method.unwrap();
        // Notifications never receive a response.
        let Some(id) = id else {
            if method == "notifications/initialized" && legacy_ready == Some(false) {
                legacy_ready = Some(true);
            }
            continue;
        };
        let params = &request["params"];
        if method == "initialize" {
            if legacy_ready.is_some() {
                write_response(&mut output, &error(id, -32600, "Already initialized"))?;
                continue;
            }
            if !params["protocolVersion"].is_string()
                || !params["capabilities"].is_object()
                || !params["clientInfo"]["name"].is_string()
                || !params["clientInfo"]["version"].is_string()
            {
                write_response(
                    &mut output,
                    &error(id, -32602, "Invalid initialize parameters"),
                )?;
                continue;
            }
            // For unsupported handshake versions, propose the supported legacy
            // version. The client must disconnect if it cannot speak that version.
            legacy_ready = Some(false);
            write_response(
                &mut output,
                &json!({"jsonrpc":"2.0","id":id,"result":{
                    "protocolVersion":LEGACY_PROTOCOL_VERSION,
                    "capabilities":{"tools":{}},
                    "serverInfo":{"name":"flexcontext","version":env!("CARGO_PKG_VERSION")},
                    "instructions":INSTRUCTIONS
                }}),
            )?;
            continue;
        }
        let meta = &params["_meta"];
        let modern = meta
            .get("io.modelcontextprotocol/protocolVersion")
            .is_some()
            || meta
                .get("io.modelcontextprotocol/clientCapabilities")
                .is_some();
        if !modern && (legacy_ready == Some(true) || method == "ping") {
            // Other _meta entries (e.g. progressToken) are valid in legacy MCP.
        } else if !modern && legacy_ready == Some(false) {
            write_response(
                &mut output,
                &error(id, -32600, "Awaiting notifications/initialized"),
            )?;
            continue;
        } else {
            let Some(version) = meta["io.modelcontextprotocol/protocolVersion"].as_str() else {
                write_response(
                    &mut output,
                    &error(
                        id,
                        -32602,
                        "Missing required MCP 2026-07-28 protocolVersion metadata",
                    ),
                )?;
                continue;
            };
            if !meta["io.modelcontextprotocol/clientCapabilities"].is_object() {
                write_response(
                    &mut output,
                    &error(id, -32602, "Missing required clientCapabilities object"),
                )?;
                continue;
            }
            if version != PROTOCOL_VERSION {
                write_response(
                    &mut output,
                    &json!({"jsonrpc":"2.0","id":id,"error":{"code":-32022,"message":"Unsupported protocol version","data":{"supported":[PROTOCOL_VERSION],"requested":version}}}),
                )?;
                continue;
            }
        }
        let result = match method {
            "server/discover" if modern => Ok(
                json!({"supportedVersions":[PROTOCOL_VERSION,LEGACY_PROTOCOL_VERSION],"capabilities":{"tools":{}},"instructions":INSTRUCTIONS}),
            ),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools":[
                {"name":"code_search", "description":"Retrieve diverse structural code context from the resident repository snapshot. budget selects source bytes/4; max_tokens bounds the complete serialized response estimate. Read code from structuredContent; text is a summary. Opt-in continuations expose omitted-source leads; fetch relevant references with expand_context to establish behavior. Optional role_hints are source-word heuristics, not facts. Other searches may still be needed.",
                 "inputSchema":{"type":"object", "properties":{"query":{"type":"string"},"budget":{"type":"integer","minimum":1,"default":4096},"max_tokens":{"type":"integer","minimum":1},"max_results":{"type":"integer","minimum":1,"maximum":100,"default":12},
                    "detail":{"type":"string","enum":["compact","full"],"default":"compact"},
                    "scope":{"type":"string","enum":["auto","repository"],"default":"auto"},
                    "include_paths":{"type":"array","items":{"type":"string"},"description":"Repository-relative exact file or directory subtree"},
                    "exclude_paths":{"type":"array","items":{"type":"string"},"description":"Exclusions override includes"},
                    "policy":{"type":"string","enum":["baseline","relations","quotas","diversity","idf","direct","implementation","focus","stable","cutoff","focused"],"default":"baseline"},
                    "cutoff":{"type":"number","minimum":0,"maximum":1,"default":0.25},
                    "continuations":{"type":"boolean","default":false},
                    "role_hints":{"type":"boolean","default":false},
                    "explain":{"type":"boolean","default":false,"description":"Candidate diagnostic trace; implies full detail"}},"required":["query"],"additionalProperties":false}},
                {"name":"expand_context", "description":"Fetch one exact omitted candidate or next navigation page via its opaque reference, bypassing ranking/quotas. References preserve original scope, reject changed snapshots, and survive identical-snapshot restarts. Partial source has follow-up leads; tiny budgets may fail explicitly. No automatic seen state: choose references from the latest response to avoid repeating source.", "inputSchema":{"type":"object","properties":{"reference":{"type":"string","maxLength":32768},"budget":{"type":"integer","minimum":1,"default":4096},"max_tokens":{"type":"integer","minimum":1},"detail":{"type":"string","enum":["compact","full"],"default":"compact"}},"required":["reference"],"additionalProperties":false}},
                {"name":"refresh_index", "description":"Reload the repository snapshot after files are edited, added or deleted.", "inputSchema":{"type":"object","properties":{},"additionalProperties":false}}
            ]})),
            "tools/call" => match params["name"].as_str() {
                Some("code_search") => {
                    match serde_json::from_value::<QueryArgs>(params["arguments"].clone()) {
                        Ok(args)
                            if args.budget > 0
                                && (1..=100).contains(&args.max_results)
                                && args.budget.checked_mul(4).is_some()
                                && args
                                    .max_tokens
                                    .is_none_or(|n| n > 0 && n.checked_mul(4).is_some()) =>
                        {
                            let detail = if args.explain {
                                crate::model::Detail::Full
                            } else {
                                args.detail
                            };
                            let options = crate::model::QueryOptions {
                                query: args.query,
                                max_bytes: args.budget * 4,
                                max_results: args.max_results,
                                detail,
                                policy: args.policy,
                                cutoff: args.cutoff,
                                explain: args.explain,
                                continuations: args.continuations,
                                role_hints: args.role_hints,
                                scope: crate::model::SearchScope {
                                    scope: args.scope,
                                    include_paths: args.include_paths,
                                    exclude_paths: args.exclude_paths,
                                },
                            };
                            match session.query_with_options(&options) {
                                Ok(mut response) => {
                                    let representation = crate::output::Representation::mcp(
                                        detail,
                                        id.clone(),
                                        modern,
                                    );
                                    match crate::output::finalize(
                                        &mut response,
                                        &representation,
                                        args.max_tokens,
                                    ) {
                                        Ok(()) => {
                                            Ok(crate::output::tool_result_detail(&response, detail))
                                        }
                                        Err(err) => Ok(tool_error(&err.to_string())),
                                    }
                                }
                                Err(err) => Ok(tool_error(&err.to_string())),
                            }
                        }
                        _ => Err((-32602, "Invalid code_search arguments")),
                    }
                }
                Some("expand_context") => {
                    match serde_json::from_value::<ExpandArgs>(params["arguments"].clone()) {
                        Ok(args)
                            if args.budget > 0
                                && args.budget.checked_mul(4).is_some()
                                && args
                                    .max_tokens
                                    .is_none_or(|n| n > 0 && n.checked_mul(4).is_some()) =>
                        {
                            match session.expand(&args.reference, args.budget * 4).and_then(
                                |mut response| {
                                    crate::output::finalize(
                                        &mut response,
                                        &crate::output::Representation::mcp(
                                            args.detail,
                                            id.clone(),
                                            modern,
                                        ),
                                        args.max_tokens,
                                    )?;
                                    Ok(crate::output::tool_result_detail(&response, args.detail))
                                },
                            ) {
                                Ok(result) => Ok(result),
                                Err(err) => Ok(tool_error(&err.to_string())),
                            }
                        }
                        _ => Err((-32602, "Invalid expand_context arguments")),
                    }
                }
                Some("refresh_index")
                    if params.get("arguments").is_none()
                        || params["arguments"]
                            .as_object()
                            .is_some_and(|args| args.is_empty()) =>
                {
                    Ok(match session.refresh() {
                        Ok(()) => {
                            json!({"content":[{"type":"text","text":"Repository snapshot refreshed."}],"isError":false})
                        }
                        Err(err) => tool_error(&err.to_string()),
                    })
                }
                Some("refresh_index") => Err((-32602, "refresh_index takes no arguments")),
                _ => Err((-32602, "Unknown tool")),
            },
            _ => Err((-32601, "Method not found")),
        };
        let response = match result {
            Ok(result) => {
                json!({"jsonrpc":"2.0","id":id,"result":if modern { complete(result) } else { result }})
            }
            Err((code, message)) => error(id, code, message),
        };
        write_response(&mut output, &response)?;
    }
    Ok(())
}
fn error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
fn tool_error(message: &str) -> Value {
    json!({"content":[{"type":"text","text":message}],"isError":true})
}
fn write_response(output: &mut impl Write, response: &Value) -> Result<()> {
    if crate::repository::request_cancelled() {
        return Ok(());
    }
    serde_json::to_writer(&mut *output, response)?;
    writeln!(output)?;
    output.flush()?;
    Ok(())
}

pub const PROTOCOL_VERSION: &str = "2026-07-28";
pub const LEGACY_PROTOCOL_VERSION: &str = "2025-06-18";
const INSTRUCTIONS: &str = "Searches the configured repository snapshot. Read code_search structuredContent. Call refresh_index after repository edits.";
pub fn complete(mut result: Value) -> Value {
    result["resultType"] = json!("complete");
    result["_meta"] = json!({"io.modelcontextprotocol/serverInfo":{"name":"flexcontext","version":env!("CARGO_PKG_VERSION")}});
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_request_has_no_response_and_does_not_cancel_next_request() {
        use std::sync::{Arc, Mutex, atomic::AtomicBool};
        let root = tempfile::tempdir().unwrap();
        let mut session = SearchSession::open(root.path(), false).unwrap();
        let registry: Cancellations = Arc::new(Mutex::new(Default::default()));
        let pending = |id: u64, cancelled| {
            Ok(Pending {
                value: json!({"jsonrpc":"2.0","id":id,"method":"ping","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":PROTOCOL_VERSION,"io.modelcontextprotocol/clientCapabilities":{}}}}),
                parse_error: false,
                key: Some(id.to_string()),
                cancelled: Arc::new(AtomicBool::new(cancelled)),
                registry: registry.clone(),
            })
        };
        let mut output = Vec::new();
        process_requests(
            &mut session,
            [pending(1u64, true), pending(2, false)],
            &mut output,
        )
        .unwrap();
        let response: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(response["id"], 2);
        assert_eq!(response["result"]["resultType"], "complete");
    }
    #[test]
    fn malformed_json_and_invalid_request_are_distinct() {
        let root = tempfile::tempdir().unwrap();
        let mut session = SearchSession::open(root.path(), false).unwrap();
        let mut output = Vec::new();
        serve(&mut session, "{\nnull\n".as_bytes(), &mut output).unwrap();
        let lines: Vec<Value> = std::str::from_utf8(&output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(lines[0]["error"]["code"], -32700);
        assert_eq!(lines[1]["error"]["code"], -32600);
    }
}
