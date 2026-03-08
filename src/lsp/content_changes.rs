//! Apply LSP content changes to document state.

use sipha::line_index::LineIndex;
use tower_lsp::lsp_types::TextDocumentContentChangeEvent;

use crate::document::DocumentAnalysis;
use crate::parser::TextEdit;

/// Byte offset for an LSP (line, character). Treats (line_count, 0) as end-of-document.
fn line_col_utf16_to_byte_eof(
    source: &str,
    line_index: &LineIndex,
    line: u32,
    character: u32,
) -> Option<usize> {
    let count = line_index.line_count();
    if line == count && character == 0 {
        return Some(source.len());
    }
    line_index
        .line_col_utf16_to_byte(source, line, character)
        .map(|p| p as usize)
}

/// Ensure the document has at least (line + 1) lines by appending newlines if needed.
fn ensure_lines(current: &mut String, line_index: &mut LineIndex, line: u32) {
    let count = line_index.line_count();
    if line >= count {
        let need = (line.saturating_sub(count).saturating_add(1)) as usize;
        current.reserve(need);
        for _ in 0..need {
            current.push('\n');
        }
        *line_index = LineIndex::new(current.as_bytes());
    }
}

/// Apply LSP content changes to the current document.
///
/// Returns the new source and, when exactly one range-based edit was applied, the corresponding
/// [`TextEdit`](crate::TextEdit) for incremental reparse.
#[must_use]
pub fn apply_content_changes(
    document: &DocumentAnalysis,
    content_changes: Vec<TextDocumentContentChangeEvent>,
) -> (String, Option<TextEdit>) {
    if content_changes.is_empty() {
        return (document.source.clone(), None);
    }
    if content_changes.iter().any(|c| c.range.is_none()) {
        let new_source = content_changes
            .into_iter()
            .find(|c| c.range.is_none())
            .map(|c| c.text)
            .unwrap_or_else(|| document.source.clone());
        return (new_source, None);
    }
    let mut current = document.source.clone();
    let mut line_index = LineIndex::new(current.as_bytes());

    for change in &content_changes {
        let range = match &change.range {
            Some(r) => r,
            None => continue,
        };
        let need_line = range.start.line.max(range.end.line);
        ensure_lines(&mut current, &mut line_index, need_line);

        let start = match line_col_utf16_to_byte_eof(
            &current,
            &line_index,
            range.start.line,
            range.start.character,
        ) {
            Some(s) => s,
            None => continue,
        };
        let end = match line_col_utf16_to_byte_eof(
            &current,
            &line_index,
            range.end.line,
            range.end.character,
        ) {
            Some(e) => e,
            None => continue,
        };
        if start <= end && end <= current.len() {
            current.replace_range(start..end, &change.text);
            line_index = LineIndex::new(current.as_bytes());
        }
    }

    let single_sipha_edit = if content_changes.len() == 1 {
        let change = &content_changes[0];
        change.range.as_ref().and_then(|range| {
            let start = document.line_index.line_col_utf16_to_byte(
                &document.source,
                range.start.line,
                range.start.character,
            )?;
            let end = line_col_utf16_to_byte_eof(
                &document.source,
                &document.line_index,
                range.end.line,
                range.end.character,
            )?;
            Some(TextEdit {
                start,
                end: end as u32,
                new_text: change.text.as_bytes().to_vec(),
            })
        })
    } else {
        None
    };
    (current, single_sipha_edit)
}
