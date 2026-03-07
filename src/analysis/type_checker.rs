//! Type inference and checking: literal types, variable types from declarations, call arity.

use sipha::error::SemanticDiagnostic;
use sipha::red::SyntaxNode;
use sipha::types::Span;
use sipha::walk::{Visitor, WalkResult};
use std::collections::HashMap;

use crate::syntax::Kind;
use crate::types::Type;

use super::error::wrong_arity_at;
use super::node_helpers::{expr_identifier, var_decl_info};
use super::scope::{ScopeId, ScopeStore};

/// Tracks inferred types and runs arity checks. Runs after scope building and validation.
pub struct TypeChecker<'a> {
    pub store: &'a ScopeStore,
    /// Root of the tree (for ancestor/sibling lookup).
    root: &'a SyntaxNode,
    stack: Vec<ScopeId>,
    next_scope_id: usize,
    /// Variable name -> Type for current and ancestor scopes (stack of maps).
    var_types: Vec<HashMap<String, Type>>,
    /// Types pushed by expression nodes (last pushed = last expression result).
    type_stack: Vec<Type>,
    /// Last primary expression identifier (for call arity: callee name).
    last_primary_ident: Option<String>,
    pub diagnostics: Vec<SemanticDiagnostic>,
}

impl<'a> TypeChecker<'a> {
    #[must_use] 
    pub fn new(store: &'a ScopeStore, root: &'a SyntaxNode) -> Self {
        Self {
            store,
            root,
            stack: vec![ScopeId(0)],
            next_scope_id: 1,
            var_types: vec![HashMap::new()],
            type_stack: Vec::new(),
            last_primary_ident: None,
            diagnostics: Vec::new(),
        }
    }

    fn current_scope(&self) -> ScopeId {
        *self.stack.last().unwrap_or(&ScopeId(0))
    }

    fn push_scope(&mut self) {
        self.stack.push(ScopeId(self.next_scope_id));
        self.next_scope_id += 1;
        self.var_types.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
            self.var_types.pop();
        }
    }

    fn lookup_var_type(&self, name: &str) -> Type {
        for map in self.var_types.iter().rev() {
            if let Some(t) = map.get(name) {
                return t.clone();
            }
        }
        Type::any()
    }

    fn add_var_type(&mut self, name: String, ty: Type) {
        if let Some(map) = self.var_types.last_mut() {
            map.insert(name, ty);
        }
    }

    /// True if the given function name accepts the given argument count (any scope).
    fn function_accepts_arity(&self, name: &str, arity: usize) -> bool {
        let mut id = Some(self.current_scope());
        while let Some(scope_id) = id {
            if let Some(scope) = self.store.get(scope_id) {
                if scope.function_accepts_arity(name, arity) {
                    return true;
                }
                id = scope.parent;
            } else {
                break;
            }
        }
        false
    }
}

impl Visitor for TypeChecker<'_> {
    fn enter_node(&mut self, node: &SyntaxNode) -> WalkResult {
        let kind = match node.kind_as::<Kind>() {
            Some(k) => k,
            None => return WalkResult::Continue(()),
        };

        match kind {
            Kind::NodeBlock
            | Kind::NodeWhileStmt
            | Kind::NodeForStmt
            | Kind::NodeForInStmt
            | Kind::NodeDoWhileStmt => self.push_scope(),
            Kind::NodeFunctionDecl | Kind::NodeClassDecl => self.push_scope(),
            _ => {}
        }

        WalkResult::Continue(())
    }

    fn leave_node(&mut self, node: &SyntaxNode) -> WalkResult {
        let kind = match node.kind_as::<Kind>() {
            Some(k) => k,
            None => return WalkResult::Continue(()),
        };

        match kind {
            Kind::NodeBlock
            | Kind::NodeWhileStmt
            | Kind::NodeForStmt
            | Kind::NodeForInStmt
            | Kind::NodeDoWhileStmt => self.pop_scope(),
            Kind::NodeFunctionDecl | Kind::NodeClassDecl => self.pop_scope(),
            Kind::NodePrimaryExpr => {
                if let Some((name, _)) = expr_identifier(node) {
                    self.last_primary_ident = Some(name.clone());
                    let ty = self.lookup_var_type(&name);
                    self.type_stack.push(ty);
                } else {
                    // Literal or other primary; do not clear last_primary_ident (may be callee)
                    let ty = infer_primary_type(node);
                    self.type_stack.push(ty);
                }
            }
            Kind::NodeCallExpr => {
                // Argument count: each "expr" in the grammar is one child node (not necessarily NodeExpr).
                let actual_arity = node.child_nodes().count();
                if let Some(callee_name) = self.last_primary_ident.take() {
                    if !self.function_accepts_arity(&callee_name, actual_arity) {
                        // Find an expected arity for the diagnostic (first overload's max), if known.
                        let mut expected = None;
                        let mut id = Some(self.current_scope());
                        while let Some(scope_id) = id {
                            if let Some(scope) = self.store.get(scope_id) {
                                if let Some(arity) = scope.get_function_arity(&callee_name) {
                                    expected = Some(arity);
                                    break;
                                }
                                id = scope.parent;
                            } else {
                                break;
                            }
                        }
                        if let Some(exp) = expected {
                            let span = node
                                .first_token().map_or_else(|| Span::new(0, 0), |t| t.text_range());
                            self.diagnostics
                                .push(wrong_arity_at(span, exp, actual_arity));
                        }
                    }
                }
                // Pop callee type + argument types (postfix order: callee, then args)
                for _ in 0..actual_arity {
                    self.type_stack.pop();
                }
                self.type_stack.pop(); // callee
                self.type_stack.push(Type::any());
            }
            Kind::NodeMemberExpr | Kind::NodeIndexExpr => {
                self.last_primary_ident = None;
                self.type_stack.push(Type::any());
            }
            Kind::NodeVarDecl => {
                if let Some(info) = var_decl_info(node) {
                    let ty = self
                        .type_stack
                        .pop()
                        .unwrap_or(Type::any());
                    self.add_var_type(info.name, ty);
                }
            }
            Kind::NodeExpr => {
                // NodeExpr wraps another expression; type already pushed by inner node.
                // Do nothing to avoid double-push.
            }
            _ => {}
        }

        WalkResult::Continue(())
    }
}

/// Infer type for a primary expression (literal; identifier handled above with lookup).
fn infer_primary_type(node: &SyntaxNode) -> Type {
    let first = node.first_token();
    let first = match first {
        Some(t) => t,
        None => return Type::any(),
    };
    match first.kind_as::<Kind>() {
        Some(Kind::TokNumber) => {
            let text = first.text();
            if text.contains('.') || text.to_lowercase().contains('e') {
                Type::real()
            } else {
                Type::int()
            }
        }
        Some(Kind::TokString) => Type::string(),
        Some(Kind::KwTrue | Kind::KwFalse) => Type::bool(),
        Some(Kind::KwNull) => Type::null(),
        _ => Type::any(),
    }
}
