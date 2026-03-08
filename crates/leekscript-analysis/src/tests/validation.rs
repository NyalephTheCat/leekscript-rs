//! Validation: wrong arity, break outside loop, default params, deprecation.

use leekscript_core::parse;

use crate::analyze;
use sipha::error::Severity;

#[test]
fn analyze_valid_program() {
    let source = "var x = 1; return x;";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.is_valid(),
        "expected no errors: {:?}",
        result.diagnostics
    );
}

#[test]
fn analyze_break_out_of_loop() {
    let source = "break;";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(result.has_errors());
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.message.contains("break outside")));
}

#[test]
fn analyze_wrong_arity() {
    let source = "function f(a, b) { return a + b; } return f(1);";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.has_errors(),
        "expected wrong-arity error: {:?}",
        result.diagnostics
    );
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.message.contains("wrong number of arguments")));
}

#[test]
fn analyze_user_function_with_default_params_rejected() {
    let source = "function f(a, b = 0) { return a + b; } return f(1);";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.has_errors(),
        "user function with default params should be rejected: {:?}",
        result.diagnostics
    );
    assert!(result.diagnostics.iter().any(|d| d
        .message
        .contains("optional/default parameters only allowed in standard functions or methods")));
}

#[test]
fn analyze_method_with_default_params_allowed() {
    let source = r"
        class C {
            function m(a, b = 0) { return a + b; }
        }
        var c = new C();
        return c.m(1);
    ";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.is_valid(),
        "method with default params should be allowed: {:?}",
        result.diagnostics
    );
}

#[test]
fn analyze_deprecation_strict_eq_and_neq() {
    let source = "return 1 === 2 !== true;";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(result.is_valid(), "program with === and !== still valid");
    let deprecations: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Deprecation)
        .collect();
    assert_eq!(
        deprecations.len(),
        2,
        "expected two deprecation diagnostics"
    );
    assert!(deprecations.iter().any(|d| d.message.contains("===")));
    assert!(deprecations.iter().any(|d| d.message.contains("!==")));
}
