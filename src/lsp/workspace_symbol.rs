//! Workspace symbols for the LeekScript LSP.
//!
//! Builds a flat list of root-level symbols (classes, functions, globals) from open documents
//! for "Go to symbol in workspace" (e.g. Ctrl+T).

use std::path::Path;

use sipha::line_index::LineIndex;
use tower_lsp::lsp_types::{Location, SymbolInformation, SymbolKind, Url};

use crate::document::RootSymbolKind;
use crate::DocumentAnalysis;

fn path_to_uri(path: &Path) -> Option<Url> {
    Url::from_file_path(path).ok()
}

fn byte_span_to_range(
    source: &str,
    line_index: &LineIndex,
    start: u32,
    end: u32,
) -> tower_lsp::lsp_types::Range {
    let (line_start, col_start) = crate::byte_offset_to_line_col_utf16(source, line_index, start);
    let (line_end, col_end) = crate::byte_offset_to_line_col_utf16(source, line_index, end);
    tower_lsp::lsp_types::Range {
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

fn root_kind_to_symbol_kind(kind: RootSymbolKind) -> SymbolKind {
    match kind {
        RootSymbolKind::Class => SymbolKind::CLASS,
        RootSymbolKind::Function => SymbolKind::FUNCTION,
        RootSymbolKind::Global => SymbolKind::VARIABLE,
    }
}

/// Compute workspace symbols from a list of document analyses, filtered by query.
///
/// Each analysis contributes root-level symbols from its definition_map (main file and included files).
/// Symbols whose name contains `query` (case-insensitive substring) are included; empty query matches all.
#[must_use]
pub fn compute_workspace_symbols(
    analyses: &[&DocumentAnalysis],
    query: &str,
) -> Vec<SymbolInformation> {
    let query_lower = query.to_lowercase();
    let mut symbols = Vec::new();
    for analysis in analyses {
        let main_path = match analysis.main_path.as_ref() {
            Some(p) => p.as_path(),
            None => continue,
        };
        for ((name, kind), (path, start, end)) in &analysis.definition_map {
            if !query_lower.is_empty() && !name.to_lowercase().contains(&query_lower) {
                continue;
            }
            let uri = match path_to_uri(path) {
                Some(u) => u,
                None => continue,
            };
            let range = if path == main_path {
                byte_span_to_range(analysis.source.as_str(), &analysis.line_index, *start, *end)
            } else if let Some(ref tree) = analysis.include_tree {
                let Some(src) = tree.source_for_path(main_path, path) else {
                    continue;
                };
                let line_index = LineIndex::new(src.as_bytes());
                byte_span_to_range(src, &line_index, *start, *end)
            } else {
                continue;
            };
            symbols.push(SymbolInformation {
                name: name.clone(),
                kind: root_kind_to_symbol_kind(*kind),
                tags: None,
                deprecated: None,
                location: Location::new(uri, range),
                container_name: None,
            });
        }
    }
    symbols
}
