use flexcontext::{
    SearchOptions, SearchSession,
    output::{Representation, finalize},
    search,
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benchmarks/fixtures/typescript")
}

#[test]
fn complete_representation_costs_and_token_budgets_are_exact() {
    let session = SearchSession::open(&fixture(), false).unwrap();
    for representation in [
        Representation::Json,
        Representation::Human,
        Representation::Mcp {
            id: json!("request-🔒-with-framing"),
        },
        Representation::McpLegacy {
            id: json!("legacy-request-🔒-with-framing"),
        },
    ] {
        for budget in [512, 1024, 2048, 8000] {
            let mut response = session.query("request upload session", 12000, 12).unwrap();
            finalize(&mut response, &representation, Some(budget)).unwrap();
            let emitted = representation.render(&response).unwrap();
            assert_eq!(response.context_cost.serialized_bytes, emitted.len());
            assert_eq!(
                response.context_cost.estimated_tokens,
                emitted.len().div_ceil(4)
            );
            assert!(emitted.len() <= budget * 4);
            assert_eq!(
                response.context_cost.source_bytes,
                response
                    .results
                    .iter()
                    .map(|r| r.content_bytes)
                    .sum::<usize>()
            );
            assert_eq!(
                response.stats.json_payload_bytes,
                serde_json::to_vec_pretty(&response).unwrap().len() + 1
            );
            assert_eq!(
                response.stats.human_payload_bytes,
                flexcontext::output::render_human(&response).len()
            );
        }
    }
    let mut response = session.query("upload", 12000, 12).unwrap();
    assert!(
        finalize(&mut response, &Representation::Json, Some(1))
            .unwrap_err()
            .to_string()
            .contains("metadata")
    );
}
fn request(id: usize, method: &str, params: Value) -> Value {
    let mut request = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
    request["params"]["_meta"] = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}});
    request
}
#[test]
fn modern_mcp_is_stateless_and_does_not_duplicate_code() {
    let mut session = SearchSession::open(&fixture(), false).unwrap();
    let mut unsupported = request(3, "ping", json!({}));
    unsupported["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"] = json!("2025-11-25");
    let mut missing_caps = request(4, "ping", json!({}));
    missing_caps["params"]["_meta"]
        .as_object_mut()
        .unwrap()
        .remove("io.modelcontextprotocol/clientCapabilities");
    let messages = [
        request(
            0,
            "tools/call",
            json!({"name":"code_search","arguments":{"query":"parse HTTP request","max_tokens":1600}}),
        ),
        request(1, "server/discover", json!({})),
        json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{}}),
        unsupported,
        missing_caps,
        request(5, "ping", json!({})),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":6,"method":"tools/list"}),
    ];
    let input = messages
        .iter()
        .map(|v| serde_json::to_string(v).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let mut bytes = Vec::new();
    flexcontext::mcp::serve(&mut session, input.as_bytes(), &mut bytes).unwrap();
    let output = String::from_utf8(bytes).unwrap();
    let lines: Vec<_> = output.lines().collect();
    let responses: Vec<Value> = lines
        .iter()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 7);
    let result = &responses[0]["result"];
    assert_eq!(result["resultType"], "complete");
    assert_eq!(
        result["content"][0]["text"],
        format!(
            "{} relevant code units found.",
            result["structuredContent"]["results"]
                .as_array()
                .unwrap()
                .len()
        )
    );
    assert!(result["content"][0]["text"].as_str().unwrap().len() < 60);
    assert_eq!(
        result["structuredContent"]["context_cost"]["serialized_bytes"],
        lines[0].len() + 1
    );
    assert!(lines[0].len() < 1600 * 4);
    assert_eq!(
        responses[1]["result"]["supportedVersions"],
        json!(["2026-07-28", "2025-06-18"])
    );
    assert_eq!(responses[2]["error"]["code"], -32602);
    assert_eq!(responses[3]["error"]["code"], -32022);
    assert_eq!(
        responses[3]["error"]["data"]["supported"],
        json!(["2026-07-28"])
    );
    assert_eq!(responses[4]["error"]["code"], -32602);
    assert_eq!(responses[5]["result"]["resultType"], "complete");
    assert_eq!(responses[6]["error"]["code"], -32602);
}

#[test]
fn source_ranges_survive_cache_and_share_one_allocation_per_file() {
    let dir = tempfile::tempdir().unwrap();
    let source = "// café 🔒\nclass Session {\n authenticate() { return validate(); }\n}\nfunction validate() { return true; }\n";
    std::fs::write(dir.path().join("session.ts"), source).unwrap();
    let first = SearchSession::open(dir.path(), true).unwrap();
    let second = SearchSession::open(dir.path(), true).unwrap();
    for session in [&first, &second] {
        let symbols = session.symbols();
        assert!(symbols.len() >= 3);
        for symbol in symbols {
            assert!(symbol.valid_ranges());
            assert!(Arc::ptr_eq(&symbols[0].source, &symbol.source));
            assert_eq!(
                symbol.content(),
                &source[symbol.start_byte..symbol.end_byte]
            );
        }
        let method = symbols.iter().find(|s| s.name == "authenticate").unwrap();
        assert_eq!(method.calls, vec!["validate"]);
        let class = symbols.iter().find(|s| s.kind == "class").unwrap();
        assert_eq!(class.calls, vec!["validate"]);
    }
    let cache = std::fs::read_to_string(dir.path().join(".flexcontext/index.json")).unwrap();
    assert_eq!(
        cache
            .matches("authenticate() { return validate(); }")
            .count(),
        1
    );
    assert!(!cache.contains("\"content\":"));
}

