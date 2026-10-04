use flexcontext::output::{Representation, compact_value, finalize};
use flexcontext::{QueryOptions, SearchSession};

const LIFECYCLE: &str = "class Session {\n  loginSession() { return issueToken(); }\n  refreshSession() { return rotateToken(); }\n  revokeSession() { return deleteToken(); }\n  expireSession() { return clearExpired(); }\n}\n";
fn fixture() -> (tempfile::TempDir, SearchSession) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("session.ts"), LIFECYCLE).unwrap();
    let session = SearchSession::open(dir.path(), false).unwrap();
    (dir, session)
}
#[test]
fn two_bodies_advertise_and_fetch_omitted_without_quota() {
    let (_dir, session) = fixture();
    let mut options = QueryOptions {
        query: "Session".into(),
        explain: true,
        ..Default::default()
    };
    let before = session.query_with_options(&options).unwrap();
    assert_eq!(
        before.results.iter().filter(|r| r.kind == "method").count(),
        2
    );
    assert!(
        before
            .trace
            .iter()
            .any(|t| t.decision == "file/container quota")
    );
    assert!(compact_value(&before).get("navigation").is_none());
    options.continuations = true;
    let mut after = session.query_with_options(&options).unwrap();
    finalize(&mut after, &Representation::CompactJson, Some(3000)).unwrap();
    let nav = after.navigation.as_ref().unwrap();
    let leads: Vec<_> = nav.leads.iter().filter(|l| l.kind == "method").collect();
    assert_eq!(leads.len(), 2);
    for lead in leads {
        let expanded = session.expand(&lead.reference, 4096).unwrap();
        assert_eq!(expanded.results.len(), 1);
        assert_eq!(expanded.results[0].symbol, lead.symbol);
        assert!(!expanded.results[0].content_truncated);
        assert!(!after.results.iter().any(|r| r.symbol == lead.symbol));
    }
}

