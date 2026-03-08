//! Go to definition for the LeekScript LSP.
//!
//! Resolves the symbol at a position and returns its definition location (single file or included file).
//! When the position is inside an `include("path")` string, returns the location of the included file.

use std::path::Path;

use sipha::line_index::LineIndex;
use tower_lsp::lsp_types::{Location, Position, Range, Url};

use crate::analysis::ResolvedSymbol;
use crate::document::RootSymbolKind;
use crate::preprocess::collect_include_path_ranges;
use crate::DocumentAnalysis;

/// Convert a byte span to an LSP range using the given source and line index.
fn byte_span_to_range(source: &str, line_index: &LineIndex, start: u32, end: u32) -> Range {
    let (line_start, col_start) = crate::byte_offset_to_line_col_utf16(source, line_index, start);
    let (line_end, col_end) = crate::byte_offset_to_line_col_utf16(source, line_index, end);
    Range {
        start: Position {
            line: line_start,
            character: col_start,
        },
        end: Position {
            line: line_end,
            character: col_end,
        },
    }
}

/// Convert a path to a file URL. Returns None if the path cannot be represented as a file URL.
fn path_to_uri(path: &std::path::Path) -> Option<Url> {
    Url::from_file_path(path).ok()
}

/// When the position is inside the string of an `include("path")`, return the location of the included file (start of file).
fn definition_for_include_path(analysis: &DocumentAnalysis, byte_offset: u32) -> Option<Location> {
    let source = analysis.source.as_str();
    let ranges = collect_include_path_ranges(analysis.root.as_ref()?, source);
    let base_dir = analysis
        .main_path
        .as_ref()
        .map(std::path::PathBuf::as_path)
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."));
    for (start, end, path_str) in ranges {
        if start <= byte_offset && byte_offset <= end {
            let resolved = base_dir.join(path_str);
            let path_for_url = std::fs::canonicalize(&resolved).ok().unwrap_or(resolved);
            let uri = path_to_uri(&path_for_url)?;
            return Some(Location::new(
                uri,
                Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
                },
            ));
        }
    }
    None
}

/// Compute the definition location for the symbol at the given LSP position.
///
/// Returns `None` if the position cannot be resolved to a byte offset, there is no parse tree,
/// the token at the position is not an identifier, or the identifier does not resolve to a symbol.
///
/// * `current_document_uri`: The URI of the document containing the position (used for variable definitions in the main file).
#[must_use]
pub fn compute_definition(
    analysis: &DocumentAnalysis,
    position: Position,
    current_document_uri: Option<&str>,
) -> Option<Location> {
    let source = analysis.source.as_str();
    let line_index = &analysis.line_index;
    let byte_offset =
        crate::line_col_utf16_to_byte(source, line_index, position.line, position.character)?;
    let root = analysis.root.as_ref()?;
    let token = root.token_at_offset(byte_offset)?;
    let kind = token.kind_as::<crate::syntax::Kind>();

    // Click on include("path") string → go to the included file.
    if kind == Some(crate::syntax::Kind::TokString) {
        if let Some(loc) = definition_for_include_path(analysis, byte_offset) {
            return Some(loc);
        }
    }

    if kind != Some(crate::syntax::Kind::TokIdent) {
        return None;
    }
    let symbol = analysis.symbol_at_offset(byte_offset)?;
    let name = token.text().to_string();

    match &symbol {
        ResolvedSymbol::Variable(v) => {
            // Variable definition is always in the current (main) document.
            let uri_str = current_document_uri?;
            let uri = Url::parse(uri_str).ok()?;
            let range = byte_span_to_range(source, line_index, v.span.start, v.span.end);
            Some(Location::new(uri, range))
        }
        ResolvedSymbol::Global(_) => definition_location_for_root(
            analysis,
            &name,
            RootSymbolKind::Global,
            current_document_uri,
        ),
        ResolvedSymbol::Function(_, _) => definition_location_for_root(
            analysis,
            &name,
            RootSymbolKind::Function,
            current_document_uri,
        ),
        ResolvedSymbol::Class(_) => definition_location_for_root(
            analysis,
            &name,
            RootSymbolKind::Class,
            current_document_uri,
        ),
    }
}

/// Build a definition Location for a root-level symbol (global, function, class) using definition_map.
fn definition_location_for_root(
    analysis: &DocumentAnalysis,
    name: &str,
    kind: RootSymbolKind,
    _current_document_uri: Option<&str>,
) -> Option<Location> {
    let (path, start, end) = analysis.definition_span_for(name, kind)?;
    let uri = path_to_uri(&path)?;

    let range = if analysis.main_path.as_ref() == Some(&path) {
        byte_span_to_range(analysis.source.as_str(), &analysis.line_index, start, end)
    } else if let Some(ref tree) = analysis.include_tree {
        let main_path = analysis.main_path.as_ref()?;
        let source = tree.source_for_path(main_path, &path)?;
        let line_index = LineIndex::new(source.as_bytes());
        byte_span_to_range(source, &line_index, start, end)
    } else {
        return None;
    };

    Some(Location::new(uri, range))
}
