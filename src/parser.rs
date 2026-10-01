use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use tree_sitter::{Node, Parser};

use crate::language::{configure_parser, structural_kind};
use crate::lexical::normalize_identifier;
use crate::model::{Language, SourceFile, Symbol};

pub fn extract_symbols(file: &SourceFile, next_id: &mut usize) -> Result<Vec<Symbol>> {
    SymbolExtractor::new().extract(file, next_id)
}

pub struct SymbolExtractor {
    parsers: HashMap<Language, Parser>,
}

impl SymbolExtractor {
    pub fn new() -> Self {
        Self {
            parsers: HashMap::new(),
        }
    }

    pub fn extract(&mut self, file: &SourceFile, next_id: &mut usize) -> Result<Vec<Symbol>> {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.parsers.entry(file.language)
        {
            let mut parser = Parser::new();
            configure_parser(&mut parser, file.language)
                .map_err(|error| anyhow!("failed to load {:?} grammar: {error}", file.language))?;
            entry.insert(parser);
        }
        let tree = self
            .parsers
            .get_mut(&file.language)
            .expect("parser was inserted")
            .parse(&file.source, None)
            .ok_or_else(|| anyhow!("Tree-sitter cancelled parsing {}", file.relative_path))?;
        let root = tree.root_node();
        collect_file(root, file, next_id)
    }
}

