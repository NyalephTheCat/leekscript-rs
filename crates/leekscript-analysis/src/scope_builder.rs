//! Scope-building pass: walk the tree and build the scope chain with variables, functions, classes.

use sipha::red::SyntaxNode;
use sipha::types::IntoSyntaxKind;
use sipha::walk::{Visitor, WalkResult};

use leekscript_core::syntax::Kind;

use leekscript_core::Type;

use super::node_helpers::{
    class_decl_info, class_field_info, class_member_visibility, class_method_is_static,
    for_in_loop_vars, function_decl_info, param_name, var_decl_info,
};
use super::scope::{ScopeId, ScopeKind, ScopeStore, VariableInfo, VariableKind};
use super::type_expr::{
    find_type_expr_child, param_and_return_types, parse_type_expr, TypeExprResult,
};

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
        // On partial/malformed trees, leave_node can pop more than enter_node pushed; keep stack non-empty.
        if self.stack.is_empty() {
            self.stack.push(self.store.root_id());
        }
        let parent = self.current().unwrap_or_else(|| self.store.root_id());
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

/// True if `node` is at program top level (not inside Block, FunctionDecl, or ClassDecl).
fn is_top_level(node: &SyntaxNode, root: &SyntaxNode) -> bool {
    for anc in node.ancestors(root) {
        match anc.kind_as::<Kind>() {
            Some(Kind::NodeBlock) | Some(Kind::NodeFunctionDecl) | Some(Kind::NodeClassDecl) => {
                return false;
            }
            _ => {}
        }
    }
    true
}

