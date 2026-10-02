use std::borrow::Cow;
use std::path::Path;

use crate::model::Language;

pub fn detect_language(path: &Path) -> Option<Language> {
    let raw_extension = path.extension()?.to_str()?;
    // The conventional uppercase .C denotes C++, unlike lowercase .c.
    if raw_extension == "C" {
        return Some(Language::Cpp);
    }
    let extension = raw_extension.to_ascii_lowercase();
    match extension.as_str() {
        "rs" => Some(Language::Rust),
        "ts" | "mts" | "cts" => Some(Language::TypeScript),
        "tsx" => Some(Language::Tsx),
        "js" | "mjs" | "cjs" | "jsx" => Some(Language::JavaScript),
        "py" | "pyi" => Some(Language::Python),
        "java" => Some(Language::Java),
        "cls" | "trigger" | "apex" => Some(Language::Apex),
        "go" => Some(Language::Go),
        "c" | "h" => Some(Language::C),
        "cs" => Some(Language::CSharp),
        "cpp" | "cc" | "cxx" | "c++" | "hpp" | "hh" | "hxx" | "h++" | "ipp" | "tpp" => {
            Some(Language::Cpp)
        }
        "m" | "mm" => Some(Language::ObjectiveC),
        "metal" => Some(Language::Metal),
        "cu" | "cuh" => Some(Language::Cuda),
        "kt" | "kts" => Some(Language::Kotlin),
        "dart" => Some(Language::Dart),
        "vue" => Some(Language::Vue),
        "lua" => Some(Language::Lua),
        _ => None,
    }
}

pub fn configure_parser(
    parser: &mut tree_sitter::Parser,
    language: Language,
) -> Result<(), tree_sitter::LanguageError> {
    let grammar = match language {
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        Language::Python => tree_sitter_python::LANGUAGE.into(),
        Language::Java => tree_sitter_java::LANGUAGE.into(),
        Language::Apex => tree_sitter_sfapex::apex::LANGUAGE.into(),
        Language::Go => tree_sitter_go::LANGUAGE.into(),
        Language::C => tree_sitter_c::LANGUAGE.into(),
        Language::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
        Language::Cpp | Language::Metal => tree_sitter_cpp::LANGUAGE.into(),
        Language::ObjectiveC => tree_sitter_objc::LANGUAGE.into(),
        Language::Cuda => tree_sitter_cuda::LANGUAGE.into(),
        Language::Kotlin => tree_sitter_kotlin_ng::LANGUAGE.into(),
        Language::Dart => tree_sitter_dart::LANGUAGE.into(),
        // Vue's script islands are parsed separately using JS/TS/JSX/TSX.
        Language::Vue => tree_sitter_html::LANGUAGE.into(),
        Language::Lua => tree_sitter_lua::LANGUAGE.into(),
    };
    parser.set_language(&grammar)
}

