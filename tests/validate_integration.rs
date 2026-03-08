//! Integration tests: parse + expand_includes + analyze (validate pipeline).

mod common;

use std::path::Path;

use leekscript_rs::document::RootSymbolKind;
use leekscript_rs::{analyze, parse, DocumentAnalysis, Severity};

#[test]
fn validate_simple_fixture() {
    let source = common::read_fixture("simple.leek");
    let root = parse(&source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        !result.has_errors(),
        "expected no errors from simple.leek: {:?}",
        result.diagnostics
    );
}

#[test]
fn validate_invalid_script_reports_errors() {
    let source = "var x = 1; return z;"; // z undefined
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.has_errors(),
        "expected errors for undefined variable"
    );
}

#[test]
fn validate_invalid_fixture_reports_errors() {
    let source = common::read_fixture("invalid_undefined.leek");
    let root = parse(&source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.has_errors(),
        "expected errors from invalid_undefined.leek: {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("unknown variable")),
        "expected 'unknown variable' in: {:?}",
        result.diagnostics
    );
}

#[test]
fn document_analysis_simple_fixture() {
    let source = common::read_fixture("simple.leek");
    let analysis = DocumentAnalysis::new(&source, None, &[], None, None);
    assert!(
        !analysis
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error),
        "expected no errors from simple.leek: {:?}",
        analysis.diagnostics
    );
    assert!(
        !analysis.scope_extents.is_empty(),
        "scope_extents should be populated"
    );
}

#[test]
fn document_analysis_with_include() {
    let source = common::read_fixture("main_with_include.leek");
    let main_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/main_with_include.leek");
    let analysis = DocumentAnalysis::new(&source, Some(main_path.as_path()), &[], None, None);
    assert!(
        !analysis
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error),
        "expected no errors with include: {:?}",
        analysis.diagnostics
    );
    assert!(
        analysis
            .definition_map
            .contains_key(&("Cell".to_string(), RootSymbolKind::Class)),
        "definition_map should contain Cell from included file: {:?}",
        analysis.definition_map.keys().collect::<Vec<_>>()
    );
}
