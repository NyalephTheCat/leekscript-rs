//! Type checking and type inference tests.

use leekscript_core::{parse, parse_signatures};

use crate::{analyze, analyze_with_signatures};

#[test]
fn typecheck_binary_op_requires_number() {
    let source = r#"return "hello" * 1;"#;
    let root = parse(source).unwrap().expect("parse");
    let result = analyze(&root);
    assert!(
        result.has_errors(),
        "expected type error for string * number: {:?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("requires number")),
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
        result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("type mismatch")),
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
        result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("type mismatch")),
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
        result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("type mismatch")),
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
        result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("invalid cast")),
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
        result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("type mismatch")),
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

// Type inference tests (assert inferred type of expressions from type_map)
mod type_inference {
    use leekscript_core::syntax::Kind;
    use leekscript_core::{parse, parse_signatures, Type};
    use sipha::red::SyntaxNode;
    use sipha::types::IntoSyntaxKind;

    use crate::scope::ScopeStore;
    use crate::scope_builder::seed_scope_from_program;
    use crate::{analyze, analyze_with_signatures, TypeMapKey};

    fn type_at_node(
        type_map: &std::collections::HashMap<TypeMapKey, Type>,
        node: &SyntaxNode,
    ) -> Option<Type> {
        let range = node.text_range();
        type_map.get(&(range.start, range.end)).cloned()
    }

    #[test]
    fn integer_literal() {
        let source = "return 42;";
        let root = parse(source).unwrap().expect("parse");
        let result = analyze(&root);
        assert!(
            result.is_valid(),
            "expected valid: {:?}",
            result.diagnostics
        );
        let nodes = root.find_all_nodes(Kind::NodePrimaryExpr.into_syntax_kind());
        let primary = nodes.first().expect("one primary expr (42)");
        let ty = type_at_node(&result.type_map, primary).expect("type for 42");
        assert_eq!(ty, Type::int(), "integer literal should infer integer");
    }

    #[test]
    fn this_in_class_infers_instance() {
        let source = r"
            class MyClass {
                MyClass getSelf() { return this; }
            }
            return null;
        ";
        let root = parse(source).unwrap().expect("parse");
        let result = analyze(&root);
        assert!(
            result.is_valid(),
            "expected valid: {:?}",
            result.diagnostics
        );
        let primary_nodes = root.find_all_nodes(Kind::NodePrimaryExpr.into_syntax_kind());
        let this_node = primary_nodes
            .iter()
            .find(|n| n.collect_text().trim() == "this")
            .expect("primary this");
        let ty = type_at_node(&result.type_map, this_node).expect("type for this");
        assert_eq!(
            ty,
            Type::instance("MyClass"),
            "this in class should infer instance type"
        );
    }

    #[test]
    fn new_constructor_infers_instance() {
        let source = "class Cell { constructor(integer id) {} } var myCell = new Cell(12);";
        let root = parse(source).unwrap().expect("parse");
        let result = analyze(&root);
        assert!(
            result.is_valid(),
            "expected valid: {:?}",
            result.diagnostics
        );
        let var_decls = root.find_all_nodes(Kind::NodeVarDecl.into_syntax_kind());
        let my_cell_decl = var_decls
            .iter()
            .find(|n| n.collect_text().contains("myCell"))
            .expect("var myCell decl");
        let ty = type_at_node(&result.type_map, my_cell_decl).expect("type for var myCell");
        assert_eq!(
            ty,
            Type::instance("Cell"),
            "var myCell = new Cell(12) should infer Cell"
        );
    }

    #[test]
    fn seed_scope_from_program_registers_class_fields() {
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
        assert_eq!(
            ty,
            Some(Type::int()),
            "Cell.x should be registered as integer after seed_scope_from_program"
        );
    }

