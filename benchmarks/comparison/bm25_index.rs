//! Experimental local BM25 index. Built lazily; baseline retrieval is unchanged.
//!
//! Each structural symbol is one document containing its original source, path,
//! and enclosing name. Fixed k1=1.2, b=0.75; positive Robertson IDF. No labels,
//! query expansions, remote inference, or per-repository parameters are used.
use std::collections::{BTreeMap, BTreeSet, HashMap};

use flexcontext::lexical::{Query, identifier_tokens, lexically_related, light_stem};
use flexcontext::model::{ScoreSignals, ScoredSymbol, Symbol};

fn token_keys(token: &str) -> BTreeSet<String> {
    let mut keys = BTreeSet::from([format!("t:{token}"), format!("s:{}", light_stem(token))]);
    let prefix: String = token.chars().take(4).collect();
    if prefix.chars().count() == 4 {
        keys.insert(format!("p:{prefix}"));
    }
    keys
}

#[derive(Debug)]
pub struct Bm25Index {
    postings: HashMap<String, Vec<(usize, usize)>>,
    vocabulary: HashMap<String, Vec<String>>,
    lengths: Vec<usize>,
    average_length: f64,
}

impl Bm25Index {
    pub fn build(symbols: &[Symbol]) -> Self {
        let mut postings: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
        let mut lengths = Vec::with_capacity(symbols.len());
        for symbol in symbols {
            let mut counts = BTreeMap::<String, usize>::new();
            for text in [
                symbol.content(),
                symbol.path.as_str(),
                symbol.containing_symbol.as_deref().unwrap_or_default(),
            ] {
                for token in identifier_tokens(text) {
                    *counts.entry(token).or_default() += 1;
                }
            }
            lengths.push(counts.values().sum());
            for (token, count) in counts {
                postings.entry(token).or_default().push((symbol.id, count));
            }
        }
        let mut vocabulary: HashMap<String, Vec<String>> = HashMap::new();
        for token in postings.keys() {
            for key in token_keys(token) {
                vocabulary.entry(key).or_default().push(token.clone());
            }
        }
        let average_length = lengths.iter().sum::<usize>() as f64 / lengths.len().max(1) as f64;
        Self {
            postings,
            vocabulary,
            lengths,
            average_length,
        }
    }

    pub fn rank(&self, symbols: &[Symbol], query: &Query) -> Vec<ScoredSymbol> {
        let mut scores = vec![0.0; symbols.len()];
        // Repeated query words do not multiply their influence. Morphological
        // variants share one document frequency and one saturated frequency.
        for term in query.tokens.iter().collect::<BTreeSet<_>>() {
            let mut words = BTreeSet::new();
            for key in token_keys(term) {
                for word in self.vocabulary.get(&key).into_iter().flatten() {
                    if lexically_related(term, word) {
                        words.insert(word);
                    }
                }
            }
            let mut frequencies = BTreeMap::<usize, usize>::new();
            for word in words {
                for &(id, count) in &self.postings[word] {
                    *frequencies.entry(id).or_default() += count;
                }
            }
            let df = frequencies.len() as f64;
            let idf = (1.0 + (symbols.len() as f64 - df + 0.5) / (df + 0.5)).ln();
            for (id, frequency) in frequencies {
                let tf = frequency as f64;
                let norm =
                    1.2 * (0.25 + 0.75 * self.lengths[id] as f64 / self.average_length.max(1.0));
                scores[id] += idf * tf * 2.2 / (tf + norm);
            }
        }
        let broad = scores.iter().filter(|s| **s > 0.0).count() >= 8;
        let mut ranked: Vec<_> = symbols
            .iter()
            .filter_map(|symbol| {
                if broad && symbol.kind == "declaration" && symbol.structural_depth > 0 {
                    return None;
                }
                let score = scores[symbol.id];
                (score > 0.0).then_some(ScoredSymbol {
                    symbol_id: symbol.id,
                    score,
                    signals: ScoreSignals {
                        body: score,
                        ..Default::default()
                    },
                })
            })
            .collect();
        flexcontext::ranking::sort_scored(&mut ranked, symbols);
        ranked
    }
}
