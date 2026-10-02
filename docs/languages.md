# Supported languages

Rust, TypeScript/TSX, JavaScript/JSX and Python are supported alongside the languages below. React components and hooks use the JavaScript/TypeScript grammars. Swift is not currently supported.

| Language | Source files | Extraction |
|---|---|---|
| Rust | `.rs` | Functions, impl methods, structs, enums, traits, declarations and imports |
| TypeScript | `.ts`, `.tsx` | Functions, methods, classes, interfaces, types, variable bindings and imports |
| JavaScript | `.js`, `.jsx`, `.mjs`, `.cjs` | Functions, methods, classes, variable bindings and imports |
| Python | `.py` | Functions, methods, classes, declarations and imports |
| Java | `.java` | Packages/modules, classes, records, interfaces, annotations, enums, methods, constructors, fields and imports |
| Apex | `.cls`, `.trigger`, `.apex` | Classes, interfaces, enums, methods, constructors and triggers; Apex grammar includes SOQL/SOSL |
| Go | `.go` | Functions, receiver methods, structs, interfaces, types, imports and declarations |
| C / C++ | `.c`, `.h`; `.C`, `.cpp`, `.cc`, `.cxx`, `.c++`, `.hpp`, `.hh`, `.hxx`, `.h++`, `.ipp`, `.tpp` | Functions, declarators, types, classes, namespaces, methods and includes |
| C# | `.cs` | Namespaces, classes, records, structs, interfaces, methods, constructors, properties, fields and imports |
| Objective-C | `.m`, `.mm`, Objective-C `.h` | Interfaces, implementations, protocols, C functions and full method selectors |
| Metal | `.metal` | C++ grammar with reserved shader/address-space qualifiers masked for parsing; original source is always returned |
| CUDA | `.cu`, `.cuh` | CUDA grammar, including device/global functions and kernel launch calls |
| Kotlin | `.kt`, `.kts` | Classes, objects, functions, methods, properties, type aliases and imports |
| Dart | `.dart` | Classes, mixins, extensions, functions, methods, getters/setters, variables and imports |
| Vue | `.vue` | Inline JS/TS/JSX/TSX scripts, including `script setup`, plus template context |
| Lua | `.lua` | Named functions, table methods and local declarations |
| React | `.jsx`, `.tsx`, `.js`, `.ts` | Functions, arrow components/hooks, separate variable bindings and `memo`/`forwardRef` callbacks |

`.h` files default to C, with source-based detection of C++ and Objective-C declarations. `.mm` currently uses the Objective-C grammar; embedded C++ syntax can recover partially. Metal uses a C++ dialect adapter rather than a complete Metal compiler grammar. Vue templates are searchable source units; inline scripts report their actual scripting language and original `.vue` locations. External scripts are indexed through their own files, and unsupported script languages and styles are omitted. These are structural parsers with error recovery, not type checkers or framework resolvers.
