//! Document symbols (outline) for the LeekScript LSP.
//!
//! Builds a hierarchy of symbols for the current document: classes (with methods), top-level
//! functions, and globals.

#![allow(deprecated)] // DocumentSymbol.deprecated field is deprecated in lsp_types; use tags instead

use sipha::line_index::LineIndex;
use sipha::red::SyntaxNode;
use sipha::types::IntoSyntaxKind;
use tower_lsp::lsp_types::{DocumentSymbol, DocumentSymbolResponse, Range, SymbolKind};

use crate::analysis::{class_decl_info, function_decl_info, var_decl_info, VarDeclKind};
use crate::syntax::Kind;
use crate::DocumentAnalysis;

fn is_top_level(node: &SyntaxNode, root: &SyntaxNode) -> bool {
    for anc in node.ancestors(root) {
        match anc.kind_as::<Kind>() {
            Some(Kind::NodeBlock) | Some(Kind::NodeFunctionDecl) | Some(Kind::NodeClassDecl) => {
                return false;
            }
            _ => {}
        }
    }
    true
}

fn byte_span_to_range(source: &str, line_index: &LineIndex, start: u32, end: u32) -> Range {
    let (line_start, col_start) = crate::byte_offset_to_line_col_utf16(source, line_index, start);
    let (line_end, col_end) = crate::byte_offset_to_line_col_utf16(source, line_index, end);
    Range {
        start: tower_lsp::lsp_types::Position {
            line: line_start,
            character: col_start,
        },
        end: tower_lsp::lsp_types::Position {
            line: line_end,
            character: col_end,
        },
    }
}

/// Compute document symbols (outline) for the current file.
///
/// Returns a nested list: top-level classes (with method children), functions, and globals.
#[must_use]
pub fn compute_document_symbols(analysis: &DocumentAnalysis) -> DocumentSymbolResponse {
    let root = match analysis.root.as_ref() {
        Some(r) => r,
        None => return DocumentSymbolResponse::Nested(Vec::new()),
    };
    let source = analysis.source.as_str();
    let line_index = &analysis.line_index;

    let mut symbols = Vec::new();

    // Classes (with method children)
    for node in root.find_all_nodes(Kind::NodeClassDecl.into_syntax_kind()) {
        if !is_top_level(&node, root) {
            continue;
        }
        let Some(info) = class_decl_info(&node) else {
            continue;
        };
        let r = node.text_range();
        let range = byte_span_to_range(source, line_index, r.start, r.end);
        let name_r = info.name_span;
        let selection_range = byte_span_to_range(source, line_index, name_r.start, name_r.end);
        let children: Vec<DocumentSymbol> = node
            .descendant_nodes()
            .filter(|n: &SyntaxNode| n.kind_as::<Kind>() == Some(Kind::NodeFunctionDecl))
            .filter_map(|n| {
                let info = function_decl_info(&n)?;
                let r = n.text_range();
                Some(DocumentSymbol {
                    name: info.name,
                    detail: None,
                    kind: SymbolKind::METHOD,
                    tags: None,
                    deprecated: None,
                    range: byte_span_to_range(source, line_index, r.start, r.end),
                    selection_range: byte_span_to_range(
                        source,
                        line_index,
                        info.name_span.start,
                        info.name_span.end,
                    ),
                    children: None,
                })
            })
            .collect();
        symbols.push(DocumentSymbol {
            name: info.name,
            detail: None,
            kind: SymbolKind::CLASS,
            tags: None,
            deprecated: None,
            range,
            selection_range,
            children: if children.is_empty() {
                None
            } else {
                Some(children)
            },
        });
    }

    // Top-level functions
    for node in root.find_all_nodes(Kind::NodeFunctionDecl.into_syntax_kind()) {
        if !is_top_level(&node, root) {
            continue;
        }
        let Some(info) = function_decl_info(&node) else {
            continue;
        };
        let r = node.text_range();
        symbols.push(DocumentSymbol {
            name: info.name,
            detail: None,
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            range: byte_span_to_range(source, line_index, r.start, r.end),
            selection_range: byte_span_to_range(
                source,
                line_index,
                info.name_span.start,
                info.name_span.end,
            ),
            children: None,
        });
    }

    // Globals
    for node in root.find_all_nodes(Kind::NodeVarDecl.into_syntax_kind()) {
        if !is_top_level(&node, root) {
            continue;
        }
        let Some(info) = var_decl_info(&node) else {
            continue;
        };
        if info.kind != VarDeclKind::Global {
            continue;
        }
        let r = node.text_range();
        symbols.push(DocumentSymbol {
            name: info.name,
            detail: None,
            kind: SymbolKind::VARIABLE,
            tags: None,
            deprecated: None,
            range: byte_span_to_range(source, line_index, r.start, r.end),
            selection_range: byte_span_to_range(
                source,
                line_index,
                info.name_span.start,
                info.name_span.end,
            ),
            children: None,
        });
    }

    // Sort by range start so outline order is stable (e.g. classes, then functions, then globals by position)
    symbols.sort_by(|a, b| {
        a.range
            .start
            .line
            .cmp(&b.range.start.line)
            .then_with(|| a.range.start.character.cmp(&b.range.start.character))
    });

    DocumentSymbolResponse::Nested(symbols)
}