impl Default for SymbolExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// One traversal produces units and positional facts. Local views use range
/// lookups afterwards; nested containers never recursively rescan an AST.
fn collect_file(root: Node<'_>, file: &SourceFile, next_id: &mut usize) -> Result<Vec<Symbol>> {
    let source: Arc<str> = Arc::from(file.source.as_str());
    let line_starts: Vec<_> = std::iter::once(0)
        .chain(
            source
                .bytes()
                .enumerate()
                .filter_map(|(i, b)| (b == b'\n').then_some(i + 1)),
        )
        .collect();
    let mut units = Vec::new();
    let mut facts: Vec<(usize, usize, u8, &str)> = Vec::new();
    let mut imports = Vec::new();
    let mut stack = vec![(root, None::<Node<'_>>, 0)];
    let mut visited = 0usize;
    while let Some((node, container, depth)) = stack.pop() {
        visited += 1;
        anyhow::ensure!(
            visited <= 1_000_000 && depth <= 256,
            "AST complexity limit exceeded in {}",
            file.relative_path
        );
        if visited.is_multiple_of(1024) {
            crate::repository::check_cancelled()?;
        }
        let kind = structural_kind(file.language, node.kind(), container.map(|n| n.kind()));
        if kind == Some("import") {
            imports.push(node.utf8_text(source.as_bytes())?.trim().to_owned());
        }
        if let Some(kind) = kind {
            let name = symbol_name(node, kind, &source);
            if !name.is_empty() {
                let content_node = node
                    .parent()
                    .filter(|p| p.kind() == "decorated_definition")
                    .unwrap_or(node);
                let start =
                    comment_start_byte(content_node, &source).unwrap_or(content_node.start_byte());
                let body = node.child_by_field_name("body");
                let mut comment_ranges = Vec::new();
                if start < content_node.start_byte() {
                    comment_ranges.push(start..content_node.start_byte());
                }
                if file.language == Language::Python
                    && let Some(body) = body
                    && let Some(statement) = body.named_child(0)
                    && statement.kind() == "expression_statement"
                    && let Some(string) = statement.named_child(0)
                    && string.kind() == "string"
                {
                    comment_ranges.push(string.byte_range());
                }
                let symbol = Symbol {
                    id: *next_id,
                    path: file.relative_path.clone(),
                    language: file.language,
                    normalized_name: normalize_identifier(&name),
                    name,
                    kind: kind.into(),
                    containing_symbol: container
                        .map(|n| symbol_name(n, "container", &source))
                        .filter(|s| !s.is_empty()),
                    structural_depth: depth,
                    start_byte: start,
                    end_byte: content_node.end_byte(),
                    start_line: line_starts.partition_point(|&offset| offset <= start),
                    end_line: content_node.end_position().row + 1,
                    source: source.clone(),
                    signature_range: node.start_byte()
                        ..body.map_or(node.end_byte(), |b| b.start_byte()),
                    body_range: body.map_or(0..0, |b| b.byte_range()),
                    comment_ranges,
                    imports: Arc::from([]),
                    identifiers: Vec::new(),
                    type_references: Vec::new(),
                    calls: Vec::new(),
                };
                *next_id += 1;
                units.push((symbol, node.byte_range()));
            }
        }
        if is_identifier_kind(node.kind()) {
            let text = node.utf8_text(source.as_bytes())?;
            if text.len() <= 160 {
                let flags = if matches!(node.kind(), "type_identifier" | "namespace_identifier") {
                    3
                } else {
                    1
                };
                facts.push((node.start_byte(), node.end_byte(), flags, text));
            }
        }
        let target = match node.kind() {
            "call_expression" | "call" => node
                .child_by_field_name("function")
                .or_else(|| node.named_child(0)),
            "macro_invocation" if file.language == Language::Rust => node
                .child_by_field_name("macro")
                .or_else(|| node.named_child(0)),
            _ => None,
        };
        if let Some(target) = target
            && let Some(name) = last_identifier(target, &source)
        {
            facts.push((node.start_byte(), node.end_byte(), 4, name));
        }
        let next_container = if kind.is_some() {
            Some(node)
        } else {
            container
        };
        let next_depth = depth + usize::from(kind.is_some());
        let mut cursor = node.walk();
        if cursor.goto_last_child() {
            loop {
                let child = cursor.node();
                if child.is_named() {
                    stack.push((child, next_container, next_depth));
                }
                if !cursor.goto_previous_sibling() {
                    break;
                }
            }
        }
    }
    imports.sort();
    imports.dedup();
    let imports: Arc<[String]> = imports.into();
    facts.sort_by_key(|fact| fact.0);
    Ok(units
        .into_iter()
        .map(|(mut symbol, range)| {
            let first = facts.partition_point(|f| f.0 < range.start);
            let end = facts.partition_point(|f| f.0 < range.end);
            let mut identifiers = BTreeSet::new();
            let mut types = BTreeSet::new();
            let mut calls = BTreeSet::new();
            for (_, stop, flags, text) in &facts[first..end] {
                if *stop > range.end {
                    continue;
                }
                if flags & 1 != 0 {
                    identifiers.insert(*text);
                }
                if flags & 2 != 0 {
                    types.insert(*text);
                }
                if flags & 4 != 0 {
                    calls.insert(*text);
                }
            }
            symbol.imports = imports.clone();
            symbol.identifiers = identifiers.into_iter().map(ToOwned::to_owned).collect();
            symbol.type_references = types.into_iter().map(ToOwned::to_owned).collect();
            symbol.calls = calls.into_iter().map(ToOwned::to_owned).collect();
            symbol
        })
        .collect())
}

fn symbol_name(node: Node<'_>, kind: &str, source: &str) -> String {
    for field in ["name", "type", "declarator"] {
        if let Some(name) = node.child_by_field_name(field) {
            if let Some(identifier) = first_identifier(name, source) {
                return identifier;
            }
            if let Ok(text) = name.utf8_text(source.as_bytes()) {
                let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
                if !compact.is_empty() && compact.len() <= 120 {
                    return compact;
                }
            }
        }
    }
    if kind == "import" {
        return import_name(node, source);
    }
    first_identifier(node, source).unwrap_or_default()
}

fn import_name(node: Node<'_>, source: &str) -> String {
    if node.kind() == "import_statement"
        && let Some(identifier) = first_identifier(node, source)
    {
        return identifier;
    }
    let text = node.utf8_text(source.as_bytes()).unwrap_or("import").trim();
    let candidate = text
        .split(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == '-'))
        .filter(|part| !part.is_empty())
        .find(|part| !matches!(*part, "use" | "import" | "from" | "as" | "require"))
        .unwrap_or("import");
    candidate.to_owned()
}

fn first_identifier(node: Node<'_>, source: &str) -> Option<String> {
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if is_identifier_kind(node.kind()) {
            return node
                .utf8_text(source.as_bytes())
                .ok()
                .map(ToOwned::to_owned);
        }
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        stack.extend(children.into_iter().rev());
    }
    None
}