fn progressive(query: &str) -> QueryOptions {
    QueryOptions {
        query: query.into(),
        continuations: true,
        ..Default::default()
    }
}
#[test]
fn pages_retries_independent_queries_and_retained_cap() {
    let dir = tempfile::tempdir().unwrap();
    let source = (0..80)
        .map(|i| format!("  work{i}() {{ return event{i}(); }}\n"))
        .collect::<String>();
    std::fs::write(
        dir.path().join("jobs.ts"),
        format!("class Jobs {{\n{source}}}"),
    )
    .unwrap();
    let session = SearchSession::open(dir.path(), false).unwrap();
    let first = session.query_with_options(&progressive("work")).unwrap();
    let nav = first.navigation.as_ref().unwrap();
    assert_eq!(nav.leads.len(), 4);
    assert_eq!(nav.retained_candidates, 64);
    assert!(nav.outside_retained > 0);
    let mut current = nav.clone();
    let mut refs = std::collections::BTreeSet::new();
    let mut symbols = std::collections::BTreeSet::new();
    loop {
        for lead in &current.leads {
            assert!(symbols.insert(lead.symbol.clone()));
        }
        let Some(next) = current.next else {
            break;
        };
        assert!(refs.insert(next.clone()));
        let page = session.expand(&next, 4096).unwrap();
        assert!(page.results.is_empty());
        assert_eq!(
            compact_value(&page),
            compact_value(&session.expand(&next, 4096).unwrap())
        );
        assert!(page.navigation.as_ref().unwrap().remaining < current.remaining);
        current = page.navigation.unwrap();
    }
    let second = session.query_with_options(&progressive("work")).unwrap();
    assert_eq!(
        serde_json::to_value(first.navigation).unwrap(),
        serde_json::to_value(second.navigation).unwrap()
    );
}
#[test]
fn stale_root_scope_tampering_and_restart() {
    let (dir, mut session) = fixture();
    let mut options = progressive("Session");
    options.scope.include_paths = vec!["session.ts".into()];
    let r = session.query_with_options(&options).unwrap();
    let reference = r
        .navigation
        .unwrap()
        .leads
        .into_iter()
        .find(|l| l.kind == "method")
        .unwrap()
        .reference;
    assert!(
        SearchSession::open(dir.path(), false)
            .unwrap()
            .expand(&reference, 4096)
            .is_ok()
    );
    let (_other_dir, other) = fixture();
    assert!(other.expand(&reference, 4096).is_err());
    let mut bad = reference.clone().into_bytes();
    bad[10] = if bad[10] == b'A' { b'B' } else { b'A' };
    assert!(
        session
            .expand(std::str::from_utf8(&bad).unwrap(), 4096)
            .is_err()
    );
    std::fs::write(
        dir.path().join("session.ts"),
        LIFECYCLE.replace("deleteToken", "destroyToken"),
    )
    .unwrap();
    // Immutable resident snapshots still resolve old source, never a different body.
    assert!(session.expand(&reference, 4096).is_ok());
    session.refresh().unwrap();
    assert!(session.expand(&reference, 4096).is_err());
}
#[test]
fn partial_source_ranges_progress_without_replaying_delivered_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let source = format!(
        "function operate() {{\n{}\n}}",
        (0..60)
            .map(|i| format!("  log('é{i}');\n"))
            .collect::<String>()
    );
    std::fs::write(dir.path().join("one.ts"), &source).unwrap();
    let session = SearchSession::open(dir.path(), false).unwrap();
    let mut options = progressive("operate");
    options.max_bytes = 256;
    let first = session.query_with_options(&options).unwrap();
    assert!(first.results[0].content_truncated);
    let mut seen = std::collections::BTreeSet::new();
    for span in &first.results[0].source_spans {
        seen.extend(span.start_byte..span.end_byte);
    }
    let mut lead = first
        .navigation
        .unwrap()
        .leads
        .into_iter()
        .find(|l| l.symbol == "operate")
        .unwrap();
    for _ in 0..100 {
        let fetched = session.expand(&lead.reference, 80).unwrap();
        assert!(fetched.results[0].content.len() <= 80);
        for span in &fetched.results[0].source_spans {
            assert!(
                fetched.results[0]
                    .content
                    .contains(&source[span.start_byte..span.end_byte])
            );
            for byte in span.start_byte..span.end_byte {
                assert!(seen.insert(byte), "repeated byte {byte}");
            }
        }
        let next = fetched.navigation.unwrap().leads.into_iter().next();
        let Some(next) = next else {
            break;
        };
        lead = next;
    }
    // Trailing punctuation/whitespace alone need not produce another lead.
    assert!(
        source
            .bytes()
            .enumerate()
            .all(|(i, b)| !b.is_ascii_alphanumeric() || seen.contains(&i))
    );
}
#[test]
fn late_pruning_and_all_representations_charge_navigation() {
    let (_dir, session) = fixture();
    let response = session.query_with_options(&progressive("Session")).unwrap();
    let mut displaced = false;
    for repr in [
        Representation::CompactJson,
        Representation::CompactHuman,
        Representation::Json,
        Representation::Human,
        Representation::mcp(flexcontext::Detail::Compact, serde_json::json!("é"), true),
        Representation::mcp(flexcontext::Detail::Compact, serde_json::json!("é"), false),
        Representation::mcp(flexcontext::Detail::Full, serde_json::json!("é"), true),
        Representation::mcp(flexcontext::Detail::Full, serde_json::json!("é"), false),
    ] {
        for budget in [1, 600, 900, 1200, 1600, 2000, 3000, 6000] {
            let mut r = response.clone();
            if finalize(&mut r, &repr, Some(budget)).is_ok() {
                let wire = repr.render(&r).unwrap();
                assert_eq!(r.context_cost.serialized_bytes, wire.len());
                assert!(wire.len() <= budget * 4);
                let nav = r.navigation.as_ref().unwrap();
                assert!(nav.leads.len() <= 4);
                assert!(wire.windows(10).any(|w| w == b"navigation"));
                if nav.displaced_source_bytes > 0 {
                    displaced = true;
                    assert!(
                        nav.leads
                            .iter()
                            .any(|l| l.reason == "serialized payload budget")
                    );
                    for lead in &nav.leads {
                        assert!(session.expand(&lead.reference, 4096).is_ok());
                    }
                }
            }
        }
    }
    assert!(displaced);
}
#[test]
fn duplicate_source_and_partial_overlap_are_not_confused() {
    let (dir, _) = fixture();
    std::fs::write(dir.path().join("copy.ts"), LIFECYCLE).unwrap();
    let session = SearchSession::open(dir.path(), false).unwrap();
    let mut r = session.query_with_options(&progressive("Session")).unwrap();
    let delivered: Vec<_> = r
        .results
        .iter()
        .filter(|r| !r.content_truncated)
        .map(|r| r.content.clone())
        .collect();
    for lead in &r.navigation.as_ref().unwrap().leads {
        let expanded = session.expand(&lead.reference, 4096).unwrap();
        assert!(!delivered.contains(&expanded.results[0].content));
    }
    let (_single_dir, single) = fixture();
    r = single.query_with_options(&progressive("Session")).unwrap();
    // Simulate only a prefix delivered: overlap must not erase the rest of that method.
    let method = r.results.iter_mut().find(|r| r.kind == "method").unwrap();
    let name = method.symbol.clone();
    method.content.truncate(5);
    method.content_bytes = 5;
    method.content_truncated = true;
    method.source_spans[0].end_byte = method.source_spans[0].start_byte + 5;
    finalize(&mut r, &Representation::CompactJson, None).unwrap();
    assert!(
        r.navigation
            .unwrap()
            .leads
            .iter()
            .any(|l| l.symbol == name && l.reason == "partial source")
    );
}
#[test]
fn hints_optional_and_never_filter_unknown_methods() {
    let (_dir, session) = fixture();
    let a = session.query_with_options(&progressive("Session")).unwrap();
    let mut options = progressive("Session");
    options.role_hints = true;
    let b = session.query_with_options(&options).unwrap();
    let a = a.navigation.unwrap();
    let b = b.navigation.unwrap();
    assert!(a.leads.iter().all(|l| l.role_hints.is_empty()));
    assert_eq!(
        a.leads.iter().map(|l| &l.symbol).collect::<Vec<_>>(),
        b.leads.iter().map(|l| &l.symbol).collect::<Vec<_>>()
    );
    assert!(b.leads.iter().any(|l| !l.role_hints.is_empty()));
}

