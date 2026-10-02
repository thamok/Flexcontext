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
        if file.language == Language::Vue {
            return self.extract_vue(file, next_id);
        }
        let parsed = crate::language::parser_source(file.language, &file.source);
        let tree = self.parse(file.language, &parsed)?;
        collect_file(tree.root_node(), file, next_id, file.language)
    }

    fn parse(&mut self, language: Language, source: &str) -> Result<tree_sitter::Tree> {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.parsers.entry(language) {
            let mut parser = Parser::new();
            configure_parser(&mut parser, language)
                .map_err(|error| anyhow!("failed to load {language:?} grammar: {error}"))?;
            entry.insert(parser);
        }
        self.parsers
            .get_mut(&language)
            .expect("parser was inserted")
            .parse(source, None)
            .ok_or_else(|| anyhow!("Tree-sitter cancelled parsing"))
    }

    fn extract_vue(&mut self, file: &SourceFile, next_id: &mut usize) -> Result<Vec<Symbol>> {
        let tree = self.parse(Language::Vue, &file.source)?;
        let mut units = Vec::new();
        let mut cursor = tree.root_node().walk();
        for element in tree.root_node().named_children(&mut cursor) {
            if element.kind() == "script_element" {
                let mut cursor = element.walk();
                let children: Vec<_> = element.named_children(&mut cursor).collect();
                let Some(tag) = children.iter().find(|n| n.kind() == "start_tag") else {
                    continue;
                };
                let Some(body) = children.iter().find(|n| n.kind() == "raw_text") else {
                    continue;
                };
                let mut language = Language::JavaScript;
                let mut external = false;
                let mut attrs = tag.walk();
                for attr in tag
                    .named_children(&mut attrs)
                    .filter(|n| n.kind() == "attribute")
                {
                    let key = attr
                        .named_child(0)
                        .and_then(|n| n.utf8_text(file.source.as_bytes()).ok())
                        .unwrap_or("");
                    let value = attr
                        .named_child(1)
                        .and_then(|n| n.utf8_text(file.source.as_bytes()).ok())
                        .unwrap_or("")
                        .trim_matches(['\'', '"']);
                    if key == "src" {
                        external = true;
                    }
                    if key == "lang" {
                        language = match value {
                            "ts" | "typescript" => Language::TypeScript,
                            "tsx" => Language::Tsx,
                            "js" | "javascript" | "jsx" => Language::JavaScript,
                            _ => {
                                external = true;
                                Language::JavaScript
                            }
                        };
                    }
                }
                if external {
                    continue;
                }
                // Preserve every original byte/line offset while isolating the script.
                let mut masked = file.source.as_bytes().to_vec();
                for (i, b) in masked.iter_mut().enumerate() {
                    if !body.byte_range().contains(&i) && *b != b'\n' && *b != b'\r' {
                        *b = b' ';
                    }
                }
                let script = self.parse(language, std::str::from_utf8(&masked)?)?;
                units.extend(collect_file(script.root_node(), file, next_id, language)?);
            } else if element.kind() == "element" {
                let Some(tag) = element.named_child(0) else {
                    continue;
                };
                let is_template = tag
                    .named_child(0)
                    .is_some_and(|n| n.utf8_text(file.source.as_bytes()).ok() == Some("template"));
                if !is_template {
                    continue;
                }
                let name = format!(
                    "{}Template",
                    std::path::Path::new(&file.relative_path)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Vue")
                );
                units.push(Symbol {
                    id: *next_id,
                    path: file.relative_path.clone(),
                    language: Language::Vue,
                    normalized_name: normalize_identifier(&name),
                    name,
                    kind: "template".into(),
                    containing_symbol: None,
                    structural_depth: 0,
                    start_byte: element.start_byte(),
                    end_byte: element.end_byte(),
                    start_line: element.start_position().row + 1,
                    end_line: element.end_position().row + 1,
                    source: Arc::from(file.source.as_str()),
                    signature_range: tag.byte_range(),
                    body_range: element.byte_range(),
                    comment_ranges: Vec::new(),
                    excerpt_ranges: Vec::new(),
                    imports: Arc::from([]),
                    identifiers: crate::lexical::identifier_tokens(
                        &file.source[element.byte_range()],
                    ),
                    type_references: Vec::new(),
                    calls: Vec::new(),
                });
                *next_id += 1;
            }
        }
        let shared_source = Arc::from(file.source.as_str());
        let imports: BTreeSet<String> = units
            .iter()
            .flat_map(|s| s.imports.iter().cloned())
            .collect();
        let imports: Arc<[String]> = imports.into_iter().collect::<Vec<_>>().into();
        for unit in &mut units {
            unit.source = Arc::clone(&shared_source);
            unit.imports = imports.clone();
        }
        Ok(units)
    }
}

