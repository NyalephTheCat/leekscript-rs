//! LSP-specific conversions (gated by `lsp` feature).
//!
//! Converts leekscript-rs types to [Language Server Protocol] types for use by
//! language servers (e.g. leekscript-lsp).
//!
//! [Language Server Protocol]: https://microsoft.github.io/language-server-protocol/

pub mod code_actions;
pub mod completion;
mod content_changes;
pub mod definition;
pub mod document_link;
pub mod document_symbol;
pub mod hover;
pub mod inlay_hints;
pub mod references;
pub mod rename;
pub mod semantic_tokens;
pub mod workspace_symbol;

pub use code_actions::compute_code_actions;
pub use completion::{
    compute_completion, resolve_completion_item, DATA_KEY_NAME, DATA_KEY_TYPE, DATA_KEY_URI,
};
pub use content_changes::apply_content_changes;
pub use definition::compute_definition;
pub use document_link::compute_document_links;
pub use document_symbol::compute_document_symbols;
pub use hover::compute_hover;
pub use inlay_hints::{compute_inlay_hints, InlayHintOptions};
pub use references::find_references;
pub use rename::{compute_rename, prepare_rename, RenameError};
pub use semantic_tokens::{
    compute_semantic_tokens, compute_semantic_tokens_fallback, compute_semantic_tokens_range,
    semantic_tokens_legend, semantic_tokens_provider,
};
pub use workspace_symbol::compute_workspace_symbols;

use sipha::error::{SemanticDiagnostic, Severity};
use sipha::line_index::LineIndex;
use sipha::red::SyntaxNode;
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Location, NumberOrString,
    Position, Range, TextDocumentContentChangeEvent,
};

/// Extension trait for [`DocumentAnalysis`](crate::DocumentAnalysis) providing LSP-specific methods.
/// Bring into scope with `use leekscript_rs::lsp::DocumentAnalysisLspExt` to call `lsp_diagnostics()` and `apply_changes()`.
pub trait DocumentAnalysisLspExt {
    /// Return diagnostics as LSP `Diagnostic` values (UTF-16 ranges).
    /// Pass `document_uri` when available so related locations (e.g. "first declared here") can be filled.
    fn lsp_diagnostics(&self, document_uri: Option<&str>) -> Vec<Diagnostic>;
    /// Apply LSP content changes and reparse (or full parse). Returns new source and new syntax root.
    fn apply_changes(
        &self,
        content_changes: Vec<TextDocumentContentChangeEvent>,
    ) -> (String, Option<SyntaxNode>);
}

impl DocumentAnalysisLspExt for crate::DocumentAnalysis {
    fn lsp_diagnostics(&self, document_uri: Option<&str>) -> Vec<Diagnostic> {
        self.diagnostics
            .iter()
            .map(|d| to_lsp_diagnostic(d, &self.source, &self.line_index, document_uri))
            .collect()
    }

    fn apply_changes(
        &self,
        content_changes: Vec<TextDocumentContentChangeEvent>,
    ) -> (String, Option<SyntaxNode>) {
        let (new_source, reparse_edit) = apply_content_changes(self, content_changes);
        let root = if let Some(edit) = reparse_edit {
            crate::reparse_or_parse(&self.source, self.root.as_ref(), &edit)
        } else {
            crate::parse(&new_source)
                .ok()
                .and_then(std::convert::identity)
        };
        (new_source, root)
    }
}

