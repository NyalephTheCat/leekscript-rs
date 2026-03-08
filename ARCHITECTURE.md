# Architecture

This document describes the layering and module boundaries of leekscript-rs, and where to add or change code for new features.

## Layer overview

Dependencies flow **downward**: higher layers use lower layers; lower layers do not depend on higher ones.

```
┌─────────────────────────────────────────────────────────────────┐
│  LSP (lsp, utf16) — optional feature                            │
├─────────────────────────────────────────────────────────────────┤
│  Tooling (formatter, visitor, tree_display, transform)          │
├─────────────────────────────────────────────────────────────────┤
│  API / orchestration (document, doc_comment)                    │
├─────────────────────────────────────────────────────────────────┤
│  Analysis (analysis/*)                                          │
├─────────────────────────────────────────────────────────────────┤
│  Parse (grammar, parser, preprocess)                            │
├─────────────────────────────────────────────────────────────────┤
│  Core (syntax, types)                                           │
└─────────────────────────────────────────────────────────────────┘
```

### Core

- **`syntax`** — Token kinds, AST node kinds, keyword set, identifier validation. Shared by grammar and tooling.
- **`types`** — Language type system (e.g. `Type`, `CastType`). Used by analysis and LSP.

No dependencies on other crate modules (only on `sipha` and std).

### Parse

- **`grammar`** — Grammar definition: expressions, statements, keywords, token stream, literals, signatures. Builds the grammar used by the parser.
- **`parser`** — Parsing entry points: `parse`, `parse_to_doc`, `reparse`, `parse_expression`, etc. Produces syntax trees.
- **`preprocess`** — Include handling: `build_include_tree`, `all_files`. Used by document/orchestration and CLI.

Parse layer depends on **Core** (`syntax`; `types` only if needed for signatures).

### Analysis

- **`analysis`** — Scope building, validation, type checking, node helpers, builtins, signature loading. Produces `AnalysisResult`, `ScopeStore`, definition maps, etc.

Depends on **Core** and **Parse**. No dependency on document, formatter, or LSP.

### API / orchestration

- **`document`** — Main entry point for “document-level” work: combines parsing, preprocessing, analysis, definition map, and (when used by LSP) drives semantic tokens and diagnostics. Exposes `DocumentAnalysis`.
- **`doc_comment`** — Doxygen-style doc comment parsing and doc maps. Used by document and can be used by LSP (e.g. hover).

Depends on **Core**, **Parse**, and **Analysis**.

### Tooling

- **`formatter`** — Code formatting.
- **`visitor`** — Tree walking utilities.
- **`tree_display`** — Pretty-printing of syntax trees.
- **`transform`** (optional feature) — Source-to-source transforms.

Depends on **Core** and **Parse** (formatter/document may also tie into **Analysis** for style decisions if needed).

### LSP

- **`lsp`** — LSP-specific logic: semantic tokens, content changes, diagnostics conversion. Uses `DocumentAnalysis`, `ScopeStore`, and analysis types.
- **`utf16`** (optional, with `utf16` feature) — UTF-16 offset/range conversions for LSP and editors.

Depends on **Core**, **Parse**, **Analysis**, and **API** (`document`). No dependency on formatter or CLI.

---

## Where to add or change things

