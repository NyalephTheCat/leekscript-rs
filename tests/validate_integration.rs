//! Integration tests: parse + expand_includes + analyze (validate pipeline).

use std::path::Path;

use leekscript_rs::{analyze, parse};

fn read_fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e))
}

#[test]
fn validate_simple_fixture() {
    let source = read_fixture("simple.leek");
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
    assert!(result.has_errors(), "expected errors for undefined variable");
}
