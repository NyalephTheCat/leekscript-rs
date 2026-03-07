//! Validation pass: resolve identifiers, check break/continue in loop, duplicate declarations.

use sipha::error::SemanticDiagnostic;
use sipha::red::SyntaxNode;
use sipha::types::Span;
use sipha::walk::{Visitor, WalkResult};
use std::collections::HashSet;

use crate::syntax::Kind;

use super::error::AnalysisError;
use super::node_helpers::{class_decl_info, for_in_loop_vars, function_decl_info, param_name, expr_identifier, var_decl_info};
use super::scope::{ScopeId, ScopeKind, ScopeStore};

/// Collects diagnostics and maintains scope stack (replaying scope structure from first pass).
/// Uses the same scope ID sequence as `ScopeBuilder` so `resolve()` looks up the correct scope.
pub struct Validator<'a> {
    pub store: &'a ScopeStore,
    stack: Vec<ScopeId>,
    /// Names declared in the current scope (for duplicate detection).
    declared_in_scope: Vec<HashSet<String>>,
    /// Index into `scope_id_sequence` for the next push.
    scope_id_index: usize,
    /// Scope IDs in walk order (from `ScopeBuilder`) so we push the same IDs.
    scope_id_sequence: &'a [ScopeId],
    pub diagnostics: Vec<SemanticDiagnostic>,
}

impl<'a> Validator<'a> {
    #[must_use] 
    pub fn new(store: &'a ScopeStore, scope_id_sequence: &'a [ScopeId]) -> Self {
        Self {
            store,
            stack: vec![ScopeId(0)],
            declared_in_scope: vec![HashSet::new()],
            scope_id_index: 0,
            scope_id_sequence,
            diagnostics: Vec::new(),
        }
    }

    fn current_scope(&self) -> ScopeId {
        *self.stack.last().unwrap_or(&ScopeId(0))
    }

    fn push_scope(&mut self) {
        let id = self
            .scope_id_sequence
            .get(self.scope_id_index)
            .copied()
            .unwrap_or_else(|| ScopeId(self.scope_id_index + 1));
        self.scope_id_index += 1;
        self.stack.push(id);
        self.declared_in_scope.push(HashSet::new());
    }

    fn pop_scope(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
            self.declared_in_scope.pop();
        }
    }

    fn resolve(&self, name: &str) -> bool {
        if self
            .store
            .resolve(self.current_scope(), name)
            .is_some()
        {
            return true;
        }
        // Fallback: variable may be declared in current or outer scope but not yet in store
        // (e.g. same pass order); treat as resolved if we've seen it in declared_in_scope.
        self.declared_in_scope
            .iter()
            .any(|set| set.contains(name))
    }

    fn in_loop(&self) -> bool {
        self.stack.iter().rev().any(|&id| {
            self.store
                .get(id)
                .is_some_and(|s| s.kind == ScopeKind::Loop)
        })
    }

    /// True when we're in the main program block (not inside a function or class body).
    fn in_main_block(&self) -> bool {
        self.stack.iter().all(|&id| {
            self.store.get(id).is_none_or(|s| {
                s.kind == ScopeKind::Main || s.kind == ScopeKind::Block
            })
        })
    }

    /// True when we're inside a function scope (used to disallow nested function declarations).
    fn in_function_scope(&self) -> bool {
        self.stack
            .iter()
            .any(|&id| self.store.get(id).is_some_and(|s| s.kind == ScopeKind::Function))
    }

    /// True when we're inside a class body (method or constructor).
    fn in_class_scope(&self) -> bool {
        self.stack
            .iter()
            .any(|&id| self.store.get(id).is_some_and(|s| s.kind == ScopeKind::Class))
    }

    /// True when we're inside a method (function scope whose parent chain includes a class).
    fn in_method_scope(&self) -> bool {
        self.in_class_scope() && self.in_function_scope()
    }
}

