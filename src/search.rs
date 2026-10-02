use std::collections::BTreeMap;
use std::time::Instant;

use anyhow::{Context, Result, bail};

use crate::cache::load_indexed_repository;
use crate::lexical::Query;
use crate::model::{
    QueryOptions, RetrievalPolicy, RetrievalTrace, ScopeMode, SearchOptions, SearchResponse,
    SearchStats,
};
use crate::ranking::{PreparedIndex, rank_indexed_candidates, rank_prepared_candidates};
use crate::relations::{RelationIndex, build_relation_graph_for, expand_ranked_with_weights};
use crate::repository::{ScanLimits, ScanStats, discover_with_limits};
use crate::selection::select_context_config;

pub fn search(options: &SearchOptions) -> Result<SearchResponse> {
    let started = Instant::now();
    crate::repository::check_cancelled()?;
    options.ranking_weights.validate()?;
    let query = Query::parse(&options.query);
    if query.is_empty() {
        bail!("query must contain at least one letter or number");
    }
    if options.max_bytes == 0 {
        bail!("--max-bytes must be greater than zero");
    }
    let root = options
        .root
        .canonicalize()
        .with_context(|| format!("cannot access repository root {}", options.root.display()))?;

    let stage = Instant::now();
    let discovery = discover_with_limits(&root, &options.scan_limits)?;
    let traversal_us = stage.elapsed().as_micros();

    let repository = load_indexed_repository(&root, &discovery.paths, options.use_cache)?;
    search_repository(
        options,
        &root,
        &repository,
        None,
        None,
        &discovery.stats,
        traversal_us,
        started,
        false,
    )
}

/// An immutable repository snapshot. Queries perform no filesystem reads.
/// Call refresh after edits to atomically replace the snapshot.
pub struct SearchSession {
    root: std::path::PathBuf,
    repository: crate::cache::IndexedRepository,
    prepared: PreparedIndex,
    relations: RelationIndex,
    use_cache: bool,
    scan: ScanStats,
    scan_limits: ScanLimits,
}

impl SearchSession {
    pub fn open(root: &std::path::Path, use_cache: bool) -> Result<Self> {
        Self::open_with_limits(root, use_cache, &ScanLimits::default())
    }
    pub fn open_with_limits(
        root: &std::path::Path,
        use_cache: bool,
        limits: &ScanLimits,
    ) -> Result<Self> {
        let root = root.canonicalize()?;
        let discovery = discover_with_limits(&root, limits)?;
        let paths = discovery.paths;
        let repository = load_indexed_repository(&root, &paths, use_cache)?;
        let prepared = PreparedIndex::build(&repository.symbols);
        let relations = RelationIndex::build(&repository.symbols);
        Ok(Self {
            root,
            repository,
            prepared,
            relations,
            use_cache,
            scan: discovery.stats,
            scan_limits: limits.clone(),
        })
    }

    pub fn index_summary(&self) -> serde_json::Value {
        serde_json::json!({"files_indexed":self.repository.files_available,"source_bytes":self.repository.source_bytes,"symbols":self.repository.symbols.len(),"scan":self.scan,"files_reused":self.repository.files_reused,"files_reparsed":self.repository.files_reparsed,"cache_fingerprint":crate::cache::CACHE_FINGERPRINT})
    }
    pub fn symbols(&self) -> &[crate::model::Symbol] {
        &self.repository.symbols
    }

    pub fn refresh(&mut self) -> Result<()> {
        *self = Self::open_with_limits(&self.root, self.use_cache, &self.scan_limits)?;
        Ok(())
    }

    pub fn query(
        &self,
        query: &str,
        max_bytes: usize,
        max_results: usize,
    ) -> Result<SearchResponse> {
        self.query_with_weights(
            query,
            max_bytes,
            max_results,
            &crate::ranking::RankingWeights::default(),
        )
    }
    pub fn query_with_weights(
        &self,
        query: &str,
        max_bytes: usize,
        max_results: usize,
        weights: &crate::ranking::RankingWeights,
    ) -> Result<SearchResponse> {
        self.query_options_with_weights(
            &QueryOptions {
                query: query.into(),
                max_bytes,
                max_results,
                detail: crate::model::Detail::Full,
                ..Default::default()
            },
            weights,
        )
    }

    pub fn query_with_options(&self, query: &QueryOptions) -> Result<SearchResponse> {
        self.query_options_with_weights(query, &crate::ranking::RankingWeights::default())
    }