/// Seed the root scope from a program AST (top-level classes, functions, globals only).
/// Also registers class fields and methods so member access (e.g. `myCell.x`) infers types.
/// Used when building scope from included files so the main file sees their declarations.
pub fn seed_scope_from_program(store: &mut ScopeStore, root: &SyntaxNode) {
    for kind in [
        Kind::NodeClassDecl,
        Kind::NodeFunctionDecl,
        Kind::NodeVarDecl,
    ] {
        for node in root.find_all_nodes(kind.into_syntax_kind()) {
            if !is_top_level(&node, root) {
                continue;
            }
            match node.kind_as::<Kind>() {
                Some(Kind::NodeClassDecl) => {
                    if let Some(info) = class_decl_info(&node) {
                        store.add_root_class(info.name.clone(), info.name_span);
                    }
                }
                Some(Kind::NodeFunctionDecl) => {
                    if let Some(info) = function_decl_info(&node) {
                        let (param_types, return_type) =
                            param_and_return_types(&node, Kind::NodeParam);
                        if let (Some(pt), Some(rt)) = (param_types, return_type) {
                            store.add_root_function_with_types(
                                info.name.clone(),
                                info.min_arity,
                                info.max_arity,
                                info.name_span,
                                Some(pt),
                                Some(rt),
                            );
                        } else {
                            store.add_root_function(
                                info.name.clone(),
                                info.min_arity,
                                info.max_arity,
                                info.name_span,
                            );
                        }
                    }
                }
                Some(Kind::NodeVarDecl) => {
                    if let Some(info) = var_decl_info(&node) {
                        if info.kind == super::node_helpers::VarDeclKind::Global {
                            let declared_type = find_type_expr_child(&node).and_then(|te| {
                                match parse_type_expr(&te) {
                                    TypeExprResult::Ok(ty) => Some(ty),
                                    TypeExprResult::Err(_) => None,
                                }
                            });
                            if let Some(ty) = declared_type {
                                store.add_root_global_with_type(info.name.clone(), ty);
                            } else {
                                store.add_root_global(info.name.clone());
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    seed_class_members_from_program(store, root);
}

/// Register class fields and methods from a program AST so that member access (obj.field, obj.method())
/// infers types. Called after root-level decls are seeded; only processes top-level classes.
fn seed_class_members_from_program(store: &mut ScopeStore, root: &SyntaxNode) {
    let kind_class = Kind::NodeClassDecl.into_syntax_kind();
    let class_decls: Vec<SyntaxNode> = root.find_all_nodes(kind_class);

    for node in root.find_all_nodes(Kind::NodeClassField.into_syntax_kind()) {
        let node_range = node.text_range();
        let anc = class_decls.iter().find(|c| {
            let r = c.text_range();
            r.start <= node_range.start && node_range.end <= r.end
        });
        let class_name = match anc.and_then(|a| class_decl_info(a)) {
            Some(info) => info.name,
            None => continue,
        };
        if let Some((field_name, ty, is_static)) = class_field_info(&node) {
            let ty = ty.unwrap_or(Type::any());
            let vis = class_member_visibility(&node, root);
            if is_static {
                store.add_class_static_field(&class_name, field_name, ty, vis);
            } else {
                store.add_class_field(&class_name, field_name, ty, vis);
            }
        }
    }

    for node in root.find_all_nodes(Kind::NodeFunctionDecl.into_syntax_kind()) {
        let node_range = node.text_range();
        let anc = class_decls.iter().find(|c| {
            let r = c.text_range();
            r.start <= node_range.start && node_range.end <= r.end
        });
        let class_name = match anc.and_then(|a| class_decl_info(a)) {
            Some(info) => info.name,
            None => continue,
        };
        if let Some(info) = function_decl_info(&node) {
            let (param_types, return_type) = param_and_return_types(&node, Kind::NodeParam);
            let params = param_types.unwrap_or_default();
            let ret = return_type.unwrap_or(Type::any());
            let is_static = class_method_is_static(&node, root);
            let vis = class_member_visibility(&node, root);
            if is_static {
                store.add_class_static_method(&class_name, info.name, params, ret, vis);
            } else {
                store.add_class_method(&class_name, info.name, params, ret, vis);
            }
        }
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
                            let (param_types, return_type) =
                                param_and_return_types(node, Kind::NodeParam);
                            let params = param_types.unwrap_or_default();
                            // A constructor returns an instance of the class when no return type is given.
                            let ret = return_type.unwrap_or_else(|| {
                                if info.name == class_name {
                                    Type::instance(class_name.clone())
                                } else {
                                    Type::any()
                                }
                            });
                            let is_static = self
                                .root
                                .as_ref()
                                .map_or(false, |root| class_method_is_static(node, root));
                            let vis = class_member_visibility(node, self.root.as_ref().unwrap());
                            if is_static {
                                self.store.add_class_static_method(
                                    &class_name,
                                    info.name,
                                    params,
                                    ret,
                                    vis,
                                );
                            } else {
                                self.store.add_class_method(
                                    &class_name,
                                    info.name,
                                    params,
                                    ret,
                                    vis,
                                );
                            }
                        }
                    }
                } else {
                    // Only register in main scope for top-level functions, not class methods.
                    if let Some(info) = function_decl_info(node) {
                        if let Some(main_scope) = self.store.get_mut(self.main_scope()) {
                            let (param_types, return_type) =
                                param_and_return_types(node, Kind::NodeParam);
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
            Kind::NodeConstructorDecl => {
                // Push Function scope so constructor params are in scope for the body (like methods).
                self.push(ScopeKind::Function);
            }
            Kind::NodeClassField => {
                if let Some(class_name) = self.class_stack.last() {
                    if let Some((field_name, ty, is_static)) = class_field_info(node) {
                        let ty = ty.unwrap_or(Type::any());
                        let vis = class_member_visibility(node, self.root.as_ref().unwrap());
                        if is_static {
                            self.store
                                .add_class_static_field(class_name, field_name, ty, vis);
                        } else {
                            self.store.add_class_field(class_name, field_name, ty, vis);
                        }
                    }
                }
            }
            Kind::NodeWhileStmt
            | Kind::NodeForStmt
            | Kind::NodeForInStmt
            | Kind::NodeDoWhileStmt => {
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
                    // For "var x = ..." do not parse a type (avoids "var" keyword as type).
                    let declared_type = if info.kind == super::node_helpers::VarDeclKind::Var {
                        None
                    } else {
                        find_type_expr_child(node).and_then(|te| match parse_type_expr(&te) {
                            TypeExprResult::Ok(t) => Some(t),
                            TypeExprResult::Err(_) => None,
                        })
                    };
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
                    let declared_type =
                        find_type_expr_child(node).and_then(|te| match parse_type_expr(&te) {
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
            | Kind::NodeConstructorDecl
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
