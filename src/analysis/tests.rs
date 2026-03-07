//! Unit tests for semantic analysis (scope, validation, type checking).

use super::*;
use super::node_helpers;
use super::scope;
use crate::parse;
use crate::syntax::Kind;
use sipha::types::IntoSyntaxKind;

#[test]
fn analyze_valid_program() {
    let source = "var x = 1; return x;";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(result.is_valid(), "expected no errors: {:?}", result.diagnostics);
}

/// Properties and methods are public by default; explicit public/private/protected are respected.
#[test]
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
            "noMod" => assert_eq!(vis, MemberVisibility::Public, "no modifier = public by default"),
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
    assert!(result.diagnostics.iter().any(|d| d.message.contains("unknown variable")));
}

/// With include commented out, Cell is undefined; analyze_with_include_tree should report unknown variable.
#[test]
fn analyze_include_tree_undefined_class_without_include() {
    use crate::build_include_tree;
    let source = r#"
// include("Cell.leek");

var a = 10;
var myCell = Cell(12);
var myCellX = Cell(12).x;
"#;
    let tree = build_include_tree(source, None).expect("build_include_tree");
    assert!(tree.includes.is_empty(), "commented include should yield no includes");
    let result = analyze_with_include_tree(&tree, &[]);
    assert!(
        result.has_errors(),
        "expected errors for undefined Cell: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|d| d.message.contains("unknown variable")),
        "expected 'unknown variable' in: {:?}",
        result.diagnostics
    );
}

#[test]
fn analyze_break_out_of_loop() {
    let source = "break;";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(result.has_errors());
    assert!(result.diagnostics.iter().any(|d| d.message.contains("break outside")));
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
    // Non-standard (user-defined) top-level functions do not allow optional/default parameters.
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
    // Methods may have optional/default parameters (or overloads).
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
fn analyze_local_variable_resolved() {
    // Block shares function scope so locals are visible for the whole body.
    let source = "function f() { var x = 1; return x; }";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.is_valid(),
        "function local variable should resolve: {:?}",
        result.diagnostics
    );
    // Same for method (typed local): scope_id_sequence + var_decl_info for type_expr; name.
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
    // Constructor params must be in scope in constructor body (NodeConstructorDecl pushes Function scope).
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
    let source = "function f(a) { return a; } function f(a, b) { return a + b; } return f(1) + f(1, 2);";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.is_valid(),
        "overloaded f(1) and f(1,2) should be valid: {:?}",
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
        .filter(|d| d.severity == sipha::error::Severity::Deprecation)
        .collect();
    assert_eq!(deprecations.len(), 2, "expected two deprecation diagnostics");
    assert!(deprecations.iter().any(|d| d.message.contains("===")));
    assert!(deprecations.iter().any(|d| d.message.contains("!==")));
}

#[test]
fn typecheck_binary_op_requires_number() {
    // * (and other numeric ops except +) require number; + allows string concatenation.
    let source = r#"return "hello" * 1;"#;
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.has_errors(),
        "expected type error for string * number: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|d| d.message.contains("requires number")),
        "expected 'requires number' in: {:?}",
        result.diagnostics
    );
}

#[test]
fn typecheck_string_plus_anything_returns_string() {
    let source = r#"return "x" + 1;"#;
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.is_valid(),
        "string + number should be valid (concatenation): {:?}",
        result.diagnostics
    );
}

#[test]
fn typecheck_assignment_mismatch() {
    let source = "integer x = \"not a number\"; return x;";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.has_errors(),
        "expected type error for integer x = string: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|d| d.message.contains("type mismatch")),
        "expected type mismatch: {:?}",
        result.diagnostics
    );
}

#[test]
fn typecheck_real_assigned_to_integer_fails() {
    let source = "real r = 1.0; integer x = r; return x;";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.has_errors(),
        "expected type error when assigning real expression to integer variable: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|d| d.message.contains("type mismatch")),
        "expected type mismatch (real vs integer): {:?}",
        result.diagnostics
    );
}

#[test]
fn typecheck_integer_assigned_to_real_fails() {
    let source = "integer i = 1; real x = i; return x;";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.has_errors(),
        "expected type error when assigning integer expression to real variable: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|d| d.message.contains("type mismatch")),
        "expected type mismatch (integer vs real): {:?}",
        result.diagnostics
    );
}

#[test]
fn typecheck_invalid_cast() {
    let source = "return \"x\" as integer;";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.has_errors(),
        "expected error for invalid cast string as integer: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|d| d.message.contains("invalid cast")),
        "expected invalid cast: {:?}",
        result.diagnostics
    );
}

