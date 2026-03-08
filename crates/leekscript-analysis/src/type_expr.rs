//! Parse `NodeTypeExpr` (and signature type nodes) into `Type`.
//!
//! Used for program type annotations (var decl, param, return, cast) and for
//! signature files (stdlib function/global types). Both grammars produce
//! `NodeTypeExpr` with the same shape: `type_optional` ( | `type_optional` )*.

use sipha::red::{SyntaxElement, SyntaxNode};
use sipha::types::{IntoSyntaxKind, Span};

use leekscript_core::syntax::Kind;
use leekscript_core::Type;

/// Result of parsing a type expression: either a type or an error span.
#[derive(Debug)]
pub enum TypeExprResult {
    Ok(Type),
    Err(Span),
}

/// Parse a `NodeTypeExpr` node into a `Type`. Works for both program and signature grammar.
#[must_use]
pub fn parse_type_expr(node: &SyntaxNode) -> TypeExprResult {
    if node.kind_as::<Kind>() != Some(Kind::NodeTypeExpr) {
        return TypeExprResult::Err(
            node.first_token()
                .map_or_else(|| Span::new(0, 0), |t| t.text_range()),
        );
    }

    // Collect elements: non-trivia tokens (ident/keyword or op) and NodeTypeParams, in order.
    let mut elements: Vec<TypeExprElement> = Vec::new();
    for child in node.children() {
        match child {
            SyntaxElement::Token(t) => {
                if t.is_trivia() {
                    continue;
                }
                let text = t.text().to_string();
                if t.kind_as::<Kind>() == Some(Kind::TokOp) {
                    elements.push(TypeExprElement::Op(text, t.text_range()));
                } else {
                    // Ident or keyword (type name)
                    elements.push(TypeExprElement::Ident(text, t.text_range()));
                }
            }
            SyntaxElement::Node(n) => {
                if n.kind_as::<Kind>() == Some(Kind::NodeTypeParams) {
                    elements.push(TypeExprElement::TypeParams(n.clone()));
                }
            }
        }
    }

    // When type_expr has nested structure (e.g. "integer getX()" or "Array<Cell> cells"), direct
    // children may be intermediate rule nodes, so elements is empty. Use the node's first token
    // as the type name and look for a descendant NodeTypeParams so we get e.g. Array<Cell> not Array<any>.
    if elements.is_empty() {
        if let Some(first_tok) = node.first_token() {
            let name = first_tok.text().to_string();
            let type_params = node
                .find_all_nodes(Kind::NodeTypeParams.into_syntax_kind())
                .into_iter()
                .next();
            let ty = parse_primary_type(name.as_str(), type_params.as_ref());
            if let Ok(ty) = ty {
                return TypeExprResult::Ok(ty);
            }
        }
        return TypeExprResult::Err(
            node.first_token()
                .map_or_else(|| Span::new(0, 0), |t| t.text_range()),
        );
    }

    // Split by |. Each segment is type_optional: type_primary + optional ?
    let segments = split_by_pipe(&elements);
    let mut types = Vec::with_capacity(segments.len());
    for seg in segments {
        match parse_type_optional_segment(seg) {
            Ok(ty) => types.push(ty),
            Err(span) => return TypeExprResult::Err(span),
        }
    }

    if types.is_empty() {
        return TypeExprResult::Err(
            node.first_token()
                .map_or_else(|| Span::new(0, 0), |t| t.text_range()),
        );
    }
    if types.len() == 1 {
        TypeExprResult::Ok(types.into_iter().next().unwrap())
    } else {
        TypeExprResult::Ok(Type::compound(types))
    }
}

#[derive(Debug)]
enum TypeExprElement {
    Ident(String, Span),
    Op(String, Span),
    TypeParams(SyntaxNode),
}

fn split_by_pipe(elements: &[TypeExprElement]) -> Vec<&[TypeExprElement]> {
    let mut segments = Vec::new();
    let mut start = 0;
    for (i, el) in elements.iter().enumerate() {
        if let TypeExprElement::Op(ref t, _) = el {
            if t == "|" {
                segments.push(&elements[start..i]);
                start = i + 1;
            }
        }
    }
    segments.push(&elements[start..]);
    segments
}

fn parse_type_optional_segment(seg: &[TypeExprElement]) -> Result<Type, Span> {
    if seg.is_empty() {
        return Err(Span::new(0, 0));
    }
    let optional = seg.last().and_then(|el| {
        if let TypeExprElement::Op(t, _) = el {
            if t == "?" {
                return Some(());
            }
        }
        None
    });
    let seg = if optional.is_some() {
        &seg[..seg.len().saturating_sub(1)]
    } else {
        seg
    };
    let ty = parse_type_primary_segment(seg)?;
    if optional.is_some() {
        Ok(Type::compound2(ty, Type::null()))
    } else {
        Ok(ty)
    }
}