impl Default for SymbolExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// One traversal produces units and positional facts. Local views use range
/// lookups afterwards; nested containers never recursively rescan an AST.
fn collect_file(
    root: Node<'_>,
    file: &SourceFile,
    next_id: &mut usize,
    language: Language,
) -> Result<Vec<Symbol>> {
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
    let mut statements = Vec::new();
    let mut objc_calls = Vec::new();
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
        let kind = effective_kind(language, node, container, &source);
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
                let body = symbol_body(node, &source);
                let implicit_body =
                    if matches!(node.kind(), "class_interface" | "protocol_declaration") {
                        let mut cursor = node.walk();
                        node.named_children(&mut cursor)
                            .find(|n| {
                                matches!(
                                    n.kind(),
                                    "method_declaration" | "property_declaration" | "declaration"
                                )
                            })
                            .map(|n| n.start_byte()..node.end_byte())
                    } else {
                        None
                    };
                let body_range = body
                    .map(|b| b.byte_range())
                    .or(implicit_body)
                    .unwrap_or(0..0);
                let signature_end = if body_range.is_empty() {
                    node.end_byte()
                } else {
                    body_range.start
                };
                let mut comment_ranges = Vec::new();
                if start < content_node.start_byte() {
                    comment_ranges.push(start..content_node.start_byte());
                }
                if language == Language::Python
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
                    language,
                    normalized_name: normalize_identifier(&name),
                    name,
                    kind: kind.into(),
                    containing_symbol: receiver_name(language, node, &source).or_else(|| {
                        container
                            .map(|n| symbol_name(n, "container", &source))
                            .filter(|s| !s.is_empty())
                    }),
                    structural_depth: depth,
                    start_byte: start,
                    end_byte: content_node.end_byte(),
                    start_line: line_starts.partition_point(|&offset| offset <= start),
                    end_line: content_node.end_position().row + 1,
                    source: source.clone(),
                    signature_range: node.start_byte()..signature_end,
                    body_range,
                    comment_ranges,
                    excerpt_ranges: Vec::new(),
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
                let type_position = node.parent().is_some_and(|p| {
                    matches!(p.kind(), "user_type" | "base_list")
                        || ["type", "returns", "return_type"]
                            .iter()
                            .any(|field| p.child_by_field_name(field) == Some(node))
                });
                let flags = if type_position
                    || matches!(node.kind(), "type_identifier" | "namespace_identifier")
                {
                    3
                } else {
                    1
                };
                facts.push((node.start_byte(), node.end_byte(), flags, text));
            }
        }
        if node
            .parent()
            .is_some_and(|p| crate::language::is_statement_block(p.kind()))
            && !is_comment_kind(node.kind())
            && !node.has_error()
            && !node.is_missing()
        {
            statements.push(node.byte_range());
        }
        let target = match node.kind() {
            "call_expression" | "call" => node
                .child_by_field_name("function")
                .or_else(|| node.named_child(0)),
            "macro_invocation" if file.language == Language::Rust => node
                .child_by_field_name("macro")
                .or_else(|| node.named_child(0)),
            "invocation_expression" => node.child_by_field_name("function"),
            "method_invocation" => node.child_by_field_name("name"),
            "function_call" => node.child_by_field_name("name"),
            _ => None,
        };
        if let Some(target) = target
            && let Some(name) = call_name(target, &source)
        {
            facts.push((node.start_byte(), node.end_byte(), 4, name));
        }
        if node.kind() == "message_expression" {
            let selector = objc_call_selector(node, &source);
            // Store selector text owned separately below; it is not a contiguous source slice.
            if !selector.is_empty() {
                objc_calls.push((node.byte_range(), selector));
            }
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
    statements.sort_by_key(|range| range.start);
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
            symbol.calls.extend(
                objc_calls
                    .iter()
                    .filter(|(r, _)| range.start <= r.start && r.end <= range.end)
                    .map(|(_, n)| n.clone()),
            );
            let first = statements.partition_point(|r| r.start < symbol.body_range.start);
            let end = statements.partition_point(|r| r.start < symbol.body_range.end);
            symbol.excerpt_ranges = statements[first..end]
                .iter()
                .filter(|r| r.end <= symbol.body_range.end)
                .cloned()
                .collect();
            symbol
        })
        .collect())
}

