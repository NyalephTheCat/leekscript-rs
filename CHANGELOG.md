# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Documentation**
  - README: fixed sipha link to point to workspace `../sipha`; added Architecture section and Examples (validate_with_signatures).
  - `ARCHITECTURE.md`: grammar phases, analysis pipeline, document analysis (LSP), include preprocessing.
  - Example `validate_with_signatures`: parse + analyze (with optional .sig) and print diagnostics.

- **Diagnostics**
  - `AnalysisError`: module-level doc table mapping variants to LeekScript Java error codes.
  - `IncludeError::Io`: clearer messages (e.g. "file not found: path", "permission denied: path").
  - `IncludeError::CircularInclude`: optional `included_from` path to show which file caused the cycle.

- **Testing**
  - Parser edge-case tests: unterminated strings, empty/whitespace input, incomplete expressions, unclosed paren/brace, recovery partial tree.
  - Criterion benchmarks: `parse`, `analyze`, `format` for small/medium/large inputs in `benches/parse_analyze_format.rs`.

- **Type inference**
  - Index expressions: infer element type from `Array<T>` and value type from `Map<K,V>` instead of always `any`.
  - `TypeMapKey` and null-check narrowing documented in type checker.

- **API**
  - `AnalyzeOptions`: struct with `include_tree` and `signature_roots`; single entry point `analyze_with_options(program_root, options)`.
  - Formatter config file: `load_formatter_options_from_dir` and `load_formatter_options_from_file`; read `.leekfmt.toml` or `leekscript.toml` [format] section. CLI uses config as base when formatting a file.

- **Builtins**
  - Documented that only class/type names are built-in; functions and globals require .sig files (see `examples/signatures/README.md`).

- **LSP (leekscript-lsp)**
  - Comment documenting incremental reparse in `did_change`.
  - Code action: "Add global declaration for '<name>'" for E033 (unknown variable).
  - `source_text_in_range` helper in util for code actions.

- **Release**
  - `Cargo.toml`: description, license, repository, keywords, categories; feature docs for `lsp`, `transform`, `utf16`.
  - `CHANGELOG.md` (this file).
  - Version bump to 0.1.1-alpha.0.

### Changed

- Formatter options from CLI: when a config file is present in the input file’s directory, its indent/brace/semicolon style are used when the corresponding CLI flags are left at their defaults.
