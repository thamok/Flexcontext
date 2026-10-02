use flexcontext::{Detail, QueryOptions, RetrievalPolicy, SearchScope, SearchSession};
use std::collections::BTreeSet;

fn fixture(files: &[(&str, &str)]) -> (tempfile::TempDir, SearchSession) {
    let dir = tempfile::tempdir().unwrap();
    for (path, source) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    let session = SearchSession::open(dir.path(), false).unwrap();
    (dir, session)
}
fn opts(query: &str) -> QueryOptions {
    QueryOptions {
        query: query.into(),
        max_bytes: 16384,
        max_results: 40,
        policy: RetrievalPolicy::Focused,
        explain: true,
        ..Default::default()
    }
}
fn bytes(response: &flexcontext::SearchResponse) -> BTreeSet<(String, usize)> {
    response
        .results
        .iter()
        .flat_map(|r| {
            r.source_spans
                .iter()
                .flat_map(move |s| (s.start_byte..s.end_byte).map(move |b| (r.path.clone(), b)))
        })
        .collect()
}

#[test]
fn related_helpers_need_lexical_evidence_and_boost_is_capped() {
    let (_dir, session) = fixture(&[(
        "one.ts",
        "class Settings { value: number; }\nfunction internal() { return 1; }\nfunction authorization(input: Settings) { return internal(); }\nfunction authorizationRetry() { return authorization(null); }\n",
    )]);
    let mut options = opts("authorization");
    options.policy = RetrievalPolicy::Relations;
    let response = session.query_with_options(&options).unwrap();
    let human_trace = flexcontext::output::render_human(&response);
    assert!(human_trace.contains("retrieval trace:"));
    assert!(human_trace.contains("no independent lexical evidence"));
    assert!(response.results.iter().any(|r| r.symbol == "authorization"));
    assert!(
        !response
            .results
            .iter()
            .any(|r| r.symbol == "Settings" || r.symbol == "internal")
    );
    assert!(
        response
            .results
            .iter()
            .any(|r| r.relations.iter().any(|r| r.symbol == "internal"))
    );
    for trace in &response.trace {
        assert!(trace.relationship_boost <= trace.direct_score.max(0.0) * 0.2 + 1e-8);
    }
}

#[test]
fn complementary_functions_are_not_limited_to_three_per_file() {
    let code = (0..7)
        .map(|i| format!("function validatePart{i}() {{ return 'validate {i}'; }}\n"))
        .collect::<String>();
    let (_dir, session) = fixture(&[("one.ts", &code)]);
    let response = session.query_with_options(&opts("validate")).unwrap();
    assert_eq!(
        response
            .results
            .iter()
            .filter(|r| r.kind == "function")
            .count(),
        7
    );
}

#[test]
fn explicit_scope_is_a_boundary_and_exclusions_win() {
    let (_dir, session) = fixture(&[
        ("src/auth.ts", "function login() { return true; }"),
        ("src/auth/skip.ts", "function loginSkip() { return true; }"),
        (
            "src/auth2/other.ts",
            "function loginOther() { return true; }",
        ),
        ("tests/login.ts", "function loginTest() { return true; }"),
    ]);
    for policy in [RetrievalPolicy::Baseline, RetrievalPolicy::Focused] {
        let mut options = opts("login");
        options.policy = policy;
        options.scope.include_paths = vec!["src/auth".into(), "src/auth.ts".into()];
        options.scope.exclude_paths = vec!["src/auth/skip.ts".into()];
        let response = session.query_with_options(&options).unwrap();
        assert!(!response.results.is_empty());
        assert!(response.results.iter().all(
            |r| r.path == "src/auth.ts" && r.relations.iter().all(|r| r.path == "src/auth.ts")
        ));
        options.scope.exclude_paths.push("src/auth.ts".into());
        assert!(
            session
                .query_with_options(&options)
                .unwrap()
                .results
                .is_empty()
        );
    }
    for bad in [
        "../src",
        "/src",
        "src//auth",
        "src/../auth",
        "C:/src",
        "src\\auth",
        "",
    ] {
        let scope = SearchScope {
            include_paths: vec![bad.into()],
            ..Default::default()
        };
        assert!(scope.validate().is_err(), "{bad}");
    }
}

#[test]
fn auto_focus_keeps_strong_alternatives_and_tests_are_searchable() {
    let sources = (0..5)
        .map(|i| {
            (
                format!("module{i}.ts"),
                format!("function authorization{i}() {{ return 'authorization'; }}"),
            )
        })
        .collect::<Vec<_>>();
    let refs = sources
        .iter()
        .map(|(p, s)| (p.as_str(), s.as_str()))
        .collect::<Vec<_>>();
    let (_dir, session) = fixture(&refs);
    let response = session.query_with_options(&opts("authorization")).unwrap();
    assert_eq!(response.focused_files.len(), 3);
    let mut repository_scope = opts("authorization");
    repository_scope.scope.scope = flexcontext::ScopeMode::Repository;
    let repository_response = session.query_with_options(&repository_scope).unwrap();
    assert!(repository_response.focused_files.is_empty());
    assert_eq!(repository_response.results.len(), response.results.len());
    assert_eq!(
        response
            .results
            .iter()
            .map(|r| &r.path)
            .collect::<BTreeSet<_>>()
            .len(),
        5
    );
    let (_dir, session) = fixture(&[
        ("src/check.ts", "function validate() { return true; }"),
        (
            "tests/check.ts",
            "function validateTest() { return 'validate test fixture'; }",
        ),
    ]);
    let mut options = opts("validate test");
    assert!(
        session
            .query_with_options(&options)
            .unwrap()
            .results
            .iter()
            .any(|r| r.path.starts_with("tests/"))
    );
    options.scope.include_paths = vec!["tests".into()];
    options.query = "validate".into();
    assert!(
        session
            .query_with_options(&options)
            .unwrap()
            .results
            .iter()
            .all(|r| r.path.starts_with("tests/"))
    );
}