/// Convert one semantic diagnostic to an LSP `Diagnostic` (range in UTF-16 line:character).
///
/// Uses the plain human-readable message for the LSP `message` field so editors show
/// a short, clear description (e.g. "variable name already used in this scope") instead
/// of the full formatted line (e.g. "3:5: error [E021]: ..."). The error code is sent
/// in the `code` field so the IDE can display it separately for filtering and docs.
/// When `document_uri` is provided, [`SemanticDiagnostic::related`] locations are
/// converted to LSP `related_information` (e.g. "first declared here").
#[must_use]
pub fn to_lsp_diagnostic(
    d: &SemanticDiagnostic,
    source: &str,
    line_index: &LineIndex,
    document_uri: Option<&str>,
) -> Diagnostic {
    let (line_start, col_start) = line_index.line_col_utf16(source, d.span.start);
    let (line_end, col_end) = line_index.line_col_utf16(source, d.span.end);
    let severity = match d.severity {
        Severity::Error => Some(DiagnosticSeverity::ERROR),
        Severity::Warning => Some(DiagnosticSeverity::WARNING),
        Severity::Deprecation => Some(DiagnosticSeverity::WARNING),
        Severity::Note => Some(DiagnosticSeverity::INFORMATION),
    };
    let message = d.message.clone();
    let related_information = build_related_information(d, source, line_index, document_uri);
    Diagnostic {
        range: Range {
            start: Position {
                line: line_start,
                character: col_start,
            },
            end: Position {
                line: line_end,
                character: col_end,
            },
        },
        severity,
        code: d.code.clone().map(NumberOrString::String),
        code_description: code_description_for(d.code.as_deref()),
        source: Some("leekscript".to_string()),
        message,
        related_information,
        tags: None,
        data: None,
    }
}

/// Build LSP related information from diagnostic related locations when document URI is available.
fn build_related_information(
    d: &SemanticDiagnostic,
    source: &str,
    line_index: &LineIndex,
    document_uri: Option<&str>,
) -> Option<Vec<DiagnosticRelatedInformation>> {
    if d.related.is_empty() {
        return None;
    }
    let uri = document_uri.and_then(|s| tower_lsp::lsp_types::Url::parse(s).ok())?;
    let mut out = Vec::with_capacity(d.related.len());
    for rel in &d.related {
        let (line_start, col_start) = line_index.line_col_utf16(source, rel.span.start);
        let (line_end, col_end) = line_index.line_col_utf16(source, rel.span.end);
        out.push(DiagnosticRelatedInformation {
            location: Location::new(
                uri.clone(),
                Range {
                    start: Position {
                        line: line_start,
                        character: col_start,
                    },
                    end: Position {
                        line: line_end,
                        character: col_end,
                    },
                },
            ),
            message: rel.message.clone(),
        });
    }
    Some(out)
}

/// Optional link to documentation for an error code (e.g. E021).
fn code_description_for(code: Option<&str>) -> Option<tower_lsp::lsp_types::CodeDescription> {
    let code = code?;
    let href = tower_lsp::lsp_types::Url::parse(
        format!("https://leek-wars.github.io/leek-wars-wiki/en/errors#{code}").as_str(),
    )
    .ok()?;
    Some(tower_lsp::lsp_types::CodeDescription { href })
}

#[cfg(test)]
#[cfg(feature = "lsp")]
mod tests {
    use sipha::error::{SemanticDiagnostic, Severity};
    use sipha::line_index::LineIndex;
    use sipha::types::Span;
    use tower_lsp::lsp_types::DiagnosticSeverity;

    use super::to_lsp_diagnostic;
    use crate::AnalysisError;

