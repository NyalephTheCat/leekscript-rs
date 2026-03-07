//! Type inference and checking: literal types, variable types, binary/unary, assignment, return, cast, calls.

use sipha::error::SemanticDiagnostic;
use sipha::red::{SyntaxElement, SyntaxNode};
use sipha::types::Span;
use sipha::walk::{Visitor, WalkResult};
use std::collections::HashMap;

/// Key for type map: (span.start, span.end) for exact span lookup.
///
/// When multiple nodes share the same span (e.g. an identifier that is both a primary expr and
/// a var decl name), the type_map may store one entry per span; LSP/hover should prefer the
/// innermost or declaration node as appropriate (e.g. use `node_at_offset` and look up the
/// node's text range).
pub type TypeMapKey = (u32, u32);

use crate::syntax::Kind;
use crate::types::{CastType, Type};
use sipha::types::IntoSyntaxKind;

use super::error::{invalid_cast_at, type_mismatch_at, wrong_arity_at};
use super::node_helpers::{
    call_argument_count, call_argument_node, class_decl_info, for_in_iterable_expr,
    for_in_loop_vars, is_ternary_expr, member_expr_member_name, node_index_in_parent,
    null_check_from_condition, primary_expr_new_constructor, primary_expr_resolvable_name,
    var_decl_info,
};
use super::scope::{ResolvedSymbol, ScopeId, ScopeStore};
use super::type_expr::{find_type_expr_child, parse_type_expr, TypeExprResult};

