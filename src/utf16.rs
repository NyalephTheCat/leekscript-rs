//! UTF-16 position conversion for LSP and editor integration.
//!
//! Requires the `utf16` feature. These helpers delegate to sipha's [`LineIndex`];
//! use them with a line index from [`ParsedDoc::line_index`] or [`LineIndex::new`].

use sipha::line_index::LineIndex;

/// Convert a byte offset to (line, character) in UTF-16 code units (0-based).
/// Useful for LSP `Position` (line, character).
#[must_use]
pub fn byte_offset_to_line_col_utf16(
    source: &str,
    line_index: &LineIndex,
    byte_offset: u32,
) -> (u32, u32) {
    line_index.line_col_utf16(source, byte_offset)
}

/// Convert (line, character) in UTF-16 code units to byte offset.
///
/// Returns `None` if the line is out of range or the position is past the end of the line.
#[must_use]
pub fn line_col_utf16_to_byte(
    source: &str,
    line_index: &LineIndex,
    line: u32,
    character: u32,
) -> Option<u32> {
    line_index.line_col_utf16_to_byte(source, line, character)
}

/// Prefix of the line up to (line, character) in UTF-16, for completion prefix.
///
/// Returns `None` if the line is out of range.
#[must_use]
pub fn line_prefix_utf16(
    source: &str,
    line_index: &LineIndex,
    line: u32,
    character: u32,
) -> Option<String> {
    line_index.line_prefix_utf16(source, line, character)
}
