use std::path::PathBuf;

use flexcontext::{SearchOptions, search};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini")
}

#[test]
fn graded_corpus_does_not_regress_from_recorded_baseline() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let report = flexcontext::evaluation::evaluate(&root.join("benchmarks"), 5, 12000).unwrap();
    let baseline: flexcontext::evaluation::Report =
        serde_json::from_str(include_str!("../benchmarks/results/baseline.json")).unwrap();
    assert_eq!(report.summary.queries, 60);
    for split in ["dev", "test"] {
        let current = &report.by_split[split];
        let previous = &baseline.by_split[split];
        assert!(
            current.recall_at_k >= previous.recall_at_k,
            "{split}: recall regressed: {current:?}"
        );
        assert!(
            current.mrr + 0.01 >= previous.mrr,
            "{split}: MRR regressed: {current:?}"
        );
        assert!(
            current.ndcg_at_k + 0.01 >= previous.ndcg_at_k,
            "{split}: nDCG regressed: {current:?}"
        );
        assert!(
            current.relationship_recall >= previous.relationship_recall,
            "{split}: relationships regressed"
        );
    }
}

#[test]
fn context_budget_is_never_exceeded() {
    let response = search(&SearchOptions {
        root: fixture(),
        query: "authentication".to_owned(),
        max_bytes: 500,
        max_results: 20,
        use_cache: false,
        ..Default::default()
    })
    .unwrap();
    assert!(response.stats.returned_bytes <= 500);
    assert_eq!(
        response.stats.returned_bytes,
        response
            .results
            .iter()
            .map(|result| result.content.len())
            .sum::<usize>()
    );
    assert_eq!(
        response.stats.human_payload_bytes,
        flexcontext::output::render_human(&response).len()
    );
    assert_eq!(
        response.stats.json_payload_bytes,
        serde_json::to_vec_pretty(&response).unwrap().len() + 1
    );
}

#[test]
fn persistent_cache_reuses_unchanged_files_and_reparses_only_changes() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("auth.rs");
    std::fs::write(&source, "pub fn authenticate_user() {}\n").unwrap();
    std::fs::write(
        temporary.path().join("session.rs"),
        "pub struct UserSession;\n",
    )
    .unwrap();
    let options = SearchOptions {
        root: temporary.path().to_owned(),
        query: "auth".to_owned(),
        max_bytes: 2_048,
        max_results: 5,
        use_cache: true,
        ..Default::default()
    };

    let cold = search(&options).unwrap();
    assert_eq!(cold.stats.files_reparsed, 2);
    assert!(!cold.stats.index_reused);

    let warm = search(&options).unwrap();
    assert_eq!(warm.stats.files_reparsed, 0);
    assert_eq!(warm.stats.files_reused, 2);
    assert!(warm.stats.index_reused);

    std::fs::write(
        &source,
        "pub fn authenticate_user_with_token(token: &str) -> bool { !token.is_empty() }\n",
    )
    .unwrap();
    let changed = search(&SearchOptions {
        query: "authenticate user token".to_owned(),
        ..options
    })
    .unwrap();
    assert_eq!(changed.stats.files_reparsed, 1);
    assert_eq!(changed.stats.files_reused, 1);
    assert!(!changed.stats.index_reused);
    assert_eq!(changed.results[0].symbol, "authenticate_user_with_token");
}