    #[test]
    fn stdlib_call_return_type() {
        let sig_src = "function abs(real|integer number) -> integer\n";
        let sig_root = parse_signatures(sig_src).unwrap().expect("sig parse");
        let program_src = "return abs(-1);";
        let program_root = parse(program_src).unwrap().expect("parse");
        let result = analyze_with_signatures(&program_root, &[sig_root]);
        assert!(
            result.is_valid(),
            "expected valid: {:?}",
            result.diagnostics
        );
        let nodes = program_root.find_all_nodes(Kind::NodeCallExpr.into_syntax_kind());
        let call = nodes.first().expect("one call expr");
        let ty = type_at_node(&result.type_map, call).expect("type for abs(-1)");
        assert_eq!(ty, Type::int(), "abs() returns integer per signature");
    }

    #[test]
    fn call_narrow_return_to_string_when_first_arg_is_string() {
        let source = r#"
            var x = "hi";
            var y = 1;
            var sum = function(string|integer a, integer b) { return a + b; };
            var z = sum(x, y);
        "#;
        let root = parse(source).unwrap().expect("parse");
        let result = analyze(&root);
        assert!(
            result.is_valid(),
            "expected valid: {:?}",
            result.diagnostics
        );
        let nodes = root.find_all_nodes(Kind::NodeCallExpr.into_syntax_kind());
        let call = nodes.last().expect("call sum(x, y)");
        let ty = type_at_node(&result.type_map, call).expect("type for sum(x, y)");
        assert_eq!(
            ty,
            Type::string(),
            "call with first arg string should narrow return to string"
        );
    }

    #[test]
    fn call_narrow_return_to_integer_when_first_arg_is_integer() {
        let source = r#"
            var x = 42;
            var y = 1;
            var sum = function(string|integer a, integer b) { return a + b; };
            var z = sum(x, y);
        "#;
        let root = parse(source).unwrap().expect("parse");
        let result = analyze(&root);
        assert!(
            result.is_valid(),
            "expected valid: {:?}",
            result.diagnostics
        );
        let nodes = root.find_all_nodes(Kind::NodeCallExpr.into_syntax_kind());
        let call = nodes.last().expect("call sum(x, y)");
        let ty = type_at_node(&result.type_map, call).expect("type for sum(x, y)");
        assert_eq!(
            ty,
            Type::int(),
            "call with first arg integer should narrow return to integer"
        );
    }

    #[test]
    fn call_no_narrow_when_first_arg_is_union() {
        let source = r#"
            var x = true ? "a" : 1;
            var y = 1;
            var sum = function(string|integer a, integer b) { return a + b; };
            var z = sum(x, y);
        "#;
        let root = parse(source).unwrap().expect("parse");
        let result = analyze(&root);
        assert!(
            result.is_valid(),
            "expected valid: {:?}",
            result.diagnostics
        );
        let nodes = root.find_all_nodes(Kind::NodeCallExpr.into_syntax_kind());
        let call = nodes.last().expect("call sum(x, y)");
        let ty = type_at_node(&result.type_map, call).expect("type for sum(x, y)");
        let expected = Type::compound2(Type::string(), Type::int());
        assert_eq!(
            ty, expected,
            "call with first arg string|integer should not narrow"
        );
    }

    #[test]
    fn call_no_narrow_when_first_arg_is_any() {
        let sig_src = "function getAny() -> any\n";
        let sig_root = parse_signatures(sig_src).unwrap().expect("sig parse");
        let program_src = r#"
            var x = getAny();
            var y = 1;
            var sum = function(string|integer a, integer b) { return a + b; };
            var z = sum(x, y);
        "#;
        let program_root = parse(program_src).unwrap().expect("parse");
        let result = analyze_with_signatures(&program_root, &[sig_root]);
        assert!(
            result.is_valid(),
            "expected valid: {:?}",
            result.diagnostics
        );
        let nodes = program_root.find_all_nodes(Kind::NodeCallExpr.into_syntax_kind());
        let call = nodes.last().expect("call sum(x, y)");
        let ty = type_at_node(&result.type_map, call).expect("type for sum(x, y)");
        let expected = Type::compound2(Type::string(), Type::int());
        assert_eq!(ty, expected, "call with first arg any should not narrow");
    }
}