fn symbol_name(node: Node<'_>, kind: &str, source: &str) -> String {
    symbol_name_at_depth(node, kind, source, 0)
}

fn symbol_name_at_depth(node: Node<'_>, kind: &str, source: &str, depth: usize) -> String {
    if depth >= 64 {
        return String::new();
    }
    if matches!(node.kind(), "method_definition" | "method_declaration")
        && node.child_by_field_name("name").is_none()
        && node.child_by_field_name("signature").is_none()
    {
        let mut cursor = node.walk();
        let keywords: Vec<_> = node
            .named_children(&mut cursor)
            .filter(|n| n.kind() == "keyword_declarator")
            .collect();
        if !keywords.is_empty() {
            return keywords
                .iter()
                .filter_map(|n| n.named_child(0))
                .filter_map(|n| n.utf8_text(source.as_bytes()).ok())
                .map(|s| format!("{s}:"))
                .collect();
        }
        let mut cursor = node.walk();
        let names: Vec<_> = node
            .named_children(&mut cursor)
            .filter(|n| n.kind() == "identifier")
            .collect();
        if names.iter().any(|n| {
            source[n.end_byte()..node.end_byte()]
                .trim_start()
                .starts_with(':')
        }) {
            return names
                .iter()
                .filter_map(|n| n.utf8_text(source.as_bytes()).ok())
                .map(|s| format!("{s}:"))
                .collect();
        }
        if let Some(name) = names.first() {
            return name
                .utf8_text(source.as_bytes())
                .unwrap_or_default()
                .to_owned();
        }
    }
    if let Some(signature) = node.child_by_field_name("signature") {
        return symbol_name_at_depth(signature, kind, source, depth + 1);
    }
    // Declarators bind the symbol; return/field types do not name it.
    for field in ["name", "declarator"] {
        if let Some(name) = node.child_by_field_name(field) {
            if name.child_by_field_name("declarator").is_some() {
                return symbol_name_at_depth(name, kind, source, depth + 1);
            }
            if matches!(
                name.kind(),
                "qualified_identifier" | "dot_index_expression" | "method_index_expression"
            ) {
                return last_identifier(name, source).unwrap_or_default().to_owned();
            }
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
    if node.kind() == "property_declaration"
        && let Some(binding) = find_node(node, &["field_identifier"])
    {
        return binding
            .utf8_text(source.as_bytes())
            .unwrap_or_default()
            .to_owned();
    }
    if matches!(
        node.kind(),
        "class_interface" | "class_implementation" | "protocol_declaration"
    ) {
        let mut cursor = node.walk();
        if let Some(name) = node
            .named_children(&mut cursor)
            .find(|n| n.kind() == "identifier")
        {
            return name
                .utf8_text(source.as_bytes())
                .unwrap_or_default()
                .to_owned();
        }
    }
    // Field/local declarations put their type or attributes before their binding.
    if matches!(
        node.kind(),
        "lexical_declaration"
            | "variable_declaration"
            | "field_declaration"
            | "top_level_variable_declaration"
            | "property_declaration"
            | "method_signature"
            | "declaration"
    ) && let Some(binding) = find_node(
        node,
        &[
            "variable_declarator",
            "variable_declaration",
            "initialized_identifier",
            "function_signature",
        ],
    ) {
        return symbol_name_at_depth(binding, kind, source, depth + 1);
    }
    if let Some(name) = node.child_by_field_name("type") {
        return first_identifier(name, source).unwrap_or_default();
    }
    first_identifier(node, source).unwrap_or_default()
}

fn find_node<'a>(node: Node<'a>, kinds: &[&str]) -> Option<Node<'a>> {
    let mut stack = Vec::new();
    let mut cursor = node.walk();
    stack.extend(
        node.named_children(&mut cursor)
            .collect::<Vec<_>>()
            .into_iter()
            .rev(),
    );
    while let Some(child) = stack.pop() {
        if kinds.contains(&child.kind()) {
            return Some(child);
        }
        let mut cursor = child.walk();
        stack.extend(
            child
                .named_children(&mut cursor)
                .collect::<Vec<_>>()
                .into_iter()
                .rev(),
        );
    }
    None
}

