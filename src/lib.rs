//! # leekscript-rs
//!
//! A [LeekScript] parser and analysis library implemented with [sipha](https://docs.rs/sipha).
//!
//! ## Architecture
//!
//! The crate is layered as follows (dependencies flow downward):
//!
//! - **Core:** [`syntax`], [`types`] — token/node kinds, type system.
//! - **Parse:** [`grammar`], [`parser`], [`preprocess`] — grammar, parsing, include handling.
//! - **Analysis:** [`analysis`] — scope, validation, type checking.
//! - **Orchestration:** [`document`], [`doc_comment`] — document-level API and doc comments.
//! - **Tooling:** [`formatter`], [`visitor`], [`tree_display`], [`transform`] — formatting, visiting, display.
//! - **LSP:** [`lsp`] (and [`utf16`] when enabled) — language server and UTF-16 utilities.
//!
//! See `ARCHITECTURE.md` in the repository root for layer details and where to add new
//! features (new syntax, LSP features, etc.).
//!
//! [LeekScript]: https://leekwars.com/
//!
//! ## Utilities from sipha
//!
//! The syntax tree ([`sipha::red::SyntaxNode`]) supports [`node_at_offset`](sipha::red::SyntaxNode::node_at_offset),
//! [`token_at_offset`](sipha::red::SyntaxNode::token_at_offset), [`first_token`](sipha::red::SyntaxNode::first_token),
//! [`last_token`](sipha::red::SyntaxNode::last_token), and [`find_ancestor`](sipha::red::SyntaxNode::find_ancestor).
//! Use [`parse_to_doc`] for a single handle (source, root, line index, and those helpers).
//!
//! ## UTF-16 (optional feature)
//!
//! Enable the `utf16` feature for LSP and editor integration: [`ParsedDoc`] then provides
//! [`offset_to_line_col_utf16`](sipha::parsed_doc::ParsedDoc::offset_to_line_col_utf16),
//! [`offset_to_line_col_utf16_1based`](sipha::parsed_doc::ParsedDoc::offset_to_line_col_utf16_1based),
//! and [`span_to_utf16_range`](sipha::parsed_doc::ParsedDoc::span_to_utf16_range).

// Re-export internal crates so the public API is unchanged.
pub mod analysis {
    pub use leekscript_analysis::*;
}
pub mod document {
    pub use leekscript_document::*;
}
pub mod doc_comment {
    pub use leekscript_core::doc_comment::*;
}
pub mod formatter {
    pub use leekscript_tooling::formatter::*;
}
pub mod grammar {
    pub use leekscript_core::grammar::*;
}
pub mod parser {
    pub use leekscript_core::parser::*;
}
pub mod preprocess {
    pub use leekscript_core::preprocess::*;
}
pub mod syntax {
    pub use leekscript_core::syntax::*;
}
pub mod tree_display {
    pub use leekscript_tooling::tree_display::*;
}
pub mod types {
    pub use leekscript_core::types::*;
}
pub mod visitor {
    pub use leekscript_tooling::visitor::*;
}

pub mod signatures;

#[cfg(feature = "transform")]
pub mod transform {
    pub use leekscript_tooling::transform::*;
}

// Parsing and include preprocessing
pub use leekscript_core::{
    all_files, build_grammar, build_include_tree, build_signature_grammar, parse,
    parse_error_to_diagnostics, parse_error_to_miette, parse_expression, parse_recovering,
    parse_recovering_multi, parse_signatures, parse_to_doc, parse_tokens, program_literals,
    reparse, reparse_or_parse, IncludeError, IncludeTree, TextEdit,
};
pub use sipha::engine::RecoverMultiResult;
pub use sipha::parsed_doc::ParsedDoc;

// Formatting
pub use leekscript_tooling::formatter::{format, FormatDriver, FormatterOptions};

// Tree display
pub use leekscript_tooling::tree_display::{
    format_syntax_tree, print_syntax_tree, TreeDisplayOptions,
};

// Transform (optional)
#[cfg(feature = "transform")]
pub use leekscript_tooling::transform::{transform, ExpandAssignAdd, TransformResult, Transformer};

// Analysis (scope + validation)
pub use leekscript_analysis::{
    analyze, analyze_with_include_tree, analyze_with_options, analyze_with_signatures,
    build_scope_extents, scope_at_offset, seed_scope_from_signatures, AnalysisError,
    AnalysisResult, AnalyzeOptions, ResolvedSymbol, ScopeId, ScopeStore,
};

// Document-level analysis (single entry point for LSP)
pub use leekscript_document::{
    build_class_super, build_definition_map, decl_span_for_name_span, DocumentAnalysis,
    DocumentAnalysisOptions, RootSymbolKind,
};

// Doc comments (Doxygen-style)
pub use leekscript_core::doc_comment::{build_doc_map, parse_doc_comment};
pub use leekscript_core::DocComment;

// Types and visitor
pub use leekscript_core::{CastType, Type};
pub use leekscript_tooling::visitor::{walk, Visitor, WalkOptions, WalkResult};

// Re-export for formatting semantic diagnostics (e.g. in CLI).
pub use sipha::error::{SemanticDiagnostic, Severity};
pub use sipha::line_index::LineIndex;

// Syntax keywords and identifier validation for completion, rename, and tooling.
pub use leekscript_core::syntax::{is_valid_identifier, KEYWORDS};

#[cfg(feature = "lsp")]
pub mod lsp;
#[cfg(feature = "lsp")]
pub use lsp::{
    apply_content_changes, compute_code_actions, compute_completion, compute_definition,
    compute_document_links, compute_document_symbols, compute_hover, compute_inlay_hints,
    compute_rename, compute_semantic_tokens, compute_semantic_tokens_fallback,
    compute_semantic_tokens_range, compute_workspace_symbols, find_references, prepare_rename,
    resolve_completion_item, semantic_tokens_legend, semantic_tokens_provider, to_lsp_diagnostic,
    DocumentAnalysisLspExt, InlayHintOptions, RenameError,
};

#[cfg(feature = "utf16")]
pub mod utf16;

#[cfg(feature = "utf16")]
pub use sipha::utf16::{byte_offset_to_utf16, span_to_utf16_range, utf16_len};

#[cfg(feature = "utf16")]
pub use utf16::{byte_offset_to_line_col_utf16, line_col_utf16_to_byte, line_prefix_utf16};
