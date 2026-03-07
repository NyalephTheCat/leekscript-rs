//! Scope-building pass: walk the tree and build the scope chain with variables, functions, classes.

use sipha::red::SyntaxNode;
use sipha::walk::{Visitor, WalkResult};

use crate::syntax::Kind;

use crate::types::Type;

use super::node_helpers::{
    class_decl_info, class_field_info, class_method_is_static, for_in_loop_vars, function_decl_info,
    param_name, var_decl_info,
};
use super::scope::{ScopeId, ScopeKind, ScopeStore, VariableInfo, VariableKind};
use super::type_expr::{find_type_expr_child, parse_type_expr, TypeExprResult};

/// Extract param types and return type from a NodeFunctionDecl (for top-level or method).
fn function_decl_types(node: &SyntaxNode) -> (Option<Vec<Type>>, Option<Type>) {
    let param_nodes: Vec<SyntaxNode> = node
        .child_nodes()
        .filter(|n| n.kind_as::<Kind>() == Some(Kind::NodeParam))
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
    // Return type: last direct child NodeTypeExpr (after params, before block).
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

/// Builds scope tree by walking the syntax tree; maintains a stack of scope IDs.
/// Records the sequence of scope IDs pushed (in walk order) so Validator can use the same IDs.
pub struct ScopeBuilder {
    pub store: ScopeStore,
    stack: Vec<ScopeId>,
    /// Scope IDs in the order they were pushed (for Validator sync).
    pub scope_id_sequence: Vec<ScopeId>,
    /// Class name at each nesting level (for recording fields/methods).
    class_stack: Vec<String>,
    /// Root node (set on first enter) for static method detection.
    root: Option<SyntaxNode>,
}

impl ScopeBuilder {
    #[must_use] 
    pub fn new() -> Self {
        let store = ScopeStore::new();
        let stack = vec![store.root_id()];
        Self {
            store,
            stack,
            scope_id_sequence: Vec::new(),
            class_stack: Vec::new(),
            root: None,
        }
    }

    /// Build scope from a program tree using an existing store (e.g. pre-seeded from signature files).
    #[must_use] 
    pub fn with_store(store: ScopeStore) -> Self {
        let root_id = store.root_id();
        Self {
            store,
            stack: vec![root_id],
            scope_id_sequence: Vec::new(),
            class_stack: Vec::new(),
            root: None,
        }
    }

    fn current(&self) -> Option<ScopeId> {
        self.stack.last().copied()
    }

    fn push(&mut self, kind: ScopeKind) {
        let parent = self.current().expect("scope stack empty");
        let id = self.store.push(kind, parent);
        self.stack.push(id);
        self.scope_id_sequence.push(id);
    }

    fn pop(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }

    fn main_scope(&self) -> ScopeId {
        self.store.root_id()
    }

    /// True when we're inside a class body (so we're building a method, not a top-level function).
    fn in_class_scope(&self) -> bool {
        self.stack.iter().any(|&id| {
            self.store
                .get(id)
                .is_some_and(|s| s.kind == ScopeKind::Class)
        })
    }
}

impl Default for ScopeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Visitor for ScopeBuilder {
    fn enter_node(&mut self, node: &SyntaxNode) -> WalkResult {
        if self.root.is_none() {
            self.root = Some(node.clone());
        }
        let kind = match node.kind_as::<Kind>() {
            Some(k) => k,
            None => return WalkResult::Continue(()),
        };

        match kind {
            Kind::NodeBlock => {
                self.push(ScopeKind::Block);
            }
            Kind::NodeFunctionDecl => {
                if self.in_class_scope() {
                    if let Some(class_name) = self.class_stack.last().cloned() {
                        if let Some(info) = function_decl_info(node) {
                            let (param_types, return_type) = function_decl_types(node);
                            let params = param_types.unwrap_or_default();
                            let ret = return_type.unwrap_or(Type::any());
                            let is_static = self
                                .root
                                .as_ref()
                                .map_or(false, |root| class_method_is_static(node, root));
                            if is_static {
                                self.store.add_class_static_method(
                                    &class_name,
                                    info.name,
                                    params,
                                    ret,
                                );
                            } else {
                                self.store.add_class_method(
                                    &class_name,
                                    info.name,
                                    params,
                                    ret,
                                );
                            }
                        }
                    }
                } else {
                    // Only register in main scope for top-level functions, not class methods.
                    if let Some(info) = function_decl_info(node) {
                        if let Some(main_scope) = self.store.get_mut(self.main_scope()) {
                            let (param_types, return_type) = function_decl_types(node);
                            if param_types.is_some() || return_type.is_some() {
                                main_scope.add_function_with_types(
                                    info.name.clone(),
                                    info.min_arity,
                                    info.max_arity,
                                    info.name_span,
                                    param_types,
                                    return_type,
                                );
                            } else {
                                main_scope.add_function(
                                    info.name.clone(),
                                    info.min_arity,
                                    info.max_arity,
                                    info.name_span,
                                );
                            }
                        }
                    }
                }
                self.push(ScopeKind::Function);
            }
            Kind::NodeClassDecl => {
                if let Some(info) = class_decl_info(node) {
                    if let Some(main_scope) = self.store.get_mut(self.main_scope()) {
                        main_scope.add_class(info.name.clone(), info.name_span);
                    }
                    self.class_stack.push(info.name.clone());
                }
                self.push(ScopeKind::Class);
            }
            Kind::NodeClassField => {
                if let Some(class_name) = self.class_stack.last() {
                    if let Some((field_name, ty, is_static)) = class_field_info(node) {
                        let ty = ty.unwrap_or(Type::any());
                        if is_static {
                            self.store.add_class_static_field(class_name, field_name, ty);
                        } else {
                            self.store.add_class_field(class_name, field_name, ty);
                        }
                    }
                }
            }
            Kind::NodeWhileStmt | Kind::NodeForStmt | Kind::NodeForInStmt | Kind::NodeDoWhileStmt => {
                self.push(ScopeKind::Loop);
                if matches!(kind, Kind::NodeForInStmt) {
                    for (name, span) in for_in_loop_vars(node) {
                        if let Some(current_id) = self.current() {
                            if let Some(scope) = self.store.get_mut(current_id) {
                                scope.add_variable(VariableInfo {
                                    name,
                                    kind: VariableKind::Local,
                                    span,
                                    declared_type: None,
                                });
                            }
                        }
                    }
                }
            }
            Kind::NodeVarDecl => {
                if let Some(info) = var_decl_info(node) {
                    let declared_type = find_type_expr_child(node)
                        .and_then(|te| match parse_type_expr(&te) {
                            TypeExprResult::Ok(t) => Some(t),
                            TypeExprResult::Err(_) => None,
                        });
                    let var_kind = match info.kind {
                        super::node_helpers::VarDeclKind::Global => {
                            if let Some(main_scope) = self.store.get_mut(self.main_scope()) {
                                main_scope.add_global(info.name.clone());
                            }
                            VariableKind::Global
                        }
                        _ => VariableKind::Local,
                    };
                    if let Some(current_id) = self.current() {
                        if let Some(scope) = self.store.get_mut(current_id) {
                            scope.add_variable(VariableInfo {
                                name: info.name,
                                kind: var_kind,
                                span: info.name_span,
                                declared_type,
                            });
                        }
                    }
                }
            }
            Kind::NodeParam => {
                if let Some((name, span)) = param_name(node) {
                    let declared_type = find_type_expr_child(node)
                        .and_then(|te| match parse_type_expr(&te) {
                            TypeExprResult::Ok(t) => Some(t),
                            TypeExprResult::Err(_) => None,
                        });
                    if let Some(current_id) = self.current() {
                        if let Some(scope) = self.store.get_mut(current_id) {
                            scope.add_variable(VariableInfo {
                                name,
                                kind: VariableKind::Parameter,
                                span,
                                declared_type,
                            });
                        }
                    }
                }
            }
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
            | Kind::NodeFunctionDecl
            | Kind::NodeClassDecl
            | Kind::NodeWhileStmt
            | Kind::NodeForStmt
            | Kind::NodeForInStmt
            | Kind::NodeDoWhileStmt => {
                if kind == Kind::NodeClassDecl {
                    self.class_stack.pop();
                }
                self.pop();
            }
            _ => {}
        }

        WalkResult::Continue(())
    }
}
