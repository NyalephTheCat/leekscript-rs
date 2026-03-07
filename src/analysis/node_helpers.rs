//! Helpers to extract names and structure from syntax nodes (VarDecl, FunctionDecl, ClassDecl, etc.).

use sipha::red::{SyntaxElement, SyntaxNode, SyntaxToken};
use sipha::types::Span;

use crate::syntax::Kind;

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

/// Returns the declaration kind and name from a NodeVarDecl.
/// The name is the token from keyword_or_ident (after "var"/"global"/"const"/"let" or type_expr).
pub fn var_decl_info(node: &SyntaxNode) -> Option<VarDeclInfo> {
    if node.kind_as::<Kind>() != Some(Kind::NodeVarDecl) {
        return None;
    }
    let mut kind = VarDeclKind::Typed;
    let mut skip_one = true;
    for child in node.children() {
        match child {
            SyntaxElement::Token(t) if !t.is_trivia() => {
                if skip_one {
                    kind = match t.text() {
                        "var" => VarDeclKind::Var,
                        "global" => VarDeclKind::Global,
                        "const" => VarDeclKind::Const,
                        "let" => VarDeclKind::Let,
                        _ => VarDeclKind::Typed,
                    };
                    skip_one = false;
                } else {
                    return Some(VarDeclInfo {
                        kind,
                        name: t.text().to_string(),
                        name_span: t.text_range(),
                    });
                }
            }
            SyntaxElement::Node(n) => {
                if skip_one {
                    skip_one = false;
                } else if let Some(tok) = n.first_token() {
                    return Some(VarDeclInfo {
                        kind,
                        name: tok.text().to_string(),
                        name_span: tok.text_range(),
                    });
                }
            }
            _ => {}
        }
    }
    None
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