fn parse_type_primary_segment(seg: &[TypeExprElement]) -> Result<Type, Span> {
    let first = seg.first().ok_or_else(|| Span::new(0, 0))?;
    let (name, _span) = match first {
        TypeExprElement::Ident(n, s) => (n.as_str(), *s),
        _ => return Err(first_span(first)),
    };

    // TypeParams may be at index 1 (sig grammar) or after Op("<") (program grammar: Ident, op_lt, TypeParams).
    let type_params = seg.iter().find_map(|el| {
        if let TypeExprElement::TypeParams(n) = el {
            Some(n)
        } else {
            None
        }
    });

    parse_primary_type(name, type_params)
}

fn first_span(el: &TypeExprElement) -> Span {
    match el {
        TypeExprElement::Ident(_, s) => *s,
        TypeExprElement::Op(_, s) => *s,
        TypeExprElement::TypeParams(n) => n
            .first_token()
            .map_or_else(|| Span::new(0, 0), |t| t.text_range()),
    }
}

fn parse_primary_type(name: &str, type_params: Option<&SyntaxNode>) -> Result<Type, Span> {
    let err_span = type_params
        .and_then(|n| n.first_token().map(|t| t.text_range()))
        .unwrap_or_else(|| Span::new(0, 0));

    match name {
        "integer" => Ok(Type::int()),
        "real" => Ok(Type::real()),
        "string" => Ok(Type::string()),
        "boolean" => Ok(Type::bool()),
        "void" => Ok(Type::void()),
        "any" => Ok(Type::any()),
        "null" => Ok(Type::null()),
        "Object" => Ok(Type::object()),
        "Class" => {
            let inner = type_params.and_then(parse_class_type_param);
            Ok(Type::class(inner))
        }
        "Array" => {
            let inner = type_params
                .and_then(parse_single_type_param)
                .unwrap_or(Type::any());
            Ok(Type::array(inner))
        }
        "Map" => {
            let (k, v) = type_params
                .and_then(parse_map_type_params)
                .unwrap_or((Type::any(), Type::any()));
            Ok(Type::map(k, v))
        }
        "Set" => {
            let inner = type_params
                .and_then(parse_single_type_param)
                .unwrap_or(Type::any());
            Ok(Type::set(inner))
        }
        "Interval" => {
            let inner = type_params
                .and_then(parse_single_type_param)
                .unwrap_or(Type::any());
            Ok(Type::interval(inner))
        }
        "Function" => {
            let (args, ret) = type_params
                .and_then(parse_function_type_params)
                .unwrap_or((vec![], Type::any()));
            Ok(Type::function(args, ret))
        }
        "var" => {
            // "var" is a keyword for untyped declarations, not a type name.
            Err(err_span)
        }
        _ => {
            // User class in type position (e.g. return type, variable): instance of that class
            if type_params.is_some() {
                return Err(err_span);
            }
            Ok(Type::instance(name.to_string()))
        }
    }
}

/// `NodeTypeParams` for Class: single `type_expr` child = class name (identifier type).
fn parse_class_type_param(node: &SyntaxNode) -> Option<String> {
    let type_expr = node
        .child_nodes()
        .find(|n| n.kind_as::<Kind>() == Some(Kind::NodeTypeExpr))?;
    let res = parse_type_expr(&type_expr);
    match res {
        TypeExprResult::Ok(Type::Class(Some(name)) | Type::Instance(name)) => Some(name),
        _ => None,
    }
}

/// `NodeTypeParams` with single `type_expr` (Array<T>, Set<T>, Interval<T>).
fn parse_single_type_param(node: &SyntaxNode) -> Option<Type> {
    let type_expr = node
        .child_nodes()
        .find(|n| n.kind_as::<Kind>() == Some(Kind::NodeTypeExpr))?;
    match parse_type_expr(&type_expr) {
        TypeExprResult::Ok(t) => Some(t),
        TypeExprResult::Err(_) => None,
    }
}

/// `NodeTypeParams` for Map: two `type_exprs` (K, V).
fn parse_map_type_params(node: &SyntaxNode) -> Option<(Type, Type)> {
    let type_exprs: Vec<SyntaxNode> = node
        .child_nodes()
        .filter(|n| n.kind_as::<Kind>() == Some(Kind::NodeTypeExpr))
        .collect();
    if type_exprs.len() >= 2 {
        let k = match parse_type_expr(&type_exprs[0]) {
            TypeExprResult::Ok(t) => t,
            _ => return None,
        };
        let v = match parse_type_expr(&type_exprs[1]) {
            TypeExprResult::Ok(t) => t,
            _ => return None,
        };
        Some((k, v))
    } else {
        None
    }
}