pub fn structural_kind(
    language: Language,
    node_kind: &str,
    parent_kind: Option<&str>,
) -> Option<&'static str> {
    match language {
        Language::Rust => match node_kind {
            "function_item" if matches!(parent_kind, Some("impl_item") | Some("trait_item")) => {
                Some("method")
            }
            "function_item" => Some("function"),
            "struct_item" => Some("struct"),
            "enum_item" => Some("enum"),
            "trait_item" => Some("trait"),
            "impl_item" => Some("impl"),
            "mod_item" => Some("module"),
            "type_item" => Some("type"),
            "const_item" | "static_item" => Some("declaration"),
            "use_declaration" => Some("import"),
            _ => None,
        },
        Language::TypeScript | Language::Tsx => match node_kind {
            "function_declaration" | "generator_function_declaration" => Some("function"),
            "method_definition" | "abstract_method_signature" | "method_signature" => {
                Some("method")
            }
            "class_declaration" | "abstract_class_declaration" => Some("class"),
            "interface_declaration" => Some("interface"),
            "type_alias_declaration" => Some("type"),
            "enum_declaration" => Some("enum"),
            "lexical_declaration" | "variable_declaration" => Some("declaration"),
            "import_statement" | "import_alias" => Some("import"),
            _ => None,
        },
        Language::JavaScript => match node_kind {
            "function_declaration" | "generator_function_declaration" => Some("function"),
            "method_definition" => Some("method"),
            "class_declaration" => Some("class"),
            "lexical_declaration" | "variable_declaration" => Some("declaration"),
            "import_statement" => Some("import"),
            _ => None,
        },
        Language::Python => match node_kind {
            "function_definition" if parent_kind == Some("class_definition") => Some("method"),
            "function_definition" => Some("function"),
            "class_definition" => Some("class"),
            "import_statement" | "import_from_statement" => Some("import"),
            "expression_statement" if parent_kind == Some("module") => Some("declaration"),
            _ => None,
        },
        Language::Java => match node_kind {
            "class_declaration" | "record_declaration" => Some("class"),
            "interface_declaration" | "annotation_type_declaration" => Some("interface"),
            "enum_declaration" => Some("enum"),
            "method_declaration"
            | "constructor_declaration"
            | "compact_constructor_declaration"
            | "annotation_type_element_declaration" => Some("method"),
            "field_declaration" | "enum_constant" => Some("declaration"),
            "import_declaration" => Some("import"),
            "package_declaration" | "module_declaration" => Some("module"),
            _ => None,
        },
        Language::Apex => match node_kind {
            "class_declaration" => Some("class"),
            "interface_declaration" => Some("interface"),
            "enum_declaration" => Some("enum"),
            "method_declaration" | "constructor_declaration" => Some("method"),
            "trigger_declaration" => Some("function"),
            "field_declaration" => Some("declaration"),
            _ => None,
        },
        Language::Go => match node_kind {
            "function_declaration" => Some("function"),
            "method_declaration" | "method_elem" => Some("method"),
            "type_spec" | "type_alias" => Some("type"),
            "const_spec" | "var_spec" => Some("declaration"),
            "import_declaration" => Some("import"),
            _ => None,
        },
        Language::C | Language::Cpp | Language::Metal | Language::Cuda | Language::ObjectiveC => {
            match node_kind {
                "class_interface" => Some("class"),
                "class_implementation" => Some("impl"),
                "protocol_declaration" => Some("interface"),
                "method_definition" | "method_declaration" => Some("method"),
                "function_definition"
                    if matches!(parent_kind, Some("class_specifier" | "struct_specifier")) =>
                {
                    Some("method")
                }
                "function_definition" => Some("function"),
                "class_specifier" => Some("class"),
                "struct_specifier" | "union_specifier" => Some("struct"),
                "enum_specifier" => Some("enum"),
                "namespace_definition" => Some("module"),
                "type_definition" | "alias_declaration" => Some("type"),
                "declaration" | "field_declaration" | "property_declaration" => Some("declaration"),
                "preproc_include" | "module_import" => Some("import"),
                "preproc_function_def" => Some("function"),
                _ => None,
            }
        }
        Language::CSharp => match node_kind {
            "class_declaration" | "record_declaration" => Some("class"),
            "struct_declaration" => Some("struct"),
            "interface_declaration" => Some("interface"),
            "enum_declaration" => Some("enum"),
            "namespace_declaration" => Some("module"),
            "method_declaration" | "constructor_declaration" | "destructor_declaration" => {
                Some("method")
            }
            "local_function_statement" => Some("function"),
            "property_declaration" | "field_declaration" => Some("declaration"),
            "delegate_declaration" => Some("type"),
            "using_directive" => Some("import"),
            _ => None,
        },
        Language::Kotlin => match node_kind {
            "class_declaration" | "object_declaration" => Some("class"),
            "function_declaration"
                if matches!(
                    parent_kind,
                    Some("class_declaration" | "object_declaration")
                ) =>
            {
                Some("method")
            }
            "function_declaration" => Some("function"),
            "property_declaration" => Some("declaration"),
            "type_alias" => Some("type"),
            "import" => Some("import"),
            _ => None,
        },
        Language::Dart => match node_kind {
            "class_declaration"
            | "mixin_declaration"
            | "extension_declaration"
            | "extension_type_declaration" => Some("class"),
            "enum_declaration" => Some("enum"),
            "function_declaration"
            | "local_function_declaration"
            | "getter_declaration"
            | "setter_declaration" => Some("function"),
            "method_declaration" => Some("method"),
            "top_level_variable_declaration" => Some("declaration"),
            "type_alias" => Some("type"),
            "import_or_export" => Some("import"),
            _ => None,
        },
        Language::Lua => match node_kind {
            "function_declaration" => Some("function"),
            "variable_declaration" => Some("declaration"),
            _ => None,
        },
        Language::Vue => None,
    }
}

pub fn is_container(kind: &str) -> bool {
    matches!(
        kind,
        "class" | "struct" | "enum" | "trait" | "impl" | "interface" | "module"
    )
}

pub fn is_statement_block(kind: &str) -> bool {
    matches!(
        kind,
        "statement_block" | "block" | "compound_statement" | "statement_list" | "constructor_body"
    )
}

/// Metal is a C++ dialect. Mask its reserved shader/address-space qualifiers
/// only in the parser input; symbol content and offsets always use the original.
pub fn parser_source(language: Language, source: &str) -> Cow<'_, str> {
    if language != Language::Metal {
        return Cow::Borrowed(source);
    }
    let mut bytes = source.as_bytes().to_vec();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if bytes[i..].starts_with(b"/*") {
            i += 2;
            while i + 1 < bytes.len() && !bytes[i..].starts_with(b"*/") {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else if matches!(bytes[i], b'\'' | b'"') {
            let quote = bytes[i];
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                } else if bytes[i] == quote {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
        } else if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if matches!(
                &source[start..i],
                "kernel"
                    | "vertex"
                    | "fragment"
                    | "device"
                    | "constant"
                    | "thread"
                    | "threadgroup"
                    | "threadgroup_imageblock"
                    | "ray_data"
                    | "object_data"
            ) {
                bytes[start..i].fill(b' ');
            }
        } else {
            i += 1;
        }
    }
    Cow::Owned(String::from_utf8(bytes).expect("mask preserves UTF-8"))
}

/// .h is shared by C, C++ and Objective-C; inspect source before indexing it.
pub fn language_for_source(path: &Path, source: &str, detected: Language) -> Language {
    if path
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("h"))
    {
        if source.contains("@interface") || source.contains("@protocol") {
            return Language::ObjectiveC;
        }
        if source.contains("namespace ")
            || source.contains("template<")
            || source.contains("template <")
            || source.contains("class ")
            || source.contains("extern \"C\"")
        {
            return Language::Cpp;
        }
        // C++ headers often contain guarded @class forward declarations.
        if source.contains("@class") {
            return Language::ObjectiveC;
        }
    }
    detected
}
