use std::collections::BTreeMap;
use std::time::Instant;

use anyhow::{Context, Result, bail};

use crate::cache::load_indexed_repository;
use crate::lexical::Query;
use crate::model::{SearchOptions, SearchResponse, SearchStats};
use crate::ranking::{PreparedIndex, rank_indexed_candidates, rank_prepared_candidates};
use crate::relations::{RelationIndex, build_relation_graph_for, expand_ranked_with_weights};
use crate::repository::{ScanLimits, ScanStats, discover_with_limits};
use crate::selection::select_context_with_weights;

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
        let options = SearchOptions {
            root: self.root.clone(),
            query: query.to_owned(),
            max_bytes,
            max_results,
            use_cache: self.use_cache,
            scan_limits: self.scan_limits.clone(),
            ranking_weights: weights.clone(),
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
    crate::ranking::apply_weights(&mut ranked, symbols, &options.ranking_weights);
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
    let ranked = expand_ranked_with_weights(ranked, &graph, symbols, &options.ranking_weights);
    let relationship_us = stage.elapsed().as_micros();

    let stage = Instant::now();
    let results = select_context_with_weights(
        &ranked,
        symbols,
        &graph,
        options.max_bytes,
        options.max_results,
        &query,
        &options.ranking_weights,
    );
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
        context_cost: crate::model::ContextCost {
            selection_budget: options.max_bytes,
            ..Default::default()
        },
        query: options.query.clone(),
        root: root.to_string_lossy().into_owned(),
        results,
        stats,
        metadata,
    };
    crate::output::finalize(&mut response, &crate::output::Representation::Json, None)?;
    Ok(response)
}
