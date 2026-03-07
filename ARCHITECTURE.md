# Architecture

## Grammar phases

Parsing is done in several phases, each with its own grammar and cached parse graph:

1. **Phase 1 (token stream)** — Lexer: keywords, identifiers, numbers, strings, operators, brackets, comments. Entry: `parse_tokens()`.
2. **Phase 2 (expression)** — Single expression only (e.g. for REPL or inline eval). Entry: `parse_expression()`.
3. **Phase 3/4 (program)** — List of statements; top-level includes `include`, function declarations, class declarations. Root is a single `NodeRoot` with statement children. Entry: `parse()`.

A separate **signature grammar** parses `.sig` files (function/global/class API only). Entry: `parse_signatures()`.

## Analysis pipeline

After parsing a program, semantic analysis runs in one pass over the tree with several visitors in sequence:

1. **ScopeBuilder** — Builds the scope store and scope ID sequence (for LSP: scope at offset).
2. **Validator** — Resolves identifiers, checks break/continue in loops, duplicate declarations, placement of include/function/global.
3. **TypeChecker** — Infers types, checks assignments and calls, records types in `type_map` (for hover/inlay hints).
4. **DeprecationChecker** — Emits deprecation diagnostics (e.g. `===` / `!==`).

All use the same `ScopeStore` and `scope_id_sequence`. Entry points: `analyze()`, `analyze_with_signatures()`, `analyze_with_include_tree()`.

## Document analysis (LSP)

`DocumentAnalysis` in `document.rs` is the single entry point for the language server: it runs parsing (with optional recovery), analysis (with optional include tree and signature roots), and builds:

- Diagnostics (parse + semantic)
- Scope store and scope extents
- Type map
- Definition map (name + kind → path, span)
- Doc comment map
- Class hierarchy (super types)

So one call gives everything needed for diagnostics, go-to-definition, hover, completion, etc.

## Include preprocessing

`build_include_tree()` parses the main file, collects `include("...")` paths, resolves them relative to the file’s directory, and recursively loads and parses included files. Circular includes are detected and reported. No source expansion: each file keeps its own AST; analysis seeds scopes from included files so the main file can reference their symbols.