    fn query_options_with_weights(
        &self,
        query: &QueryOptions,
        weights: &crate::ranking::RankingWeights,
    ) -> Result<SearchResponse> {
        let options = SearchOptions {
            root: self.root.clone(),
            query: query.query.clone(),
            max_bytes: query.max_bytes,
            max_results: query.max_results,
            use_cache: self.use_cache,
            scan_limits: self.scan_limits.clone(),
            ranking_weights: weights.clone(),
            retrieval: query.clone(),
        };
        search_repository(
            &options,
            &self.root,
            &self.repository,
            Some(&self.prepared),
            Some(&self.relations),
            &self.scan,
            0,
            Instant::now(),
            true,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn search_repository(
    options: &SearchOptions,
    root: &std::path::Path,
    repository: &crate::cache::IndexedRepository,
    prepared: Option<&PreparedIndex>,
    relations: Option<&RelationIndex>,
    scan: &ScanStats,
    traversal_us: u128,
    started: Instant,
    resident: bool,
) -> Result<SearchResponse> {
    crate::repository::check_cancelled()?;
    options.ranking_weights.validate()?;
    options.retrieval.scope.validate()?;
    anyhow::ensure!(
        options.retrieval.cutoff.is_finite() && (0.0..=1.0).contains(&options.retrieval.cutoff),
        "cutoff must be between zero and one"
    );
    let policy = options.retrieval.policy;
    let query = Query::parse(&options.query);
    if query.is_empty() {
        bail!("query must contain at least one letter or number");
    }
    if options.max_bytes == 0 || options.max_results == 0 {
        bail!("budgets must be greater than zero");
    }
    let symbols = &repository.symbols;
    let stage = Instant::now();
    let mut ranked = if let Some(prepared) = prepared {
        rank_prepared_candidates(symbols, &query, &repository.index, prepared)
    } else {
        rank_indexed_candidates(symbols, &query, &repository.index)
    };
    if matches!(
        policy,
        RetrievalPolicy::Idf | RetrievalPolicy::Direct | RetrievalPolicy::Focused
    ) {
        let owned;
        let prepared = match prepared {
            Some(p) => p,
            None => {
                owned = PreparedIndex::build(symbols);
                &owned
            }
        };
        prepared.weight_by_file_frequency(
            &mut ranked,
            symbols,
            &query,
            policy != RetrievalPolicy::Direct,
            policy != RetrievalPolicy::Idf,
        );
    }
    crate::ranking::apply_weights(&mut ranked, symbols, &options.ranking_weights);
    ranked.retain(|s| options.retrieval.scope.allows(&symbols[s.symbol_id].path));
    if matches!(
        policy,
        RetrievalPolicy::Implementation | RetrievalPolicy::Focused
    ) {
        crate::focused::prefer_implementation(
            &mut ranked,
            symbols,
            &query,
            &options.retrieval.scope,
        );
    }
    let direct = ranked.clone();
    let candidate_symbols = ranked.len();
    let candidate_and_ranking_us = stage.elapsed().as_micros();

    let stage = Instant::now();
    let shortlist_size = options.max_results.saturating_mul(4).clamp(16, 64);
    let shortlist: Vec<_> = ranked
        .iter()
        .take(shortlist_size)
        .map(|candidate| candidate.symbol_id)
        .collect();
    let graph = relations.map_or_else(
        || build_relation_graph_for(symbols, &shortlist),
        |index| index.graph_for(symbols, &shortlist),
    );
    let mut ranked = expand_ranked_with_weights(ranked, &graph, symbols, &options.ranking_weights);
    ranked.retain(|s| options.retrieval.scope.allows(&symbols[s.symbol_id].path));
    let direct_map: std::collections::HashMap<_, _> = direct
        .iter()
        .enumerate()
        .map(|(i, s)| (s.symbol_id, (i, s)))
        .collect();
    let mut rejected = std::collections::HashMap::new();
    if matches!(
        policy,
        RetrievalPolicy::Relations | RetrievalPolicy::Focused
    ) {
        ranked.retain_mut(|s| {
            if let Some((_, anchor)) = direct_map.get(&s.symbol_id) {
                let boost = if crate::focused::independent(anchor) {
                    s.signals
                        .structural_relation
                        .min(anchor.score.max(0.0) * 0.2)
                } else {
                    0.0
                };
                s.signals.structural_relation = boost;
                s.score = anchor.score + boost;
                true
            } else {
                rejected.insert(s.symbol_id, "no independent lexical evidence".to_owned());
                false
            }
        });
        crate::ranking::sort_scored(&mut ranked, symbols);
    }
    let relationship_order: std::collections::HashMap<_, _> = ranked
        .iter()
        .enumerate()
        .map(|(i, s)| (s.symbol_id, (i, s.signals.structural_relation)))
        .collect();
    if matches!(policy, RetrievalPolicy::Cutoff | RetrievalPolicy::Focused) {
        let best = direct.first().map_or(0.0, |s| s.score);
        ranked.retain(|s| {
            let keep = direct_map.get(&s.symbol_id).is_some_and(|(_, s)| {
                s.score >= best * options.retrieval.cutoff && crate::focused::independent(s)
            });
            if !keep {
                rejected.insert(s.symbol_id, "below direct relevance cutoff".into());
            }
            keep
        });
    }
    let focus = if matches!(policy, RetrievalPolicy::Focus | RetrievalPolicy::Focused)
        && options.retrieval.scope.scope == ScopeMode::Auto
    {
        crate::focused::concentrate(&mut ranked, symbols, &query)
    } else {
        Default::default()
    };
    let relationship_us = stage.elapsed().as_micros();

    let stage = Instant::now();
    let mut selection_weights = options.ranking_weights.clone();
    if matches!(
        policy,
        RetrievalPolicy::Diversity | RetrievalPolicy::Focused
    ) {
        selection_weights.diversity_penalty = 0.0;
    }
    let (mut results, decisions) =
        if matches!(policy, RetrievalPolicy::Stable | RetrievalPolicy::Focused) {
            let constrained;
            let candidates = if policy == RetrievalPolicy::Stable {
                let (order, reasons) =
                    crate::selection::constrained_order(&ranked, symbols, &selection_weights);
                constrained = order;
                rejected.extend(reasons);
                &constrained
            } else {
                &ranked
            };
            crate::focused::select(
                candidates,
                symbols,
                &graph,
                &query,
                options.max_bytes,
                options.max_results,
            )
        } else {
            select_context_config(
                &ranked,
                symbols,
                &graph,
                options.max_bytes,
                options.max_results,
                &query,
                &selection_weights,
                !matches!(policy, RetrievalPolicy::Quotas),
            )
        };
    for result in &mut results {
        result
            .relations
            .retain(|r| options.retrieval.scope.allows(&r.path));
    }
    rejected.extend(decisions);
    let trace = if options.retrieval.explain {
        let ids: std::collections::BTreeSet<_> = direct_map
            .keys()
            .chain(relationship_order.keys())
            .chain(rejected.keys())
            .copied()
            .collect();
        ids.into_iter()
            .map(|id| {
                let symbol = &symbols[id];
                RetrievalTrace {
                    path: symbol.path.clone(),
                    symbol: symbol.name.clone(),
                    start_line: symbol.start_line,
                    direct_rank: direct_map.get(&id).map(|(i, _)| i + 1),
                    direct_score: direct_map.get(&id).map_or(0.0, |(_, s)| s.score),
                    relationship_rank: relationship_order.get(&id).map_or(0, |(i, _)| i + 1),
                    relationship_boost: relationship_order.get(&id).map_or(0.0, |(_, v)| *v),
                    decision: rejected
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| "not selected".into()),
                    truncated: results.iter().any(|r| {
                        r.path == symbol.path
                            && r.start_byte == symbol.start_byte
                            && r.symbol == symbol.name
                            && r.content_truncated
                    }),
                    scope_promotion: focus.get(&id).copied().unwrap_or(0.0),
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    crate::repository::check_cancelled()?;
    let selection_us = stage.elapsed().as_micros();
    let returned_bytes = results.iter().map(|result| result.content_bytes).sum();
    let approximate_tokens = results.iter().map(|result| result.approximate_tokens).sum();
    let stats = SearchStats {
        scan: scan.clone(),
        skipped_binary_or_unreadable_files: repository.files_skipped,
        files_scanned: scan.files_scanned,
        files_parsed: if resident {
            0
        } else {
            repository.files_reparsed
        },
        files_indexed: repository.files_available,
        source_bytes: repository.source_bytes,
        symbols: symbols.len(),
        candidate_symbols,
        returned_symbols: results.len(),
        returned_bytes,
        human_payload_bytes: 0,
        json_payload_bytes: 0,
        approximate_tokens,
        traversal_us,
        cache_load_us: if resident {
            0
        } else {
            repository.cache_load_us
        },
        parse_and_extract_us: if resident {
            0
        } else {
            repository.parse_and_extract_us
        },
        index_us: if resident { 0 } else { repository.index_us },
        candidate_and_ranking_us,
        relationship_us,
        selection_us,
        cache_write_us: if resident {
            0
        } else {
            repository.cache_write_us
        },
        elapsed_us: started.elapsed().as_micros(),
        files_reused: if resident {
            repository.files_available
        } else {
            repository.files_reused
        },
        files_reparsed: if resident {
            0
        } else {
            repository.files_reparsed
        },
        index_reused: resident || repository.index_reused,
    };
    let metadata = BTreeMap::from([
        (
            "snapshot".to_owned(),
            if resident {
                "resident; explicit refresh required after edits"
            } else {
                "fresh filesystem scan"
            }
            .to_owned(),
        ),
        ("retrieval".to_owned(), "structural lexical".to_owned()),
        (
            "token_estimate".to_owned(),
            "serialized UTF-8 bytes / 4 rounded up; heuristic, not a model tokenizer".to_owned(),
        ),
        (
            "cache".to_owned(),
            if options.use_cache {
                ".flexcontext/index.json"
            } else {
                "disabled"
            }
            .to_owned(),
        ),
    ]);
    let mut response = SearchResponse {
        policy,
        focused_files: focus
            .keys()
            .map(|&id| symbols[id].path.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect(),
        context_cost: crate::model::ContextCost {
            selection_budget: options.max_bytes,
            ..Default::default()
        },
        query: options.query.clone(),
        root: root.to_string_lossy().into_owned(),
        results,
        stats,
        metadata,
        scope: options.retrieval.scope.clone(),
        trace,
    };
    crate::output::finalize(
        &mut response,
        &crate::output::Representation::json(options.retrieval.detail),
        None,
    )?;
    Ok(response)
}
