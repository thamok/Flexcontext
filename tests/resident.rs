use flexcontext::{QueryOptions, SearchOptions, SearchSession, search};
use serde_json::{Value, json};

#[test]
fn resident_scores_match_cli_and_current_queries_replace_changed_and_deleted_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.ts");
    std::fs::write(
        &path,
        "export function authenticateUser(token: string) { return token; }\n",
    )
    .unwrap();
    let mut session = SearchSession::open(dir.path(), true).unwrap();
    let resident = session.query("auth", 4096, 12).unwrap();
    let cli = search(&SearchOptions {
        root: dir.path().into(),
        query: "auth".into(),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(resident.results[0].score, cli.results[0].score);
    assert_eq!(resident.stats.cache_load_us, 0);
    assert_eq!(resident.stats.traversal_us, 0);
    assert_eq!(resident.stats.files_parsed, 0);
    std::fs::write(
        &path,
        "export function authorizationPolicy() { return false; }\n",
    )
    .unwrap();
    let query = QueryOptions {
        query: "auth".into(),
        ..Default::default()
    };
    assert_eq!(
        session.query_current(&query).unwrap().results[0].symbol,
        "authorizationPolicy"
    );
    std::fs::remove_file(path).unwrap();
    assert!(session.query_current(&query).unwrap().results.is_empty());
}

fn mcp_call(session: &mut SearchSession, name: &str, arguments: Value, modern: bool) -> Value {
    let mut request = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":arguments}});
    let input = if modern {
        request["params"]["_meta"] = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}});
        format!("{request}\n")
    } else {
        format!(
            "{}\n{}\n{request}\n",
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"resident-test","version":"1"}}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
    };
    let mut output = Vec::new();
    flexcontext::mcp::serve(session, input.as_bytes(), &mut output).unwrap();
    let response: Value = serde_json::from_str(
        std::str::from_utf8(&output)
            .unwrap()
            .lines()
            .last()
            .unwrap(),
    )
    .unwrap();
    assert!(response.get("error").is_none(), "{response}");
    response["result"].clone()
}

fn mcp_search(session: &mut SearchSession, modern: bool) -> Value {
    mcp_call(
        session,
        "code_search",
        json!({"query":"auth","detail":"full"}),
        modern,
    )
}

