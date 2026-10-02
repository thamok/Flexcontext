use flexcontext::{
    SearchSession,
    model::{Language, SourceFile},
    parser::extract_symbols,
};
use std::path::{Path, PathBuf};

fn extract(language: Language, source: &str) -> Vec<flexcontext::model::Symbol> {
    extract_symbols(
        &SourceFile {
            absolute_path: PathBuf::from("test"),
            relative_path: "test".into(),
            language,
            source: source.into(),
        },
        &mut 0,
    )
    .unwrap()
}

#[test]
fn loads_all_requested_grammars_and_extracts_named_behavior() {
    let cases = [
        (
            Language::Java,
            "public class Session { public boolean validateToken(String token) { return token != null; } }",
            "validateToken",
            "method",
            Some("Session"),
        ),
        (
            Language::Apex,
            "public class Session { public static Boolean validateToken(String token) { return token != null; } }",
            "validateToken",
            "method",
            Some("Session"),
        ),
        (
            Language::Go,
            "package session\ntype Session struct {}\nfunc (s *Session) ValidateToken(token string) bool { return token != \"\" }",
            "ValidateToken",
            "method",
            Some("Session"),
        ),
        (
            Language::C,
            "typedef struct Session Session;\nSession *validate_token(Session *session) { return session; }",
            "validate_token",
            "function",
            None,
        ),
        (
            Language::Cpp,
            "class Session { public: bool validateToken(int token) { return token > 0; } };",
            "validateToken",
            "method",
            Some("Session"),
        ),
        (
            Language::CSharp,
            "public class Session { public bool ValidateToken(string token) { return token != null; } }",
            "ValidateToken",
            "method",
            Some("Session"),
        ),
        (
            Language::ObjectiveC,
            "@implementation Session\n- (BOOL)validateToken:(NSString *)token atTime:(int)time { return token != nil; }\n@end",
            "validateToken:atTime:",
            "method",
            Some("Session"),
        ),
        (
            Language::Metal,
            "#include <metal_stdlib>\nusing namespace metal;\nkernel void validateToken(device float *tokens [[buffer(0)]], uint id [[thread_position_in_grid]]) { tokens[id] = 1.0; }",
            "validateToken",
            "function",
            None,
        ),
        (
            Language::Cuda,
            "__global__ void validateToken(float *tokens) { tokens[threadIdx.x] = 1.0; }",
            "validateToken",
            "function",
            None,
        ),
        (
            Language::Kotlin,
            "class Session { fun validateToken(token: String): Boolean { return token.isNotEmpty() }\n}",
            "validateToken",
            "method",
            Some("Session"),
        ),
        (
            Language::Dart,
            "class Session { bool validateToken(String token) { return token.isNotEmpty; } }",
            "validateToken",
            "method",
            Some("Session"),
        ),
        (
            Language::Lua,
            "local Session = {}\nfunction Session:validateToken(token)\n return token ~= nil\nend",
            "validateToken",
            "method",
            Some("Session"),
        ),
        (
            Language::Tsx,
            "export const ValidateToken = (props: { token: string }) => { return <div>{props.token}</div>; };",
            "ValidateToken",
            "function",
            None,
        ),
        (
            Language::JavaScript,
            "export const ValidateToken = React.memo((props) => { return <div>{props.token}</div>; });",
            "ValidateToken",
            "function",
            None,
        ),
    ];
    for (language, source, name, kind, container) in cases {
        let mut parser = tree_sitter::Parser::new();
        flexcontext::language::configure_parser(&mut parser, language).unwrap();
        let tree = parser
            .parse(
                flexcontext::language::parser_source(language, source).as_ref(),
                None,
            )
            .unwrap();
        assert!(
            !tree.root_node().has_error(),
            "{language:?}: {}",
            tree.root_node().to_sexp()
        );
        let symbols = extract(language, source);
        let symbol = symbols
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("{language:?}: missing {name}: {symbols:#?}"));
        assert_eq!(symbol.kind, kind, "{language:?}");
        assert_eq!(
            symbol.containing_symbol.as_deref(),
            container,
            "{language:?}"
        );
        assert!(!symbol.body_range.is_empty(), "{language:?}: missing body");
        assert!(symbol.valid_ranges(), "{language:?}: invalid ranges");
        assert_eq!(
            &source[symbol.start_byte..symbol.end_byte],
            symbol.content()
        );
    }
}