| Goal | Where to work |
|------|----------------|
| **New token or AST node kind** | `syntax` (and possibly grammar keywords). |
| **New language type** | `types`. |
| **New expression or statement syntax** | `grammar` (expressions, statements, keywords, etc.), then `parser` if new entry points are needed. |
| **New keyword or literal** | `grammar` (keywords, literals) and `syntax` if it affects kinds. |
| **Scope or name resolution rules** | `analysis`: scope, scope_builder, node_helpers. |
| **New validation or diagnostic** | `analysis`: validator, and possibly type_checker or node_helpers. |
| **Type inference / type checking** | `analysis`: type_checker, types, node_helpers. |
| **Single “document” entry (e.g. for LSP)** | `document`: add or extend `DocumentAnalysis` (and options). |
| **Doc comments / Doxygen** | `doc_comment`. |
| **Formatting rules** | `formatter`. |
| **Semantic highlighting** | `lsp/semantic_tokens`. |
| **New LSP feature (hover, goto def, completion, rename)** | New module under `lsp/` (e.g. `lsp/hover.rs`), using `DocumentAnalysis`, `ScopeStore`, and existing helpers. Keep LSP-specific types and conversions inside `lsp/`. |
| **Content changes / incremental updates** | `lsp/content_changes` and `parser` (reparse, apply_content_changes). |
| **CLI behavior** | `cli` (and the `main` binary). |
| **Include expansion (alternative)** | See [Include handling: preprocess vs transform](#include-handling-preprocess-vs-transform) — could be implemented as a transform. |

---

## Include handling: preprocess vs transform

**Current (preprocess):** The `preprocess` module builds an **include tree**: parse the main file, collect `include("path")` from the AST, load and parse each included file (with circular-include detection), and return a tree of (path, source, root, includes). No source expansion: each file keeps its own AST. Analysis then **seeds scope** from included files and analyzes the main file; the main AST still contains `NodeInclude` nodes.

**Alternative (include-as-transform):** Includes could be handled as a **transformation** after parsing:

1. **Parse** the main file.
2. **Parse all included files** — same as today: resolve paths, load files, parse, repeat until no new includes; detect circular includes and fail.
3. **Link / transform** — replace each `include("path")` node in the main program with the **top-level statements** of the included file’s program root (splice the included program’s statement list in place of the include node). Result: a **single merged AST** (one program node whose statement list is main + inlined includes in order).
4. **Validation** — run scope, type-check, and validation on this single tree. No separate “seed scope from includes”; included declarations are just part of the tree.

**Benefits of the transform approach:** One tree to analyze; no special include-tree handling in analysis or document; same mental model as “include = paste statements here.”

**Considerations:** Source mapping for diagnostics and LSP (e.g. “error in `lib.leek` line 5”) would need either (a) per-node file/path metadata on the merged tree, or (b) a combined line map (path + line for each span). The existing `IncludeTree` could still be built for path/source lookup and then the transform applied to produce the merged AST for analysis.

Implementing this would mean: adding a transform (e.g. under `transform` or a dedicated `include_expand`) that takes the main root + a map path→(root, source), walks the main program, and replaces each `NodeInclude` with the children of the included program node; then either switching analysis to use that merged root (and optional file metadata) or keeping both paths behind an option.

---

## Error handling and results

- **Parser:** Returns `Result`. Fail-fast for parse errors; diagnostics can be converted via `parse_error_to_diagnostics` / `parse_error_to_miette`.
- **Analysis:** Returns `AnalysisResult` with `Vec<SemanticDiagnostic>`. Collects all diagnostics; does not fail-fast. This split (parse = fail-fast, analysis = collect all) is intentional and should be preserved when adding new checks.

---

## Crate layout (current)

The codebase is split into internal crates under `leekscript-rs/crates/`, with the main `leekscript-rs` crate re-exporting everything so the public API is unchanged:

- **`leekscript-core`** — `syntax`, `types`, `grammar`, `parser`, `preprocess`, `doc_comment`. Parsing and include handling.
- **`leekscript-analysis`** — `analysis/*`. Scope, validation, type checking.
- **`leekscript-document`** — document-level API: `DocumentAnalysis`, definition map, doc maps, `build_class_super`.
- **`leekscript-tooling`** — `formatter`, `visitor`, `tree_display`, optional `transform` (feature-gated).
- **`leekscript-rs`** (main) — Re-exports the above; also contains `lsp/`, `utf16`, and the CLI binary. No duplicate modules; consumers still depend only on `leekscript-rs`.

All crates live in the same repo. The workspace is defined at the repo root (`parsing/Cargo.toml`); `leekscript-rs` depends on the four internal crates and exposes a single, backward-compatible API.

---

## Naming conventions

- **Parsing:** `parse_*` for parsing entry points.
- **Building structures:** `build_*` (e.g. scope, definition map, include tree, grammar).
- **Analysis:** `analyze*` for analysis entry points.
- **Grammar:** `add_*` for adding rules in the grammar.

These are not enforced by tooling but are used consistently in the codebase.