#[test]
fn mcp_automatically_tracks_edits_additions_renames_and_deletions() {
    for use_cache in [false, true] {
        for modern in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("auth.ts");
            std::fs::write(&path, "function authOld() { return 'old'; }").unwrap();
            let mut session = SearchSession::open(dir.path(), use_cache).unwrap();
            assert_eq!(
                mcp_search(&mut session, modern)["structuredContent"]["results"][0]["symbol"],
                "authOld"
            );

            // Same-length edits must also invalidate the snapshot.
            std::fs::write(&path, "function authNew() { return 'new'; }").unwrap();
            let changed = mcp_search(&mut session, modern);
            assert_eq!(changed["isError"], false);
            assert_eq!(
                changed["structuredContent"]["results"][0]["symbol"],
                "authNew"
            );
            assert!(
                changed["structuredContent"]["results"][0]["content"]
                    .as_str()
                    .unwrap()
                    .contains("'new'")
            );
            assert_eq!(changed["structuredContent"]["stats"]["files_reparsed"], 1);

            let nested = dir.path().join("nested");
            std::fs::create_dir(&nested).unwrap();
            let added = nested.join("created.ts");
            std::fs::write(&added, "function authCreated() { return true; }").unwrap();
            let created = mcp_search(&mut session, modern);
            assert_eq!(
                created["structuredContent"]["results"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
            assert_eq!(
                created["structuredContent"]["stats"]["files_reparsed"],
                if use_cache { 1 } else { 2 }
            );
            assert_eq!(
                created["structuredContent"]["stats"]["files_reused"],
                usize::from(use_cache)
            );

            std::fs::rename(&added, nested.join("renamed.ts")).unwrap();
            let renamed = mcp_search(&mut session, modern);
            let results = renamed["structuredContent"]["results"].as_array().unwrap();
            assert!(results.iter().any(|r| r["path"] == "nested/renamed.ts"));
            assert!(!results.iter().any(|r| r["path"] == "nested/created.ts"));

            std::fs::remove_file(&path).unwrap();
            std::fs::remove_dir_all(&nested).unwrap();
            let deleted = mcp_search(&mut session, modern);
            assert_eq!(deleted["isError"], false);
            assert!(
                deleted["structuredContent"]["results"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(deleted["structuredContent"]["stats"]["files_indexed"], 0);
            assert!(
                !deleted["structuredContent"]["stats"]["index_reused"]
                    .as_bool()
                    .unwrap()
            );
        }
    }
}

#[test]
fn unchanged_source_reuses_memory_without_loading_or_writing_the_cache() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("auth.ts"),
        "function auth() { return true; }",
    )
    .unwrap();
    // Keep skipped files in the metadata inventory too, so they do not cause repeated refreshes.
    std::fs::write(dir.path().join("binary.ts"), [0, 0, 0]).unwrap();
    let mut session = SearchSession::open(dir.path(), true).unwrap();
    let source = session.symbols()[0].source.clone();
    std::fs::remove_dir_all(dir.path().join(".flexcontext")).unwrap();
    std::fs::create_dir(dir.path().join("node_modules")).unwrap();
    std::fs::write(
        dir.path().join("node_modules/ignored.ts"),
        "function authIgnored() {}",
    )
    .unwrap();
    std::fs::write(dir.path().join("notes.txt"), "not indexed").unwrap();
    let query = QueryOptions {
        query: "auth".into(),
        ..Default::default()
    };
    for _ in 0..3 {
        let response = session.query_current(&query).unwrap();
        assert_eq!(response.results.len(), 1);
        assert_eq!(response.stats.files_parsed, 0);
        assert_eq!(response.stats.files_reparsed, 0);
        assert_eq!(response.stats.cache_load_us, 0);
        assert_eq!(response.stats.cache_write_us, 0);
        assert_eq!(response.stats.index_us, 0);
        assert!(response.stats.index_reused);
        assert_eq!(
            response.context_cost.serialized_bytes,
            flexcontext::output::Representation::json(query.detail)
                .render(&response)
                .unwrap()
                .len()
        );
        assert!(std::sync::Arc::ptr_eq(
            &source,
            &session.symbols()[0].source
        ));
        assert!(!dir.path().join(".flexcontext").exists());
    }
}

#[test]
fn automatic_checks_honor_ignore_changes_and_file_size_limits() {
    use flexcontext::repository::ScanLimits;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.ts");
    std::fs::write(&path, "function auth() {}").unwrap();
    let mut session = SearchSession::open_with_limits(
        dir.path(),
        false,
        &ScanLimits {
            max_file_bytes: 64,
            ..Default::default()
        },
    )
    .unwrap();
    let query = QueryOptions {
        query: "auth".into(),
        ..Default::default()
    };
    std::fs::write(dir.path().join(".gitignore"), "auth.ts\n").unwrap();
    assert!(session.query_current(&query).unwrap().results.is_empty());
    std::fs::remove_file(dir.path().join(".gitignore")).unwrap();
    assert_eq!(session.query_current(&query).unwrap().results.len(), 1);
    std::fs::write(&path, format!("function auth() {{}}{}", " ".repeat(64))).unwrap();
    assert!(session.query_current(&query).unwrap().results.is_empty());
    std::fs::write(&path, "function auth() {}").unwrap();
    assert_eq!(session.query_current(&query).unwrap().results.len(), 1);
    std::fs::write(&path, [0, 0, 0]).unwrap();
    assert!(session.query_current(&query).unwrap().results.is_empty());
    std::fs::write(&path, "function auth() {}").unwrap();
    assert_eq!(session.query_current(&query).unwrap().results.len(), 1);
}

#[test]
fn failed_automatic_and_explicit_refreshes_preserve_snapshot_and_retry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.ts");
    std::fs::write(&path, "function authOld() {}").unwrap();
    let mut session = SearchSession::open(dir.path(), true).unwrap();
    let source = session.symbols()[0].source.clone();
    let cache = dir.path().join(".flexcontext");
    std::fs::remove_dir_all(&cache).unwrap();
    std::fs::write(&cache, "blocks cache creation").unwrap();
    std::fs::write(&path, "function authChanged() {}").unwrap();
    for _ in 0..2 {
        let response = mcp_search(&mut session, true);
        assert_eq!(response["isError"], true);
        assert!(response.get("structuredContent").is_none());
        assert!(
            response["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("cannot refresh repository snapshot")
        );
        assert!(std::sync::Arc::ptr_eq(
            &source,
            &session.symbols()[0].source
        ));
    }
    let explicit = mcp_call(&mut session, "refresh_index", json!({}), true);
    assert_eq!(explicit["isError"], true);
    assert!(std::sync::Arc::ptr_eq(
        &source,
        &session.symbols()[0].source
    ));
    std::fs::remove_file(&cache).unwrap();
    let recovered = mcp_search(&mut session, true);
    assert_eq!(recovered["isError"], false);
    assert_eq!(
        recovered["structuredContent"]["results"][0]["symbol"],
        "authChanged"
    );
    assert_eq!(
        mcp_call(&mut session, "refresh_index", json!({}), true)["isError"],
        false
    );
}

#[test]
fn automatic_scan_failures_do_not_serve_stale_source_or_poison_recovery() {
    use flexcontext::repository::ScanLimits;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("auth.ts"), "function auth() {}").unwrap();
    let mut session = SearchSession::open_with_limits(
        dir.path(),
        false,
        &ScanLimits {
            max_source_files: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let added = dir.path().join("added.ts");
    std::fs::write(&added, "function authAdded() {}").unwrap();
    let failed = mcp_search(&mut session, false);
    assert_eq!(failed["isError"], true);
    assert!(failed.get("structuredContent").is_none());
    assert!(
        failed["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("repository limit exceeded")
    );
    assert_eq!(session.symbols().len(), 1);
    std::fs::remove_file(added).unwrap();
    assert_eq!(mcp_search(&mut session, false)["isError"], false);
}

#[test]
fn parser_failure_during_refresh_is_atomic_and_recovers_on_next_search() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.rs");
    std::fs::write(&path, "fn authOld() {}").unwrap();
    let mut session = SearchSession::open(dir.path(), true).unwrap();
    let nested = format!(
        "{}fn authNew() {{}}{}",
        "mod nested {".repeat(270),
        "}".repeat(270)
    );
    std::fs::write(&path, nested).unwrap();
    let failed = mcp_search(&mut session, true);
    assert_eq!(failed["isError"], true);
    assert!(failed.get("structuredContent").is_none());
    assert!(
        failed["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("complexity limit")
    );
    assert_eq!(
        session.query("auth", 4096, 12).unwrap().results[0].symbol,
        "authOld"
    );
    std::fs::write(&path, "fn authNew() {}").unwrap();
    assert_eq!(
        mcp_search(&mut session, true)["structuredContent"]["results"][0]["symbol"],
        "authNew"
    );
}

#[cfg(unix)]
#[test]
fn automatic_checks_detect_atomic_replacement_with_same_size_and_mtime() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.ts");
    std::fs::write(&path, "function authOld() {}").unwrap();
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let mut session = SearchSession::open(dir.path(), true).unwrap();
    let replacement = dir.path().join("replacement");
    std::fs::write(&replacement, "function authNew() {}").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&replacement)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    std::fs::rename(replacement, path).unwrap();
    assert_eq!(
        mcp_search(&mut session, true)["structuredContent"]["results"][0]["symbol"],
        "authNew"
    );
}

#[test]
fn mcp_expansion_checks_changes_before_resolving_old_references() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.ts");
    let source = "class Auth {\n authOne() { return 1; }\n authTwo() { return 2; }\n authThree() { return 3; }\n authFour() { return 4; }\n}";
    std::fs::write(&path, source).unwrap();
    let mut session = SearchSession::open(dir.path(), true).unwrap();
    let response = mcp_call(
        &mut session,
        "code_search",
        json!({"query":"Auth","continuations":true}),
        true,
    );
    let reference = response["structuredContent"]["navigation"]["leads"][0]["reference"].clone();
    assert!(reference.is_string(), "{response}");
    assert_eq!(
        mcp_call(
            &mut session,
            "expand_context",
            json!({"reference":reference}),
            true
        )["isError"],
        false
    );
    std::fs::write(&path, source.replace("return 4", "return 5")).unwrap();
    let stale = mcp_call(
        &mut session,
        "expand_context",
        json!({"reference":reference}),
        true,
    );
    assert_eq!(stale["isError"], true);
    assert!(stale.get("structuredContent").is_none());
    let response = mcp_call(
        &mut session,
        "code_search",
        json!({"query":"Auth","continuations":true}),
        true,
    );
    let fresh = response["structuredContent"]["navigation"]["leads"][0]["reference"].clone();
    assert_eq!(
        mcp_call(
            &mut session,
            "expand_context",
            json!({"reference":fresh}),
            true
        )["isError"],
        false
    );
}