fn callable_value<'a>(node: Node<'a>, source: &str) -> Option<Node<'a>> {
    if !matches!(
        node.kind(),
        "variable_declarator" | "lexical_declaration" | "variable_declaration"
    ) {
        return None;
    }
    let binding = if node.kind() == "variable_declarator" {
        node
    } else {
        find_node(node, &["variable_declarator"])?
    };
    let value = binding.child_by_field_name("value")?;
    match value.kind() {
        "arrow_function" | "function_expression" | "generator_function" => Some(value),
        "call_expression" => {
            // React wrappers keep the component's function as an explicit argument.
            let function = value.child_by_field_name("function")?;
            let name = function.child_by_field_name("property").unwrap_or(function);
            if matches!(
                name.utf8_text(source.as_bytes()).ok(),
                Some("memo" | "forwardRef")
            ) {
                return value.child_by_field_name("arguments").and_then(|args| {
                    let mut cursor = args.walk();
                    args.named_children(&mut cursor)
                        .find(|n| matches!(n.kind(), "arrow_function" | "function_expression"))
                });
            }
            None
        }
        _ => None,
    }
}

fn effective_kind(
    language: Language,
    node: Node<'_>,
    container: Option<Node<'_>>,
    source: &str,
) -> Option<&'static str> {
    let kind = structural_kind(language, node.kind(), container.map(|n| n.kind()));
    let binding_kind = || {
        // Local closures remain declarations for the established broad-query
        // suppression; top-level components/hooks are structural functions.
        let local = container.is_some_and(|p| {
            matches!(
                p.kind(),
                "function_declaration" | "generator_function_declaration" | "method_definition"
            ) || callable_value(p, source).is_some()
        });
        if callable_value(node, source).is_some() && !local {
            "function"
        } else {
            "declaration"
        }
    };
    if matches!(
        language,
        Language::TypeScript | Language::Tsx | Language::JavaScript
    ) {
        if matches!(node.kind(), "lexical_declaration" | "variable_declaration") {
            let mut cursor = node.walk();
            if node
                .named_children(&mut cursor)
                .filter(|n| n.kind() == "variable_declarator")
                .count()
                > 1
            {
                return None;
            }
            if callable_value(node, source).is_some() {
                return Some(binding_kind());
            }
        }
        if node.kind() == "variable_declarator"
            && node.parent().is_some_and(|p| {
                let mut cursor = p.walk();
                p.named_children(&mut cursor)
                    .filter(|n| n.kind() == "variable_declarator")
                    .count()
                    > 1
            })
        {
            return Some(binding_kind());
        }
    }
    if language == Language::Go && node.kind() == "type_spec" {
        return Some(match node.child_by_field_name("type").map(|n| n.kind()) {
            Some("struct_type") => "struct",
            Some("interface_type") => "interface",
            _ => "type",
        });
    }
    if language == Language::Lua
        && node.kind() == "function_declaration"
        && node
            .child_by_field_name("name")
            .is_some_and(|n| n.kind() == "method_index_expression")
    {
        return Some("method");
    }
    if language == Language::Lua && lua_callable(node).is_some() {
        return Some("function");
    }
    if matches!(language, Language::Cpp | Language::Cuda | Language::Metal)
        && node.kind() == "function_definition"
        && receiver_name(language, node, source).is_some()
    {
        return Some("method");
    }
    kind
}

