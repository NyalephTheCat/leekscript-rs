//! Integration tests: format pipeline and round-trip.

use std::path::Path;

use leekscript_rs::formatter::{FormatterOptions, IndentStyle};
use leekscript_rs::{format, parse};

fn read_fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e))
}

#[test]
fn format_simple_fixture_round_trip() {
    let source = read_fixture("simple.leek");
    let root = parse(&source).unwrap().expect("parse");
    let options = FormatterOptions::default();
    let formatted = format(&root, &options);
    let root2 = parse(&formatted).unwrap().expect("re-parse after format");
    assert_eq!(root.kind(), root2.kind());
}

#[test]
fn format_canonical_round_trip() {
    let source = read_fixture("simple.leek");
    let root = parse(&source).unwrap().expect("parse");
    let options = FormatterOptions {
        canonical_format: true,
        indent_style: IndentStyle::Tabs,
        ..FormatterOptions::default()
    };
    let formatted = format(&root, &options);
    let root2 = parse(&formatted).unwrap().expect("re-parse after canonical format");
    assert_eq!(root.kind(), root2.kind());
    assert!(formatted.contains('\t'));
}
