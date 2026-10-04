//! Opt-in, stateless navigation over a bounded subset of discovered candidates.
//! References are integrity checked, not authentication credentials. The server's
//! repository and the originating explicit scope remain the access boundaries.
use crate::model::{ScoredSymbol, SearchResponse, SearchResult, SearchScope, Symbol};
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    ops::Range,
    path::Path,
};

const RETAINED: usize = 64;
const PAGE: usize = 4;
const MAX_REFERENCE: usize = 32768;
const MAX_RANGES: usize = 256;
const MAX_COPIES: usize = 4096;

#[derive(Clone)]
struct SigningKey([u8; 32]);
impl std::fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SigningKey(<redacted>)")
    }
}
// One key, no sessions or seen-state database. Publish a complete key atomically
// so simultaneous CLI searches use the same signer. Deleting it expires refs.
fn signing_key(root: &Path, create: bool) -> Result<SigningKey> {
    use std::io::Write;
    let directory = root.join(".flexcontext");
    let path = directory.join("progressive-key-v1");
    if !path.exists() && create {
        std::fs::create_dir_all(&directory)?;
        let mut key = [0u8; 32];
        getrandom::fill(&mut key)
            .map_err(|e| anyhow::anyhow!("cannot generate continuation key: {e}"))?;
        let temporary = directory.join(format!(
            ".progressive-key-{}",
            URL_SAFE_NO_PAD.encode(&key[..12])
        ));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        let publish = (|| -> Result<()> {
            file.write_all(&key)?;
            file.sync_all()?;
            match std::fs::hard_link(&temporary, &path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
                Err(e) => Err(e.into()),
            }
        })();
        let _ = std::fs::remove_file(&temporary);
        publish?;
    }
    let bytes = std::fs::read(path)
        .map_err(|_| anyhow::anyhow!("continuation signing key unavailable; search again"))?;
    let key: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid continuation signing key"))?;
    Ok(SigningKey(key))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lead {
    pub reference: String,
    pub path: String,
    pub symbol: String,
    pub kind: String,
    pub containing_symbol: Option<String>,
    pub start_line: usize,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub role_hints: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Navigation {
    pub leads: Vec<Lead>,
    pub next: Option<String>,
    pub remaining: usize,
    pub retained_candidates: usize,
    pub outside_retained: usize,
    pub displaced_source_bytes: usize,
    pub coverage: String,
}
#[derive(Debug, Clone)]
pub(crate) struct State {
    key: SigningKey,
    snapshot: String,
    scope: SearchScope,
    candidates: Vec<(Symbol, String, Vec<Range<usize>>)>,
    // Indexed, byte-identical symbols overlapping original delivered source.
    copies: HashMap<usize, Vec<(String, Range<usize>)>>,
    outside: usize,
    hints: bool,
    pub(crate) expansion: bool,
    pub(crate) displaced: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Item {
    id: usize,
    reason: String,
    ranges: Vec<Range<usize>>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    version: u8,
    snapshot: String,
    scope: SearchScope,
    page: bool,
    hints: bool,
    outside: usize,
    items: Vec<Item>,
}

pub(crate) fn snapshot(root: &Path, symbols: &[Symbol]) -> String {
    let mut digest = Sha256::new();
    digest.update(root.to_string_lossy().as_bytes());
    digest.update(crate::cache::CACHE_FINGERPRINT.as_bytes());
    let mut paths = std::collections::HashSet::new();
    for s in symbols {
        if paths.insert(&s.path) {
            digest.update((s.path.len() as u64).to_le_bytes());
            digest.update(s.path.as_bytes());
            digest.update((s.source.len() as u64).to_le_bytes());
            digest.update(s.source.as_bytes());
        }
        digest.update((s.id as u64).to_le_bytes());
        digest.update((s.start_byte as u64).to_le_bytes());
        digest.update((s.end_byte as u64).to_le_bytes());
    }
    format!("{:x}", digest.finalize())
}
fn encode(reference: &Reference, key: &SigningKey) -> Result<String> {
    // Compact JSON tuples are substantially cheaper for language-model tokenizers
    // than base64-encoded JSON. The tool still treats this as an opaque string.
    let wire: Wire = (
        reference.version,
        reference.snapshot.clone(),
        (
            reference.scope.scope,
            reference.scope.include_paths.clone(),
            reference.scope.exclude_paths.clone(),
        ),
        reference.page,
        reference.hints,
        reference.outside,
        reference
            .items
            .iter()
            .map(|i| {
                (
                    i.id,
                    reason_code(&i.reason),
                    i.ranges.iter().map(|r| (r.start, r.end)).collect(),
                )
            })
            .collect(),
    );
    let bytes = serde_json::to_vec(&wire).expect("serializable reference");
    let mut mac = Hmac::<Sha256>::new_from_slice(&key.0).expect("valid key");
    mac.update(&bytes);
    let checksum = mac.finalize().into_bytes();
    let encoded = format!(
        "fc1.{}.{}",
        URL_SAFE_NO_PAD.encode(checksum),
        String::from_utf8(bytes).expect("JSON UTF-8")
    );
    ensure!(
        encoded.len() <= MAX_REFERENCE,
        "scope or continuation exceeds reference size limit; narrow the search scope"
    );
    Ok(encoded)
}
fn decode(reference: &str, key: &SigningKey) -> Result<Reference> {
    ensure!(
        reference.len() <= MAX_REFERENCE,
        "continuation reference too large"
    );
    let parts: Vec<_> = reference.splitn(3, '.').collect();
    ensure!(
        parts.len() == 3 && parts[0] == "fc1",
        "invalid continuation reference"
    );
    let bytes = parts[2].as_bytes();
    let mut mac = Hmac::<Sha256>::new_from_slice(&key.0).expect("valid key");
    mac.update(bytes);
    mac.verify_slice(&URL_SAFE_NO_PAD.decode(parts[1])?)
        .map_err(|_| anyhow::anyhow!("continuation signature mismatch; search again"))?;
    let (version, snapshot, (scope, include_paths, exclude_paths), page, hints, outside, items): Wire = serde_json::from_slice(bytes)?;
    let r = Reference {
        version,
        snapshot,
        scope: SearchScope {
            scope,
            include_paths,
            exclude_paths,
        },
        page,
        hints,
        outside,
        items: items
            .into_iter()
            .map(|(id, reason, ranges)| Item {
                id,
                reason: reason_text(reason).into(),
                ranges: ranges.into_iter().map(|(start, end)| start..end).collect(),
            })
            .collect(),
    };
    ensure!(
        r.version == 1 && !r.items.is_empty() && r.items.len() <= RETAINED,
        "unsupported or empty continuation"
    );
    ensure!(
        r.page || r.items.len() == 1,
        "source reference requires one candidate"
    );
    r.scope.validate()?;
    Ok(r)
}
type Wire = (
    u8,
    String,
    (crate::model::ScopeMode, Vec<String>, Vec<String>),
    bool,
    bool,
    usize,
    Vec<(usize, u8, Vec<(usize, usize)>)>,
);
const REASONS: &[&str] = &[
    "not selected",
    "name quota",
    "file/container quota",
    "overlapping source",
    "excerpt does not fit",
    "selected",
    "candidate pool limit",
    "result limit",
    "source budget",
    "partial source",
    "serialized payload budget",
];
fn reason_code(reason: &str) -> u8 {
    REASONS.iter().position(|r| *r == reason).unwrap_or(0) as u8
}
fn reason_text(code: u8) -> &'static str {
    REASONS
        .get(code as usize)
        .copied()
        .unwrap_or("not selected")
}

pub(crate) fn prepare(
    root: &Path,
    symbols: &[Symbol],
    ranked: &[ScoredSymbol],
    reasons: &HashMap<usize, String>,
    scope: &SearchScope,
    hints: bool,
    results: &[SearchResult],
) -> Result<State> {
    let mut copies: HashMap<usize, Vec<(String, Range<usize>)>> = HashMap::new();
    let mut copy_count = 0;
    for candidate in ranked.iter().take(RETAINED) {
        let original = &symbols[candidate.symbol_id];
        for other in symbols {
            if original.id != other.id
                && original.content() == other.content()
                && results.iter().any(|r| {
                    r.path == other.path
                        && r.source_spans.iter().any(|span| {
                            span.start_byte < other.end_byte && other.start_byte < span.end_byte
                        })
                })
            {
                copy_count += 1;
                ensure!(
                    copy_count <= MAX_COPIES,
                    "too many duplicate source mappings; narrow the search"
                );
                copies
                    .entry(original.id)
                    .or_default()
                    .push((other.path.clone(), other.start_byte..other.end_byte));
            }
        }
    }
    Ok(State {
        copies,
        key: signing_key(root, true)?,
        snapshot: snapshot(root, symbols),
        scope: scope.clone(),
        candidates: ranked
            .iter()
            .take(RETAINED)
            .map(|r| {
                let s = symbols[r.symbol_id].clone();
                let range = s.start_byte..s.end_byte;
                (
                    s,
                    reasons
                        .get(&r.symbol_id)
                        .cloned()
                        .unwrap_or_else(|| "not selected".into()),
                    vec![range],
                )
            })
            .collect(),
        outside: ranked.len().saturating_sub(RETAINED),
        hints,
        expansion: false,
        displaced: 0,
    })
}
fn subtract(
    mut ranges: Vec<Range<usize>>,
    removed: impl IntoIterator<Item = Range<usize>>,
) -> Vec<Range<usize>> {
    for cut in removed {
        ranges = ranges
            .into_iter()
            .flat_map(|r| {
                if cut.start >= r.end || cut.end <= r.start {
                    return vec![r];
                }
                let mut remaining = Vec::new();
                if r.start < cut.start {
                    remaining.push(r.start..cut.start);
                }
                if cut.end < r.end {
                    remaining.push(cut.end..r.end);
                }
                remaining
            })
            .collect();
    }
    ranges
}
fn meaningful(s: &Symbol, ranges: &[Range<usize>]) -> bool {
    ranges
        .iter()
        .any(|r| s.source[r.clone()].chars().any(|c| !c.is_whitespace()))
}
fn hints(s: &Symbol) -> Vec<String> {
    // Source vocabulary only. These are heuristic cues, never eligibility rules.
    let text = format!(
        "{} {} {} {} {}",
        s.name,
        s.signature(),
        s.calls.join(" "),
        s.identifiers.join(" "),
        s.comments()
    );
    let tokens = crate::lexical::identifier_tokens(&text);
    [
        "cancel",
        "timeout",
        "expire",
        "revoke",
        "delete",
        "clear",
        "close",
        "cleanup",
        "authorize",
        "authenticate",
        "login",
        "refresh",
        "config",
        "test",
        "parse",
        "serialize",
        "read",
        "write",
        "cache",
        "queue",
        "retry",
    ]
    .into_iter()
    .filter(|word| tokens.iter().any(|t| t == word))
    .take(4)
    .map(str::to_owned)
    .collect()
}
impl State {
    fn reference(&self, mut items: Vec<Item>, page: bool, outside: usize) -> Result<String> {
        for item in &mut items {
            if let Some((s, _, _)) = self.candidates.iter().find(|(s, _, _)| s.id == item.id)
                && item.ranges.len() == 1
                && item.ranges[0] == (s.start_byte..s.end_byte)
            {
                item.ranges.clear();
            }
        }
        encode(
            &Reference {
                version: 1,
                snapshot: self.snapshot.clone(),
                scope: self.scope.clone(),
                page,
                hints: self.hints,
                outside,
                items,
            },
            &self.key,
        )
    }
}
/// Recomputed only after monotonic source removal, never by rerunning selection.
pub(crate) fn update(response: &mut SearchResponse) -> Result<()> {
    let Some(state) = &response.continuation_state else {
        return Ok(());
    };
    let mut omitted = Vec::new();
    let mut outside = state.outside;
    let mut range_count = 0;
    for (s, reason, initial) in &state.candidates {
        // Byte-identical complete source is not additional evidence, even at another path.
        if response
            .results
            .iter()
            .any(|r| !r.content_truncated && r.content == s.content())
        {
            continue;
        }
        let removed = response
            .results
            .iter()
            .filter(|r| r.path == s.path)
            .flat_map(|r| {
                r.source_spans
                    .iter()
                    .map(|span| span.start_byte..span.end_byte)
            });
        let mut ranges = subtract(initial.clone(), removed);
        // Translate only indexed, byte-identical symbols. A same name or an
        // arbitrary string match is not evidence that implementations coincide.
        for (path, copy) in state.copies.get(&s.id).into_iter().flatten() {
            let removed = response
                .results
                .iter()
                .filter(|r| &r.path == path)
                .flat_map(|r| &r.source_spans)
                .filter_map(|span| {
                    let start = span.start_byte.max(copy.start);
                    let end = span.end_byte.min(copy.end);
                    (start < end).then(|| {
                        (s.start_byte + start - copy.start)..(s.start_byte + end - copy.start)
                    })
                });
            ranges = subtract(ranges, removed);
        }
        if !meaningful(s, &ranges) {
            continue;
        }
        range_count += ranges.len();
        if range_count > MAX_RANGES {
            outside += 1;
            continue;
        }
        let delivered = response
            .results
            .iter()
            .any(|r| r.path == s.path && r.start_byte == s.start_byte && r.symbol == s.name);
        let reason = if delivered || ranges != *initial {
            "partial source"
        } else if reason == "selected" {
            "serialized payload budget"
        } else {
            reason
        };
        omitted.push((
            s,
            Item {
                id: s.id,
                reason: reason.into(),
                ranges,
            },
        ));
    }
    let leads = omitted
        .iter()
        .take(PAGE)
        .map(|(s, item)| {
            Ok(Lead {
                reference: state.reference(vec![item.clone()], false, outside)?,
                path: s.path.clone(),
                symbol: s.name.clone(),
                kind: s.kind.clone(),
                containing_symbol: s.containing_symbol.clone(),
                start_line: s.start_line,
                reason: item.reason.clone(),
                role_hints: if state.hints { hints(s) } else { vec![] },
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let next = (omitted.len() > PAGE)
        .then(|| {
            state.reference(
                omitted.iter().skip(PAGE).map(|(_, i)| i.clone()).collect(),
                true,
                outside,
            )
        })
        .transpose()?;
    response.navigation = Some(Navigation { leads, next, remaining: omitted.len(), retained_candidates: state.candidates.len(), outside_retained: outside, displaced_source_bytes: state.displaced, coverage: "Retained discovered candidates only; leads are not behavioral evidence. Other searches may be needed. References survive restart only for the identical repository/index snapshot and scope; refresh after edits.".into() });
    Ok(())
}

pub(crate) fn expand(
    root: &Path,
    symbols: &[Symbol],
    reference: &str,
    max_bytes: usize,
) -> Result<SearchResponse> {
    crate::repository::check_cancelled()?;
    ensure!(max_bytes > 0, "source budget must be positive");
    let key = signing_key(root, false)?;
    let r = decode(reference, &key)?;
    ensure!(
        r.snapshot == snapshot(root, symbols),
        "stale or wrong-repository continuation; search again"
    );
    let mut candidates = Vec::new();
    let mut range_count = 0;
    for mut item in r.items {
        let s = symbols
            .get(item.id)
            .ok_or_else(|| anyhow::anyhow!("unknown candidate"))?;
        ensure!(
            r.scope.allows(&s.path),
            "candidate is outside originating scope"
        );
        if item.ranges.is_empty() {
            item.ranges.push(s.start_byte..s.end_byte);
        }
        range_count += item.ranges.len();
        ensure!(
            range_count <= MAX_RANGES && !item.ranges.is_empty(),
            "invalid continuation ranges"
        );
        let mut end = s.start_byte;
        for range in &item.ranges {
            ensure!(
                range.start >= end
                    && range.start < range.end
                    && range.end <= s.end_byte
                    && s.source.get(range.clone()).is_some(),
                "invalid source range"
            );
            end = range.end;
        }
        candidates.push((s.clone(), item.reason, item.ranges));
    }
    let state = State {
        copies: HashMap::new(),
        key,
        snapshot: r.snapshot,
        scope: r.scope.clone(),
        candidates,
        outside: r.outside,
        hints: r.hints,
        expansion: !r.page,
        displaced: 0,
    };
    let mut results = Vec::new();
    if !r.page {
        let (s, _, ranges) = &state.candidates[0];
        let mut content = String::new();
        let mut spans = Vec::new();
        for range in ranges {
            let marker = if content.is_empty() {
                ""
            } else {
                "\n[… omitted …]\n"
            };
            let available = max_bytes.saturating_sub(content.len() + marker.len());
            let mut end = range.end.min(range.start.saturating_add(available));
            while end > range.start && !s.source.is_char_boundary(end) {
                end -= 1;
            }
            if end == range.start {
                break;
            }
            content.push_str(marker);
            content.push_str(&s.source[range.start..end]);
            spans.push(crate::slicing::span_for_range(s, range.start..end));
            if end < range.end {
                break;
            }
        }
        ensure!(
            !spans.is_empty(),
            "insufficient source budget for next UTF-8 character; increase budget"
        );
        let complete = spans.len() == 1
            && spans[0].start_byte == s.start_byte
            && spans[0].end_byte == s.end_byte;
        results.push(SearchResult {
            path: s.path.clone(),
            language: s.language,
            symbol: s.name.clone(),
            kind: s.kind.clone(),
            containing_symbol: s.containing_symbol.clone(),
            start_byte: s.start_byte,
            end_byte: s.end_byte,
            start_line: s.start_line,
            end_line: s.end_line,
            signature: s.signature().to_owned(),
            score: 0.0,
            lexical_score: 0.0,
            structural_score: 0.0,
            diversity_score: 0.0,
            final_score: 0.0,
            signals: Default::default(),
            content_bytes: content.len(),
            approximate_tokens: content.len().div_ceil(4),
            content,
            content_truncated: !complete,
            source_spans: spans,
            relations: vec![],
        });
    }
    let mut response = SearchResponse {
        navigation: None,
        continuation_state: Some(state),
        policy: Default::default(),
        focused_files: vec![],
        context_cost: crate::model::ContextCost {
            selection_budget: max_bytes,
            ..Default::default()
        },
        query: "exact continuation".into(),
        root: root.to_string_lossy().into_owned(),
        results,
        stats: Default::default(),
        metadata: BTreeMap::new(),
        scope: r.scope,
        trace: vec![],
    };
    crate::output::finalize(
        &mut response,
        &crate::output::Representation::CompactJson,
        None,
    )?;
    crate::repository::check_cancelled()?;
    Ok(response)
}
