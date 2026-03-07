//! Helpers to extract names and structure from syntax nodes (VarDecl, FunctionDecl, ClassDecl, etc.).

use sipha::red::{SyntaxElement, SyntaxNode, SyntaxToken};
use sipha::types::Span;

use crate::syntax::{Kind, FIELD_RHS};

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

/// Info extracted from a NodeVarDecl (var/global/const/let or typed).
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

/// Returns the right-hand side expression of a `NodeBinaryExpr` when the grammar labels it (e.g. mul-level `a * b`).
/// Uses the named field "rhs" so structure is stable regardless of child order.
pub fn binary_expr_rhs(node: &SyntaxNode) -> Option<SyntaxNode> {
    if node.kind_as::<Kind>() != Some(Kind::NodeBinaryExpr) {
        return None;
    }
    node.field_by_id(FIELD_RHS)
}

/// Returns the declaration kind and name from a NodeVarDecl.
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

/// Info extracted from a NodeFunctionDecl (name and parameter counts).
pub struct FunctionDeclInfo {
    pub name: String,
    pub name_span: Span,
    /// Total number of parameters.
    pub arity: usize,
    /// Minimum number of arguments (params without a default value).
    pub min_arity: usize,
    /// Maximum number of arguments (same as arity; for overloads we store multiple ranges).
    pub max_arity: usize,
}

/// Returns true if this NodeParam has a default value (`= expr`).
pub fn param_has_default(node: &SyntaxNode) -> bool {
    if node.kind_as::<Kind>() != Some(Kind::NodeParam) {
        return false;
    }
    node.descendant_tokens()
        .iter()
        .any(|t| t.text() == "=")
}

/// Returns name and parameter counts from a NodeFunctionDecl.
/// For default parameters, min_arity is the number of required params; max_arity is total.
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
    let arity = params.len();
    let min_arity = params
        .iter()
        .take_while(|p| !param_has_default(p))
        .count();
    Some(FunctionDeclInfo {
        name,
        name_span,
        arity,
        min_arity,
        max_arity: arity,
    })
}

/// Info extracted from a NodeClassDecl (name only for scope).
pub struct ClassDeclInfo {
    pub name: String,
    pub name_span: Span,
}

/// Returns class name from a NodeClassDecl.
pub fn class_decl_info(node: &SyntaxNode) -> Option<ClassDeclInfo> {
    if node.kind_as::<Kind>() != Some(Kind::NodeClassDecl) {
        return None;
    }
    let tokens: Vec<SyntaxToken> = node.non_trivia_tokens().collect();
    let class_idx = tokens.iter().position(|t| t.text() == "class")?;
    let name_token = tokens.get(class_idx + 1)?;
    Some(ClassDeclInfo {
        name: name_token.text().to_string(),
        name_span: name_token.text_range(),
    })
}

/// If the node is or contains a simple identifier (single identifier token),
/// returns its text and span. Handles NodeExpr and NodePrimaryExpr (expr may not wrap in NodeExpr in the grammar).
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

/// Check if this node is a simple identifier expression (for resolution).
pub fn is_identifier_expr(node: &SyntaxNode) -> bool {
    expr_identifier(node).is_some()
}

/// Variable name(s) and span(s) from a NodeForInStmt: key and optionally value (for key : valueVar in expr).
/// Skips type_expr nodes and for/var/in/paren tokens so we get only the loop variable identifiers.
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

/// Parameter name and span from a NodeParam (for scope building).
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
