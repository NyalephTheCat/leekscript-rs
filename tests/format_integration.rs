//! Integration tests: format pipeline and round-trip.

mod common;

use leekscript_rs::formatter::{FormatterOptions, IndentStyle};
use leekscript_rs::{format, parse};

#[test]
fn format_simple_fixture_round_trip() {
    let source = common::read_fixture("simple.leek");
    let root = parse(&source).unwrap().expect("parse");
    let options = FormatterOptions::default();
    let formatted = format(&root, &options);
    let root2 = parse(&formatted).unwrap().expect("re-parse after format");
    assert_eq!(root.kind(), root2.kind());
}

#[test]
fn format_canonical_round_trip() {
    let source = common::read_fixture("simple.leek");
    let root = parse(&source).unwrap().expect("parse");
    let options = FormatterOptions {
        canonical_format: true,
        indent_style: IndentStyle::Tabs,
        ..FormatterOptions::default()
    };
    let formatted = format(&root, &options);
    let root2 = parse(&formatted)
        .unwrap()
        .expect("re-parse after canonical format");
    assert_eq!(root.kind(), root2.kind());
    assert!(formatted.contains('\t'));
}

#[test]
fn format_weird_spacing_round_trip() {
    let source = common::read_fixture("format_weird_spacing.leek");
    let root = parse(&source).unwrap().expect("parse");
    let options = FormatterOptions::default();
    let formatted = format(&root, &options);
    let root2 = parse(&formatted).unwrap().expect("re-parse after format");
    assert_eq!(root.kind(), root2.kind());
}