#[test]
fn extensions_and_ambiguous_headers() {
    for (extension, language) in [
        ("java", Language::Java),
        ("cls", Language::Apex),
        ("trigger", Language::Apex),
        ("go", Language::Go),
        ("c", Language::C),
        ("C", Language::Cpp),
        ("cs", Language::CSharp),
        ("cxx", Language::Cpp),
        ("hpp", Language::Cpp),
        ("m", Language::ObjectiveC),
        ("mm", Language::ObjectiveC),
        ("metal", Language::Metal),
        ("cu", Language::Cuda),
        ("cuh", Language::Cuda),
        ("kt", Language::Kotlin),
        ("kts", Language::Kotlin),
        ("dart", Language::Dart),
        ("vue", Language::Vue),
        ("lua", Language::Lua),
        ("jsx", Language::JavaScript),
        ("tsx", Language::Tsx),
    ] {
        assert_eq!(
            flexcontext::language::detect_language(Path::new(&format!("file.{extension}"))),
            Some(language)
        );
    }
    for (source, language) in [
        ("int validate_token(void);", Language::C),
        ("namespace auth { class Session {}; }", Language::Cpp),
        ("@interface Session : NSObject\n@end", Language::ObjectiveC),
    ] {
        assert_eq!(
            flexcontext::language::language_for_source(Path::new("session.h"), source, Language::C),
            language
        );
    }
}

#[test]
fn java_records_interfaces_annotations_constructors_and_calls() {
    let source = "package example.auth;\nimport java.time.Instant;\n@interface Validated { String value(); }\ninterface TokenValidator { boolean validateToken(String token); }\nrecord Session(Instant expiresAt) implements TokenValidator {\n Session { if (expiresAt == null) { throw new IllegalArgumentException(); } }\n public boolean validateToken(String token) { return token != null && !hasExpired(Instant.now()); }\n boolean hasExpired(Instant now) { return !expiresAt.isAfter(now); }\n static class Builder { Session build(Instant expires) { return new Session(expires); } }\n}\n";
    let mut parser = tree_sitter::Parser::new();
    flexcontext::language::configure_parser(&mut parser, Language::Java).unwrap();
    assert!(!parser.parse(source, None).unwrap().root_node().has_error());
    let symbols = extract(Language::Java, source);
    for (name, kind, owner) in [
        ("Validated", "interface", None),
        ("value", "method", Some("Validated")),
        ("TokenValidator", "interface", None),
        ("Session", "class", None),
        ("Session", "method", Some("Session")),
        ("validateToken", "method", Some("Session")),
        ("hasExpired", "method", Some("Session")),
        ("Builder", "class", Some("Session")),
        ("build", "method", Some("Builder")),
    ] {
        assert!(
            symbols.iter().any(|s| s.name == name
                && s.kind == kind
                && s.containing_symbol.as_deref() == owner),
            "missing {name} {kind} {owner:?}: {symbols:#?}"
        );
    }
    assert!(symbols.iter().all(|s| s.valid_ranges()));
    let validate = symbols
        .iter()
        .find(|s| s.name == "validateToken" && !s.body_range.is_empty())
        .unwrap();
    assert!(validate.calls.iter().any(|n| n == "hasExpired"));
    let graph = flexcontext::relations::build_relation_graph(&symbols);
    let target = symbols.iter().find(|s| s.name == "hasExpired").unwrap();
    assert!(
        graph
            .outgoing
            .get(&validate.id)
            .unwrap()
            .iter()
            .any(|r| r.kind == "calls" && r.target_id == target.id)
    );
}

