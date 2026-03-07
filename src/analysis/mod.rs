//! Semantic analysis: scope building and validation.
//!
//! **Signature files (.sig):** When `.sig` files are provided (e.g. stdlib), they are parsed
//! with the signature grammar, then type expressions in each `function`/`global` declaration
//! are turned into `Type` via `parse_type_expr` and seeded into the root scope. The type checker
//! uses these for inference (e.g. call return types, global variable types). Use
//! `analyze_with_signatures(program_root, signature_roots)` so that built-in names resolve and
//! types are inferred from the .sig definitions.

mod deprecation;
mod error;
mod node_helpers;
mod scope;
mod scope_builder;
mod scope_extents;
mod type_checker;
mod type_expr;
mod validator;

pub use error::AnalysisError;
pub use node_helpers::{
    binary_expr_rhs, call_argument_count, class_decl_info, function_decl_info, member_expr_member_name,
    primary_expr_resolvable_name, var_decl_info, ClassDeclInfo, FunctionDeclInfo, VarDeclInfo,
    VarDeclKind,
};
pub use scope::{ResolvedSymbol, Scope, ScopeId, ScopeKind, ScopeStore, VariableInfo, VariableKind};
pub use scope_extents::{build_scope_extents, scope_at_offset};
pub use scope_builder::ScopeBuilder;
pub use type_checker::{TypeChecker, TypeMapKey};
pub use type_expr::{find_type_expr_child, parse_type_expr, TypeExprResult};
pub use validator::Validator;

use sipha::error::SemanticDiagnostic;
use sipha::red::{SyntaxElement, SyntaxNode};
use sipha::types::{IntoSyntaxKind, Span};
use sipha::walk::WalkOptions;

use crate::syntax::Kind;
use crate::types::Type;

/// Result of running scope building and validation.
#[derive(Debug)]
pub struct AnalysisResult {
    pub diagnostics: Vec<SemanticDiagnostic>,
    pub scope_store: ScopeStore,
    /// Map from expression span (start, end) to inferred type (for formatter type annotations).
    pub type_map: std::collections::HashMap<TypeMapKey, Type>,
    /// Scope IDs in walk order (for LSP: compute scope at offset from scope-extent list).
    pub scope_id_sequence: Vec<ScopeId>,
}

impl AnalysisResult {
    #[must_use] 
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == sipha::error::Severity::Error)
    }

    #[must_use] 
    pub fn is_valid(&self) -> bool {
        !self.has_errors()
    }
}

/// Build scope from the tree and run validation. Returns diagnostics and the scope store.
///
/// Pass order: (1) `ScopeBuilder` runs first and builds the scope store and scope ID sequence.
/// (2) Validator and (3) `TypeChecker` then use that store so resolution and type checking see the same scopes.
/// Class names that are commonly used as API/namespace (e.g. PathManager.getCachedReachableCells).
/// Seeded so they infer as Class<T> when no .sig is used.
const BUILTIN_CLASS_NAMES: &[&str] = &[
    "Class", "Object", "CellManager", "Fight", "Items", "PathManager", "Stats",
    "Logger", "Entity", "Cell", "Consequences", "bool",
];

#[must_use] 
pub fn analyze(root: &SyntaxNode) -> AnalysisResult {
    let options = WalkOptions::nodes_only();
    let mut builder = ScopeBuilder::new();
    let _ = root.walk(&mut builder, &options);

    for name in BUILTIN_CLASS_NAMES {
        builder.store.add_root_class((*name).to_string(), Span::new(0, 0));
    }

    let mut validator = Validator::new(&builder.store, &builder.scope_id_sequence);
    let _ = root.walk(&mut validator, &options);

    let mut type_checker = TypeChecker::new(&builder.store, root);
    let _ = root.walk(&mut type_checker, &options);

    let mut deprecation_checker = deprecation::DeprecationChecker::new();
    let _ = root.walk(&mut deprecation_checker, &options);

    let mut diagnostics = validator.diagnostics;
    diagnostics.extend(type_checker.diagnostics);
    diagnostics.extend(deprecation_checker.diagnostics);

    let type_map = type_checker.type_map;
    AnalysisResult {
        diagnostics,
        scope_store: builder.store,
        type_map,
        scope_id_sequence: builder.scope_id_sequence,
    }
}

