//! Code actions for the LeekScript LSP.
//!
//! Provides quick fixes for diagnostics, e.g. replacing deprecated `===`/`!==` with `==`/`!=`.

use std::collections::HashMap;

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, Diagnostic, NumberOrString, TextEdit, Url, WorkspaceEdit,
};

/// Build code actions for the given diagnostics in the document.
///
/// Returns quick fixes for deprecation diagnostics: `===` → `==` and `!==` → `!=`.
#[must_use]
pub fn compute_code_actions(uri: &Url, diagnostics: &[Diagnostic]) -> Vec<CodeAction> {
    let mut actions = Vec::new();
    for diag in diagnostics {
        let (title, new_text) = match &diag.code {
            Some(NumberOrString::String(code)) => match code.as_str() {
                "deprecated_strict_eq" => ("Replace with ==".to_string(), "==".to_string()),
                "deprecated_strict_neq" => ("Replace with !=".to_string(), "!=".to_string()),
                _ => continue,
            },
            _ => continue,
        };
        let edit = WorkspaceEdit {
            changes: Some(HashMap::from([(
                uri.clone(),
                vec![TextEdit {
                    range: diag.range,
                    new_text: new_text.clone(),
                }],
            )])),
            document_changes: None,
            change_annotations: None,
        };
        actions.push(CodeAction {
            title,
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diag.clone()]),
            edit: Some(edit),
            ..Default::default()
        });
    }
    actions
}