#[test]
fn path_only_and_unknown_queries_do_not_fill_the_budget() {
    let (_dir, session) = fixture(&[("authorization.ts", "function unrelated() { return 7; }")]);
    for query in ["authorization", "xyzzynotpresent"] {
        assert!(
            session
                .query_with_options(&opts(query))
                .unwrap()
                .results
                .is_empty()
        );
    }
}

#[test]
fn stable_budgets_preserve_source_bytes_guards_and_provenance() {
    let source = format!(
        "function authorize(enabled: boolean) {{\n  if (enabled) {{\n    if (canAccess()) {{\n      return grantAuthorization();\n    }}\n{}  }}\n  return denyAuthorization();\n}}\nfunction auditAuthorization() {{ return true; }}\n",
        (0..100)
            .map(|i| format!("    logEvent({i});\n"))
            .collect::<String>()
    );
    let (_dir, session) = fixture(&[("auth.ts", &source)]);
    let mut previous = BTreeSet::new();
    for budget in [256, 512, 768, 1024, 2048, 4096, 8192, 16384] {
        let mut options = opts("grant authorization");
        options.max_bytes = budget;
        let response = session.query_with_options(&options).unwrap();
        let current = bytes(&response);
        assert!(previous.is_subset(&current), "source lost at {budget}");
        assert!(response.stats.returned_bytes <= budget);
        for result in &response.results {
            for span in &result.source_spans {
                assert!(
                    result
                        .content
                        .contains(&source[span.start_byte..span.end_byte])
                );
            }
            if result.content.contains("return grantAuthorization()") {
                assert!(result.content.contains("if (enabled)"));
                assert!(result.content.contains("if (canAccess())"));
                assert!(result.content.contains("function authorize"));
            }
            if result.content_truncated {
                assert!(result.content.contains("[… omitted …]"));
            }
        }
        previous = current;
    }
    assert!(!previous.is_empty());
}

#[test]
fn compact_and_full_account_the_actual_wire_and_cli_mcp_agree() {
    let (dir, mut session) =
        fixture(&[("src/auth.ts", "function authenticate() { return '🔒'; }")]);
    for detail in [Detail::Compact, Detail::Full] {
        let mut options = opts("authenticate");
        options.explain = false;
        options.detail = detail;
        let response = session.query_with_options(&options).unwrap();
        for repr in [
            flexcontext::output::Representation::json(detail),
            flexcontext::output::Representation::human(detail),
            flexcontext::output::Representation::mcp(detail, serde_json::json!("🔒"), true),
            flexcontext::output::Representation::mcp(detail, serde_json::json!("🔒"), false),
        ] {
            for budget in [256, 512, 1024, 4096] {
                let mut response = response.clone();
                if flexcontext::output::finalize(&mut response, &repr, Some(budget)).is_ok() {
                    let wire = repr.render(&response).unwrap();
                    assert_eq!(response.context_cost.serialized_bytes, wire.len());
                    assert_eq!(
                        response.context_cost.estimated_tokens,
                        wire.len().div_ceil(4)
                    );
                    assert!(wire.len() <= budget * 4);
                }
            }
        }
    }
    let cli = std::process::Command::new(env!("CARGO_BIN_EXE_flexcontext"))
        .args([
            "search",
            dir.path().to_str().unwrap(),
            "authenticate",
            "--json",
            "--policy",
            "focused",
            "--include-paths",
            "src",
        ])
        .output()
        .unwrap();
    assert!(cli.status.success());
    let cli: serde_json::Value = serde_json::from_slice(&cli.stdout).unwrap();
    assert!(cli.get("stats").is_none());
    assert!(cli["results"][0].get("signals").is_none());
    let request = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}},"name":"code_search","arguments":{"query":"authenticate","policy":"focused","include_paths":["src"]}}});
    let mut wire = Vec::new();
    flexcontext::mcp::serve(&mut session, format!("{request}\n").as_bytes(), &mut wire).unwrap();
    let mcp: serde_json::Value = serde_json::from_slice(&wire).unwrap();
    assert_eq!(
        cli["results"],
        mcp["result"]["structuredContent"]["results"]
    );
    assert_eq!(cli["scope"], mcp["result"]["structuredContent"]["scope"]);
    assert_eq!(
        mcp["result"]["structuredContent"]["context_cost"]["serialized_bytes"],
        wire.len()
    );
}