/// Seed the root scope from parsed signature file(s). Each element of `signature_roots`
/// should be the root node returned by `parse_signatures()` (may be a wrapper or `NodeSigFile`).
pub fn seed_scope_from_signatures(store: &mut ScopeStore, signature_roots: &[SyntaxNode]) {
    for root in signature_roots {
        let file_nodes: Vec<SyntaxNode> = if root.kind_as::<Kind>() == Some(Kind::NodeSigFile) {
            vec![root.clone()]
        } else {
            root.children()
                .filter_map(|c| match c {
                    SyntaxElement::Node(n) if n.kind_as::<Kind>() == Some(Kind::NodeSigFile) => {
                        Some(n.clone())
                    }
                    _ => None,
                })
                .collect()
        };
        for file in file_nodes {
            for child in file.children() {
                let SyntaxElement::Node(n) = child else { continue };
                if n.kind_as::<Kind>() == Some(Kind::NodeSigGlobal) {
                    if let Some(name) = sig_global_name(&n) {
                        if let Some(type_node) = find_type_expr_child(&n) {
                            if let TypeExprResult::Ok(ty) = parse_type_expr(&type_node) {
                                store.add_root_global_with_type(name, ty);
                            } else {
                                store.add_root_global(name);
                            }
                        } else {
                            store.add_root_global(name);
                        }
                    }
                } else if n.kind_as::<Kind>() == Some(Kind::NodeSigFunction) {
                    if let Some((name, min_arity, max_arity)) = sig_function_info(&n) {
                        let (param_types, return_type) = sig_function_types(&n);
                        if let Some(pt) = param_types {
                            store.add_root_function_with_types(
                                name,
                                min_arity,
                                max_arity,
                                Span::new(0, 0),
                                Some(pt),
                                return_type,
                            );
                        } else {
                            store.add_root_function(name, min_arity, max_arity, Span::new(0, 0));
                        }
                    }
                } else if n.kind_as::<Kind>() == Some(Kind::NodeSigClass) {
                    if let Some(class_name) = sig_class_name(&n) {
                        store.add_root_class(class_name.clone(), Span::new(0, 0));
                        for method_node in n.find_all_nodes(Kind::NodeSigMethod.into_syntax_kind()) {
                            if let (Some(method_name), Some(param_types), Some(return_type)) = (
                                sig_method_name(&method_node),
                                sig_method_param_types(&method_node),
                                sig_method_return_type(&method_node),
                            ) {
                                let ret = return_type;
                                if sig_method_is_static(&method_node) {
                                    store.add_class_static_method(
                                        &class_name,
                                        method_name,
                                        param_types,
                                        ret,
                                    );
                                } else {
                                    store.add_class_method(&class_name, method_name, param_types, ret);
                                }
                            }
                        }
                        for field_node in n.find_all_nodes(Kind::NodeSigField.into_syntax_kind()) {
                            if let (Some(field_name), Some(ty)) =
                                (sig_field_name(&field_node), sig_field_type(&field_node))
                            {
                                if sig_field_is_static(&field_node) {
                                    store.add_class_static_field(
                                        &class_name,
                                        field_name,
                                        ty,
                                    );
                                } else {
                                    store.add_class_field(&class_name, field_name, ty);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn sig_global_name(node: &SyntaxNode) -> Option<String> {
    let tokens: Vec<_> = node
        .descendant_tokens()
        .into_iter()
        .filter(|t| t.kind_as::<Kind>() == Some(Kind::TokIdent))
        .collect();
    tokens.last().map(|t| t.text().to_string())
}

fn sig_class_name(node: &SyntaxNode) -> Option<String> {
    let tokens: Vec<String> = node
        .descendant_tokens()
        .iter()
        .filter(|t| t.kind_as::<Kind>() == Some(Kind::TokIdent))
        .map(|t| t.text().to_string())
        .collect();
    // In "class ClassName ..." the first ident may be "class" (keyword); the class name is the next ident.
    tokens
        .iter()
        .find(|s| s.as_str() != "class")
        .or(tokens.first())
        .cloned()
}

/// True if this NodeSigMethod has "static" in its tokens.
fn sig_method_is_static(node: &SyntaxNode) -> bool {
    node.descendant_tokens()
        .iter()
        .any(|t| t.text() == "static")
}

/// Return type from NodeSigMethod (first type_expr is the return type).
fn sig_method_return_type(node: &SyntaxNode) -> Option<Type> {
    let te = find_type_expr_child(node)?;
    match parse_type_expr(&te) {
        TypeExprResult::Ok(t) => Some(t),
        TypeExprResult::Err(_) => None,
    }
}

/// Method name: ident token immediately before "(".
fn sig_method_name(node: &SyntaxNode) -> Option<String> {
    let tokens: Vec<String> = node
        .descendant_tokens()
        .iter()
        .filter(|t| !t.is_trivia())
        .map(|t| t.text().to_string())
        .collect();
    let lparen_idx = tokens.iter().position(|s| s == "(")?;
    tokens.into_iter().nth(lparen_idx.checked_sub(1)?)
}

/// Param types from NodeSigMethod (NodeSigParam children; each has a type_expr).
fn sig_method_param_types(node: &SyntaxNode) -> Option<Vec<Type>> {
    let param_nodes: Vec<SyntaxNode> = node
        .find_all_nodes(Kind::NodeSigParam.into_syntax_kind())
        .into_iter()
        .collect();
    let mut param_types = Vec::with_capacity(param_nodes.len());
    for p in &param_nodes {
        let te = find_type_expr_child(p)?;
        let ty = match parse_type_expr(&te) {
            TypeExprResult::Ok(t) => t,
            TypeExprResult::Err(_) => return None,
        };
        param_types.push(ty);
    }
    Some(param_types)
}

/// True if this NodeSigField has "static" in its tokens.
fn sig_field_is_static(node: &SyntaxNode) -> bool {
    node.descendant_tokens()
        .iter()
        .any(|t| t.text() == "static")
}

/// Field type from NodeSigField (single type_expr).
fn sig_field_type(node: &SyntaxNode) -> Option<Type> {
    let te = find_type_expr_child(node)?;
    match parse_type_expr(&te) {
        TypeExprResult::Ok(t) => Some(t),
        TypeExprResult::Err(_) => None,
    }
}

/// Field name: last ident in NodeSigField (order is [static?] [final?] type_expr ident).
fn sig_field_name(node: &SyntaxNode) -> Option<String> {
    let idents: Vec<String> = node
        .descendant_tokens()
        .iter()
        .filter(|t| !t.is_trivia() && t.kind_as::<Kind>() == Some(Kind::TokIdent))
        .map(|t| t.text().to_string())
        .collect();
    idents.into_iter().last()
}

/// Returns (name, `min_arity`, `max_arity`). Params with "?" after the name (omittable) count toward `max_arity` only.
fn sig_function_info(node: &SyntaxNode) -> Option<(String, usize, usize)> {
    let tokens: Vec<_> = node
        .descendant_tokens()
        .into_iter()
        .filter(|t| t.kind_as::<Kind>() == Some(Kind::TokIdent))
        .collect();
    let name = tokens.first()?.text().to_string();
    let params: Vec<_> = node
        .child_nodes()
        .filter(|n| n.kind_as::<Kind>() == Some(Kind::NodeSigParam))
        .collect();
    let max_arity = params.len();
    let min_arity = params
        .iter()
        .filter(|p| !p.descendant_tokens().iter().any(|t| t.text() == "?"))
        .count();
    Some((name, min_arity, max_arity))
}

/// Returns (param_types, return_type) from a NodeSigFunction when types can be parsed.
fn sig_function_types(node: &SyntaxNode) -> (Option<Vec<Type>>, Option<Type>) {
    let param_nodes: Vec<SyntaxNode> = node
        .child_nodes()
        .filter(|n| n.kind_as::<Kind>() == Some(Kind::NodeSigParam))
        .collect();
    let mut param_types = Vec::with_capacity(param_nodes.len());
    for p in &param_nodes {
        if let Some(te) = find_type_expr_child(p) {
            if let TypeExprResult::Ok(ty) = parse_type_expr(&te) {
                param_types.push(ty);
            } else {
                return (None, None);
            }
        } else {
            return (None, None);
        }
    }
    // Return type: last NodeTypeExpr that is a direct child (after params) is the return type.
    let child_nodes: Vec<SyntaxNode> = node.child_nodes().collect();
    let return_type_node = child_nodes
        .iter()
        .rev()
        .find(|c| c.kind_as::<Kind>() == Some(Kind::NodeTypeExpr));
    let return_type = return_type_node.and_then(|te| match parse_type_expr(te) {
        TypeExprResult::Ok(t) => Some(t),
        TypeExprResult::Err(_) => None,
    });
    (Some(param_types), return_type)
}

/// Analyze the program with the root scope pre-seeded from signature files (e.g. stdlib constants and functions).
/// This allows references to global constants and built-in functions to resolve without errors.
/// Also seeds built-in type/class names (e.g. `Class`, `bool`) so that common `LeekScript` code validates.
#[must_use] 
pub fn analyze_with_signatures(
    program_root: &SyntaxNode,
    signature_roots: &[SyntaxNode],
) -> AnalysisResult {
    let options = WalkOptions::nodes_only();
    let mut store = ScopeStore::new();
    seed_scope_from_signatures(&mut store, signature_roots);
    // Built-in type/class names and common API classes for static validation.
    for name in [
        "Class",
        "bool",
        "CellManager",
        "Fight",
        "Items",
        "PathManager",
        "Stats",
        "Logger",
        "Object",
        "Entity",
        "Cell",
        "Consequences",
    ] {
        store.add_root_class(name.to_string(), Span::new(0, 0));
    }
    let mut builder = ScopeBuilder::with_store(store);
    let _ = program_root.walk(&mut builder, &options);

    let mut validator = Validator::new(&builder.store, &builder.scope_id_sequence);
    let _ = program_root.walk(&mut validator, &options);

    let mut type_checker = TypeChecker::new(&builder.store, program_root);
    let _ = program_root.walk(&mut type_checker, &options);

    let mut deprecation_checker = deprecation::DeprecationChecker::new();
    let _ = program_root.walk(&mut deprecation_checker, &options);

    let mut diagnostics = validator.diagnostics;
    diagnostics.extend(type_checker.diagnostics);
    diagnostics.extend(deprecation_checker.diagnostics);

    let type_map = type_checker.type_map;
    AnalysisResult {
        diagnostics,
        scope_store: builder.store,
        type_map,
        scope_id_sequence: builder.scope_id_sequence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use sipha::types::IntoSyntaxKind;

    /// Returns the inferred type for a node's span from the analysis type_map, if any.
    fn type_at_node(
        type_map: &std::collections::HashMap<TypeMapKey, Type>,
        node: &SyntaxNode,
    ) -> Option<Type> {
        let range = node.text_range();
        type_map.get(&(range.start, range.end)).cloned()
    }

    #[test]
    fn analyze_valid_program() {
        let source = "var x = 1; return x;";
        let root = parse(source).unwrap().expect("parse");
        let result = analyze(&root);
        assert!(result.is_valid(), "expected no errors: {:?}", result.diagnostics);
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

    // ─── Type inference tests: assert inferred type of expressions from input types ───

    #[test]
    fn type_inference_binary_int_plus_int() {
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
    fn type_inference_binary_real_plus_int() {
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
    fn type_inference_string_concatenation_returns_string() {
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
    fn type_inference_literal_integer() {
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
    fn type_inference_literal_string() {
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
    fn type_inference_typed_variable_from_declaration() {
        let source = "integer x = 1; return x;";
        let root = parse(source).unwrap().expect("parse");
        let result = analyze(&root);
        assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
        // First primary is "1", second is "x" (in return).
        let nodes = root.find_all_nodes(Kind::NodePrimaryExpr.into_syntax_kind());
        assert!(nodes.len() >= 2, "expected at least 2 primary exprs");
        let x_expr = &nodes[1];
        let ty = type_at_node(&result.type_map, x_expr).expect("type for x");
        assert_eq!(ty, Type::int(), "variable x declared as integer should infer integer");
    }

    #[test]
    fn type_inference_function_call_return_type() {
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
    fn type_inference_binary_comparison_returns_bool() {
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
    fn type_inference_unary_minus_number() {
        let source = "return -1;";
        let root = parse(source).unwrap().expect("parse");
        let result = analyze(&root);
        assert!(result.is_valid(), "expected valid: {:?}", result.diagnostics);
        let nodes = root.find_all_nodes(Kind::NodeUnaryExpr.into_syntax_kind());
        let unary = nodes.first().expect("one unary expr");
        let ty = type_at_node(&result.type_map, unary).expect("type for -1");
        assert_eq!(ty, Type::int(), "unary minus of integer literal should infer integer");
    }

    #[test]
    fn type_inference_stdlib_call_return_type() {
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

    /// getMP from .sig has signature (real|integer entity?) -> integer (optional param: arity 0 or 1).
    /// Verifies that a call with 1 argument gets the correct return type from the .sig.
    #[test]
    fn type_inference_get_mp_signature_one_param() {
        use crate::parser::parse_signatures;
        let sig_src = "function getMP(real|integer entity?) -> integer\n";
        let sig_root = parse_signatures(sig_src).unwrap().expect("sig parse");
        let program_src = "return getMP(1);";
        let program_root = parse(program_src).unwrap().expect("parse");
        let result = analyze_with_signatures(&program_root, &[sig_root]);
        assert!(
            result.is_valid(),
            "getMP(1) should type-check: {:?}",
            result.diagnostics
        );
        let calls: Vec<_> = program_root
            .find_all_nodes(Kind::NodeCallExpr.into_syntax_kind())
            .into_iter()
            .collect();
        assert_eq!(calls.len(), 1, "expected one call expr getMP(1)");
        let ty = type_at_node(&result.type_map, &calls[0]).expect("type for getMP(1)");
        assert_eq!(
            ty,
            Type::int(),
            "getMP(1) with 1 param should infer integer from .sig"
        );
    }

    /// getMP with 0 params (optional entity omitted): same .sig, arity 0 should resolve and return integer.
    #[test]
    fn type_inference_get_mp_signature_zero_params() {
        use crate::parser::parse_signatures;
        let sig_src = "function getMP(real|integer entity?) -> integer\n";
        let sig_root = parse_signatures(sig_src).unwrap().expect("sig parse");
        let program_src = "return getMP();";
        let program_root = parse(program_src).unwrap().expect("parse");
        let result = analyze_with_signatures(&program_root, &[sig_root]);
        assert!(
            result.is_valid(),
            "getMP() should type-check: {:?}",
            result.diagnostics
        );
        let calls: Vec<_> = program_root
            .find_all_nodes(Kind::NodeCallExpr.into_syntax_kind())
            .into_iter()
            .collect();
        assert_eq!(calls.len(), 1, "expected one call expr getMP()");
        let ty = type_at_node(&result.type_map, &calls[0]).expect("type for getMP()");
        assert_eq!(
            ty,
            Type::int(),
            "getMP() with 0 params should infer integer from .sig"
        );
    }

    /// getMP as a value (e.g. when passed to a callback) should have type Function< => integer> | Function<integer => integer> (union of 0-arg and 1-arg).
    #[test]
    fn type_inference_get_mp_signature_as_value_union() {
        use crate::parser::parse_signatures;
        let sig_src = "function getMP(real|integer entity?) -> integer\n";
        let sig_root = parse_signatures(sig_src).unwrap().expect("sig parse");
        let program_src = "return getMP;";
        let program_root = parse(program_src).unwrap().expect("parse");
        let result = analyze_with_signatures(&program_root, &[sig_root]);
        assert!(
            result.is_valid(),
            "getMP as value should type-check: {:?}",
            result.diagnostics
        );
        let primary_nodes: Vec<_> = program_root
            .find_all_nodes(Kind::NodePrimaryExpr.into_syntax_kind())
            .into_iter()
            .collect();
        let getmp_node = primary_nodes
            .iter()
            .find(|n| n.collect_text().trim() == "getMP")
            .expect("primary expr 'getMP'");
        let ty = type_at_node(&result.type_map, getmp_node).expect("type for getMP (as value)");
        let (zero_arg, one_arg) = match &ty {
            Type::Compound(variants) if variants.len() == 2 => {
                let f0 = variants.iter().find(|t| matches!(t, Type::Function { args, .. } if args.is_empty()));
                let f1 = variants.iter().find(|t| matches!(t, Type::Function { args, .. } if args.len() == 1));
                (f0, f1)
            }
            _ => (None, None),
        };
        assert!(
            zero_arg.is_some() && one_arg.is_some(),
            "getMP as value should be union Function< => integer> | Function<... => integer>, got {:?}",
            ty
        );
        if let Some(Type::Function { return_type, .. }) = zero_arg {
            assert_eq!(**return_type, Type::int(), "0-arg variant should return integer");
        }
        if let Some(Type::Function { return_type, .. }) = one_arg {
            assert_eq!(**return_type, Type::int(), "1-arg variant should return integer");
        }
    }

    /// Static method from .sig: class PathManager { static Map getCachedReachableCells(Cell, integer, Array) } infers member and call.
    #[test]
    fn type_inference_static_method_from_sig() {
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
        assert!(
            result.is_valid(),
            "PathManager.getCachedReachableCells from .sig should type-check: {:?}",
            result.diagnostics
        );
        let member_nodes: Vec<_> = program_root
            .find_all_nodes(Kind::NodeMemberExpr.into_syntax_kind())
            .into_iter()
            .collect();
        let get_cached = member_nodes
            .iter()
            .find(|n| super::node_helpers::member_expr_member_name(n).as_deref() == Some("getCachedReachableCells"))
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

    #[test]
    fn type_inference_class_name_is_class_t() {
        // Using the name of a class as an expression (e.g. the class value) should infer Class<T>.
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
            .expect("primary expr 'MyClass' (class name used as value)");
        let ty = type_at_node(&result.type_map, class_name_node).expect("type for MyClass");
        assert_eq!(
            ty,
            Type::class(Some("MyClass".to_string())),
            "using the name of a class should infer Class<T>, not the instance type"
        );
    }

    #[test]
    fn type_inference_class_static_member_access() {
        // ClassName.staticField and ClassName.staticMethod() should infer from static members.
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
        assert_eq!(
            ty_default,
            Type::int(),
            "Util.DEFAULT (static field) should infer as integer, got {:?}",
            ty_default
        );
    }

    #[test]
    fn type_inference_class_static_method_access() {
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
            .find(|n| super::node_helpers::member_expr_member_name(n).as_deref() == Some("add"))
            .expect("member expr Util.add");
        let ty_add = type_at_node(&result.type_map, add_member).expect("type for Util.add");
        let expected_fn = Type::function(
            vec![Type::int(), Type::int()],
            Type::int(),
        );
        assert_eq!(
            ty_add,
            expected_fn,
            "Util.add should infer as Function<integer, integer => integer>; got {:?}",
            ty_add
        );
        assert_eq!(
            ty_add.to_string(),
            "Function<integer, integer => integer>",
            "Util.add annotation should display as Function<integer, integer => integer>",
        );
        // Call expression Util.add(1, 2) should infer as integer (return type of static method).
        let call_nodes = root.find_all_nodes(Kind::NodeCallExpr.into_syntax_kind());
        let add_call = call_nodes
            .iter()
            .find(|n| n.collect_text().contains("1, 2"))
            .expect("call expr Util.add(1, 2)");
        let ty_call = type_at_node(&result.type_map, add_call).expect("type for Util.add(1, 2)");
        assert_eq!(
            ty_call,
            Type::int(),
            "static method call Util.add(1, 2) should infer as integer, got {:?}",
            ty_call
        );
    }

    #[test]
    fn type_inference_this_in_class() {
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
        let this_node = primary_nodes
            .iter()
            .find(|n| n.collect_text().trim() == "this")
            .expect("primary expr 'this' in class method");
        let ty = type_at_node(&result.type_map, this_node).expect("type for this");
        assert_eq!(
            ty,
            Type::instance("MyClass"),
            "this inside a class should infer as the class instance type (MyClass), not Class<MyClass>"
        );
    }

    #[test]
    fn type_inference_this_property() {
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
        // Find this.value (member expr with .value)
        let value_member = member_nodes
            .iter()
            .find(|n| n.collect_text().trim().ends_with(".value"))
            .expect("member expr this.value");
        let ty = type_at_node(&result.type_map, value_member).expect("type for this.value");
        assert_eq!(
            ty,
            Type::int(),
            "this.value should infer as integer from class field type"
        );
    }

    #[test]
    fn type_inference_this_method_call() {
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
        assert!(
            !call_nodes.is_empty(),
            "expected at least one call (this.getOne())"
        );
        // Any this.getOne() call should have inferred type integer (method return type).
        let get_one_call = call_nodes.first().expect("at least one call");
        let ty = type_at_node(&result.type_map, get_one_call).expect("type for this.getOne()");
        assert_eq!(
            ty,
            Type::int(),
            "this.getOne() should infer return type integer from method signature"
        );
    }
}
