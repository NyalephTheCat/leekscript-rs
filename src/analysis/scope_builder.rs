//! Scope-building pass: walk the tree and build the scope chain with variables, functions, classes.

use sipha::red::SyntaxNode;
use sipha::walk::{Visitor, WalkResult};

use crate::syntax::Kind;

use super::node_helpers::{class_decl_info, function_decl_info, param_name, var_decl_info};
use super::scope::{ScopeId, ScopeKind, ScopeStore, VariableInfo, VariableKind};

/// Builds scope tree by walking the syntax tree; maintains a stack of scope IDs.
pub struct ScopeBuilder {
    pub store: ScopeStore,
    stack: Vec<ScopeId>,
}

impl ScopeBuilder {
    pub fn new() -> Self {
        let store = ScopeStore::new();
        let stack = vec![store.root_id()];
        Self { store, stack }
    }

    /// Build scope from a program tree using an existing store (e.g. pre-seeded from signature files).
    pub fn with_store(store: ScopeStore) -> Self {
        let root_id = store.root_id();
        Self {
            store,
            stack: vec![root_id],
        }
    }

    fn current(&self) -> Option<ScopeId> {
        self.stack.last().copied()
    }

    fn push(&mut self, kind: ScopeKind) {
        let parent = self.current().expect("scope stack empty");
        let id = self.store.push(kind, parent);
        self.stack.push(id);
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
                .map_or(false, |s| s.kind == ScopeKind::Class)
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
        let kind = match node.kind_as::<Kind>() {
            Some(k) => k,
            None => return WalkResult::Continue(()),
        };

        match kind {
            Kind::NodeBlock => {
                self.push(ScopeKind::Block);
            }
            Kind::NodeFunctionDecl => {
                // Only register in main scope for top-level functions, not class methods.
                if !self.in_class_scope() {
                    if let Some(info) = function_decl_info(node) {
                        if let Some(main_scope) = self.store.get_mut(self.main_scope()) {
                            main_scope.add_function(
                                info.name.clone(),
                                info.min_arity,
                                info.max_arity,
                                info.name_span,
                            );
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
                }
                self.push(ScopeKind::Class);
            }
            Kind::NodeWhileStmt | Kind::NodeForStmt | Kind::NodeForInStmt | Kind::NodeDoWhileStmt => {
                self.push(ScopeKind::Loop);
            }
            Kind::NodeVarDecl => {
                if let Some(info) = var_decl_info(node) {
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
                            });
                        }
                    }
                }
            }
            Kind::NodeParam => {
                if let Some((name, span)) = param_name(node) {
                    if let Some(current_id) = self.current() {
                        if let Some(scope) = self.store.get_mut(current_id) {
                            scope.add_variable(VariableInfo {
                                name,
                                kind: VariableKind::Parameter,
                                span,
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
                self.pop();
            }
            _ => {}
        }

        WalkResult::Continue(())
    }
}
