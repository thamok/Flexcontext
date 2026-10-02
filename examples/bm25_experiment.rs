//! Benchmark-only stdio worker. This is deliberately not a production policy:
//! the fixed BM25 experiment failed the development evidence recall gate.
#[path = "../benchmarks/comparison/bm25_index.rs"]
mod bm25;

use std::io::{BufRead, Write};

use anyhow::{Result, ensure};
use flexcontext::{QueryOptions, SearchSession, lexical::Query, ranking::RankingWeights};
use serde_json::{Value, json};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 3 && args[1] == "serve",
        "usage: bm25_experiment serve ROOT"
    );
    let session = SearchSession::open(std::path::Path::new(&args[2]), true)?;
    let index = std::sync::OnceLock::new();
    for line in std::io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line?)?;
        let id = request["id"].clone();
        let response = if request["method"] == "server/discover" {
            json!({"jsonrpc":"2.0", "id":id, "result":{}})
        } else {
            let args = &request["params"]["arguments"];
            let query = args["query"].as_str().unwrap_or_default();
            let max_bytes = args["budget"].as_u64().unwrap_or(4096) as usize * 4;
            let max_results = args["max_results"].as_u64().unwrap_or(100) as usize;
            let results = if args["policy"] == "bm25" {
                let symbols = session.symbols();
                let query = Query::parse(query);
                let ranked = index
                    .get_or_init(|| bm25::Bm25Index::build(symbols))
                    .rank(symbols, &query);
                let shortlist: Vec<_> = ranked
                    .iter()
                    .take(max_results.saturating_mul(4).clamp(16, 64))
                    .map(|s| s.symbol_id)
                    .collect();
                let graph = flexcontext::relations::build_relation_graph_for(symbols, &shortlist);
                flexcontext::selection::select_context_config(
                    &ranked,
                    symbols,
                    &graph,
                    max_bytes,
                    max_results,
                    &query,
                    &RankingWeights::default(),
                    true,
                )
                .0
            } else {
                session
                    .query_with_options(&QueryOptions {
                        query: query.into(),
                        max_bytes,
                        max_results,
                        ..Default::default()
                    })?
                    .results
            };
            json!({"jsonrpc":"2.0", "id":id, "result":{"structuredContent":{"results":results}}})
        };
        println!("{response}");
        std::io::stdout().flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rare_terms_outrank_repeated_common_terms_and_duplicate_queries_are_stable() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("cache.py"),
            "def load_cache():\n    return cache.read()\n\ndef reject_expired():\n    # reject expired cached tokens\n    return expired(token)\n").unwrap();
        for n in 0..12 {
            std::fs::write(
                temp.path().join(format!("f{n}.py")),
                format!("def load_{n}():\n    return cache.load(cache.read())\n"),
            )
            .unwrap();
        }
        let session = SearchSession::open(temp.path(), false).unwrap();
        let index = bm25::Bm25Index::build(session.symbols());
        let results = index.rank(session.symbols(), &Query::parse("cache expired"));
        assert_eq!(
            session.symbols()[results[0].symbol_id].name,
            "reject_expired"
        );
        assert!(results.iter().all(|r| r.score.is_finite()));
        let repeated = index.rank(
            session.symbols(),
            &Query::parse(&"cache expired ".repeat(70)),
        );
        assert_eq!(
            results
                .iter()
                .map(|r| (r.symbol_id, r.score))
                .collect::<Vec<_>>(),
            repeated
                .iter()
                .map(|r| (r.symbol_id, r.score))
                .collect::<Vec<_>>()
        );
        assert!(
            index
                .rank(session.symbols(), &Query::parse("用户"))
                .is_empty()
        );
    }
}