#[test]
fn objective_c_header_keeps_a_compact_container_signature() {
    let source = "@interface Session : NSObject\n- (BOOL)validateToken:(NSString *)token;\n@end";
    let symbols = extract(Language::ObjectiveC, source);
    let class = symbols.iter().find(|s| s.kind == "class").unwrap();
    assert_eq!(class.name, "Session");
    assert_eq!(class.signature(), "@interface Session : NSObject");
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "validateToken:" && s.kind == "method")
    );
}

#[test]
fn guarded_objective_c_forward_declarations_do_not_hide_cpp_headers() {
    let source = "#ifdef __OBJC__\n@class NSString;\n#endif\nnamespace WTF { class StringImpl : private Base { public: void utf8() {} }; }";
    let language =
        flexcontext::language::language_for_source(Path::new("StringImpl.h"), source, Language::C);
    assert_eq!(language, Language::Cpp);
    let symbols = extract(language, source);
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "StringImpl" && s.kind == "class")
    );
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "utf8" && s.kind == "method")
    );
}

#[test]
fn vue_scripts_and_template_keep_original_coordinates() {
    let source = "<!-- π -->\n<template><button @click=\"validateToken\">Validate token</button></template>\n<script setup lang=\"ts\">\nimport { ref } from 'vue';\nconst token = ref('');\nfunction validateToken(): boolean { return token.value.length > 0; }\n</script>\n<style>.validateToken { color: red; }</style>";
    let symbols = extract(Language::Vue, source);
    let function = symbols.iter().find(|s| s.name == "validateToken").unwrap();
    assert_eq!(function.language, Language::TypeScript);
    assert_eq!(function.start_line, 6);
    assert!(function.content().starts_with("function validateToken"));
    assert!(symbols.iter().any(|s| s.kind == "template"));
    assert!(
        symbols
            .iter()
            .all(|s| s.valid_ranges() && !s.content().contains("color: red"))
    );
}

#[test]
fn separate_react_bindings_and_methods_retain_behavior() {
    let source =
        "export const First = () => <span>first</span>, Second = () => <span>second</span>;";
    let symbols = extract(Language::Tsx, source);
    for name in ["First", "Second"] {
        let s = symbols.iter().find(|s| s.name == name).unwrap();
        assert_eq!(s.kind, "function");
        assert!(!s.body_range.is_empty());
    }
}

#[test]
fn local_arrow_bindings_keep_broad_query_suppression() {
    let source = "export function createConcurrencyLimiter(capacity: number) { const release = () => { return capacity; }; return release; }";
    let symbols = extract(Language::TypeScript, source);
    let local = symbols.iter().find(|s| s.name == "release").unwrap();
    assert_eq!(local.kind, "declaration");
    assert_eq!(
        local.containing_symbol.as_deref(),
        Some("createConcurrencyLimiter")
    );
    assert!(!local.body_range.is_empty());
}

#[test]
fn lua_assigned_functions_and_cpp_qualified_methods_have_bodies() {
    for (language, source, name) in [
        (
            Language::Lua,
            "local validate = function(token) return token ~= nil end",
            "validate",
        ),
        (
            Language::Lua,
            "validate = function(token) return token ~= nil end",
            "validate",
        ),
        (
            Language::Lua,
            "local API = { validate = function(token) return token ~= nil end }",
            "validate",
        ),
        (
            Language::Cpp,
            "bool Session::validate(int token) { return token > 0; }",
            "validate",
        ),
    ] {
        let symbols = extract(language, source);
        let symbol = symbols
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("{language:?}: {symbols:#?}"));
        assert_eq!(
            symbol.kind,
            if language == Language::Cpp {
                "method"
            } else {
                "function"
            }
        );
        assert!(!symbol.body_range.is_empty());
    }
}