#[test]
fn stdio_lifecycle_errors_and_repeated_queries() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("auth.ts"),
        "function auth() { return true; }",
    )
    .unwrap();
    let mut session = SearchSession::open(dir.path(), false).unwrap();
    let mut requests = vec![
        json!({"jsonrpc":"2.0","id":0,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":1,"method":"server/discover"}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"code_search","arguments":{"query":"auth","budget":128,"detail":"full"}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"code_search","arguments":{"query":"auth","budget":128,"detail":"full"}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"code_search","arguments":{"query":"auth","budget":0}}}),
        json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"refresh_index","arguments":{}}}),
        json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"code_search","arguments":{"query":"!!!"}}}),
    ];
    for request in &mut requests {
        if request["id"].is_null() {
            continue;
        }
        if !request["params"].is_object() {
            request["params"] = json!({});
        }
        request["params"]["_meta"] = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}});
    }
    let input = requests
        .iter()
        .map(|r| serde_json::to_string(r).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n{bad json\n";
    let mut output = Vec::new();
    flexcontext::mcp::serve(&mut session, input.as_bytes(), &mut output).unwrap();
    let responses: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 9);
    assert_eq!(responses[0]["result"]["resultType"], "complete");
    assert_eq!(
        responses[1]["result"]["supportedVersions"],
        json!(["2026-07-28", "2025-06-18"])
    );
    assert_eq!(responses[2]["result"]["tools"].as_array().unwrap().len(), 3);
    for response in &responses[3..5] {
        assert_eq!(
            response["result"]["structuredContent"]["results"][0]["symbol"],
            "auth"
        );
        assert_eq!(
            response["result"]["structuredContent"]["stats"]["cache_load_us"],
            0
        );
    }
    assert_eq!(responses[5]["error"]["code"], -32602);
    assert_eq!(responses[6]["result"]["isError"], false);
    assert_eq!(responses[7]["result"]["isError"], true);
    assert_eq!(responses[8]["error"]["code"], -32700);
}