fn lua_callable(node: Node<'_>) -> Option<Node<'_>> {
    let assignment = match node.kind() {
        "variable_declaration" => node.named_child(0)?,
        "assignment_statement"
            if node
                .parent()
                .is_none_or(|p| p.kind() != "variable_declaration") =>
        {
            node
        }
        "field" => {
            let value = node.child_by_field_name("value")?;
            return (value.kind() == "function_definition").then_some(value);
        }
        _ => return None,
    };
    let values = find_node(assignment, &["expression_list"])?;
    let value = values.named_child(0)?;
    (value.kind() == "function_definition").then_some(value)
}

fn symbol_body<'a>(node: Node<'a>, source: &str) -> Option<Node<'a>> {
    node.child_by_field_name("body")
        .or_else(|| callable_value(node, source).and_then(|n| n.child_by_field_name("body")))
        .or_else(|| lua_callable(node).and_then(|n| n.child_by_field_name("body")))
        .or_else(|| {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).find(|n| {
                matches!(
                    n.kind(),
                    "compound_statement"
                        | "function_body"
                        | "class_body"
                        | "enum_class_body"
                        | "implementation_definition"
                )
            })
        })
}

fn receiver_name(language: Language, node: Node<'_>, source: &str) -> Option<String> {
    if language == Language::Go {
        return node
            .child_by_field_name("receiver")
            .and_then(|r| find_node(r, &["type_identifier"]))
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .map(ToOwned::to_owned);
    }
    if language == Language::Lua {
        return node
            .child_by_field_name("name")
            .and_then(|n| n.child_by_field_name("table"))
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .map(ToOwned::to_owned);
    }
    if let Some(declarator) = node.child_by_field_name("declarator")
        && let Some(qualified) = find_node(declarator, &["qualified_identifier"])
    {
        return qualified
            .child_by_field_name("scope")
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .map(ToOwned::to_owned);
    }
    None
}

fn objc_call_selector(node: Node<'_>, source: &str) -> String {
    let mut cursor = node.walk();
    let methods: Vec<_> = node.children_by_field_name("method", &mut cursor).collect();
    let has_arguments = methods.iter().any(|n| {
        source[n.end_byte()..node.end_byte()]
            .trim_start()
            .starts_with(':')
    });
    methods
        .iter()
        .filter_map(|n| n.utf8_text(source.as_bytes()).ok())
        .map(|s| {
            if has_arguments {
                format!("{s}:")
            } else {
                s.to_owned()
            }
        })
        .collect()
}

fn import_name(node: Node<'_>, source: &str) -> String {
    if let Some(path) = node.child_by_field_name("path") {
        return path
            .utf8_text(source.as_bytes())
            .unwrap_or_default()
            .trim_matches(['<', '>', '\'', '"'])
            .to_owned();
    }
    if node.kind() == "import_statement"
        && let Some(identifier) = first_identifier(node, source)
    {
        return identifier;
    }
    let text = node.utf8_text(source.as_bytes()).unwrap_or("import").trim();
    let candidate = text
        .split(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == '-'))
        .filter(|part| !part.is_empty())
        .find(|part| {
            !matches!(
                *part,
                "use" | "using" | "include" | "import" | "from" | "as" | "require"
            )
        })
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

fn call_name<'a>(node: Node<'_>, source: &'a str) -> Option<&'a str> {
    // Generic arguments and nested calls can follow the callee in traversal order.
    // Prefer the grammar's binding field before a conservative identifier fallback.
    let mut node = node;
    for _ in 0..256 {
        if let Some(binding) = ["name", "property", "field", "method", "function"]
            .iter()
            .find_map(|field| node.child_by_field_name(field))
        {
            node = binding;
        } else {
            return last_identifier(node, source);
        }
    }
    None
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