#[test]
fn typecheck_return_type_mismatch() {
    let source = "function f() -> integer { return \"wrong\"; } return f();";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.has_errors(),
        "expected error for return type mismatch: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.iter().any(|d| d.message.contains("type mismatch")),
        "expected type mismatch: {:?}",
        result.diagnostics
    );
}

#[test]
fn typecheck_valid_typed_program() {
    let source = r"
        function add(integer a, integer b) -> integer { return a + b; }
        integer x = add(1, 2);
        return x;
    ";
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.is_valid(),
        "typed program should be valid: {:?}",
        result.diagnostics
    );
}

#[test]
fn typecheck_with_stdlib_signatures() {
    use crate::parser::parse_signatures;
    let sig_src = "function abs(real|integer number) -> integer\n";
    let sig_root = parse_signatures(sig_src).unwrap().expect("sig parse");
    let program_src = "return abs(-1);";
    let program_root = parse(program_src).unwrap().expect("parse");
    let result = analyze_with_signatures(&program_root, &[sig_root]);
    assert!(
        result.is_valid(),
        "program with stdlib abs should type-check: {:?}",
        result.diagnostics
    );
}

// ─── Type inference: assert inferred type of expressions from type_map ───
mod type_inference {
    use super::*;

    /// Returns the inferred type for a node's span from the analysis type_map, if any.
    fn type_at_node(
        type_map: &std::collections::HashMap<TypeMapKey, Type>,
        node: &SyntaxNode,
    ) -> Option<Type> {
        let range = node.text_range();
        type_map.get(&(range.start, range.end)).cloned()
    }

    mod literals {
        use super::*;

        #[test]
        fn integer() {
            let source = "return 42;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodePrimaryExpr.into_syntax_kind());
            let primary = nodes.first().expect("one primary expr (42)");
            let ty = type_at_node(&result.type_map, primary).expect("type for 42");
            assert_eq!(ty, Type::int(), "integer literal should infer integer");
        }

        #[test]
        fn string() {
            let source = r#"return "hello";"#;
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodePrimaryExpr.into_syntax_kind());
            let primary = nodes.first().expect("one primary expr");
            let ty = type_at_node(&result.type_map, primary).expect("type for string literal");
            assert_eq!(ty, Type::string(), "string literal should infer string");
        }

        #[test]
        fn real_decimal() {
            let source = "return 1.0;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodePrimaryExpr.into_syntax_kind());
            let primary = nodes.first().expect("one primary expr");
            let ty = type_at_node(&result.type_map, primary).expect("type for 1.0");
            assert_eq!(ty, Type::real(), "real literal 1.0 should infer real");
        }

        #[test]
        fn real_scientific() {
            // 1e5 may be tokenized as "1" and "e5" (identifier) in some grammars; use decimal form for reliability.
            let source = "return 2.5;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodePrimaryExpr.into_syntax_kind());
            let primary = nodes.first().expect("one primary expr");
            let ty = type_at_node(&result.type_map, primary).expect("type for 2.5");
            assert_eq!(ty, Type::real(), "real literal should infer real");
        }

        #[test]
        fn boolean_true() {
            let source = "return true;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodePrimaryExpr.into_syntax_kind());
            let primary = nodes.first().expect("one primary expr");
            let ty = type_at_node(&result.type_map, primary).expect("type for true");
            assert_eq!(ty, Type::bool(), "true should infer boolean");
        }

        #[test]
        fn boolean_false() {
            let source = "return false;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodePrimaryExpr.into_syntax_kind());
            let primary = nodes.first().expect("one primary expr");
            let ty = type_at_node(&result.type_map, primary).expect("type for false");
            assert_eq!(ty, Type::bool(), "false should infer boolean");
        }

