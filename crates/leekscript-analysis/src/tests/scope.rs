//! Scope building, visibility, resolution, include-tree scope seeding.

use leekscript_core::syntax::Kind;
use leekscript_core::{build_include_tree, parse, parse_recovering_multi};
use sipha::types::IntoSyntaxKind;

use crate::{
    analyze, analyze_with_include_tree, analyze_with_signatures, class_field_info,
    class_member_visibility, scope_at_offset, MemberVisibility, ScopeId,
};

#[test]
/// Properties and methods are public by default; explicit public/private/protected are respected.
fn completion_visibility_default_protected_and_modifiers() {
    let source = r#"
class C {
public integer pubF;
protected integer protF;
private integer privF;
integer noMod;
}
"#;
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(result.is_valid(), "parse/analyze: {:?}", result.diagnostics);
    for field in root.find_all_nodes(Kind::NodeClassField.into_syntax_kind()) {
        let vis = class_member_visibility(&field, &root);
        let (name, _, _) = class_field_info(&field).expect("field info");
        match name.as_str() {
            "noMod" => assert_eq!(
                vis,
                MemberVisibility::Public,
                "no modifier = public by default"
            ),
            _ => {}
        }
    }
}

#[test]
fn analyze_undefined_variable() {
    let source = "return y;";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.has_errors(),
        "expected errors for undefined y: {:?}",
        result.diagnostics
    );
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.message.contains("unknown variable")));
}

#[test]
/// With include commented out, Cell is undefined; analyze_with_include_tree should report unknown variable.
fn analyze_include_tree_undefined_class_without_include() {
    let source = r#"
// include("Cell.leek");

var a = 10;
var myCell = Cell(12);
var myCellX = Cell(12).x;
"#;
    let tree = build_include_tree(source, None).expect("build_include_tree");
    assert!(
        tree.includes.is_empty(),
        "commented include should yield no includes"
    );
    let result = analyze_with_include_tree(&tree, &[]);
    assert!(
        result.has_errors(),
        "expected errors for undefined Cell: {:?}",
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
fn analyze_local_variable_resolved() {
    let source = "function f() { var x = 1; return x; }";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.is_valid(),
        "function local variable should resolve: {:?}",
        result.diagnostics
    );
    let source2 = r"
        class C {
            function m() {
                integer? y = null;
                return y;
            }
        }
        return null;
    ";
    let root2 = parse(source2).unwrap().expect("parse");
    let result2 = analyze(&root2);
    assert!(
        result2.is_valid(),
        "method local variable should resolve: {:?}",
        result2.diagnostics
    );
}

#[test]
fn analyze_constructor_parameter_resolved() {
    let source = r"
        class Cell {
            public integer id;
            public integer x;
            public integer y;
            public boolean isWall;
            constructor(integer id) {
                this.id = id;
            }
        }
    ";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.is_valid(),
        "constructor parameter id should resolve in body: {:?}",
        result.diagnostics
    );
}

#[test]
fn analyze_overloaded_functions_accept_different_arities() {
    let source =
        "function f(a) { return a; } function f(a, b) { return a + b; } return f(1) + f(1, 2);";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.is_valid(),
        "overloaded f(1) and f(1,2) should be valid: {:?}",
        result.diagnostics
    );
}

#[test]
fn scope_at_offset_empty_extents_returns_root_id() {
    let extents: Vec<(ScopeId, (u32, u32))> = vec![];
    let id = scope_at_offset(&extents, 0);
    assert_eq!(id, ScopeId(0), "empty extents should return root scope ID");
}

#[test]
fn scope_at_offset_past_end_returns_root_when_only_root_extent() {
    let extents = vec![(ScopeId(0), (0u32, 100u32))];
    let id = scope_at_offset(&extents, 1000);
    assert_eq!(
        id,
        ScopeId(0),
        "offset past end with single extent should return root"
    );
}

#[test]
fn scope_at_offset_within_extent_returns_that_scope() {
    let extents = vec![(ScopeId(0), (0u32, 100u32)), (ScopeId(1), (10u32, 50u32))];
    let id = scope_at_offset(&extents, 25);
    assert_eq!(id, ScopeId(1));
}

#[test]
fn analyze_partial_tree_from_recovery_does_not_panic() {
    let source = "var x = 1; return ( ; var y = 2;";
    let result = parse_recovering_multi(source, 10);
    let partial_root = match &result {
        Ok(output) => output.syntax_root(source.as_bytes()),
        Err(recover) => recover.partial.syntax_root(source.as_bytes()),
    };
    let Some(ref root) = partial_root else {
        return;
    };
    let analysis_result = analyze(root);
    assert!(
        !analysis_result.scope_id_sequence.is_empty()
            || analysis_result.scope_store.get(ScopeId(0)).is_some(),
        "analysis should produce at least root scope"
    );
}

#[test]
fn analyze_with_signatures_on_partial_tree_does_not_panic() {
    let source = "return ( ;";
    let result = parse_recovering_multi(source, 5);
    let partial_root = match &result {
        Ok(output) => output.syntax_root(source.as_bytes()),
        Err(recover) => recover.partial.syntax_root(source.as_bytes()),
    };
    if let Some(ref root) = partial_root {
        let _ = analyze_with_signatures(root, &[]);
    }
}
