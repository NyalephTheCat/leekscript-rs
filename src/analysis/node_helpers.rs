//! Helpers to extract names and structure from syntax nodes (`VarDecl`, `FunctionDecl`, `ClassDecl`, etc.).

use sipha::red::{SyntaxElement, SyntaxNode, SyntaxToken};
use sipha::types::Span;

use crate::syntax::{Kind, FIELD_RHS};
use crate::types::Type;

/// Declaration kind for a variable declaration node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarDeclKind {
    Var,
    Global,
    Const,
    Let,
    /// Typed form (e.g. `integer x = 0`) with no leading keyword.
    Typed,
}

/// Info extracted from a `NodeVarDecl` (var/global/const/let or typed).
pub struct VarDeclInfo {
    pub kind: VarDeclKind,
    pub name: String,
    pub name_span: Span,
}

/// Collect identifier tokens (name, span) from node's subtree in source order, stopping at first "=".
fn idents_before_assign(
    node: &SyntaxNode,
    idents: &mut Vec<(String, Span)>,
    first_token_text: &mut Option<String>,
) -> bool {
    use sipha::red::SyntaxElement;
    for elem in node.children() {
        match elem {
            SyntaxElement::Token(t) if !t.is_trivia() => {
                if first_token_text.is_none() {
                    *first_token_text = Some(t.text().to_string());
                }
                if t.text() == "=" {
                    return true;
                }
                if t.kind_as::<Kind>() == Some(Kind::TokIdent) {
                    idents.push((t.text().to_string(), t.text_range()));
                }
            }
            SyntaxElement::Node(n) => {
                if idents_before_assign(&n, idents, first_token_text) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Returns the right-hand side expression of a `NodeBinaryExpr`.
/// Prefers the named field "rhs" when present (legacy mul-level); otherwise uses the last node child
/// (sipha precedence climbing produces [op, lower] so RHS is the last node).
#[must_use]
pub fn binary_expr_rhs(node: &SyntaxNode) -> Option<SyntaxNode> {
    if node.kind_as::<Kind>() != Some(Kind::NodeBinaryExpr) {
        return None;
    }
    node.field_by_id(FIELD_RHS).or_else(|| node.child_nodes().last())
}

/// Returns the member name (identifier after the dot) from a `NodeMemberExpr`.
#[must_use]
pub fn member_expr_member_name(node: &SyntaxNode) -> Option<String> {
    if node.kind_as::<Kind>() != Some(Kind::NodeMemberExpr) {
        return None;
    }
    let mut saw_dot = false;
    for child in node.children() {
        if let SyntaxElement::Token(t) = &child {
            if !t.is_trivia() {
                if t.text() == "." {
                    saw_dot = true;
                } else if saw_dot {
                    return Some(t.text().to_string());
                }
            }
        }
    }
    None
}

/// Returns the declaration kind and name from a `NodeVarDecl`.
/// For "var x", "global T x": name is the first identifier after the keyword.
/// For typed form "Array<EffectOverTime> arr" or "integer? y": name is the *last* identifier before "=" (type names come first).
pub fn var_decl_info(node: &SyntaxNode) -> Option<VarDeclInfo> {
    if node.kind_as::<Kind>() != Some(Kind::NodeVarDecl) {
        return None;
    }
    let mut idents = Vec::new();
    let mut first_token_text: Option<String> = None;
    idents_before_assign(node, &mut idents, &mut first_token_text);
    let first_token_text = first_token_text.as_deref();
    let last_idx = idents.len().saturating_sub(1);
    let (kind, name_idx) = match first_token_text {
        Some("var") => (VarDeclKind::Var, 0),
        Some("global") => (VarDeclKind::Global, last_idx),
        Some("const") => (VarDeclKind::Const, 0),
        Some("let") => (VarDeclKind::Let, 0),
        _ => (VarDeclKind::Typed, last_idx),
    };
    let (name, name_span) = idents.get(name_idx).cloned()?;
    Some(VarDeclInfo {
        kind,
        name,
        name_span,
    })
}

/// Info extracted from a `NodeFunctionDecl` (name and parameter counts).
pub struct FunctionDeclInfo {
    pub name: String,
    pub name_span: Span,
    /// Minimum number of arguments (params without a default value).
    pub min_arity: usize,
    /// Maximum number of arguments (total params; for overloads we store multiple ranges).
    pub max_arity: usize,
}

/// Number of argument expressions in a `NodeCallExpr`. Used for type inference so the call
/// is inferred as the function's return type. The grammar uses lparen, optional(expr, zero_or_more(comma, expr)), rparen;
/// the optional may be one node (with 0, 1, or more children) or optional + zero_or_more as siblings.
#[must_use]
pub fn call_argument_count(node: &SyntaxNode) -> usize {
    if node.kind_as::<Kind>() != Some(Kind::NodeCallExpr) {
        return 0;
    }
    let content_nodes: Vec<SyntaxNode> = node.child_nodes().collect();
    if content_nodes.is_empty() {
        return 0;
    }
    let first_children = content_nodes[0].child_nodes().count();
    if content_nodes.len() == 1 {
        // Single node: optional(expr, zero_or_more(...)) with 0, 1, or more expr children.
        return first_children;
    }
    // First node = optional (0 or 1 expr), rest = one zero_or_more node per (comma, expr).
    first_children.min(1) + content_nodes.len().saturating_sub(1)
}

/// Returns the syntax node for the `i`-th argument expression of a `NodeCallExpr` (0-based), for error spans.
#[must_use]
pub fn call_argument_node(node: &SyntaxNode, i: usize) -> Option<SyntaxNode> {
    if node.kind_as::<Kind>() != Some(Kind::NodeCallExpr) {
        return None;
    }
    let content_nodes: Vec<SyntaxNode> = node.child_nodes().collect();
    let args: Vec<SyntaxNode> = if content_nodes.len() == 1 {
        content_nodes[0].child_nodes().collect()
    } else {
        let mut v = Vec::new();
        if let Some(expr) = content_nodes.first().and_then(|n| n.child_nodes().next()) {
            v.push(expr);
        }
        for n in &content_nodes[1..] {
            if let Some(expr) = n.child_nodes().next() {
                v.push(expr);
            }
        }
        v
    };
    args.into_iter().nth(i)
}

/// Returns true if this `NodeParam` has a default value (`= expr`).
pub fn param_has_default(node: &SyntaxNode) -> bool {
    if node.kind_as::<Kind>() != Some(Kind::NodeParam) {
        return false;
    }
    node.descendant_tokens()
        .iter()
        .any(|t| t.text() == "=")
}

/// Returns name and parameter counts from a `NodeFunctionDecl`.
/// For default parameters, `min_arity` is the number of required params; `max_arity` is total.
pub fn function_decl_info(node: &SyntaxNode) -> Option<FunctionDeclInfo> {
    if node.kind_as::<Kind>() != Some(Kind::NodeFunctionDecl) {
        return None;
    }
    let tokens: Vec<SyntaxToken> = node.non_trivia_tokens().collect();
    let lparen_idx = tokens.iter().position(|t| t.text() == "(")?;
    let name_token = tokens.get(lparen_idx.checked_sub(1)?)?;
    let name = name_token.text().to_string();
    let name_span = name_token.text_range();
    let params: Vec<SyntaxNode> = node
        .child_nodes()
        .filter(|n| n.kind_as::<Kind>() == Some(Kind::NodeParam))
        .collect();
    let min_arity = params
        .iter()
        .take_while(|p| !param_has_default(p))
        .count();
    let max_arity = params.len();
    Some(FunctionDeclInfo {
        name,
        name_span,
        min_arity,
        max_arity,
    })
}

/// Info extracted from a `NodeClassDecl` (name and optional super class).
pub struct ClassDeclInfo {
    pub name: String,
    pub name_span: Span,
    /// Name of the class this extends, if any.
    pub super_class: Option<String>,
}

/// Returns class name and optional super class from a `NodeClassDecl`.
pub fn class_decl_info(node: &SyntaxNode) -> Option<ClassDeclInfo> {
    if node.kind_as::<Kind>() != Some(Kind::NodeClassDecl) {
        return None;
    }
    let tokens: Vec<SyntaxToken> = node.non_trivia_tokens().collect();
    let class_idx = tokens.iter().position(|t| t.text() == "class")?;
    let name_token = tokens.get(class_idx + 1)?;
    let name = name_token.text().to_string();
    let name_span = name_token.text_range();
    let super_class = tokens
        .iter()
        .skip(class_idx + 2)
        .position(|t| t.text() == "extends")
        .and_then(|extends_offset| {
            let idx = class_idx + 2 + extends_offset + 1;
            tokens.get(idx).filter(|t| t.text() != "{").map(|t| t.text().to_string())
        });
    Some(ClassDeclInfo {
        name,
        name_span,
        super_class,
    })
}

/// If the node is or contains a simple identifier (single identifier token),
/// returns its text and span. Handles `NodeExpr` and `NodePrimaryExpr` (expr may not wrap in `NodeExpr` in the grammar).
pub fn expr_identifier(node: &SyntaxNode) -> Option<(String, Span)> {
    let kind = node.kind_as::<Kind>()?;
    if kind != Kind::NodeExpr && kind != Kind::NodePrimaryExpr {
        return None;
    }
    let first = node.first_token()?;
    let last = node.last_token()?;
    if first.offset() != last.offset() {
        return None;
    }
    if first.kind_as::<Kind>() == Some(Kind::TokIdent) {
        Some((first.text().to_string(), first.text_range()))
    } else {
        None
    }
}

/// Name to use for type/scope resolution from a primary expr: identifier, or "this"/"super" when
/// the single token is the corresponding keyword (class methods don't use the `function` keyword).
#[must_use]
pub fn primary_expr_resolvable_name(node: &SyntaxNode) -> Option<String> {
    if let Some((name, _)) = expr_identifier(node) {
        return Some(name);
    }
    let kind = node.kind_as::<Kind>()?;
    if kind != Kind::NodePrimaryExpr {
        return None;
    }
    let first = node.first_token()?;
    let last = node.last_token()?;
    if first.offset() != last.offset() {
        return None;
    }
    match first.kind_as::<Kind>() {
        Some(Kind::KwThis) => Some("this".to_string()),
        Some(Kind::KwSuper) => Some("super".to_string()),
        _ => None,
    }
}

/// Check if this node is a simple identifier expression (for resolution).
#[allow(dead_code)]
pub fn is_identifier_expr(node: &SyntaxNode) -> bool {
    expr_identifier(node).is_some()
}

/// Returns the direct child node of `NodeForInStmt` that is the iterable expression (the expr after `in`).
#[must_use]
pub fn for_in_iterable_expr(for_in_node: &SyntaxNode) -> Option<SyntaxNode> {
    if for_in_node.kind_as::<Kind>() != Some(Kind::NodeForInStmt) {
        return None;
    }
    let mut seen_in = false;
    for child in for_in_node.children() {
        if let SyntaxElement::Token(t) = &child {
            if !t.is_trivia() && t.text() == "in" {
                seen_in = true;
                continue;
            }
        }
        if seen_in {
            if let SyntaxElement::Node(n) = child {
                return Some(n.clone());
            }
        }
    }
    None
}

/// Variable name(s) and span(s) from a `NodeForInStmt`: key and optionally value (for key : valueVar in expr).
/// Skips `type_expr` nodes and for/var/in/paren tokens so we get only the loop variable identifiers.
pub fn for_in_loop_vars(node: &SyntaxNode) -> Vec<(String, Span)> {
    if node.kind_as::<Kind>() != Some(Kind::NodeForInStmt) {
        return Vec::new();
    }
    let skip_tokens: &[&str] = &["for", "(", ")", "var", "in", ":"];
    let mut vars = Vec::new();
    let mut state = 0u8; // 0 = need key, 1 = need colon or in, 2 = need value
    for child in node.children() {
        match child {
            SyntaxElement::Token(t) if !t.is_trivia() => {
                let text = t.text();
                if text == "in" {
                    break;
                }
                if skip_tokens.contains(&text) {
                    if text == ":" && state == 1 {
                        state = 2;
                    }
                    continue;
                }
                if state == 0 {
                    vars.push((text.to_string(), t.text_range()));
                    state = 1;
                } else if state == 2 {
                    vars.push((text.to_string(), t.text_range()));
                    break;
                }
            }
            SyntaxElement::Node(n) => {
                if n.kind_as::<Kind>() == Some(Kind::NodeTypeExpr) {
                    continue;
                }
                if state == 0 {
                    if let Some(tok) = n.first_token() {
                        if !tok.is_trivia() && !skip_tokens.contains(&tok.text()) {
                            vars.push((tok.text().to_string(), tok.text_range()));
                            state = 1;
                        }
                    }
                } else if state == 2 {
                    if let Some(tok) = n.first_token() {
                        if !tok.is_trivia() && !skip_tokens.contains(&tok.text()) {
                            vars.push((tok.text().to_string(), tok.text_range()));
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    vars
}

/// True if this `NodeExpr` is a ternary expression (cond ? then : else).
#[must_use]
pub fn is_ternary_expr(node: &SyntaxNode) -> bool {
    if node.kind_as::<Kind>() != Some(Kind::NodeExpr) {
        return false;
    }
    let mut has_question = false;
    let mut has_colon = false;
    for child in node.children() {
        if let SyntaxElement::Token(t) = child {
            if !t.is_trivia() {
                match t.text() {
                    "?" => has_question = true,
                    ":" => has_colon = true,
                    _ => {}
                }
            }
        }
    }
    has_question && has_colon
}

/// Null-check ops: then branch is non-null when condition is true.
const NULL_CHECK_OPS: &[&str] = &["!=", "!==", "==", "==="];

/// Returns (var_name, then_is_non_null) if `node` is a null-check condition (e.g. `var != null`, `var === null`).
/// Searches recursively for a binary with one identifier and one null. `root` is used to get parent/siblings.
#[must_use]
pub fn null_check_from_condition(condition_node: &SyntaxNode, root: &SyntaxNode) -> Option<(String, bool)> {
    find_null_check_binary(condition_node, root)
}

fn find_null_check_binary(node: &SyntaxNode, root: &SyntaxNode) -> Option<(String, bool)> {
    if node.kind_as::<Kind>() == Some(Kind::NodeBinaryExpr) {
        let op = node
            .children()
            .find_map(|c| {
                if let SyntaxElement::Token(t) = c {
                    if !t.is_trivia() {
                        let text = t.text();
                        if NULL_CHECK_OPS.contains(&text) {
                            return Some(text.to_string());
                        }
                    }
                }
                None
            })?;
        let rhs = binary_expr_rhs(node)?;
        let lhs = prev_sibling_node(node, root)?;
        let (var_name, _var_on_left) = if let Some((name, _)) = expr_identifier(&lhs) {
            if is_null_literal(&rhs) {
                (name, true)
            } else {
                return find_null_check_binary_recurse(node, root);
            }
        } else if let Some((name, _)) = expr_identifier(&rhs) {
            if is_null_literal(&lhs) {
                (name, false)
            } else {
                return find_null_check_binary_recurse(node, root);
            }
        } else {
            return find_null_check_binary_recurse(node, root);
        };
        // then_is_non_null: for "var != null" or "null != var", then branch is non-null
        let then_is_non_null = op == "!=" || op == "!==";
        return Some((var_name, then_is_non_null));
    }
    find_null_check_binary_recurse(node, root)
}

fn find_null_check_binary_recurse(node: &SyntaxNode, root: &SyntaxNode) -> Option<(String, bool)> {
    for child in node.child_nodes() {
        if let Some(r) = find_null_check_binary(&child, root) {
            return Some(r);
        }
    }
    None
}

/// Previous sibling that is a node (skips tokens).
fn prev_sibling_node(node: &SyntaxNode, root: &SyntaxNode) -> Option<SyntaxNode> {
    let ancestors: Vec<SyntaxNode> = node.ancestors(root);
    let parent = ancestors.first()?;
    let mut idx = None;
    for (i, c) in parent.children().enumerate() {
        if let SyntaxElement::Node(n) = c {
            if n.text_range() == node.text_range() {
                idx = Some(i);
                break;
            }
        }
    }
    let i = idx?;
    parent
        .children()
        .take(i)
        .filter_map(|e| e.as_node().cloned())
        .last()
}

fn is_null_literal(node: &SyntaxNode) -> bool {
    node.first_token()
        .map(|t| t.text() == "null")
        .unwrap_or(false)
}

/// Index of `node` among `parent`'s children (including tokens). Returns `None` if not a direct child.
#[must_use]
pub fn node_index_in_parent(node: &SyntaxNode, parent: &SyntaxNode) -> Option<usize> {
    for (i, c) in parent.children().enumerate() {
        if let SyntaxElement::Node(n) = c {
            if n.text_range() == node.text_range() {
                return Some(i);
            }
        }
    }
    None
}

/// Parameter name and span from a `NodeParam` (for scope building).
pub fn param_name(node: &SyntaxNode) -> Option<(String, Span)> {
    if node.kind_as::<Kind>() != Some(Kind::NodeParam) {
        return None;
    }
    let tokens: Vec<SyntaxToken> = node.non_trivia_tokens().collect();
    let name_token = tokens
        .iter()
        .take_while(|t| t.text() != "=")
        .last()
        .or_else(|| tokens.first())?;
    Some((name_token.text().to_string(), name_token.text_range()))
}

/// Field name, optional declared type, and whether it's static from a `NodeClassField`.
/// Returns (name, type, is_static) where type is None if the field has no type annotation.
#[must_use]
pub fn class_field_info(node: &SyntaxNode) -> Option<(String, Option<Type>, bool)> {
    use super::type_expr::{parse_type_expr, TypeExprResult};
    if node.kind_as::<Kind>() != Some(Kind::NodeClassField) {
        return None;
    }
    let tokens: Vec<SyntaxToken> = node.non_trivia_tokens().collect();
    let is_static = tokens.first().map_or(false, |t| t.text() == "static");
    let name_token = tokens
        .iter()
        .take_while(|t| t.text() != "=" && t.text() != ";")
        .last()?;
    let name = name_token.text().to_string();
    let type_expr_node = node
        .child_nodes()
        .find(|n| n.kind_as::<Kind>() == Some(Kind::NodeTypeExpr));
    let ty = type_expr_node.and_then(|te| match parse_type_expr(&te) {
        TypeExprResult::Ok(t) => Some(t),
        TypeExprResult::Err(_) => None,
    });
    Some((name, ty, is_static))
}

/// True if this `NodeFunctionDecl` is a static class method (has "static" as preceding sibling in class body).
#[must_use]
pub fn class_method_is_static(node: &SyntaxNode, root: &SyntaxNode) -> bool {
    if node.kind_as::<Kind>() != Some(Kind::NodeFunctionDecl) {
        return false;
    }
    let ancestors: Vec<SyntaxNode> = node.ancestors(root);
    let parent = match ancestors.first() {
        Some(p) => p,
        None => return false,
    };
    let node_start = node.text_range().start;
    // Last non-trivia token in parent that ends before this node (source order) - if "static", method is static.
    let mut last_before: Option<String> = None;
    for token in parent.descendant_tokens() {
        if token.is_trivia() {
            continue;
        }
        let range = token.text_range();
        if range.end <= node_start {
            last_before = Some(token.text().to_string());
        } else {
            break;
        }
    }
    last_before.as_deref() == Some("static")
}