fn last_identifier<'a>(node: Node<'_>, source: &'a str) -> Option<&'a str> {
    let mut found = None;
    visit(node, &mut |child| {
        if is_identifier_kind(child.kind())
            && let Ok(text) = child.utf8_text(source.as_bytes())
        {
            found = Some(text);
        }
    });
    found
}

fn visit(node: Node<'_>, callback: &mut impl FnMut(Node<'_>)) {
    let mut cursor = node.walk();
    loop {
        if cursor.node().is_named() {
            callback(cursor.node());
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return;
            }
        }
    }
}

fn is_identifier_kind(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "type_identifier"
            | "field_identifier"
            | "property_identifier"
            | "shorthand_property_identifier_pattern"
            | "namespace_identifier"
    )
}

fn comment_start_byte(node: Node<'_>, source: &str) -> Option<usize> {
    let mut sibling = node.prev_named_sibling();
    let mut start = None;
    let mut next_start_line = node.start_position().row;
    while let Some(previous) = sibling {
        if !is_comment_kind(previous.kind())
            || next_start_line.saturating_sub(previous.end_position().row) > 2
        {
            break;
        }
        if source_slice(source, previous.end_byte(), node.start_byte())
            .is_ok_and(|gap| gap.lines().any(|line| !line.trim().is_empty()))
        {
            break;
        }
        start = Some(previous.start_byte());
        next_start_line = previous.start_position().row;
        sibling = previous.prev_named_sibling();
    }
    start
}

fn is_comment_kind(kind: &str) -> bool {
    kind == "comment" || kind.contains("comment")
}

fn source_slice(source: &str, start: usize, end: usize) -> Result<&str> {
    source
        .get(start..end)
        .with_context(|| format!("invalid Tree-sitter byte range {start}..{end}"))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn file(language: Language, source: &str) -> SourceFile {
        SourceFile {
            absolute_path: PathBuf::from("test"),
            relative_path: "test".to_owned(),
            language,
            source: source.to_owned(),
        }
    }

    #[test]
    fn extracts_rust_nested_structures_and_calls() {
        let source = "/// A session\nstruct Session;\nimpl Session { fn authenticate_user(&self) { validate_token(); } }";
        let symbols = extract_symbols(&file(Language::Rust, source), &mut 0).unwrap();
        assert!(
            symbols
                .iter()
                .any(|symbol| symbol.name == "Session" && symbol.kind == "struct")
        );
        let method = symbols
            .iter()
            .find(|symbol| symbol.name == "authenticate_user")
            .unwrap();
        assert_eq!(method.kind, "method");
        assert_eq!(method.containing_symbol.as_deref(), Some("Session"));
        assert!(method.calls.contains(&"validate_token".to_owned()));
    }

    #[test]
    fn extracts_typescript_and_python() {
        let ts = extract_symbols(
            &file(
                Language::TypeScript,
                "export class AuthService { validateToken(token: string) { return token; } }",
            ),
            &mut 0,
        )
        .unwrap();
        assert!(
            ts.iter()
                .any(|symbol| symbol.name == "validateToken" && symbol.kind == "method")
        );
        let py = extract_symbols(
            &file(
                Language::Python,
                "class UserSession:\n    def authenticate_user(self):\n        validate_token()\n",
            ),
            &mut 0,
        )
        .unwrap();
        assert!(
            py.iter()
                .any(|symbol| symbol.name == "authenticate_user" && symbol.kind == "method")
        );
    }
}