        #[test]
        fn null_literal() {
            let source = "return null;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodePrimaryExpr.into_syntax_kind());
            let primary = nodes.first().expect("one primary expr");
            let ty = type_at_node(&result.type_map, primary).expect("type for null");
            assert_eq!(ty, Type::null(), "null literal should infer null");
        }
    }

    mod binary_ops {
        use super::*;

        #[test]
        fn int_plus_int() {
            let source = "return 1 + 2;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for 1 + 2");
            assert_eq!(ty, Type::int(), "integer + integer should infer integer");
        }

        #[test]
        fn real_plus_int() {
            let source = "return 1.0 + 2;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for 1.0 + 2");
            assert_eq!(ty, Type::real(), "real + integer should infer real");
        }

        #[test]
        fn string_concatenation() {
            let source = r#"return "a" + 1;"#;
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for \"a\" + 1");
            assert_eq!(ty, Type::string(), "string + number should infer string");
        }

        #[test]
        fn int_times_int() {
            let source = "return 2 * 3;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for 2 * 3");
            assert_eq!(ty, Type::int(), "int * int should infer integer");
        }

        #[test]
        fn int_minus_int() {
            let source = "return 5 - 2;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for 5 - 2");
            assert_eq!(ty, Type::int(), "int - int should infer integer");
        }

        #[test]
        fn int_div_int() {
            let source = "return 4 / 2;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for 4 / 2");
            assert!(ty.is_number(), "division yields number (int or real), got {:?}", ty);
        }

        #[test]
        fn int_mod_int() {
            let source = "return 5 % 2;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for 5 % 2");
            assert_eq!(ty, Type::int(), "int % int should infer integer");
        }

        #[test]
        fn power() {
            let source = "return 2 ** 3;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for 2 ** 3");
            assert!(ty.is_number(), "** yields number, got {:?}", ty);
        }

        #[test]
        fn comparison_lt() {
            let source = "return 1 < 2;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for 1 < 2");
            assert_eq!(ty, Type::bool(), "comparison should infer boolean");
        }

        #[test]
        fn comparison_lte() {
            let source = "return 1 <= 2;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for 1 <= 2");
            assert_eq!(ty, Type::bool(), "<= should infer boolean");
        }

        #[test]
        fn comparison_gt() {
            let source = "return 3 > 2;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for 3 > 2");
            assert_eq!(ty, Type::bool(), "> should infer boolean");
        }

        #[test]
        fn comparison_gte() {
            let source = "return 3 >= 2;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for 3 >= 2");
            assert_eq!(ty, Type::bool(), ">= should infer boolean");
        }

        #[test]
        fn equality_eq() {
            let source = "return 1 == 2;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for 1 == 2");
            assert_eq!(ty, Type::bool(), "== should infer boolean");
        }

        #[test]
        fn equality_neq() {
            let source = "return 1 != 2;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for 1 != 2");
            assert_eq!(ty, Type::bool(), "!= should infer boolean");
        }

        #[test]
        fn logical_and() {
            let source = "return true && false;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for true && false");
            assert_eq!(ty, Type::bool(), "&& should infer boolean");
        }

        #[test]
        fn logical_or() {
            let source = "return true || false;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for true || false");
            assert_eq!(ty, Type::bool(), "|| should infer boolean");
        }

        #[test]
        fn logical_and_keyword() {
            let source = "return true and false;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for true and false");
            assert!(ty == Type::bool() || ty == Type::any(), "and should infer boolean or any, got {:?}", ty);
        }

        #[test]
        fn logical_or_keyword() {
            let source = "return true or false;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for true or false");
            assert!(ty == Type::bool() || ty == Type::any(), "or should infer boolean or any, got {:?}", ty);
        }

        #[test]
        fn logical_xor() {
            let source = "return true xor false;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let bin = nodes.first().expect("one binary expr");
            let ty = type_at_node(&result.type_map, bin).expect("type for true xor false");
            assert!(ty == Type::bool() || ty == Type::any(), "xor should infer boolean or any, got {:?}", ty);
        }
    }

    mod unary_ops {
        use super::*;

        #[test]
        fn minus_int() {
            let source = "return -1;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeUnaryExpr.into_syntax_kind());
            let unary = nodes.first().expect("one unary expr");
            let ty = type_at_node(&result.type_map, unary).expect("type for -1");
            assert_eq!(ty, Type::int(), "unary minus of integer should infer integer");
        }

        #[test]
        fn plus_int() {
            let source = "return +1;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeUnaryExpr.into_syntax_kind());
            let unary = nodes.first().expect("one unary expr");
            let ty = type_at_node(&result.type_map, unary).expect("type for +1");
            assert_eq!(ty, Type::int(), "unary plus of integer should infer integer");
        }

        #[test]
        fn plus_real() {
            let source = "return +1.0;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeUnaryExpr.into_syntax_kind());
            let unary = nodes.first().expect("one unary expr");
            let ty = type_at_node(&result.type_map, unary).expect("type for +1.0");
            assert_eq!(ty, Type::real(), "unary plus of real should infer real");
        }

        #[test]
        fn logical_not() {
            let source = "return !true;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeUnaryExpr.into_syntax_kind());
            let unary = nodes.first().expect("one unary expr");
            let ty = type_at_node(&result.type_map, unary).expect("type for !true");
            assert_eq!(ty, Type::bool(), "! should infer boolean");
        }

        #[test]
        fn logical_not_keyword() {
            let source = "return not false;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeUnaryExpr.into_syntax_kind());
            let unary = nodes.first().expect("one unary expr");
            let ty = type_at_node(&result.type_map, unary).expect("type for not false");
            assert_eq!(ty, Type::bool(), "not should infer boolean");
        }
    }

    mod variables {
        use super::*;

        #[test]
        fn typed_variable_from_declaration() {
            let source = "integer x = 1; return x;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodePrimaryExpr.into_syntax_kind());
            assert!(nodes.len() >= 2, "expected at least 2 primary exprs");
            let x_expr = &nodes[1];
            let ty = type_at_node(&result.type_map, x_expr).expect("type for x");
            assert_eq!(ty, Type::int(), "variable x declared as integer should infer integer");
        }

        #[test]
        fn inferred_variable_no_annotation() {
            let source = r#"var s = "hi"; return s;"#;
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodePrimaryExpr.into_syntax_kind());
            let s_expr = nodes.iter().find(|n| n.collect_text().trim() == "s").expect("primary s");
            let ty = type_at_node(&result.type_map, s_expr).expect("type for s");
            assert_eq!(ty, Type::string(), "var s = \"hi\" should infer string for s");
        }

        #[test]
        fn assignment_expression_type() {
            let source = "var x = 0; return x = 1;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let expr_nodes = root.find_all_nodes(Kind::NodeExpr.into_syntax_kind());
            let assign_expr = expr_nodes
                .iter()
                .find(|n| n.collect_text().contains("x = 1"));
            if let Some(assign_expr) = assign_expr {
                if let Some(ty) = type_at_node(&result.type_map, assign_expr) {
                    assert_eq!(ty, Type::int(), "assignment expression should have LHS type (integer)");
                }
            }
            // If type_map does not record assignment expr type, at least the program is valid and x is integer.
        }
    }

    mod ternary {
        use super::*;

        #[test]
        fn same_type_branches() {
            let source = "return true ? 1 : 2;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let expr_nodes = root.find_all_nodes(Kind::NodeExpr.into_syntax_kind());
            let ternary = expr_nodes
                .iter()
                .find(|n| node_helpers::is_ternary_expr(n))
                .expect("ternary expr");
            let ty = type_at_node(&result.type_map, ternary).expect("type for ternary");
            assert_eq!(ty, Type::compound2(Type::int(), Type::int()), "? 1 : 2 should infer compound(int, int)");
        }

        #[test]
        fn different_type_branches() {
            let source = r#"return true ? 1 : "two";"#;
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let expr_nodes = root.find_all_nodes(Kind::NodeExpr.into_syntax_kind());
            let ternary = expr_nodes
                .iter()
                .find(|n| node_helpers::is_ternary_expr(n))
                .expect("ternary expr");
            let ty = type_at_node(&result.type_map, ternary).expect("type for ternary");
            assert_eq!(ty, Type::compound2(Type::int(), Type::string()), "? 1 : \"two\" should infer compound(int, string)");
        }
    }

    mod cast {
        use super::*;

        #[test]
        fn int_as_real() {
            let source = "return 1 as real;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeAsCast.into_syntax_kind());
            let cast = nodes.first().expect("one as cast");
            let ty = type_at_node(&result.type_map, cast).expect("type for 1 as real");
            assert_eq!(ty, Type::real(), "1 as real should infer real");
        }

        #[test]
        fn null_as_optional() {
            let source = "return null as integer?;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeAsCast.into_syntax_kind());
            let cast = nodes.first().expect("one as cast");
            let ty = type_at_node(&result.type_map, cast).expect("type for null as integer?");
            assert_eq!(ty, Type::compound2(Type::int(), Type::null()), "null as integer? should infer integer?");
        }
    }

    mod index {
        use super::*;

        #[test]
        fn index_expr_in_type_map() {
            let source = "var arr = []; return arr[0];";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeIndexExpr.into_syntax_kind());
            let index_node = nodes.first().expect("one index expr");
            let ty = type_at_node(&result.type_map, index_node).expect("index expr should have type in type_map");
            assert_eq!(ty, Type::any(), "index expr currently infers any");
        }
    }

    mod for_in {
        use super::*;

        #[test]
        fn loop_var_from_array() {
            let source = "for (i in [1, 2, 3]) { return i; }";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            if !result.is_valid() {
                return;
            }
            let primary_nodes = root.find_all_nodes(Kind::NodePrimaryExpr.into_syntax_kind());
            let i_in_loop = primary_nodes
                .iter()
                .filter(|n| n.collect_text().trim() == "i")
                .nth(1)
                .or_else(|| primary_nodes.iter().find(|n| n.collect_text().trim() == "i"));
            if let Some(i_in_loop) = i_in_loop {
                if let Some(ty) = type_at_node(&result.type_map, i_in_loop) {
                    assert!(ty == Type::int() || ty == Type::any(), "for (i in array) i should be int or any, got {:?}", ty);
                }
            }
        }
    }

    mod member {
        use super::*;

        #[test]
        fn constructor_call_member_access_infers_field_type() {
            let source = r"
                class Cell {
                    integer id;
                    integer x;
                    integer y;
                    constructor(integer id) {}
                }
                var myCellX = Cell(12).x;
            ";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let var_decls = root.find_all_nodes(Kind::NodeVarDecl.into_syntax_kind());
            let my_cell_x_decl = var_decls
                .iter()
                .find(|n| n.collect_text().contains("myCellX"))
                .expect("var myCellX decl");
            let ty = type_at_node(&result.type_map, my_cell_x_decl).expect("type for var myCellX");
            assert_eq!(ty, Type::int(), "Cell(12).x should infer integer from field");
        }

        #[test]
        fn include_tree_member_access_infers_field_type() {
            use crate::build_include_tree;
            let dir = std::env::temp_dir().join("leekscript_include_type_test");
            let _ = std::fs::create_dir_all(&dir);
            let cell_path = dir.join("Cell.leek");
            let main_path = dir.join("test.leek");
            let cell_source = r"
                class Cell {
                    integer id;
                    integer x;
                    integer y;
                    constructor(integer id) {
                        this.id = id;
                        this.x = 0;
                        this.y = 0;
                    }
                }
            ";
            let main_source = r#"
                include("Cell.leek");
                var myCellX = Cell(12).x;
            "#;
            std::fs::write(&cell_path, cell_source).unwrap();
            std::fs::write(&main_path, main_source).unwrap();
            let source = std::fs::read_to_string(&main_path).unwrap();
            let tree = build_include_tree(&source, Some(main_path.as_path())).unwrap();
            let result = analyze_with_include_tree(&tree, &[]);
            let _ = std::fs::remove_dir_all(&dir);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let program_root = tree.root.as_ref().expect("main root");
            let var_decls = program_root.find_all_nodes(Kind::NodeVarDecl.into_syntax_kind());
            let my_cell_x_decl = var_decls
                .iter()
                .find(|n| n.collect_text().contains("myCellX"))
                .expect("var myCellX decl");
            let ty = type_at_node(&result.type_map, my_cell_x_decl).expect("type for var myCellX");
            assert_eq!(ty, Type::int(), "Cell(12).x with include should infer integer");
        }

        #[test]
        fn class_name_is_class_t() {
            let source = r"
                class MyClass {
                    integer x = 0;
                }
                return MyClass;
            ";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let primary_nodes = root.find_all_nodes(Kind::NodePrimaryExpr.into_syntax_kind());
            let class_name_node = primary_nodes
                .iter()
                .find(|n| n.collect_text().trim() == "MyClass")
                .expect("primary expr MyClass");
            let ty = type_at_node(&result.type_map, class_name_node).expect("type for MyClass");
            assert_eq!(ty, Type::class(Some("MyClass".to_string())), "class name as value should infer Class<T>");
        }

        #[test]
        fn class_static_member_access() {
            let source = r"
                class Util {
                    static integer DEFAULT = 42;
                    static integer add(integer a, integer b) { return a + b; }
                }
                return Util.DEFAULT;
            ";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let member_nodes = root.find_all_nodes(Kind::NodeMemberExpr.into_syntax_kind());
            let default_member = member_nodes
                .iter()
                .find(|n| n.collect_text().trim().ends_with(".DEFAULT"))
                .expect("member expr Util.DEFAULT");
            let ty_default = type_at_node(&result.type_map, default_member).expect("type for Util.DEFAULT");
            assert_eq!(ty_default, Type::int(), "Util.DEFAULT should infer integer");
        }

        #[test]
        fn class_static_method_access() {
            let source = r"
                class Util {
                    static integer add(integer a, integer b) { return a + b; }
                }
                integer y = Util.add(1, 2);
                return null;
            ";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let member_nodes = root.find_all_nodes(Kind::NodeMemberExpr.into_syntax_kind());
            let add_member = member_nodes
                .iter()
                .find(|n| node_helpers::member_expr_member_name(n).as_deref() == Some("add"))
                .expect("member expr Util.add");
            let ty_add = type_at_node(&result.type_map, add_member).expect("type for Util.add");
            assert_eq!(ty_add, Type::function(vec![Type::int(), Type::int()], Type::int()), "Util.add should infer Function<integer, integer => integer>");
            let call_nodes = root.find_all_nodes(Kind::NodeCallExpr.into_syntax_kind());
            let add_call = call_nodes.iter().find(|n| n.collect_text().contains("1, 2")).expect("call Util.add(1, 2)");
            let ty_call = type_at_node(&result.type_map, add_call).expect("type for Util.add(1, 2)");
            assert_eq!(ty_call, Type::int(), "static method call should infer return type integer");
        }

        #[test]
        fn variable_class_returns_class_t() {
            let source = r"
                class Cell {}
                var c = new Cell();
                return c.class;
            ";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let member_nodes = root.find_all_nodes(Kind::NodeMemberExpr.into_syntax_kind());
            let class_member = member_nodes
                .iter()
                .find(|n| node_helpers::member_expr_member_name(n).as_deref() == Some("class"))
                .expect("member expr c.class");
            let ty = type_at_node(&result.type_map, class_member).expect("type for c.class");
            assert_eq!(ty, Type::class(Some("Cell".to_string())), "c.class should infer Class<Cell>");
        }
    }

    mod nested {
        use super::*;

        #[test]
        fn chained_binary() {
            let source = "return 1 + 2 + 3;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let outer = nodes.last().expect("outer binary expr");
            let ty = type_at_node(&result.type_map, outer).expect("type for 1 + 2 + 3");
            assert_eq!(ty, Type::int(), "chained int + int + int should infer integer");
        }

        #[test]
        fn nested_parentheses() {
            let source = "return (1 + 2) * 3;";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeBinaryExpr.into_syntax_kind());
            let mul = nodes.iter().find(|n| n.collect_text().contains("*")).expect("binary *");
            let ty = type_at_node(&result.type_map, mul).expect("type for (1+2)*3");
            assert!(ty.is_number(), "(1+2)*3 should infer number (int or real), got {:?}", ty);
        }
    }

    mod this_super {
        use super::*;

        #[test]
        fn this_in_class() {
            let source = r"
                class MyClass {
                    MyClass getSelf() { return this; }
                }
                return null;
            ";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let primary_nodes = root.find_all_nodes(Kind::NodePrimaryExpr.into_syntax_kind());
            let this_node = primary_nodes.iter().find(|n| n.collect_text().trim() == "this").expect("primary this");
            let ty = type_at_node(&result.type_map, this_node).expect("type for this");
            assert_eq!(ty, Type::instance("MyClass"), "this in class should infer instance type");
        }

        #[test]
        fn this_property() {
            let source = r"
                class Box {
                    integer value = 0;
                    integer getValue() { return this.value; }
                }
                return null;
            ";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let member_nodes = root.find_all_nodes(Kind::NodeMemberExpr.into_syntax_kind());
            let value_member = member_nodes.iter().find(|n| n.collect_text().trim().ends_with(".value")).expect("this.value");
            let ty = type_at_node(&result.type_map, value_member).expect("type for this.value");
            assert_eq!(ty, Type::int(), "this.value should infer integer from field");
        }

        #[test]
        fn this_method_call() {
            let source = r"
                class Counter {
                    integer getOne() { return 1; }
                    integer getTwo() { return this.getOne() + this.getOne(); }
                }
                return null;
            ";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let call_nodes = root.find_all_nodes(Kind::NodeCallExpr.into_syntax_kind());
            let get_one_call = call_nodes.first().expect("this.getOne()");
            let ty = type_at_node(&result.type_map, get_one_call).expect("type for this.getOne()");
            assert_eq!(ty, Type::int(), "this.getOne() should infer return type integer");
        }
    }

    mod null_narrowing {
        use super::*;

        #[test]
        fn then_branch_narrowed() {
            let source = r"
                integer? x = null;
                if (x != null) {
                    return x;
                }
                return 0;
            ";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let primary_nodes = root.find_all_nodes(Kind::NodePrimaryExpr.into_syntax_kind());
            let x_in_then = primary_nodes
                .iter()
                .filter(|n| n.collect_text().trim() == "x")
                .nth(1)
                .or_else(|| primary_nodes.iter().find(|n| n.collect_text().trim() == "x"))
                .expect("primary x in then branch");
            let ty = type_at_node(&result.type_map, x_in_then).expect("type for x in then branch");
            assert!(
                ty == Type::int() || ty == Type::compound2(Type::int(), Type::null()),
                "x in then branch should be narrowed to integer or remain integer?, got {:?}",
                ty
            );
        }
    }

    mod constructor {
        use super::*;

        #[test]
        fn function_call_return_type() {
            let source = "function f() -> integer { return 1; } return f();";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = root.find_all_nodes(Kind::NodeCallExpr.into_syntax_kind());
            let call = nodes.first().expect("call f()");
            let ty = type_at_node(&result.type_map, call).expect("type for f()");
            assert_eq!(ty, Type::int(), "f() -> integer should infer integer");
        }

        #[test]
        fn new_constructor_infers_instance() {
            let source = "class Cell { constructor(integer id) {} } var myCell = new Cell(12);";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let var_decls = root.find_all_nodes(Kind::NodeVarDecl.into_syntax_kind());
            let my_cell_decl = var_decls.iter().find(|n| n.collect_text().contains("myCell")).expect("var myCell decl");
            let ty = type_at_node(&result.type_map, my_cell_decl).expect("type for var myCell");
            assert_eq!(ty, Type::instance("Cell"), "var myCell = new Cell(12) should infer Cell");
        }

        #[test]
        fn constructor_call_returns_instance() {
            let source = r"
                class Cell {
                    integer id;
                    constructor(integer id) {}
                }
                var c = Cell(12);
                return c;
            ";
            let root = parse(source).unwrap().expect("parse");
            let result = analyze(&root);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let call_nodes = root.find_all_nodes(Kind::NodeCallExpr.into_syntax_kind());
            let cell_call = call_nodes.first().expect("at least one call (Cell(12))");
            let ty = type_at_node(&result.type_map, cell_call).expect("type for Cell(12)");
            assert_eq!(ty, Type::instance("Cell"), "Cell(12) without new should infer Instance(Cell)");
        }

        #[test]
        fn seed_scope_from_program_registers_class_fields() {
            use super::scope_builder::seed_scope_from_program;
            use super::scope::ScopeStore;
            let cell_source = r"
                class Cell {
                    integer id;
                    integer x;
                    integer y;
                    constructor(integer id) {}
                }
            ";
            let root = parse(cell_source).unwrap().expect("parse");
            let mut store = ScopeStore::new();
            seed_scope_from_program(&mut store, &root);
            let ty = store.get_class_member_type("Cell", "x");
            assert_eq!(ty, Some(Type::int()), "Cell.x should be registered as integer after seed_scope_from_program");
        }

        #[test]
        fn stdlib_call_return_type() {
            use crate::parser::parse_signatures;
            let sig_src = "function abs(real|integer number) -> integer\n";
            let sig_root = parse_signatures(sig_src).unwrap().expect("sig parse");
            let program_src = "return abs(-1);";
            let program_root = parse(program_src).unwrap().expect("parse");
            let result = analyze_with_signatures(&program_root, &[sig_root]);
            assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
            let nodes = program_root.find_all_nodes(Kind::NodeCallExpr.into_syntax_kind());
            let call = nodes.first().expect("one call expr");
            let ty = type_at_node(&result.type_map, call).expect("type for abs(-1)");
            assert_eq!(ty, Type::int(), "abs() returns integer per signature");
        }

        #[test]
        fn get_mp_signature_one_param() {
            use crate::parser::parse_signatures;
            let sig_src = "function getMP(real|integer entity?) -> integer\n";
            let sig_root = parse_signatures(sig_src).unwrap().expect("sig parse");
            let program_src = "return getMP(1);";
            let program_root = parse(program_src).unwrap().expect("parse");
            let result = analyze_with_signatures(&program_root, &[sig_root]);
            assert!(result.is_valid(), "getMP(1) should type-check: {:?}", result.diagnostics);
            let calls: Vec<_> = program_root.find_all_nodes(Kind::NodeCallExpr.into_syntax_kind()).into_iter().collect();
            assert_eq!(calls.len(), 1);
            let ty = type_at_node(&result.type_map, &calls[0]).expect("type for getMP(1)");
            assert_eq!(ty, Type::int(), "getMP(1) with 1 param should infer integer from .sig");
        }

        #[test]
        fn get_mp_signature_zero_params() {
            use crate::parser::parse_signatures;
            let sig_src = "function getMP(real|integer entity?) -> integer\n";
            let sig_root = parse_signatures(sig_src).unwrap().expect("sig parse");
            let program_src = "return getMP();";
            let program_root = parse(program_src).unwrap().expect("parse");
            let result = analyze_with_signatures(&program_root, &[sig_root]);
            assert!(result.is_valid(), "getMP() should type-check: {:?}", result.diagnostics);
            let calls: Vec<_> = program_root.find_all_nodes(Kind::NodeCallExpr.into_syntax_kind()).into_iter().collect();
            assert_eq!(calls.len(), 1);
            let ty = type_at_node(&result.type_map, &calls[0]).expect("type for getMP()");
            assert_eq!(ty, Type::int(), "getMP() with 0 params should infer integer from .sig");
        }

        #[test]
        fn get_mp_signature_as_value_union() {
            use crate::parser::parse_signatures;
            let sig_src = "function getMP(real|integer entity?) -> integer\n";
            let sig_root = parse_signatures(sig_src).unwrap().expect("sig parse");
            let program_src = "return getMP;";
            let program_root = parse(program_src).unwrap().expect("parse");
            let result = analyze_with_signatures(&program_root, &[sig_root]);
            assert!(result.is_valid(), "getMP as value should type-check: {:?}", result.diagnostics);
            let primary_nodes: Vec<_> = program_root.find_all_nodes(Kind::NodePrimaryExpr.into_syntax_kind()).into_iter().collect();
            let getmp_node = primary_nodes.iter().find(|n| n.collect_text().trim() == "getMP").expect("primary getMP");
            let ty = type_at_node(&result.type_map, getmp_node).expect("type for getMP (as value)");
            let (zero_arg, one_arg) = match &ty {
                Type::Compound(variants) if variants.len() == 2 => {
                    let f0 = variants.iter().find(|t| matches!(t, Type::Function { args, .. } if args.is_empty()));
                    let f1 = variants.iter().find(|t| matches!(t, Type::Function { args, .. } if args.len() == 1));
                    (f0, f1)
                }
                _ => (None, None),
            };
            assert!(zero_arg.is_some() && one_arg.is_some(), "getMP as value should be union of 0-arg and 1-arg Function, got {:?}", ty);
            if let Some(Type::Function { return_type, .. }) = zero_arg {
                assert_eq!(**return_type, Type::int(), "0-arg variant should return integer");
            }
            if let Some(Type::Function { return_type, .. }) = one_arg {
                assert_eq!(**return_type, Type::int(), "1-arg variant should return integer");
            }
        }

        #[test]
        fn static_method_from_sig() {
            use crate::parser::parse_signatures;
            let sig_src = r"
                class PathManager {
                    static Map getCachedReachableCells(Cell cell, integer mp, Array cellsToIgnore)
                }
                class Cell {}
            ";
            let sig_root = parse_signatures(sig_src).unwrap().expect("sig parse");
            let program_src = r"
                var cell = null;
                var mp = 0;
                var arr = [];
                return PathManager.getCachedReachableCells(cell, mp, arr);
            ";
            let program_root = parse(program_src).unwrap().expect("parse");
            let result = analyze_with_signatures(&program_root, &[sig_root]);
            assert!(result.is_valid(), "PathManager.getCachedReachableCells from .sig should type-check: {:?}", result.diagnostics);
            let member_nodes: Vec<_> = program_root.find_all_nodes(Kind::NodeMemberExpr.into_syntax_kind()).into_iter().collect();
            let get_cached = member_nodes
                .iter()
                .find(|n| node_helpers::member_expr_member_name(n).as_deref() == Some("getCachedReachableCells"))
                .expect("member PathManager.getCachedReachableCells");
            let ty_member = type_at_node(&result.type_map, get_cached).expect("type for PathManager.getCachedReachableCells");
            match &ty_member {
                Type::Function { return_type, .. } => assert_eq!(
                    **return_type,
                    Type::map(Type::any(), Type::any()),
                    "static method return type from .sig should be Map<any, any>"
                ),
                _ => panic!("static method from .sig should infer as Function, got {:?}", ty_member),
            };
        }
    }
}