    #[test]
    fn to_lsp_diagnostic_error_single_line() {
        // "var x = 1" — span of "x" is bytes 4..5
        let source = "var x = 1";
        let line_index = LineIndex::new(source.as_bytes());
        let span = Span::new(4, 5);
        let diag = AnalysisError::UnknownVariableOrFunction.at(span);
        let lsp = to_lsp_diagnostic(&diag, source, &line_index, None);

        assert_eq!(lsp.range.start.line, 0);
        assert_eq!(lsp.range.start.character, 4);
        assert_eq!(lsp.range.end.line, 0);
        assert_eq!(lsp.range.end.character, 5);
        assert_eq!(lsp.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(
            lsp.code,
            Some(tower_lsp::lsp_types::NumberOrString::String(
                "E033".to_string()
            ))
        );
        assert!(lsp.message.contains("unknown variable or function"));
        assert_eq!(lsp.source.as_deref(), Some("leekscript"));
    }

    #[test]
    fn to_lsp_diagnostic_multiline_span() {
        let source = "var a = 1;\nreturn z;\n";
        let line_index = LineIndex::new(source.as_bytes());
        // Span covering "return z" on second line (bytes 11..19; byte 10 is \n)
        let span = Span::new(11, 19);
        let diag = AnalysisError::UnknownVariableOrFunction.at(span);
        let lsp = to_lsp_diagnostic(&diag, source, &line_index, None);

        assert_eq!(lsp.range.start.line, 1);
        assert_eq!(lsp.range.start.character, 0);
        assert_eq!(lsp.range.end.line, 1);
        assert_eq!(lsp.range.end.character, 8); // "return z" = 8 chars
        assert_eq!(lsp.severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn to_lsp_diagnostic_severity_mapping() {
        let source = "x";
        let line_index = LineIndex::new(source.as_bytes());
        let span = Span::new(0, 1);

        let err = SemanticDiagnostic::error(span, "error");
        let lsp_err = to_lsp_diagnostic(&err, source, &line_index, None);
        assert_eq!(lsp_err.severity, Some(DiagnosticSeverity::ERROR));

        let warn = SemanticDiagnostic::warning(span, "warning");
        let lsp_warn = to_lsp_diagnostic(&warn, source, &line_index, None);
        assert_eq!(lsp_warn.severity, Some(DiagnosticSeverity::WARNING));

        let dep = SemanticDiagnostic::deprecation(span, "deprecated");
        let lsp_dep = to_lsp_diagnostic(&dep, source, &line_index, None);
        assert_eq!(lsp_dep.severity, Some(DiagnosticSeverity::WARNING));

        let note = SemanticDiagnostic {
            span,
            message: "note".to_string(),
            severity: Severity::Note,
            code: None,
            file_id: None,
            related: vec![],
        };
        let lsp_note = to_lsp_diagnostic(&note, source, &line_index, None);
        assert_eq!(lsp_note.severity, Some(DiagnosticSeverity::INFORMATION));
    }

    #[test]
    fn to_lsp_diagnostic_utf16_column() {
        // "é" is 1 UTF-8 byte (C3 A9) but 1 UTF-16 code unit; "x" is 1 byte and 1 UTF-16.
        // So after "é", character index should be 1 in UTF-16.
        let source = "éx";
        let line_index = LineIndex::new(source.as_bytes());
        // Span for "x" (byte 2)
        let span = Span::new(2, 3);
        let diag = SemanticDiagnostic::error(span, "bad");
        let lsp = to_lsp_diagnostic(&diag, source, &line_index, None);

        assert_eq!(lsp.range.start.line, 0);
        assert_eq!(lsp.range.start.character, 1);
        assert_eq!(lsp.range.end.line, 0);
        assert_eq!(lsp.range.end.character, 2);
    }

    #[test]
    fn to_lsp_diagnostic_from_analysis_pipeline() {
        let source = "return z;";
        let root = crate::parse(source).unwrap().expect("parse");
        let result = crate::analyze(&root);
        assert!(result.has_errors());
        let diag = &result.diagnostics[0];
        let line_index = LineIndex::new(source.as_bytes());
        let lsp = to_lsp_diagnostic(diag, source, &line_index, None);

        assert_eq!(lsp.severity, Some(DiagnosticSeverity::ERROR));
        assert!(lsp.message.contains("unknown variable or function") || lsp.message.contains("z"));
        assert_eq!(lsp.source.as_deref(), Some("leekscript"));
    }

    #[test]
    fn to_lsp_diagnostic_related_information() {
        // Duplicate variable: main diagnostic at second "a", related at first "a"
        let source = "var a = 1;\nvar a;";
        let line_index = LineIndex::new(source.as_bytes());
        let first_span = Span::new(4, 5); // first "a"
        let second_span = Span::new(13, 14); // second "a"
        let diag = AnalysisError::VariableNameUnavailable
            .at_with_related(second_span, vec![(first_span, "first declared here")]);
        let lsp_no_uri = to_lsp_diagnostic(&diag, source, &line_index, None);
        let lsp_with_uri =
            to_lsp_diagnostic(&diag, source, &line_index, Some("file:///tmp/main.leek"));

        assert!(lsp_no_uri.related_information.is_none());
        let related = lsp_with_uri.related_information.as_ref().unwrap();
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].message, "first declared here");
        assert_eq!(related[0].location.range.start.line, 0);
        assert_eq!(related[0].location.range.start.character, 4);
    }
}
