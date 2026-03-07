//! Load parsed signature (.sig) file nodes into the scope store.
//!
//! Extracts globals, functions, classes (with methods and fields) from signature AST nodes
//! and registers them in the root scope for resolution and type inference.

use sipha::red::{SyntaxElement, SyntaxNode};
use sipha::types::IntoSyntaxKind;

use crate::syntax::Kind;
use crate::types::Type;

use super::scope::{MemberVisibility, ScopeStore};
use super::type_expr::{find_type_expr_child, parse_type_expr, TypeExprResult};

/// Seed the root scope from parsed signature file(s). Each element of `signature_roots`
/// should be the root node returned by `parse_signatures()` (may be a wrapper or `NodeSigFile`).
pub(crate) fn seed_scope_from_signatures(store: &mut ScopeStore, signature_roots: &[SyntaxNode]) {
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
                                sipha::types::Span::new(0, 0),
                                Some(pt),
                                return_type,
                            );
                        } else {
                            store.add_root_function(name, min_arity, max_arity, sipha::types::Span::new(0, 0));
                        }
                    }
                } else if n.kind_as::<Kind>() == Some(Kind::NodeSigClass) {
                    if let Some(class_name) = sig_class_name(&n) {
                        store.add_root_class(class_name.clone(), sipha::types::Span::new(0, 0));
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
                                        MemberVisibility::Public,
                                    );
                                } else {
                                    store.add_class_method(
                                        &class_name,
                                        method_name,
                                        param_types,
                                        ret,
                                        MemberVisibility::Public,
                                    );
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
                                        MemberVisibility::Public,
                                    );
                                } else {
                                    store.add_class_field(
                                        &class_name,
                                        field_name,
                                        ty,
                                        MemberVisibility::Public,
                                    );
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

/// Return type from NodeSigMethod (last direct child NodeTypeExpr).
fn sig_method_return_type(node: &SyntaxNode) -> Option<Type> {
    super::type_expr::param_and_return_types(node, Kind::NodeSigParam).1
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
    super::type_expr::param_and_return_types(node, Kind::NodeSigParam).0
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
    super::type_expr::param_and_return_types(node, Kind::NodeSigParam)
}