#[test]
fn punctuation_and_unicode_tails_remain_fetchable() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("s.ts"), "class Session {\naSession() { return \"💣\"; }\nbSession() { return \"💣\"; }\ncSession() { return \"💣\"; }\ndSession() { return \"💣\"; }\n}").unwrap();
    let session = SearchSession::open(dir.path(), false).unwrap();
    let initial = session.query_with_options(&progressive("Session")).unwrap();
    let lead = initial
        .navigation
        .unwrap()
        .leads
        .into_iter()
        .find(|l| l.kind == "method")
        .unwrap();
    let first = session.expand(&lead.reference, 21).unwrap();
    assert!(first.results[0].content.ends_with('"'));
    assert!(first.results[0].content_truncated);
    let next = &first.navigation.as_ref().unwrap().leads[0];
    let tail = session.expand(&next.reference, 80).unwrap();
    assert!(tail.results[0].content.contains("💣"));
    assert!(tail.navigation.unwrap().leads.is_empty());
    let mut tiny = session.expand(&lead.reference, 4096).unwrap();
    assert!(finalize(&mut tiny, &Representation::CompactJson, Some(1)).is_err());
}
#[test]
fn oversized_scope_fails_before_advertising_unresolvable_reference() {
    let (_dir, session) = fixture();
    let mut options = progressive("Session");
    options.scope.exclude_paths = (0..400)
        .map(|i| format!("unused{i}{}", "x".repeat(100)))
        .collect();
    let error = session.query_with_options(&options).unwrap_err();
    assert!(error.to_string().contains("reference size limit"));
}
#[test]
fn authenticated_scope_and_key_expiration() {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let (dir, session) = fixture();
    let mut options = progressive("Session");
    options.scope.include_paths = vec!["session.ts".into()];
    let response = session.query_with_options(&options).unwrap();
    let reference = &response.navigation.unwrap().leads[0].reference;
    let parts: Vec<_> = reference.splitn(3, '.').collect();
    let mut payload: serde_json::Value = serde_json::from_str(parts[2]).unwrap();
    payload[2][1] = serde_json::json!([]);
    let payload = serde_json::to_string(&payload).unwrap();
    let forged = format!("fc1.{}.{}", parts[1], payload);
    assert!(session.expand(&forged, 4096).is_err());
    let forged = format!(
        "fc1.{}.{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(payload.as_bytes())),
        payload
    );
    assert!(session.expand(&forged, 4096).is_err());
    std::fs::remove_file(dir.path().join(".flexcontext/progressive-key-v1")).unwrap();
    assert!(session.expand(reference, 4096).is_err());
    let new = session.query_with_options(&options).unwrap();
    assert!(
        session
            .expand(&new.navigation.unwrap().leads[0].reference, 4096)
            .is_ok()
    );
    assert!(session.expand(reference, 4096).is_err());
}
#[test]
fn concurrent_independent_queries_share_signer_without_seen_state() {
    let (dir, _) = fixture();
    let references: Vec<_> = std::thread::scope(|scope| {
        (0..8)
            .map(|_| {
                scope.spawn(|| {
                    let session = SearchSession::open(dir.path(), false).unwrap();
                    session
                        .query_with_options(&progressive("Session"))
                        .unwrap()
                        .navigation
                        .unwrap()
                        .leads[0]
                        .reference
                        .clone()
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|t| t.join().unwrap())
            .collect()
    });
    assert!(references.iter().all(|r| r == &references[0]));
    let session = SearchSession::open(dir.path(), false).unwrap();
    assert!(references.iter().all(|r| session.expand(r, 4096).is_ok()));
}
#[test]
fn cli_mcp_expansion_agree_in_both_protocols_and_details() {
    let (dir, mut session) = fixture();
    let cli = std::process::Command::new(env!("CARGO_BIN_EXE_flexcontext"))
        .args([
            "search",
            dir.path().to_str().unwrap(),
            "Session",
            "--continuations",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(cli.status.success());
    let initial: serde_json::Value = serde_json::from_slice(&cli.stdout).unwrap();
    let reference = initial["navigation"]["leads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["kind"] == "method")
        .unwrap()["reference"]
        .as_str()
        .unwrap();
    let cli = std::process::Command::new(env!("CARGO_BIN_EXE_flexcontext"))
        .args(["expand", dir.path().to_str().unwrap(), reference, "--json"])
        .output()
        .unwrap();
    assert!(cli.status.success());
    let cli: serde_json::Value = serde_json::from_slice(&cli.stdout).unwrap();
    for modern in [true, false] {
        for detail in ["compact", "full"] {
            let mut requests = Vec::new();
            if !modern {
                requests.push(serde_json::json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}));
                requests.push(
                    serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
                );
            }
            let mut params = serde_json::json!({"name":"expand_context","arguments":{"reference":reference,"max_tokens":4000,"detail":detail}});
            if modern {
                params["_meta"] = serde_json::json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}});
            }
            requests.push(
                serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":params}),
            );
            let input = requests
                .iter()
                .map(|r| format!("{r}\n"))
                .collect::<String>();
            let mut output = Vec::new();
            flexcontext::mcp::serve(&mut session, input.as_bytes(), &mut output).unwrap();
            let line = std::str::from_utf8(&output)
                .unwrap()
                .lines()
                .last()
                .unwrap();
            let mcp: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(mcp["result"]["isError"], false, "{mcp}");
            let body = &mcp["result"]["structuredContent"];
            assert_eq!(body["results"][0]["content"], cli["results"][0]["content"]);
            assert_eq!(body["scope"], cli["scope"]);
            assert_eq!(body["navigation"], cli["navigation"]);
            assert_eq!(body["context_cost"]["serialized_bytes"], line.len() + 1);
        }
    }
}

#[test]
fn copies_inside_delivered_parents_or_partial_copies_are_not_new_source() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.ts"),
        "function outerOperate() { function operate() { return evidence(); } return operate(); }",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.ts"),
        "function operate() { return evidence(); }",
    )
    .unwrap();
    let session = SearchSession::open(dir.path(), false).unwrap();
    let mut options = progressive("outerOperate");
    options.max_results = 1;
    let r = session.query_with_options(&options).unwrap();
    assert!(r.results[0].content.contains("function operate()"));
    assert!(
        !r.navigation
            .unwrap()
            .leads
            .iter()
            .any(|l| l.path == "b.ts" && l.symbol == "operate")
    );
    let source = format!(
        "function operate() {{\n{}\n}}",
        (0..60)
            .map(|i| format!("  log('e{i}');\n"))
            .collect::<String>()
    );
    std::fs::write(dir.path().join("a.ts"), &source).unwrap();
    std::fs::write(dir.path().join("b.ts"), &source).unwrap();
    let session = SearchSession::open(dir.path(), false).unwrap();
    options.query = "operate".into();
    options.max_bytes = 256;
    let r = session.query_with_options(&options).unwrap();
    let first = &r.results[0];
    let lead = r
        .navigation
        .as_ref()
        .unwrap()
        .leads
        .iter()
        .find(|l| l.path != first.path)
        .unwrap();
    let fetched = session.expand(&lead.reference, 4096).unwrap();
    assert!(fetched.results[0].content_truncated);
    for span in &fetched.results[0].source_spans {
        assert!(first.source_spans.iter().all(|prior| prior.end_byte <= span.start_byte || span.end_byte <= prior.start_byte));
    }
}
