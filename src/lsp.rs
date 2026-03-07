//! LSP-specific conversions (gated by `lsp` feature).
//!
//! Converts leekscript-rs types to [Language Server Protocol] types for use by
//! language servers (e.g. leekscript-lsp).
//!
//! [Language Server Protocol]: https://microsoft.github.io/language-server-protocol/

use sipha::error::{SemanticDiagnostic, Severity};
use sipha::line_index::LineIndex;
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range,
};

/// Convert one semantic diagnostic to an LSP `Diagnostic` (range in UTF-16 line:character).
#[must_use]
pub fn to_lsp_diagnostic(
    d: &SemanticDiagnostic,
    source: &str,
    line_index: &LineIndex,
) -> Diagnostic {
    let (line_start, col_start) = line_index.line_col_utf16(source, d.span.start);
    let (line_end, col_end) = line_index.line_col_utf16(source, d.span.end);
    let severity = match d.severity {
        Severity::Error => Some(DiagnosticSeverity::ERROR),
        Severity::Warning => Some(DiagnosticSeverity::WARNING),
        Severity::Deprecation => Some(DiagnosticSeverity::WARNING),
        Severity::Note => Some(DiagnosticSeverity::INFORMATION),
    };
    let message = d.format_with_source(source.as_bytes(), line_index);
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
        code_description: None,
        source: Some("leekscript".to_string()),
        message,
        related_information: None,
        tags: None,
        data: None,
    }
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
        let lsp = to_lsp_diagnostic(&diag, source, &line_index);

        assert_eq!(lsp.range.start.line, 0);
        assert_eq!(lsp.range.start.character, 4);
        assert_eq!(lsp.range.end.line, 0);
        assert_eq!(lsp.range.end.character, 5);
        assert_eq!(lsp.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(lsp.code, Some(tower_lsp::lsp_types::NumberOrString::String("E033".to_string())));
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
        let lsp = to_lsp_diagnostic(&diag, source, &line_index);

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
        let lsp_err = to_lsp_diagnostic(&err, source, &line_index);
        assert_eq!(lsp_err.severity, Some(DiagnosticSeverity::ERROR));

        let warn = SemanticDiagnostic::warning(span, "warning");
        let lsp_warn = to_lsp_diagnostic(&warn, source, &line_index);
        assert_eq!(lsp_warn.severity, Some(DiagnosticSeverity::WARNING));

        let dep = SemanticDiagnostic::deprecation(span, "deprecated");
        let lsp_dep = to_lsp_diagnostic(&dep, source, &line_index);
        assert_eq!(lsp_dep.severity, Some(DiagnosticSeverity::WARNING));

        let note = SemanticDiagnostic {
            span,
            message: "note".to_string(),
            severity: Severity::Note,
            code: None,
            file_id: None,
        };
        let lsp_note = to_lsp_diagnostic(&note, source, &line_index);
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
        let lsp = to_lsp_diagnostic(&diag, source, &line_index);

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
        let lsp = to_lsp_diagnostic(diag, source, &line_index);

        assert_eq!(lsp.severity, Some(DiagnosticSeverity::ERROR));
        assert!(lsp.message.contains("unknown variable or function") || lsp.message.contains("z"));
        assert_eq!(lsp.source.as_deref(), Some("leekscript"));
    }
}
