use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::{Deserialize, Serialize};

use crate::lexical::{Query, identifier_tokens, light_stem};
use crate::model::Symbol;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LexicalIndex {
    postings: BTreeMap<String, Vec<usize>>,
}

impl LexicalIndex {
    pub fn build(symbols: &[Symbol]) -> Self {
        Self::build_iter(symbols.iter())
    }

    pub fn build_iter<'a>(symbols: impl IntoIterator<Item = &'a Symbol>) -> Self {
        let mut postings: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for symbol in symbols {
            let mut keys = BTreeSet::new();
            keys.insert(format!("n:{}", symbol.normalized_name));
            for text in [
                symbol.name.as_str(),
                symbol.containing_symbol.as_deref().unwrap_or(""),
                &symbol.path,
                &symbol.comments(),
                symbol.signature(),
                symbol.body(),
            ] {
                add_text_keys(&mut keys, text);
            }
            for identifier in &symbol.identifiers {
                add_text_keys(&mut keys, identifier);
            }
            for key in keys {
                postings.entry(key).or_default().push(symbol.id);
            }
        }
        Self { postings }
    }

    pub fn candidates(&self, query: &Query) -> Vec<usize> {
        let mut candidates = HashSet::new();
        if let Some(ids) = self.postings.get(&format!("n:{}", query.normalized)) {
            candidates.extend(ids);
        }
        let keys: BTreeSet<_> = query
            .tokens
            .iter()
            .flat_map(|token| token_keys(token))
            .collect();
        for key in keys {
            if let Some(ids) = self.postings.get(&key) {
                candidates.extend(ids);
            }
        }
        // Deduplicate in a flat hash table before sorting: memory scales with
        // distinct candidates rather than the sum of overlapping posting lists.
        // Ascending IDs preserve the previous tree union's exact ordering.
        let mut candidates: Vec<_> = candidates.into_iter().collect();
        candidates.sort_unstable();
        candidates
    }

    pub fn posting_count(&self) -> usize {
        self.postings.len()
    }
}

fn add_text_keys(keys: &mut BTreeSet<String>, text: &str) {
    for token in identifier_tokens(text) {
        keys.extend(token_keys(&token));
    }
}

pub(crate) fn token_keys(token: &str) -> BTreeSet<String> {
    let mut keys = BTreeSet::from([format!("t:{token}"), format!("s:{}", light_stem(token))]);
    let prefix: String = token.chars().take(4).collect();
    if prefix.chars().count() == 4 {
        keys.insert(format!("p:{prefix}"));
    }
    keys
}

#[cfg(test)]
mod tests {
    use crate::model::Language;

    use super::*;

    #[test]
    fn bulk_posting_union_matches_tree_union_with_overlapping_aliases() {
        let mut postings = BTreeMap::new();
        for (term, modulus) in [("auth", 2), ("authentication", 3), ("token", 5)] {
            for key in token_keys(term) {
                postings
                    .entry(key)
                    .or_insert_with(Vec::new)
                    .extend((0..4096).filter(|id| id % modulus == 0));
            }
        }
        postings.insert("n:authtoken".into(), vec![3, 9999]);
        let index = LexicalIndex { postings };
        for input in [
            "auth token",
            "auth auth",
            "authentication",
            "absent",
            "",
            &"auth ".repeat(70),
        ] {
            let query = Query::parse(input);
            let mut reference = BTreeSet::new();
            if let Some(ids) = index.postings.get(&format!("n:{}", query.normalized)) {
                reference.extend(ids.iter().copied());
            }
            for token in &query.tokens {
                for key in token_keys(token) {
                    reference.extend(index.postings.get(&key).into_iter().flatten().copied());
                }
            }
            assert_eq!(
                index.candidates(&query),
                reference.into_iter().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn retrieves_morphological_candidates_without_scanning() {
        let symbol = Symbol {
            id: 0,
            path: "auth.rs".to_owned(),
            language: Language::Rust,
            name: "authenticate_user".to_owned(),
            normalized_name: "authenticateuser".to_owned(),
            kind: "function".to_owned(),
            containing_symbol: None,
            structural_depth: 0,
            start_byte: 0,
            end_byte: 2,
            start_line: 1,
            end_line: 1,
            source: std::sync::Arc::from(format!(
                "{}{}",
                "{}".to_owned(),
                "fn authenticate_user()".to_owned()
            )),
            signature_range: ("{}".to_owned()).len()
                ..("{}".to_owned()).len() + ("fn authenticate_user()".to_owned()).len(),
            body_range: 0..("{}".to_owned()).len(),
            comment_ranges: Vec::new(),
            excerpt_ranges: Vec::new(),
            imports: std::sync::Arc::from([]),
            identifiers: Vec::new(),
            type_references: Vec::new(),
            calls: Vec::new(),
        };
        let index = LexicalIndex::build(&[symbol]);
        assert_eq!(index.candidates(&Query::parse("authentication")), [0]);
    }
}