/// `NodeTypeParams` for Function: either `=> R` (0 args) or `T1, T2, ... => R` or `(T1, T2) => R`.
fn parse_function_type_params(node: &SyntaxNode) -> Option<(Vec<Type>, Type)> {
    let type_exprs: Vec<SyntaxNode> = node
        .child_nodes()
        .filter(|n| n.kind_as::<Kind>() == Some(Kind::NodeTypeExpr))
        .collect();
    if type_exprs.is_empty() {
        return None;
    }
    // Check for arrow form: last type_expr is return type, rest are params (or single (T1,T2)=>R might be one child).
    // Grammar: => type_expr (zero params) | type_expr => type_expr (one param) | type_expr , type_expr => type_expr | ...
    if type_exprs.len() == 1 {
        // Could be => R (zero params) — then the single child is R.
        // In the grammar, zero-param is "arrow" "type_expr", so we have one type_expr = return type.
        let ret = match parse_type_expr(&type_exprs[0]) {
            TypeExprResult::Ok(t) => t,
            _ => return None,
        };
        return Some((vec![], ret));
    }
    // Two or more: last is return, rest are params (or we have (T1,T2) as one node? No — type_params contains multiple type_exprs)
    let ret = match parse_type_expr(type_exprs.last()?) {
        TypeExprResult::Ok(t) => t,
        _ => return None,
    };
    let args: Result<Vec<Type>, ()> = type_exprs[..type_exprs.len() - 1]
        .iter()
        .map(|n| match parse_type_expr(n) {
            TypeExprResult::Ok(t) => Ok(t),
            TypeExprResult::Err(_) => Err(()),
        })
        .collect();
    let args = args.ok()?;
    Some((args, ret))
}

/// Find a child node of `parent` that is `NodeTypeExpr` (for var decl, param, etc.).
#[must_use]
pub fn find_type_expr_child(parent: &SyntaxNode) -> Option<SyntaxNode> {
    parent
        .child_nodes()
        .find(|n| n.kind_as::<Kind>() == Some(Kind::NodeTypeExpr))
}

/// Parameter types and return type for a `NodeAnonFn` (lambda or anonymous function).
/// Params without type annotation get `Type::any()`; return type is always `Type::any()` (no syntax for it).
/// Collects params in source order by sorting by span start (`find_all_nodes` is depth-first and may not match source order).
#[must_use]
pub fn anon_fn_types(node: &SyntaxNode) -> (Vec<Type>, Type) {
    if node.kind_as::<Kind>() != Some(Kind::NodeAnonFn) {
        return (Vec::new(), Type::any());
    }
    let mut param_nodes: Vec<SyntaxNode> = node.find_all_nodes(Kind::NodeParam.into_syntax_kind());
    param_nodes.sort_by_key(|p| p.text_range().start);
    let param_types: Vec<Type> = param_nodes
        .iter()
        .map(|p| {
            find_type_expr_child(p)
                .and_then(|te| match parse_type_expr(&te) {
                    TypeExprResult::Ok(t) => Some(t),
                    TypeExprResult::Err(_) => None,
                })
                .unwrap_or(Type::any())
        })
        .collect();
    (param_types, Type::any())
}