#[test]
fn corrupt_fingerprint_and_ranges_invalidate_cache_without_panicking() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("auth.rs"), "pub fn authenticate() {}\n").unwrap();
    let options = SearchOptions {
        root: dir.path().into(),
        query: "auth".into(),
        ..Default::default()
    };
    search(&options).unwrap();
    let path = dir.path().join(".flexcontext/index.json");
    let mut cache: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    cache["compatibility"] = json!("old-extractor");
    std::fs::write(&path, serde_json::to_vec(&cache).unwrap()).unwrap();
    assert_eq!(search(&options).unwrap().stats.files_reparsed, 1);
    let mut cache: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    cache["files"]["auth.rs"]["symbols"][0]["end_byte"] = json!(usize::MAX);
    std::fs::write(&path, serde_json::to_vec(&cache).unwrap()).unwrap();
    assert_eq!(search(&options).unwrap().stats.files_reparsed, 1);
}
#[cfg(unix)]
#[test]
fn replacement_with_same_size_and_mtime_is_reparsed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.rs");
    std::fs::write(&path, "pub fn auth_old() {}\n").unwrap();
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let options = SearchOptions {
        root: dir.path().into(),
        query: "auth".into(),
        ..Default::default()
    };
    search(&options).unwrap();
    let replacement = dir.path().join("replacement");
    std::fs::write(&replacement, "pub fn auth_new() {}\n").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&replacement)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    std::fs::rename(replacement, &path).unwrap();
    let response = search(&options).unwrap();
    assert_eq!(response.stats.files_reparsed, 1);
    assert_eq!(response.results[0].symbol, "auth_new");
}
#[test]
fn discovery_enforces_ignore_size_count_depth_and_binary_limits() {
    use flexcontext::repository::{ScanLimits, discover_with_limits};
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".gitignore"), "ignored.rs\n").unwrap();
    for file in ["ignored.rs", "a.rs", "b.rs"] {
        std::fs::write(dir.path().join(file), "fn auth() {}\n").unwrap();
    }
    std::fs::write(dir.path().join("binary.rs"), [0, 1, 2]).unwrap();
    std::fs::write(dir.path().join("large.rs"), " ".repeat(100)).unwrap();
    std::fs::create_dir(dir.path().join("vendor")).unwrap();
    std::fs::write(dir.path().join("vendor/v.rs"), "fn auth() {}").unwrap();
    std::fs::create_dir_all(dir.path().join("one/two/three")).unwrap();
    std::fs::write(dir.path().join("one/two/three/deep.rs"), "fn auth() {}").unwrap();
    let limits = ScanLimits {
        max_file_bytes: 50,
        max_depth: 2,
        ..Default::default()
    };
    let discovery = discover_with_limits(dir.path(), &limits).unwrap();
    assert_eq!(discovery.paths.len(), 3);
    assert_eq!(discovery.stats.oversized_files, 1);
    assert_eq!(discovery.stats.excluded_directories, 1);
    assert_eq!(discovery.stats.depth_limited_directories, 1);
    assert!(
        discover_with_limits(
            dir.path(),
            &ScanLimits {
                max_source_files: 1,
                ..limits.clone()
            }
        )
        .is_err()
    );
    assert!(
        discover_with_limits(
            dir.path(),
            &ScanLimits {
                max_source_bytes: 10,
                ..limits.clone()
            }
        )
        .is_err()
    );
    let response = search(&SearchOptions {
        root: dir.path().into(),
        query: "auth".into(),
        scan_limits: limits,
        use_cache: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(response.stats.skipped_binary_or_unreadable_files, 1);
    assert_eq!(response.stats.files_indexed, 2);
}
#[test]
fn nested_ast_is_bounded_and_score_components_are_observable() {
    let dir = tempfile::tempdir().unwrap();
    let nested = format!(
        "{}fn auth() {{}}{}",
        "mod nested {".repeat(270),
        "}".repeat(270)
    );
    std::fs::write(dir.path().join("deep.rs"), nested).unwrap();
    assert!(
        SearchSession::open(dir.path(), false)
            .err()
            .unwrap()
            .to_string()
            .contains("complexity limit")
    );
    let session = SearchSession::open(&fixture(), false).unwrap();
    for result in session.query("request", 12000, 5).unwrap().results {
        assert!(
            (result.final_score
                - result.lexical_score
                - result.structural_score
                - result.diversity_score)
                .abs()
                < 1e-10
        );
    }
}

#[test]
fn cached_source_becoming_binary_does_not_reuse_stale_postings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.rs");
    std::fs::write(&path, "pub fn authenticate() {}\n").unwrap();
    let options = SearchOptions {
        root: dir.path().into(),
        query: "auth".into(),
        ..Default::default()
    };
    assert_eq!(search(&options).unwrap().results.len(), 1);
    std::fs::write(path, [0, 0, 0]).unwrap();
    let result = search(&options).unwrap();
    assert!(result.results.is_empty());
    assert!(!result.stats.index_reused);
    assert_eq!(result.stats.skipped_binary_or_unreadable_files, 1);
    assert!(
        SearchSession::open(dir.path(), true)
            .unwrap()
            .query("auth", 12000, 5)
            .unwrap()
            .results
            .is_empty()
    );
}
