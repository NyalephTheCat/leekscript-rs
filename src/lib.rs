//! # leekscript-rs
//!
//! A `LeekScript` parser implemented with [sipha](https://docs.rs/sipha).
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

pub mod analysis;
pub mod document;
pub mod doc_comment;
pub mod formatter;
pub mod grammar;
pub mod parser;
pub mod preprocess;
pub mod syntax;
pub mod tree_display;
#[cfg(feature = "transform")]
pub mod transform;
pub mod types;
pub mod visitor;

// Parsing and include preprocessing
pub use grammar::{build_grammar, build_signature_grammar};
pub use preprocess::{build_include_tree, all_files, IncludeError, IncludeTree};
pub use parser::{
    parse, parse_error_to_diagnostics, parse_error_to_miette, parse_expression, parse_recovering,
    parse_recovering_multi, parse_signatures, parse_to_doc, parse_tokens, program_literals, reparse,
    TextEdit,
};
pub use sipha::engine::RecoverMultiResult;
pub use sipha::parsed_doc::ParsedDoc;

// Formatting
pub use formatter::{format, FormatDriver, FormatterOptions};

// Tree display
pub use tree_display::{format_syntax_tree, print_syntax_tree, TreeDisplayOptions};

// Transform (optional)
#[cfg(feature = "transform")]
pub use transform::{transform, ExpandAssignAdd, TransformResult, Transformer};

// Analysis (scope + validation)
pub use analysis::{
    analyze, analyze_with_options, analyze_with_include_tree, analyze_with_signatures,
    build_scope_extents, scope_at_offset, seed_scope_from_signatures,
    AnalyzeOptions, AnalysisError, AnalysisResult, ResolvedSymbol, ScopeId, ScopeStore,
};

// Document-level analysis (single entry point for LSP)
pub use document::{
    build_class_super, build_definition_map, decl_span_for_name_span, DocumentAnalysis,
    RootSymbolKind,
};

// Doc comments (Doxygen-style)
pub use doc_comment::{build_doc_map, parse_doc_comment, DocComment};

// Types and visitor
pub use types::{CastType, Type};
pub use visitor::{walk, Visitor, WalkOptions, WalkResult};

// Re-export for formatting semantic diagnostics (e.g. in CLI).
pub use sipha::error::{SemanticDiagnostic, Severity};
pub use sipha::line_index::LineIndex;

// Syntax keywords and identifier validation for completion, rename, and tooling.
pub use syntax::{is_valid_identifier, KEYWORDS};

#[cfg(feature = "lsp")]
pub mod lsp;
#[cfg(feature = "lsp")]
pub use lsp::to_lsp_diagnostic;

#[cfg(feature = "utf16")]
pub mod utf16;

#[cfg(feature = "utf16")]
pub use sipha::utf16::{byte_offset_to_utf16, span_to_utf16_range, utf16_len};

#[cfg(feature = "utf16")]
pub use utf16::{
    byte_offset_to_line_col_utf16, line_col_utf16_to_byte, line_prefix_utf16,
};
