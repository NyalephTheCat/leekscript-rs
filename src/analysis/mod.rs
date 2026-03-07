//! Semantic analysis: scope building and validation.

mod error;
mod node_helpers;
mod scope;
mod scope_builder;
mod type_checker;
mod validator;

pub use error::AnalysisError;
pub use node_helpers::binary_expr_rhs;
pub use scope::{ResolvedSymbol, Scope, ScopeId, ScopeKind, ScopeStore, VariableInfo, VariableKind};
pub use scope_builder::ScopeBuilder;
pub use type_checker::TypeChecker;
pub use validator::Validator;

use sipha::error::SemanticDiagnostic;
use sipha::red::{SyntaxElement, SyntaxNode};
use sipha::types::Span;
use sipha::walk::WalkOptions;

use crate::syntax::Kind;

/// Result of running scope building and validation.
#[derive(Debug)]
pub struct AnalysisResult {
    pub diagnostics: Vec<SemanticDiagnostic>,
    pub scope_store: ScopeStore,
}

impl AnalysisResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == sipha::error::Severity::Error)
    }

    pub fn is_valid(&self) -> bool {
        !self.has_errors()
    }
}

/// Build scope from the tree and run validation. Returns diagnostics and the scope store.
///
/// Pass order: (1) ScopeBuilder runs first and builds the scope store and scope ID sequence.
/// (2) Validator and (3) TypeChecker then use that store so resolution and type checking see the same scopes.
pub fn analyze(root: &SyntaxNode) -> AnalysisResult {
    let options = WalkOptions::nodes_only();
    let mut builder = ScopeBuilder::new();
    let _ = root.walk(&mut builder, &options);

    let mut validator = Validator::new(&builder.store, &builder.scope_id_sequence);
    let _ = root.walk(&mut validator, &options);

    let mut type_checker = TypeChecker::new(&builder.store, root);
    let _ = root.walk(&mut type_checker, &options);

    let mut diagnostics = validator.diagnostics;
    diagnostics.extend(type_checker.diagnostics);

    AnalysisResult {
        diagnostics,
        scope_store: builder.store,
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
                        store.add_root_global(name);
                    }
                } else if n.kind_as::<Kind>() == Some(Kind::NodeSigFunction) {
                    if let Some((name, min_arity, max_arity)) = sig_function_info(&n) {
                        store.add_root_function(name, min_arity, max_arity, Span::new(0, 0));
                    }
                } else if n.kind_as::<Kind>() == Some(Kind::NodeSigClass) {
                    if let Some(name) = sig_class_name(&n) {
                        store.add_root_class(name, Span::new(0, 0));
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
    let tokens: Vec<_> = node
        .descendant_tokens()
        .into_iter()
        .filter(|t| t.kind_as::<Kind>() == Some(Kind::TokIdent))
        .collect();
    // First ident in "class ClassName ..." is the class name.
    tokens.first().map(|t| t.text().to_string())
}

/// Returns (name, min_arity, max_arity). Params with "?" after the name (omittable) count toward max_arity only.
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

/// Analyze the program with the root scope pre-seeded from signature files (e.g. stdlib constants and functions).
/// This allows references to global constants and built-in functions to resolve without errors.
/// Also seeds built-in type/class names (e.g. `Class`, `bool`) so that common LeekScript code validates.
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

    let mut diagnostics = validator.diagnostics;
    diagnostics.extend(type_checker.diagnostics);

    AnalysisResult {
        diagnostics,
        scope_store: builder.store,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use crate::parser::parse_signatures;
    use std::path::Path;

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
}