/// Tracks inferred types and runs arity checks. Runs after scope building and validation.
pub struct TypeChecker<'a> {
    pub store: &'a ScopeStore,
    /// Root of the tree (for ancestor lookup in for-in).
    root: &'a SyntaxNode,
    stack: Vec<ScopeId>,
    next_scope_id: usize,
    /// Variable name -> Type for current and ancestor scopes (stack of maps).
    var_types: Vec<HashMap<String, Type>>,
    /// Types pushed by expression nodes (last pushed = last expression result).
    type_stack: Vec<Type>,
    /// Last primary expression identifier (for call arity: callee name).
    last_primary_ident: Option<String>,
    /// Return type of the function we're currently inside (for return stmt check).
    current_function_return_type: Option<Type>,
    /// Class we're currently inside (for `this` inference).
    current_class: Option<String>,
    /// Super class of the current class (for `super` inference).
    current_super_class: Option<String>,
    pub diagnostics: Vec<SemanticDiagnostic>,
    /// Map from expression span (start, end) to inferred type (for formatter type annotations).
    pub type_map: HashMap<TypeMapKey, Type>,
    /// Pending null-check narrowings: (var_name, then_ty, else_ty, closing_node). Popped when leaving if/ternary.
    /// When we see `if (x != null)` (or `x == null`), we narrow the type of `x` in the then-branch (or else-branch)
    /// to the non-null variant. Other patterns (e.g. `x == null` in else for then-branch narrowing) can be extended here.
    null_check_narrowing: Vec<(String, Type, Type, SyntaxNode)>,
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
            current_function_return_type: None,
            current_class: None,
            current_super_class: None,
            diagnostics: Vec::new(),
            type_map: HashMap::new(),
            null_check_narrowing: Vec::new(),
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
        // Check scope for declared type (variable/param from scope builder).
        let mut id = Some(self.current_scope());
        while let Some(scope_id) = id {
            if let Some(scope) = self.store.get(scope_id) {
                if let Some(v) = scope.get_variable(name) {
                    if let Some(ref t) = v.declared_type {
                        return t.clone();
                    }
                }
                if scope.has_global(name) {
                    if let Some(scope2) = self.store.get(scope_id) {
                        if let Some(ty) = scope2.get_global_type(name) {
                            return ty;
                        }
                    }
                    return Type::any();
                }
                id = scope.parent;
            } else {
                break;
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

    /// Get function param types and return type for a call, if known.
    fn get_function_type(&self, name: &str, arity: usize) -> Option<(Vec<Type>, Type)> {
        let mut id = Some(self.current_scope());
        while let Some(scope_id) = id {
            if let Some(scope) = self.store.get(scope_id) {
                if let Some((params, ret)) = scope.get_function_type(name, arity) {
                    return Some((params, ret));
                }
                id = scope.parent;
            } else {
                break;
            }
        }
        None
    }

    /// Record the current top-of-stack type for this node's span (for formatter type annotations).
    fn record_expression_type(&mut self, node: &SyntaxNode, ty: &Type) {
        let span = node.text_range();
        self.type_map
            .insert((span.start, span.end), ty.clone());
    }

    /// Type for an identifier: "this" -> instance of current class, "super" -> instance of super class, class name -> Class<T>, function -> Function type, else variable type.
    fn resolve_identifier_type(&self, name: &str) -> Type {
        if name == "this" {
            if let Some(ref c) = self.current_class {
                return Type::instance(c.clone());
            }
        }
        if name == "super" {
            if let Some(ref s) = self.current_super_class {
                return Type::instance(s.clone());
            }
        }
        if let Some(sym) = self.store.resolve(self.current_scope(), name) {
            match sym {
                ResolvedSymbol::Class(class_name) => return Type::class(Some(class_name)),
                ResolvedSymbol::Function(_, _) => {
                    if let Some(ty) = self.store.get_function_type_as_value(self.current_scope(), name) {
                        return ty;
                    }
                }
                _ => {}
            }
        }
        // Fallback: name might be a class in root scope (e.g. from another file) that wasn't resolved above.
        if self.store.root_has_class(name) {
            return Type::class(Some(name.to_string()));
        }
        self.lookup_var_type(name)
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
            Kind::NodeFunctionDecl | Kind::NodeClassDecl | Kind::NodeConstructorDecl => {
                self.push_scope();
                if kind == Kind::NodeFunctionDecl {
                    // Set return type only when it follows "->" (add_function_decl form). Class methods
                    // have type_expr first (return type) but we skip Class("function") to avoid "function m()" ambiguity.
                    let mut saw_arrow = false;
                    for child in node.children() {
                        if let SyntaxElement::Token(t) = &child {
                            if t.text() == "->" {
                                saw_arrow = true;
                                continue;
                            }
                        }
                        if let SyntaxElement::Node(n) = &child {
                            if n.kind_as::<Kind>() == Some(Kind::NodeTypeExpr) {
                                if saw_arrow {
                                    if let TypeExprResult::Ok(t) = parse_type_expr(n) {
                                        self.current_function_return_type = Some(t);
                                    }
                                    break;
                                }
                                // Class method: type_expr is return type; skip if it's "function" (keyword as type).
                                if let TypeExprResult::Ok(t) = parse_type_expr(n) {
                                    let is_function_type = matches!(t, Type::Class(Some(ref n)) if n == "function")
                                        || matches!(t, Type::Instance(ref n) if n == "function");
                                    if !is_function_type {
                                        self.current_function_return_type = Some(t);
                                    }
                                    break;
                                }
                            }
                        }
                        saw_arrow = false;
                    }
                } else if kind == Kind::NodeClassDecl {
                    if let Some(info) = class_decl_info(node) {
                        self.current_class = Some(info.name.clone());
                        self.current_super_class = info.super_class;
                    }
                }
                // NodeConstructorDecl: only push_scope, no class/return type to set
            }
            _ => {}
        }

        // When entering the then/else branch of an if or ternary with a pending null check, push a scope and apply the narrowed type.
        let to_apply = self.null_check_narrowing.last().and_then(|top| {
            let ancestors: Vec<SyntaxNode> = node.ancestors(self.root);
            let parent = ancestors.first()?;
            if top.3.text_range() != parent.text_range() {
                return None;
            }
            let idx = node_index_in_parent(node, parent);
            let parent_kind = parent.kind_as::<Kind>();
            let (then_ty, else_ty) = (top.1.clone(), top.2.clone());
            let var_name = top.0.clone();
            if parent_kind == Some(Kind::NodeIfStmt) {
                if idx == Some(4) {
                    Some((var_name, then_ty))
                } else if idx == Some(6) {
                    Some((var_name, else_ty))
                } else {
                    None
                }
            } else if parent_kind == Some(Kind::NodeExpr) && is_ternary_expr(parent) {
                if idx == Some(2) {
                    Some((var_name, then_ty))
                } else if idx == Some(4) {
                    Some((var_name, else_ty))
                } else {
                    None
                }
            } else {
                None
            }
        });
        if let Some((var_name, ty)) = to_apply {
            self.push_scope();
            self.add_var_type(var_name, ty);
        }

        WalkResult::Continue(())
    }

    fn leave_node(&mut self, node: &SyntaxNode) -> WalkResult {
        let kind = match node.kind_as::<Kind>() {
            Some(k) => k,
            None => return WalkResult::Continue(()),
        };

        // If we're leaving the iterable expression of a for-in, infer loop var types from it.
        if let Some(for_in) = node.find_ancestor(self.root, Kind::NodeForInStmt.into_syntax_kind()) {
            if let Some(iterable_node) = for_in_iterable_expr(&for_in) {
                if iterable_node.text_range() == node.text_range() {
                    if let Some(iterable_ty) = self.type_stack.pop() {
                        let (key_ty, value_ty) = iterable_key_value_types(&iterable_ty);
                        for (i, (var_name, _)) in for_in_loop_vars(&for_in).into_iter().enumerate() {
                            let ty = if i == 0 { key_ty.clone() } else { value_ty.clone() };
                            self.add_var_type(var_name, ty);
                        }
                    }
                }
            }
        }

        // When leaving the then/else branch of an if or ternary (we had pushed a scope when entering), pop it.
        let parent = node.ancestors(self.root).into_iter().next();
        let closing_range = self.null_check_narrowing.last().map(|t| t.3.text_range());
        let is_branch = parent.is_some_and(|ref p| {
            closing_range == Some(p.text_range())
                && {
                    let idx = node_index_in_parent(node, p);
                    let k = p.kind_as::<Kind>();
                    (k == Some(Kind::NodeIfStmt) && (idx == Some(4) || idx == Some(6)))
                        || (k == Some(Kind::NodeExpr) && is_ternary_expr(p) && (idx == Some(2) || idx == Some(4)))
                }
        });
        if is_branch {
            self.pop_scope();
        }

        // When leaving the condition of an if or ternary, push null-check narrowing if it's "var != null" etc.
        if let Some(parent) = node.ancestors(self.root).into_iter().next() {
            let idx = node_index_in_parent(node, &parent);
            let is_if_condition = parent.kind_as::<Kind>() == Some(Kind::NodeIfStmt) && idx == Some(2);
            let is_ternary_condition = parent.kind_as::<Kind>() == Some(Kind::NodeExpr)
                && is_ternary_expr(&parent)
                && idx == Some(0);
            if is_if_condition || is_ternary_condition {
                if let Some((var_name, then_is_non_null)) = null_check_from_condition(node, self.root) {
                    let var_ty = self.lookup_var_type(&var_name);
                    let (then_ty, else_ty) = if then_is_non_null {
                        (Type::non_null(&var_ty), Type::null())
                    } else {
                        (Type::null(), Type::non_null(&var_ty))
                    };
                    self.null_check_narrowing.push((var_name, then_ty, else_ty, parent));
                }
            }
        }

        match kind {
            Kind::NodeBlock
            | Kind::NodeWhileStmt
            | Kind::NodeForStmt
            | Kind::NodeForInStmt
            | Kind::NodeDoWhileStmt => self.pop_scope(),
            Kind::NodeIfStmt => {
                if self
                    .null_check_narrowing
                    .last()
                    .is_some_and(|t| t.3.text_range() == node.text_range())
                {
                    self.null_check_narrowing.pop();
                }
            }
            Kind::NodeFunctionDecl | Kind::NodeClassDecl | Kind::NodeConstructorDecl => {
                if kind == Kind::NodeFunctionDecl {
                    self.current_function_return_type = None;
                } else if kind == Kind::NodeClassDecl {
                    self.current_class = None;
                    self.current_super_class = None;
                }
                self.pop_scope();
            }
            Kind::NodePrimaryExpr => {
                if let Some((class_name, num_args)) = primary_expr_new_constructor(node) {
                    for _ in 0..num_args {
                        self.type_stack.pop();
                    }
                    let ty = Type::instance(class_name);
                    self.type_stack.push(ty.clone());
                    self.record_expression_type(node, &ty);
                } else if let Some(name) = primary_expr_resolvable_name(node) {
                    self.last_primary_ident = Some(name.clone());
                    let ty = self.resolve_identifier_type(&name);
                    self.type_stack.push(ty.clone());
                    self.record_expression_type(node, &ty);
                } else {
                    // Literal or other primary; do not clear last_primary_ident (may be callee)
                    let ty = infer_primary_type(node);
                    self.type_stack.push(ty.clone());
                    self.record_expression_type(node, &ty);
                }
            }
            Kind::NodeCallExpr => {
                let actual_arity = call_argument_count(node);
                let callee_name = self.last_primary_ident.take();
                if let Some(ref name) = callee_name {
                    if !self.function_accepts_arity(name, actual_arity) {
                        let mut expected = None;
                        let mut id = Some(self.current_scope());
                        while let Some(scope_id) = id {
                            if let Some(scope) = self.store.get(scope_id) {
                                if let Some(arity) = scope.get_function_arity(name) {
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
                let mut arg_types: Vec<Type> = (0..actual_arity)
                    .filter_map(|_| self.type_stack.pop())
                    .collect();
                arg_types.reverse();
                let callee_ty = self.type_stack.pop(); // callee (e.g. from this.method or f)
                let result_type = if let Some(ref name) = callee_name {
                    if let Some((param_types, return_type)) = self.get_function_type(name, actual_arity) {
                        if param_types.len() == arg_types.len() {
                            for (i, (arg_ty, param_ty)) in arg_types.iter().zip(param_types.iter()).enumerate() {
                                if !param_ty.assignable_from(arg_ty) {
                                    if let Some(arg_node) = call_argument_node(node, i) {
                                        if let Some(tok) = arg_node.first_token() {
                                            self.diagnostics.push(type_mismatch_at(
                                                tok.text_range(),
                                                &param_ty.to_string(),
                                                &arg_ty.to_string(),
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                        return_type
                    } else if let Some(Type::Class(Some(ref class_name))) = callee_ty.as_ref() {
                        // Constructor call: ClassName(args) produces an instance of that class.
                        Type::instance(class_name.clone())
                    } else {
                        Type::any()
                    }
                } else if let Some(Type::Function {
                    args: param_types,
                    return_type,
                }) = callee_ty.as_ref()
                {
                    // Callee from member expr (e.g. ClassName.staticMethod() or this.method()); use param types to check args and return type for result.
                    if param_types.len() == arg_types.len() {
                        for (i, (arg_ty, param_ty)) in arg_types.iter().zip(param_types.iter()).enumerate() {
                            if !param_ty.assignable_from(arg_ty) {
                                if let Some(arg_node) = call_argument_node(node, i) {
                                    if let Some(tok) = arg_node.first_token() {
                                        self.diagnostics.push(type_mismatch_at(
                                            tok.text_range(),
                                            &param_ty.to_string(),
                                            &arg_ty.to_string(),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    (**return_type).clone()
                } else {
                    Type::any()
                };
                self.type_stack.push(result_type.clone());
                self.record_expression_type(node, &result_type);
            }
            Kind::NodeMemberExpr => {
                self.last_primary_ident = None;
                let base_ty = self.type_stack.pop().unwrap_or(Type::any());
                let ty = if member_expr_member_name(node).as_deref() == Some("class") {
                    // variable.class returns the class of the variable (Class<T> where T is the variable's class).
                    match &base_ty {
                        Type::Class(Some(c)) => Type::class(Some(c.clone())),
                        Type::Instance(c) => Type::class(Some(c.clone())),
                        _ => Type::class(None),
                    }
                } else if let Type::Instance(class_name) = &base_ty {
                    // this.x or instance.x: use field type or method type (function) from class members.
                    member_expr_member_name(node)
                        .and_then(|name| self.store.get_class_member_type(class_name, &name))
                        .unwrap_or(Type::any())
                } else if let Type::Class(Some(class_name)) = &base_ty {
                    // ClassName.staticField or ClassName.staticMethod: use static members.
                    member_expr_member_name(node)
                        .and_then(|name| self.store.get_class_static_member_type(class_name, &name))
                        .unwrap_or(Type::any())
                } else {
                    Type::any()
                };
                self.type_stack.push(ty.clone());
                self.record_expression_type(node, &ty);
            }
            Kind::NodeIndexExpr => {
                self.last_primary_ident = None;
                // Stack: [..., receiver_ty, index_ty]. Infer element type from Array<T> or Map<K,V>.
                let _index_ty = self.type_stack.pop().unwrap_or(Type::any());
                let receiver_ty = self.type_stack.pop().unwrap_or(Type::any());
                let element_ty = match &receiver_ty {
                    Type::Array(elem) => *elem.clone(),
                    Type::Map(_, val) => *val.clone(),
                    _ => Type::any(),
                };
                self.type_stack.push(element_ty.clone());
                self.record_expression_type(node, &element_ty);
            }
            Kind::NodeVarDecl => {
                if let Some(info) = var_decl_info(node) {
                    let rhs_ty = self.type_stack.pop().unwrap_or(Type::any());
                    let declared = self
                        .store
                        .get(self.current_scope())
                        .and_then(|s| s.get_variable(&info.name))
                        .and_then(|v| v.declared_type.clone());
                    let ty_to_store = if let Some(ref d) = declared {
                        if !d.assignable_from(&rhs_ty) {
                            let span = node
                                .first_token()
                                .map_or_else(|| Span::new(0, 0), |t| t.text_range());
                            self.diagnostics.push(type_mismatch_at(
                                span,
                                &d.to_string(),
                                &rhs_ty.to_string(),
                            ));
                        }
                        d.clone()
                    } else {
                        rhs_ty
                    };
                    self.add_var_type(info.name.clone(), ty_to_store.clone());
                    self.record_expression_type(node, &ty_to_store);
                    // Record type at variable name span so hover on identifier shows inferred type.
                    let r = info.name_span;
                    self.type_map
                        .insert((r.start, r.end), ty_to_store);
                }
            }
            Kind::NodeExpr => {
                // Check for ternary: cond ? then : else -> union(then_ty, else_ty).
                if is_ternary_expr(node) && self.type_stack.len() >= 3 {
                    let else_ty = self.type_stack.pop().unwrap();
                    let then_ty = self.type_stack.pop().unwrap();
                    let _cond_ty = self.type_stack.pop().unwrap();
                    let result_ty = Type::compound2(then_ty, else_ty);
                    self.type_stack.push(result_ty.clone());
                    self.record_expression_type(node, &result_ty);
                    if self
                        .null_check_narrowing
                        .last()
                        .is_some_and(|t| t.3.text_range() == node.text_range())
                    {
                        self.null_check_narrowing.pop();
                    }
                } else {
                    // Check for assignment: has = token and two types on stack.
                    let is_assign = node.children().any(|c| {
                        matches!(c, SyntaxElement::Token(t) if t.text() == "=")
                    });
                    if is_assign && self.type_stack.len() >= 2 {
                        let rhs = self.type_stack.pop().unwrap();
                        let lhs = self.type_stack.pop().unwrap();
                        if !lhs.assignable_from(&rhs) {
                            let span = node
                                .first_token()
                                .map_or_else(|| Span::new(0, 0), |t| t.text_range());
                            self.diagnostics.push(type_mismatch_at(
                                span,
                                &lhs.to_string(),
                                &rhs.to_string(),
                            ));
                        }
                        self.type_stack.push(lhs);
                    }
                }
            }
            Kind::NodeBinaryExpr => {
                let op = node
                    .children()
                    .find_map(|c| {
                        if let SyntaxElement::Token(t) = c {
                            if t.kind_as::<Kind>() == Some(Kind::TokOp) {
                                return Some(t.text().to_string());
                            }
                            if t.kind_as::<Kind>() == Some(Kind::KwInstanceof) {
                                return Some("instanceof".to_string());
                            }
                            if t.kind_as::<Kind>() == Some(Kind::KwIn) {
                                return Some("in".to_string());
                            }
                        }
                        None
                    })
                    .unwrap_or_default();
                if self.type_stack.len() >= 2 {
                    let right = self.type_stack.pop().unwrap();
                    let left = self.type_stack.pop().unwrap();
                    let (result, err) = check_binary_op(&op, &left, &right);
                    if let Some((_, msg)) = err {
                        let span = node
                            .first_token()
                            .map_or_else(|| Span::new(0, 0), |t| t.text_range());
                        self.diagnostics.push(
                            SemanticDiagnostic::error(span, msg).with_code(super::error::AnalysisError::TypeMismatch.code()),
                        );
                    }
                    let result_ty = result.unwrap_or(Type::any());
                    self.type_stack.push(result_ty.clone());
                    self.record_expression_type(node, &result_ty);
                }
            }
            Kind::NodeUnaryExpr => {
                let op = node
                    .first_token()
                    .map(|t| t.text().to_string())
                    .unwrap_or_default();
                if let Some(operand) = self.type_stack.pop() {
                    let (result, err) = check_unary_op(&op, &operand);
                    if let Some((_, msg)) = err {
                        let span = node
                            .first_token()
                            .map_or_else(|| Span::new(0, 0), |t| t.text_range());
                        self.diagnostics.push(
                            SemanticDiagnostic::error(span, msg).with_code(super::error::AnalysisError::TypeMismatch.code()),
                        );
                    }
                    let result_ty = result.unwrap_or(Type::any());
                    self.type_stack.push(result_ty.clone());
                    self.record_expression_type(node, &result_ty);
                }
            }
            Kind::NodeReturnStmt => {
                let expr_type = self.type_stack.pop().unwrap_or(Type::void());
                if let Some(ref expected) = self.current_function_return_type {
                    if !expected.assignable_from(&expr_type) {
                        let span = node
                            .first_token()
                            .map_or_else(|| Span::new(0, 0), |t| t.text_range());
                        self.diagnostics.push(type_mismatch_at(
                            span,
                            &expected.to_string(),
                            &expr_type.to_string(),
                        ));
                    }
                }
            }
            Kind::NodeAsCast => {
                if let Some(expr_ty) = self.type_stack.pop() {
                    let ty = if let Some(te) = find_type_expr_child(node) {
                        if let TypeExprResult::Ok(target_ty) = parse_type_expr(&te) {
                            let cast = Type::check_cast(&expr_ty, &target_ty);
                            if cast == CastType::Incompatible {
                                let span = node
                                    .first_token()
                                    .map_or_else(|| Span::new(0, 0), |t| t.text_range());
                                self.diagnostics.push(invalid_cast_at(
                                    span,
                                    &expr_ty.to_string(),
                                    &target_ty.to_string(),
                                ));
                            }
                            target_ty
                        } else {
                            Type::any()
                        }
                    } else {
                        Type::any()
                    };
                    self.type_stack.push(ty.clone());
                    self.record_expression_type(node, &ty);
                }
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

/// Key and value types for for-in loop variables from iterable type.
/// First variable gets key_ty, second (if present) gets value_ty.
fn iterable_key_value_types(iterable: &Type) -> (Type, Type) {
    match iterable {
        Type::Array(elem) => (Type::int(), *elem.clone()),
        Type::Map(k, v) => (*k.clone(), *v.clone()),
        Type::Set(elem) => (Type::int(), *elem.clone()),
        Type::Interval(elem) => (Type::int(), *elem.clone()),
        _ => (Type::any(), Type::any()),
    }
}

/// Check binary operator: (result_type, optional (span, message) for error).
fn check_binary_op(op: &str, left: &Type, right: &Type) -> (Option<Type>, Option<(Span, String)>) {
    let numeric_ops = ["+", "-", "*", "/", "\\", "%", "**"];
    let compare_ops = ["<", "<=", ">", ">="];
    let equality_ops = ["==", "!="];
    let logical_ops = ["&&", "||", "and", "or", "xor"];
    // String + anything or anything + string => string (concatenation).
    if op == "+" && (left == &Type::String || right == &Type::String || left == &Type::Any || right == &Type::Any) {
        return (Some(Type::string()), None);
    }
    if numeric_ops.contains(&op) {
        if left != &Type::Any && right != &Type::Any && (!left.is_number() || !right.is_number()) {
            return (
                Some(Type::real()),
                Some((
                    Span::new(0, 0),
                    format!("operator `{op}` requires number, got {} and {}", left, right),
                )),
            );
        }
        // Integer + integer => integer; otherwise real.
        let result = if left == &Type::Int && right == &Type::Int {
            Type::int()
        } else {
            Type::real()
        };
        (Some(result), None)
    } else if compare_ops.contains(&op) {
        if left != &Type::Any && right != &Type::Any && (!left.is_number() || !right.is_number()) {
            return (
                Some(Type::bool()),
                Some((
                    Span::new(0, 0),
                    format!("comparison requires number, got {} and {}", left, right),
                )),
            );
        }
        (Some(Type::bool()), None)
    } else if equality_ops.contains(&op) {
        (Some(Type::bool()), None)
    } else if op == "instanceof" {
        // x instanceof Y: left is value, right is class (Class<T>); result is boolean.
        (Some(Type::bool()), None)
    } else if op == "in" {
        // x in y: membership; result is boolean.
        (Some(Type::bool()), None)
    } else if logical_ops.contains(&op) {
        // Any type is truthy/falsy; logical ops accept any type and produce boolean.
        (Some(Type::bool()), None)
    } else {
        (Some(Type::any()), None)
    }
}

/// Check unary operator: (result_type, optional (span, message)).
fn check_unary_op(op: &str, operand: &Type) -> (Option<Type>, Option<(Span, String)>) {
    match op {
        "-" | "+" => {
            if operand != &Type::Any && !operand.is_number() {
                return (
                    Some(Type::real()),
                    Some((
                        Span::new(0, 0),
                        format!("unary `{op}` requires number, got {operand}"),
                    )),
                );
            }
            // Preserve integer: -n and +n are integer when n is integer, real when n is real.
            let result_ty = match operand {
                Type::Int => Type::int(),
                _ if operand.is_number() => Type::real(),
                _ => Type::real(),
            };
            (Some(result_ty), None)
        }
        "!" | "not" => {
            // Any type is truthy/falsy; unary not accepts any type and produces boolean.
            (Some(Type::bool()), None)
        }
        _ => (Some(Type::any()), None),
    }
}
