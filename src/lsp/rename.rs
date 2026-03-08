//! Rename symbol for the LeekScript LSP.
//!
//! Produces a WorkspaceEdit that renames the symbol at the given position everywhere
//! it is used (respecting scope) and at its definition, including in included files.

use std::collections::HashMap;

use sipha::line_index::LineIndex;
use tower_lsp::lsp_types::{Position, Range, TextEdit, Url, WorkspaceEdit};

use crate::analysis::ResolvedSymbol;
use crate::document::RootSymbolKind;
use crate::syntax::{is_valid_identifier, Kind};
use crate::DocumentAnalysis;

use super::references::find_references;

/// Error from preparing or performing a rename.
#[derive(Debug)]
pub enum RenameError {
    /// The new name is not a valid identifier.
    InvalidName(String),
    /// The position does not refer to a renamable symbol (e.g. not an identifier or unresolved).
    NotRenamable,
}

/// Convert a path to a file URL.
fn path_to_uri(path: &std::path::Path) -> Option<Url> {
    Url::from_file_path(path).ok()
}

/// Byte span to LSP range for a given source and line index.
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

/// Return the definition location for a root-level symbol when it is in an included file.
/// Used to add an edit in that file when renaming.
fn definition_location_included_file(
    analysis: &DocumentAnalysis,
    name: &str,
    kind: RootSymbolKind,
) -> Option<(Url, Range)> {
    let (path, start, end) = analysis.definition_span_for(name, kind)?;
    if analysis.main_path.as_ref() == Some(&path) {
        return None;
    }
    let tree = analysis.include_tree.as_ref()?;
    let main_path = analysis.main_path.as_ref()?;
    let source = tree.source_for_path(main_path, &path)?;
    let line_index = LineIndex::new(source.as_bytes());
    let uri = path_to_uri(&path)?;
    let range = byte_span_to_range(source, &line_index, start, end);
    Some((uri, range))
}

/// Prepare rename: return the range of the identifier under the cursor so the client
/// can show it as the default new name and validate before applying.
///
/// Returns `None` if the position is not on a renamable identifier.
#[must_use]
pub fn prepare_rename(
    analysis: &DocumentAnalysis,
    position: Position,
    _current_document_uri: Option<&str>,
) -> Option<Range> {
    let source = analysis.source.as_str();
    let line_index = &analysis.line_index;
    let byte_offset =
        crate::line_col_utf16_to_byte(source, line_index, position.line, position.character)?;
    let root = analysis.root.as_ref()?;
    let token = root.token_at_offset(byte_offset)?;
    if token.kind_as::<Kind>() != Some(Kind::TokIdent) {
        return None;
    }
    let _symbol = analysis.symbol_at_offset(byte_offset)?;
    let range = token.text_range();
    let (line_start, col_start) =
        crate::byte_offset_to_line_col_utf16(source, line_index, range.start);
    let (line_end, col_end) = crate::byte_offset_to_line_col_utf16(source, line_index, range.end);
    Some(Range {
        start: Position {
            line: line_start,
            character: col_start,
        },
        end: Position {
            line: line_end,
            character: col_end,
        },
    })
}

/// Compute the workspace edit that renames the symbol at the given position to `new_name`.
///
/// Returns an error if the new name is invalid or the position does not refer to a renamable symbol.
pub fn compute_rename(
    analysis: &DocumentAnalysis,
    position: Position,
    current_document_uri: Option<&str>,
    new_name: &str,
) -> Result<WorkspaceEdit, RenameError> {
    if !is_valid_identifier(new_name) {
        return Err(RenameError::InvalidName(new_name.to_string()));
    }
    let source = analysis.source.as_str();
    let line_index = &analysis.line_index;
    let byte_offset =
        crate::line_col_utf16_to_byte(source, line_index, position.line, position.character)
            .ok_or(RenameError::NotRenamable)?;
    let root = analysis.root.as_ref().ok_or(RenameError::NotRenamable)?;
    let token = root
        .token_at_offset(byte_offset)
        .ok_or(RenameError::NotRenamable)?;
    if token.kind_as::<Kind>() != Some(Kind::TokIdent) {
        return Err(RenameError::NotRenamable);
    }
    let symbol = analysis
        .symbol_at_offset(byte_offset)
        .ok_or(RenameError::NotRenamable)?;
    let name = token.text().to_string();

    let mut locations = find_references(analysis, position, current_document_uri, true);

    if let ResolvedSymbol::Global(_) = &symbol {
        if let Some((uri, range)) =
            definition_location_included_file(analysis, &name, RootSymbolKind::Global)
        {
            locations.push(tower_lsp::lsp_types::Location::new(uri, range));
        }
    } else if let ResolvedSymbol::Function(_, _) = &symbol {
        if let Some((uri, range)) =
            definition_location_included_file(analysis, &name, RootSymbolKind::Function)
        {
            locations.push(tower_lsp::lsp_types::Location::new(uri, range));
        }
    } else if let ResolvedSymbol::Class(_) = &symbol {
        if let Some((uri, range)) =
            definition_location_included_file(analysis, &name, RootSymbolKind::Class)
        {
            locations.push(tower_lsp::lsp_types::Location::new(uri, range));
        }
    }

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    for loc in locations {
        changes.entry(loc.uri).or_default().push(TextEdit {
            range: loc.range,
            new_text: new_name.to_string(),
        });
    }

    Ok(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}