#[test]
fn compact_containers_and_nested_excerpts_do_not_duplicate_source() {
    let dir = tempfile::tempdir().unwrap();
    let mut source = "class Guard { decide() {\n".to_owned();
    for n in 0..80 {
        source.push_str(&format!("const padding{n} = {n};\n"));
    }
    source.push_str("const decideNested = () => { return false; };\nreturn decideNested();\n} }\n");
    std::fs::write(dir.path().join("guard.ts"), &source).unwrap();
    let session = SearchSession::open(dir.path(), false).unwrap();
    for budget in [1024, 2048, 12000] {
        let response = session.query("decide return", budget, 10).unwrap();
        let mut seen = std::collections::BTreeSet::new();
        for result in &response.results {
            for span in &result.source_spans {
                for byte in span.start_byte..span.end_byte {
                    assert!(
                        seen.insert(byte),
                        "duplicate source byte {byte} at budget {budget}"
                    );
                }
            }
        }
        assert!(response.results.iter().any(|r| r.symbol == "decide"));
    }
}

#[test]
fn multilingual_cache_roundtrip_preserves_excerpts() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("session.go"), "package session\nfunc ValidateToken(token string) bool {\n if token == \"\" { return false }\n return true\n}").unwrap();
    let cold = SearchSession::open(dir.path(), true).unwrap();
    let warm = SearchSession::open(dir.path(), true).unwrap();
    let a = cold.query("ValidateToken", 240, 1).unwrap();
    let b = warm.query("ValidateToken", 240, 1).unwrap();
    assert_eq!(a.results[0].content, b.results[0].content);
    assert_eq!(a.results[0].source_spans, b.results[0].source_spans);
    assert!(!warm.symbols()[0].excerpt_ranges.is_empty());
}

#[test]
fn oversized_methods_in_every_language_retain_decisive_statements() {
    // Late statements exercise original-file AST slicing, including methods
    // which require an enclosing class and cannot be reparsed on their own.
    let cases = [
        (
            "Guard.java",
            "class Guard { boolean decide() {",
            "int padding{n} = {n};",
            "return false;",
            "} }",
            "decide",
        ),
        (
            "guard.cls",
            "public class Guard { public static Boolean decide() {",
            "Integer padding{n} = {n};",
            "return false;",
            "} }",
            "decide",
        ),
        (
            "guard.go",
            "package guard\nfunc Decide() bool {",
            "padding{n} := {n}\n_ = padding{n}",
            "return false",
            "}",
            "Decide",
        ),
        (
            "guard.c",
            "int decide(void) {",
            "int padding{n} = {n};",
            "return 0;",
            "}",
            "decide",
        ),
        (
            "guard.cpp",
            "class Guard { public: bool decide() {",
            "int padding{n} = {n};",
            "return false;",
            "} };",
            "decide",
        ),
        (
            "guard.cs",
            "class Guard { public bool Decide() {",
            "int padding{n} = {n};",
            "return false;",
            "} }",
            "Decide",
        ),
        (
            "guard.m",
            "@implementation Guard\n- (BOOL)decide {",
            "int padding{n} = {n};",
            "return NO;",
            "}\n@end",
            "decide",
        ),
        (
            "guard.metal",
            "kernel void decide(device float *values) {",
            "int padding{n} = {n};",
            "values[0] = 0;",
            "}",
            "decide",
        ),
        (
            "guard.cu",
            "__global__ void decide(float *values) {",
            "int padding{n} = {n};",
            "values[0] = 0;",
            "}",
            "decide",
        ),
        (
            "guard.kt",
            "class Guard { fun decide(): Boolean {",
            "val padding{n} = {n}",
            "return false",
            "}\n}",
            "decide",
        ),
        (
            "guard.dart",
            "class Guard { bool decide() {",
            "final padding{n} = {n};",
            "return false;",
            "} }",
            "decide",
        ),
        (
            "guard.lua",
            "function decide()",
            "local padding{n} = {n}",
            "return false",
            "end",
            "decide",
        ),
        (
            "guard.jsx",
            "export const Decide = () => {",
            "const padding{n} = {n};",
            "return <button disabled>denied</button>;",
            "};",
            "Decide",
        ),
        (
            "guard.tsx",
            "export const Decide = () => {",
            "const padding{n} = {n};",
            "return <button disabled>denied</button>;",
            "};",
            "Decide",
        ),
        (
            "guard.vue",
            "<template><button>Guard</button></template>\n<script setup lang=\"ts\">\nfunction decide() {",
            "const padding{n} = {n};",
            "return false;",
            "}\n</script>",
            "decide",
        ),
    ];
    for (path, prefix, padding, statement, suffix, name) in cases {
        let dir = tempfile::tempdir().unwrap();
        let mut source = format!("{prefix}\n");
        for n in 0..80 {
            source.push_str(&padding.replace("{n}", &n.to_string()));
            source.push('\n');
        }
        source.push_str(statement);
        source.push('\n');
        source.push_str(suffix);
        std::fs::write(dir.path().join(path), &source).unwrap();
        let session = SearchSession::open(dir.path(), true).unwrap();
        for budget in [1024, 2048] {
            let response = session
                .query(
                    &format!(
                        "{name} {}",
                        if path.ends_with("metal") || path.ends_with("cu") {
                            "values"
                        } else if path.ends_with("jsx") || path.ends_with("tsx") {
                            "denied"
                        } else {
                            "return"
                        }
                    ),
                    budget,
                    4,
                )
                .unwrap();
            assert!(response.stats.returned_bytes <= budget, "{path}");
            let method = response
                .results
                .iter()
                .find(|r| r.symbol == name)
                .unwrap_or_else(|| panic!("{path}: no method: {:#?}", response.results));
            assert!(method.content_truncated, "{path}");
            assert!(
                method.content.contains(statement),
                "{path}: decisive statement missing: {}",
                method.content
            );
            for span in &method.source_spans {
                assert!(
                    method
                        .content
                        .contains(&source[span.start_byte..span.end_byte]),
                    "{path}: invented span"
                );
                assert_eq!(
                    span.start_line,
                    source[..span.start_byte]
                        .bytes()
                        .filter(|&b| b == b'\n')
                        .count()
                        + 1
                );
                assert_eq!(
                    span.end_line,
                    source[..span.end_byte]
                        .bytes()
                        .filter(|&b| b == b'\n')
                        .count()
                        + 1
                );
            }
        }
    }
}