#[test]
fn graph_does_not_resolve_loader_or_external_types_to_unrelated_locals() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("auth.ts"), "import type { Document } from 'mongoose';\nexport interface IPluginAuth extends Document { auth: string }\nexport function findAuth(): IPluginAuth { return require('./auth'); }\n").unwrap();
    std::fs::write(
        dir.path().join("vite.ts"),
        "const require = makeRequire();\nfunction test() { type Document = string; }\n",
    )
    .unwrap();
    let response = search(&SearchOptions {
        root: dir.path().into(),
        query: "auth".into(),
        use_cache: false,
        ..Default::default()
    })
    .unwrap();
    let relations: Vec<_> = response.results.iter().flat_map(|r| &r.relations).collect();
    assert!(
        !relations
            .iter()
            .any(|r| r.kind == "calls" && r.symbol == "require")
    );
    assert!(!relations.iter().any(|r| r.symbol == "type"));
    assert!(
        !relations
            .iter()
            .any(|r| r.kind == "type_reference" && r.path == "vite.ts")
    );
    assert!(
        relations
            .iter()
            .any(|r| r.kind == "file_import" && r.symbol == "Document")
    );
    assert!(
        relations
            .iter()
            .any(|r| r.kind == "type_reference" && r.symbol == "IPluginAuth")
    );
}