impl Visitor for Validator<'_> {
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
            | Kind::NodeDoWhileStmt => {
                self.push_scope();
                if matches!(kind, Kind::NodeForInStmt) {
                    for (name, _) in for_in_loop_vars(node) {
                        if let Some(declared) = self.declared_in_scope.last_mut() {
                            let _ = declared.insert(name);
                        }
                    }
                }
            }
            Kind::NodeInclude => {
                if !self.in_main_block() {
                    if let Some(tok) = node.first_token() {
                        self.diagnostics
                            .push(AnalysisError::IncludeOnlyInMainBlock.at(tok.text_range()));
                    }
                }
            }
            Kind::NodeFunctionDecl => {
                // Nested functions are not allowed (methods inside classes are; they don't have Function on stack yet).
                if self.in_function_scope() {
                    if let Some(info) = function_decl_info(node) {
                        self.diagnostics
                            .push(AnalysisError::FunctionOnlyInMainBlock.at(info.name_span));
                    }
                } else if self.in_main_block() {
                    // User-defined top-level functions: no optional/default parameters allowed.
                    if let Some(info) = function_decl_info(node) {
                        if info.min_arity < info.max_arity {
                            self.diagnostics
                                .push(AnalysisError::OptionalParamsOnlyInStandardFunctionsOrMethods.at(info.name_span));
                        }
                        // Duplicate: same name and same (min_arity, max_arity) signature.
                        if let Some(main) = self.store.get(self.store.root_id()) {
                            if let Some(existing_span) = main.get_function_span_for_arity_range(
                                &info.name,
                                info.min_arity,
                                info.max_arity,
                            ) {
                                if existing_span != info.name_span
                                    && existing_span != Span::new(0, 0)
                                {
                                    self.diagnostics
                                        .push(AnalysisError::DuplicateFunctionName.at(info.name_span));
                                }
                            }
                        }
                    }
                }
                self.push_scope();
            }
            Kind::NodeClassDecl => {
                if let Some(info) = class_decl_info(node) {
                    if let Some(main) = self.store.get(self.store.root_id()) {
                        if let Some(first_span) = main.get_class_first_span(&info.name) {
                            if first_span != info.name_span
                                && first_span != Span::new(0, 0)
                            {
                                self.diagnostics
                                    .push(AnalysisError::DuplicateClassName.at(info.name_span));
                            }
                        }
                    }
                }
                self.push_scope();
            }
            Kind::NodeParam => {
                if let Some((name, _)) = param_name(node) {
                    if let Some(declared) = self.declared_in_scope.last_mut() {
                        let _ = declared.insert(name);
                    }
                }
            }
            Kind::NodeVarDecl => {
                if let Some(info) = var_decl_info(node) {
                    if info.kind == super::node_helpers::VarDeclKind::Global
                        && !self.in_main_block()
                    {
                        self.diagnostics
                            .push(AnalysisError::GlobalOnlyInMainBlock.at(info.name_span));
                    }
                    if let Some(declared) = self.declared_in_scope.last_mut() {
                        if !declared.insert(info.name.clone()) {
                            self.diagnostics
                                .push(AnalysisError::VariableNameUnavailable.at(info.name_span));
                        }
                    }
                }
            }
            Kind::NodePrimaryExpr => {
                if let Some((name, span)) = expr_identifier(node) {
                    if name == "this" && self.in_method_scope() {
                        // "this" is valid in method scope.
                    } else if !self.resolve(&name) {
                        self.diagnostics
                            .push(AnalysisError::UnknownVariableOrFunction.at(span));
                    }
                }
            }
            Kind::NodeBreakStmt => {
                if !self.in_loop() {
                    if let Some(tok) = node.first_token() {
                        self.diagnostics
                            .push(AnalysisError::BreakOutOfLoop.at(tok.text_range()));
                    }
                }
            }
            Kind::NodeContinueStmt => {
                if !self.in_loop() {
                    if let Some(tok) = node.first_token() {
                        self.diagnostics
                            .push(AnalysisError::ContinueOutOfLoop.at(tok.text_range()));
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
                self.pop_scope();
            }
            _ => {}
        }

        WalkResult::Continue(())
    }
}