#[test]
fn expansion_corpus_is_resolved_and_retrieval_is_gated() {
    let report = flexcontext::evaluation::evaluate_suites(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benchmarks"),
        5,
        12000,
        &Default::default(),
        &["multilingual"],
    )
    .unwrap();
    assert_eq!(report.summary.queries, 43);
    assert!(report.summary.recall_at_k >= 0.95, "{:#?}", report.summary);
    assert!(report.summary.mrr >= 0.70, "{:#?}", report.summary);
    assert!(report.summary.relationship_recall.is_finite());
}

#[test]
fn java_corpus_is_resolved_and_retrieval_is_gated() {
    let report = flexcontext::evaluation::evaluate_suites(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benchmarks"),
        5,
        12000,
        &Default::default(),
        &["java"],
    )
    .unwrap();
    assert_eq!(report.summary.queries, 9);
    assert_eq!(report.summary.recall_at_k, 1.0, "{:#?}", report.summary);
    assert!(report.summary.mrr >= 0.70, "{:#?}", report.summary);
}

#[test]
fn relation_resolution_rejects_cross_language_and_noncallable_names() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("foreign.rs"),
        "fn foreign_helper() {}\nstruct NamedType;",
    )
    .unwrap();
    std::fs::write(dir.path().join("caller.go"), "package caller\nvar local_helper = 42\nfunc Run() { foreign_helper(); local_helper(); NamedType() }").unwrap();
    let session = SearchSession::open(dir.path(), false).unwrap();
    let graph = flexcontext::relations::build_relation_graph(session.symbols());
    let run = session.symbols().iter().find(|s| s.name == "Run").unwrap();
    assert!(
        graph
            .outgoing
            .get(&run.id)
            .into_iter()
            .flatten()
            .all(|r| r.kind != "calls")
    );
}
