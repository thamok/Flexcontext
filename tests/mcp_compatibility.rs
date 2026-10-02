use flexcontext::SearchSession;
use serde_json::{Value, json};

fn initialize(version: &str) -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":version,"capabilities":{},
        "clientInfo":{"name":"test-client","version":"1"}
    }})
}

fn run(messages: &[Value]) -> Vec<(Value, usize)> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("auth.ts"),
        "function authenticate() { return 'unique-code-payload-🔒'; }",
    )
    .unwrap();
    let mut session = SearchSession::open(dir.path(), false).unwrap();
    let input = messages
        .iter()
        .map(|v| format!("{v}\n"))
        .collect::<String>();
    let mut bytes = Vec::new();
    flexcontext::mcp::serve(&mut session, input.as_bytes(), &mut bytes).unwrap();
    String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|line| (serde_json::from_str(line).unwrap(), line.len() + 1))
        .collect()
}

#[test]
fn legacy_handshake_tools_and_single_copy_payload_with_exact_cost() {
    let responses = run(&[
        initialize("2025-06-18"),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":"search-🔒","method":"tools/call","params":{
            "_meta":{"progressToken":"progress-1"},
            "name":"code_search","arguments":{"query":"authenticate","max_tokens":1600,"detail":"full"}
        }}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"refresh_index"}}),
        json!({"jsonrpc":"2.0","id":5,"method":"ping"}),
        json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"code_search","arguments":{"query":"authenticate","max_tokens":1}}}),
    ]);
    assert_eq!(responses.len(), 6);
    assert_eq!(responses[0].0["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(
        responses[0].0["result"]["serverInfo"]["name"],
        "flexcontext"
    );
    assert_eq!(
        responses[1].0["result"]["tools"].as_array().unwrap().len(),
        2
    );
    for (response, _) in &responses {
        assert!(response["result"].get("resultType").is_none());
        assert!(response["result"].get("_meta").is_none());
    }
    let (response, bytes) = &responses[2];
    let result = &response["result"];
    assert_eq!(
        result["content"],
        json!([{"type":"text","text":"1 relevant code units found."}])
    );
    assert_eq!(result["isError"], false);
    assert_eq!(
        result["structuredContent"]["results"][0]["symbol"],
        "authenticate"
    );
    assert_eq!(
        response
            .to_string()
            .matches("unique-code-payload-🔒")
            .count(),
        1
    );
    let cost = &result["structuredContent"]["context_cost"];
    assert_eq!(cost["serialized_bytes"], *bytes);
    assert_eq!(cost["estimated_tokens"], bytes.div_ceil(4));
    assert!(*bytes <= 1600 * 4);
    assert_eq!(cost["representation"], "mcp-jsonrpc-2025-06-18");
    assert_eq!(responses[3].0["result"]["isError"], false);
    assert_eq!(responses[4].0["result"], json!({}));
    assert_eq!(responses[5].0["result"]["isError"], true);
    assert!(responses[5].0["result"].get("structuredContent").is_none());
}

#[test]
fn legacy_lifecycle_negotiation_and_invalid_initialization() {
    let responses = run(&[
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":0,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
        initialize("unsupported-version"),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"ping"}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        initialize("2025-06-18"),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/list"}),
    ]);
    assert_eq!(responses.len(), 7);
    assert_eq!(responses[0].0["error"]["code"], -32602);
    assert_eq!(responses[1].0["error"]["code"], -32602);
    assert_eq!(responses[2].0["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(responses[3].0["error"]["code"], -32600);
    assert_eq!(responses[4].0["result"], json!({}));
    assert_eq!(responses[5].0["error"]["code"], -32600);
    assert_eq!(
        responses[6].0["result"]["tools"].as_array().unwrap().len(),
        2
    );
}

#[test]
fn modern_metadata_is_never_bypassed_by_legacy_state() {
    let responses = run(&[
        initialize("2025-06-18"),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"ping","params":{"_meta":{
            "io.modelcontextprotocol/protocolVersion":"2026-07-28"
        }}}),
        json!({"jsonrpc":"2.0","id":3,"method":"ping","params":{"_meta":{
            "io.modelcontextprotocol/protocolVersion":"2025-06-18",
            "io.modelcontextprotocol/clientCapabilities":{}
        }}}),
        json!({"jsonrpc":"2.0","id":4,"method":"ping","params":{"_meta":{
            "io.modelcontextprotocol/protocolVersion":"2026-07-28",
            "io.modelcontextprotocol/clientCapabilities":{}
        }}}),
        json!({"jsonrpc":"2.0","id":5,"method":"ping"}),
    ]);
    assert_eq!(responses[1].0["error"]["code"], -32602);
    assert_eq!(responses[2].0["error"]["code"], -32022);
    assert_eq!(responses[3].0["result"]["resultType"], "complete");
    assert_eq!(responses[4].0["result"], json!({}));
}