/// Extract parameter types and return type from a function-like node (program `NodeFunctionDecl`
/// or signature `NodeSigFunction` / `NodeSigMethod`). Collects direct children with `param_kind`
/// (e.g. `Kind::NodeParam` or `Kind::NodeSigParam`), parses each param's type, and uses the last
/// direct child `NodeTypeExpr` as the return type.
#[must_use]
pub fn param_and_return_types(
    node: &SyntaxNode,
    param_kind: Kind,
) -> (Option<Vec<Type>>, Option<Type>) {
    let param_nodes: Vec<SyntaxNode> = node
        .child_nodes()
        .filter(|n| n.kind_as::<Kind>() == Some(param_kind))
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

#[cfg(test)]
mod tests {
    use super::*;
    use leekscript_core::parse_signatures;
    use sipha::red::SyntaxElement;
    use sipha::types::IntoSyntaxKind;

    fn find_first_type_expr(node: &SyntaxNode) -> Option<SyntaxNode> {
        if node.kind_as::<Kind>() == Some(Kind::NodeTypeExpr) {
            return Some(node.clone());
        }
        for child in node.children() {
            if let SyntaxElement::Node(n) = child {
                if let Some(found) = find_first_type_expr(&n) {
                    return Some(found);
                }
            }
        }
        None
    }

    #[test]
    fn parse_type_expr_integer_from_sig() {
        let root = parse_signatures("global integer FOO\n")
            .unwrap()
            .expect("parse");
        let type_expr_node = find_first_type_expr(&root).expect("NodeTypeExpr");
        let result = parse_type_expr(&type_expr_node);
        match &result {
            TypeExprResult::Ok(t) => assert_eq!(*t, Type::int()),
            TypeExprResult::Err(_) => panic!("expected Ok(integer), got {:?}", result),
        }
    }

    #[test]
    fn parse_type_expr_real_optional_from_sig() {
        let root = parse_signatures("global real? FOO\n")
            .unwrap()
            .expect("parse");
        let type_expr_node = find_first_type_expr(&root).expect("NodeTypeExpr");
        let result = parse_type_expr(&type_expr_node);
        match &result {
            TypeExprResult::Ok(t) => {
                assert_eq!(*t, Type::compound2(Type::real(), Type::null()));
            }
            TypeExprResult::Err(_) => panic!("expected Ok(real|null), got {:?}", result),
        }
    }

    #[test]
    fn parse_type_expr_union_from_sig() {
        let root = parse_signatures("function f(real|integer x) -> real\n")
            .unwrap()
            .expect("parse");
        let type_expr_nodes = root.find_all_nodes(Kind::NodeTypeExpr.into_syntax_kind());
        assert!(!type_expr_nodes.is_empty());
        let param_type = parse_type_expr(&type_expr_nodes[0]);
        match &param_type {
            TypeExprResult::Ok(t) => {
                assert_eq!(*t, Type::compound2(Type::real(), Type::int()));
            }
            TypeExprResult::Err(_) => panic!("expected Ok(real|integer), got {:?}", param_type),
        }
    }

    /// Find a NodeTypeExpr whose first token text is `name` (e.g. "Function").
    fn find_type_expr_with_name(root: &SyntaxNode, name: &str) -> Option<SyntaxNode> {
        for node in root.find_all_nodes(Kind::NodeTypeExpr.into_syntax_kind()) {
            let first = node.first_token()?;
            if first.text() == name {
                return Some(node);
            }
        }
        None
    }

    #[test]
    fn parse_type_expr_function_two_args_from_program() {
        use leekscript_core::parse;

        let source = "var f = null as Function<integer, integer => void>;";
        let root = parse(source).unwrap().expect("parse");
        let type_expr_node =
            find_type_expr_with_name(&root, "Function").expect("Function type node");
        let result = parse_type_expr(&type_expr_node);
        match &result {
            TypeExprResult::Ok(ty) => {
                if let Type::Function { args, return_type } = ty {
                    assert_eq!(args.len(), 2, "expected 2 param types");
                    assert_eq!(args[0], Type::int());
                    assert_eq!(args[1], Type::int());
                    assert_eq!(**return_type, Type::void());
                } else {
                    panic!("expected Type::Function, got {:?}", ty);
                }
            }
            TypeExprResult::Err(_) => panic!("expected Ok(Function<...>), got {:?}", result),
        }
    }

    #[test]
    fn parse_type_expr_function_zero_params_from_sig() {
        let source = "function noArg(Function< => boolean> pred) -> void\n";
        let root = parse_signatures(source).unwrap().expect("parse");
        let type_expr_node =
            find_type_expr_with_name(&root, "Function").expect("Function type node");
        let result = parse_type_expr(&type_expr_node);
        match &result {
            TypeExprResult::Ok(ty) => {
                if let Type::Function { args, return_type } = ty {
                    assert!(args.is_empty(), "expected 0 param types");
                    assert_eq!(**return_type, Type::bool());
                } else {
                    panic!("expected Type::Function, got {:?}", ty);
                }
            }
            TypeExprResult::Err(_) => {
                panic!("expected Ok(Function< => boolean>), got {:?}", result)
            }
        }
    }

    #[test]
    fn parse_type_expr_function_one_param_from_sig() {
        let source = "function oneArg(Function<integer => void> fn) -> void\n";
        let root = parse_signatures(source).unwrap().expect("parse");
        let type_expr_node =
            find_type_expr_with_name(&root, "Function").expect("Function type node");
        let result = parse_type_expr(&type_expr_node);
        match &result {
            TypeExprResult::Ok(ty) => {
                if let Type::Function { args, return_type } = ty {
                    assert_eq!(args.len(), 1);
                    assert_eq!(args[0], Type::int());
                    assert_eq!(**return_type, Type::void());
                } else {
                    panic!("expected Type::Function, got {:?}", ty);
                }
            }
            TypeExprResult::Err(_) => {
                panic!("expected Ok(Function<integer => void>), got {:?}", result)
            }
        }
    }
}