#[test]
fn slices_late_relevant_branch_with_valid_utf8_ranges_and_budget() {
    let dir = tempfile::tempdir().unwrap();
    let mut source = "// Größer\nfunction authenticateUser(token: string) {\n".to_owned();
    for i in 0..100 {
        source.push_str(&format!("  const padding{i} = 'irrelevant';\n"));
    }
    source.push_str("  if (!token) { throw new Error('auth denied 🔒'); }\n  return token;\n}\n");
    std::fs::write(dir.path().join("auth.ts"), &source).unwrap();
    let session = SearchSession::open(dir.path(), false).unwrap();
    let whole = session.query("auth", 32768, 12).unwrap();
    assert!(
        !whole
            .results
            .iter()
            .find(|r| r.symbol == "authenticateUser")
            .unwrap()
            .content_truncated
    );
    for budget in [512, 2048, 4096, 8192] {
        let response = session.query("auth", budget, 12).unwrap();
        assert!(response.stats.returned_bytes <= budget);
        assert!(response.stats.approximate_tokens <= budget / 4);
        let function = response
            .results
            .iter()
            .find(|r| r.symbol == "authenticateUser")
            .unwrap();
        assert!(function.content_truncated);
        if budget >= 2048 {
            assert!(function.content.contains("auth denied 🔒"));
        }
        for span in &function.source_spans {
            assert!(
                function
                    .content
                    .contains(&source[span.start_byte..span.end_byte])
            );
            assert_eq!(
                span.start_line,
                source[..span.start_byte]
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count()
                    + 1
            );
        }
    }
}

#[test]
fn field_postings_preserve_all_scores_for_composite_morphological_and_long_queries() {
    use flexcontext::{
        cache::load_indexed_repository,
        lexical::Query,
        ranking::{PreparedIndex, rank_indexed_candidates, rank_prepared_candidates},
        repository::discover_source_paths,
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini");
    let (paths, _) = discover_source_paths(&root).unwrap();
    let repository = load_indexed_repository(&root, &paths, false).unwrap();
    let prepared = PreparedIndex::build(&repository.symbols);
    for input in [
        "auth",
        "authentication",
        "validate token",
        "UserSession",
        "parseRequest",
        "configuration",
        "auth auth",
        "🔒",
        &"auth ".repeat(65),
    ] {
        let query = Query::parse(input);
        let reference = rank_indexed_candidates(&repository.symbols, &query, &repository.index);
        let indexed =
            rank_prepared_candidates(&repository.symbols, &query, &repository.index, &prepared);
        assert_eq!(reference.len(), indexed.len(), "{input}");
        for (left, right) in reference.iter().zip(&indexed) {
            assert_eq!(left.symbol_id, right.symbol_id, "{input}");
            assert_eq!(
                serde_json::to_value(&left.signals).unwrap(),
                serde_json::to_value(&right.signals).unwrap(),
                "{input}"
            );
        }
    }
}

#[test]
fn soft_diversity_retains_anchor_and_promotes_another_path_and_kind() {
    use flexcontext::{
        model::{Language, ScoreSignals, ScoredSymbol, SourceFile},
        parser::SymbolExtractor,
        relations::RelationGraph,
        selection::select_context,
    };
    let file = |path: &str, source: &str| SourceFile {
        absolute_path: path.into(),
        relative_path: path.into(),
        language: Language::TypeScript,
        source: source.into(),
    };
    let mut extractor = SymbolExtractor::new();
    let mut id = 0;
    let mut symbols = extractor
        .extract(
            &file(
                "one.ts",
                "function auth() {}\nfunction authTwo() {}\nfunction authThree() {}",
            ),
            &mut id,
        )
        .unwrap();
    symbols.extend(
        extractor
            .extract(
                &file("two.ts", "interface AuthConfig { token: string }"),
                &mut id,
            )
            .unwrap(),
    );
    let ranked: Vec<_> = symbols
        .iter()
        .enumerate()
        .map(|(i, symbol)| ScoredSymbol {
            symbol_id: symbol.id,
            score: 30.0 - i as f64 * 0.5,
            signals: ScoreSignals::default(),
        })
        .collect();
    let results = select_context(&ranked, &symbols, &RelationGraph::default(), 4096, 3);
    assert_eq!(results[0].symbol, "auth");
    assert_eq!(results[1].symbol, "AuthConfig");
}
