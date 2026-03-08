//! DocumentAnalysis and intermediate state resilience.

use leekscript_document::DocumentAnalysis;

#[test]
fn document_analysis_empty_source_does_not_panic() {
    let analysis = DocumentAnalysis::new("", None, &[], None, None);
    assert_eq!(analysis.source, "");
    assert!(
        !analysis.scope_extents.is_empty(),
        "DocumentAnalysis should always have at least root extent"
    );
}

#[test]
fn document_analysis_incomplete_syntax_does_not_panic() {
    let source = "var x = ";
    let analysis = DocumentAnalysis::new(source, None, &[], None, None);
    let _ = &analysis.source;
    let _ = &analysis.scope_extents;
    let _ = &analysis.scope_store;
}

#[test]
fn document_analysis_unclosed_brace_does_not_panic() {
    let source = "function f() { return 1; ";
    let analysis = DocumentAnalysis::new(source, None, &[], None, None);
    let _ = &analysis.diagnostics;
    let _ = &analysis.scope_extents;
}

#[test]
fn document_analysis_symbol_at_offset_no_root_returns_none() {
    let analysis = DocumentAnalysis::new("", None, &[], None, None);
    assert!(analysis.symbol_at_offset(0).is_none());
}

#[test]
fn document_analysis_type_at_offset_no_root_returns_none() {
    let analysis = DocumentAnalysis::new("", None, &[], None, None);
    assert!(analysis.type_at_offset(0).is_none());
}
