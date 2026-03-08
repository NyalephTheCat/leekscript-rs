//! Scope model for `LeekScript`: block chain with variables, globals, functions, classes.

use sipha::types::Span;
use std::collections::HashMap;

use leekscript_core::doc_comment::DocComment;
use leekscript_core::Type;

/// Identifies a scope in the scope store (used for parent chain).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ScopeId(pub usize);

/// Kind of a scope block (determines what can be declared and how lookup works).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeKind {
    /// Root/main block: holds globals, user functions, classes.
    Main,
    /// Function or method body.
    Function,
    /// Class body (for method/constructor scope we use Function with class context if needed).
    Class,
    /// Loop body (for break/continue validation).
    Loop,
    /// Plain block `{ ... }`.
    Block,
}

/// Variable binding: local, global, or parameter.
#[derive(Clone, Debug)]
pub struct VariableInfo {
    pub name: String,
    pub kind: VariableKind,
    pub span: Span,
    /// Declared type from annotation (e.g. `integer x = 0`), if any.
    pub declared_type: Option<Type>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariableKind {
    Local,
    Global,
    Parameter,
}

/// One overload or default-param variant of a function.
#[derive(Clone, Debug)]
pub struct FunctionOverload {
    pub min_arity: usize,
    pub max_arity: usize,
    pub span: Span,
    /// When from .sig or annotated decl: param types and return type.
    pub param_types: Option<Vec<Type>>,
    pub return_type: Option<Type>,
}

/// Documentation and complexity metadata for a root function or global (from .sig).
#[derive(Clone, Debug, Default)]
pub struct SigMeta {
    /// Doxygen-style doc (from `///` lines or `/**` … `*/` in .sig). May contain @param, @return, @complexity, etc.
    pub doc: Option<DocComment>,
    /// Complexity code 1–13 when set via @complexity in the doc (convenience copy; also in doc.complexity).
    pub complexity: Option<u8>,
}

/// Display string for complexity code 1–13 from .sig `@complexity N`.
#[must_use]
pub fn complexity_display_string(code: u8) -> &'static str {
    match code {
        1 => "O(1)",
        2 => "O(log(n))",
        3 => "O(√n)",
        4 => "O(n)",
        5 => "O(nlog*(n))",
        6 => "O(nlog(n))",
        7 => "O(n²)",
        8 => "O(n³)",
        9 => "2^poly(log(n))",
        10 => "2^poly(n)",
        11 => "O(n!)",
        12 => "2^2^poly(n)",
        13 => "∞",
        _ => "?",
    }
}

/// Visibility of a class member. Properties and methods are **public by default** when no modifier is given.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum MemberVisibility {
    #[default]
    Public,
    Protected,
    Private,
}

/// Fields and methods of a class (for member access type inference: this.x, `this.method()`, Class.staticMember).
#[derive(Clone, Debug, Default)]
pub struct ClassMembers {
    /// Instance field name -> (declared type, visibility).
    pub fields: HashMap<String, (Type, MemberVisibility)>,
    /// Instance method name -> (param types, return type, visibility).
    pub methods: HashMap<String, (Vec<Type>, Type, MemberVisibility)>,
    /// Static field name -> (declared type, visibility).
    pub static_fields: HashMap<String, (Type, MemberVisibility)>,
    /// Static method name -> (param types, return type, visibility).
    pub static_methods: HashMap<String, (Vec<Type>, Type, MemberVisibility)>,
}

/// Single scope: variables and optional main-only data.
#[derive(Debug)]
pub struct Scope {
    pub kind: ScopeKind,
    pub parent: Option<ScopeId>,
    variables: HashMap<String, VariableInfo>,
    /// Main scope only: names declared as global.
    globals: Option<std::collections::HashSet<String>>,
    /// Main scope only: global name -> type (when from .sig).
    global_types: Option<HashMap<String, Type>>,
    /// Main scope only: user function name -> overloads (arity + optional types).
    functions: Option<HashMap<String, Vec<FunctionOverload>>>,
    /// Main scope only: class name -> first declaration span.
    classes: Option<HashMap<String, Span>>,
}

impl Scope {
    #[must_use]
    pub fn new_main() -> Self {
        Self {
            kind: ScopeKind::Main,
            parent: None,
            variables: HashMap::new(),
            globals: Some(std::collections::HashSet::new()),
            global_types: Some(HashMap::new()),
            functions: Some(HashMap::new()),
            classes: Some(HashMap::new()),
        }
    }

    #[must_use]
    pub fn new_child(kind: ScopeKind, parent: ScopeId) -> Self {
        Self {
            kind,
            parent: Some(parent),
            variables: HashMap::new(),
            globals: None,
            global_types: None,
            functions: None,
            classes: None,
        }
    }

    pub fn add_variable(&mut self, info: VariableInfo) {
        self.variables.insert(info.name.clone(), info);
    }

    #[must_use]
    pub fn has_variable(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }

    /// Variable names in this scope (for LSP completion).
    #[must_use]
    pub fn variable_names(&self) -> Vec<String> {
        self.variables.keys().cloned().collect()
    }

    /// Global names in this scope (main scope only; for LSP completion).
    #[must_use]
    pub fn global_names(&self) -> Option<Vec<String>> {
        self.globals.as_ref().map(|g| g.iter().cloned().collect())
    }

    /// Function names in this scope (main scope only; for LSP completion).
    #[must_use]
    pub fn function_names(&self) -> Option<Vec<String>> {
        self.functions.as_ref().map(|f| f.keys().cloned().collect())
    }

    /// Class names in this scope (main scope only; for LSP completion).
    #[must_use]
    pub fn class_names(&self) -> Option<Vec<String>> {
        self.classes.as_ref().map(|c| c.keys().cloned().collect())
    }

    /// Add a global name (main scope only).
    pub fn add_global(&mut self, name: String) {
        if let Some(g) = &mut self.globals {
            g.insert(name);
        }
    }

    /// Add a user function (main scope only). Supports overloads and default params via (`min_arity`, `max_arity`).
    pub fn add_function(&mut self, name: String, min_arity: usize, max_arity: usize, span: Span) {
        if let Some(f) = &mut self.functions {
            f.entry(name).or_default().push(FunctionOverload {
                min_arity,
                max_arity,
                span,
                param_types: None,
                return_type: None,
            });
        }
    }

    /// Add a user function with optional param/return types (main scope only).
    pub fn add_function_with_types(
        &mut self,
        name: String,
        min_arity: usize,
        max_arity: usize,
        span: Span,
        param_types: Option<Vec<Type>>,
        return_type: Option<Type>,
    ) {
        if let Some(f) = &mut self.functions {
            f.entry(name).or_default().push(FunctionOverload {
                min_arity,
                max_arity,
                span,
                param_types,
                return_type,
            });
        }
    }

    /// Add a class name (main scope only). Keeps first declaration; span is for duplicate reporting.
    /// A non-empty span (e.g. from the document) overwrites an empty one (e.g. from builtins/signatures)
    /// so that LSP rename/definition use the real location.
    pub fn add_class(&mut self, name: String, span: Span) {
        if let Some(c) = &mut self.classes {
            let is_empty = span.start >= span.end;
            match c.entry(name) {
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(span);
                }
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    let existing = e.get();
                    if existing.start >= existing.end && !is_empty {
                        e.insert(span);
                    }
                }
            }
        }
    }

    #[must_use]
    pub fn has_global(&self, name: &str) -> bool {
        self.globals.as_ref().is_some_and(|g| g.contains(name))
    }

    /// True if this scope has a function with the given name that accepts the given arity.
    #[must_use]
    pub fn function_accepts_arity(&self, name: &str, arity: usize) -> bool {
        self.functions.as_ref().is_some_and(|f| {
            f.get(name).is_some_and(|ranges| {
                ranges
                    .iter()
                    .any(|o| o.min_arity <= arity && arity <= o.max_arity)
            })
        })
    }

    /// Span of the first overload for a function name (for go-to-def).
    #[must_use]
    pub fn get_function_span(&self, name: &str) -> Option<Span> {
        self.functions
            .as_ref()
            .and_then(|f| f.get(name).and_then(|v| v.first()).map(|o| o.span))
    }

    /// Span of an existing overload with the same (`min_arity`, `max_arity`), if any (for duplicate same-signature).
    #[must_use]
    pub fn get_function_span_for_arity_range(
        &self,
        name: &str,
        min_arity: usize,
        max_arity: usize,
    ) -> Option<Span> {
        self.functions.as_ref().and_then(|f| {
            f.get(name).and_then(|ranges| {
                ranges
                    .iter()
                    .find(|o| o.min_arity == min_arity && o.max_arity == max_arity)
                    .map(|o| o.span)
            })
        })
    }

    /// Legacy: single arity (for resolve symbol). Returns first range's max if any.
    #[must_use]
    pub fn get_function_arity(&self, name: &str) -> Option<usize> {
        self.functions
            .as_ref()
            .and_then(|f| f.get(name).and_then(|v| v.first()).map(|o| o.max_arity))
    }

    /// Get param types and return type for a function call that passes `arity` arguments, if known.
    #[must_use]
    pub fn get_function_type(&self, name: &str, arity: usize) -> Option<(Vec<Type>, Type)> {
        let overloads = self.functions.as_ref()?.get(name)?;
        let o = overloads
            .iter()
            .find(|o| o.min_arity <= arity && arity <= o.max_arity)?;
        let params = o.param_types.clone()?;
        let ret = o.return_type.clone().unwrap_or(Type::any());
        Some((params, ret))
    }

    /// Get the type of a function when used as a value (e.g. `foo` without calling).
    /// For a single overload with optional params (`min_arity` < `max_arity`), returns a union of
    /// function types per arity, e.g. getMP(entity?) -> integer gives
    /// `Function< => integer> | Function<integer => integer>`.
    #[must_use]
    pub fn get_function_type_as_value(&self, name: &str) -> Option<Type> {
        let overloads = self.functions.as_ref()?.get(name)?;
        let o = overloads.first()?;
        let ret = o.return_type.clone().unwrap_or(Type::any());
        let param_types = o.param_types.clone().unwrap_or_default();
        if o.min_arity < o.max_arity && o.min_arity <= param_types.len() {
            // Optional params: union of function types for each arity.
            let mut variants = Vec::with_capacity(o.max_arity - o.min_arity + 1);
            for arity in o.min_arity..=o.max_arity {
                let args: Vec<Type> = param_types.iter().take(arity).cloned().collect();
                variants.push(Type::function(args, ret.clone()));
            }
            Some(if variants.len() == 1 {
                variants.into_iter().next().unwrap()
            } else {
                Type::compound(variants)
            })
        } else {
            Some(Type::function(param_types, ret))
        }
    }

    /// Add a global name with type (main scope only). Used when seeding from .sig.
    pub fn add_global_with_type(&mut self, name: String, ty: Type) {
        if let Some(g) = &mut self.globals {
            g.insert(name.clone());
        }
        if let Some(gt) = &mut self.global_types {
            gt.insert(name, ty);
        }
    }

    /// Get the type of a global, if known (from .sig).
    #[must_use]
    pub fn get_global_type(&self, name: &str) -> Option<Type> {
        self.global_types.as_ref()?.get(name).cloned()
    }

    #[must_use]
    pub fn has_class(&self, name: &str) -> bool {
        self.classes.as_ref().is_some_and(|c| c.contains_key(name))
    }

    /// First declaration span for a class in this scope (for duplicate diagnostic).
    #[must_use]
    pub fn get_class_first_span(&self, name: &str) -> Option<Span> {
        self.classes.as_ref().and_then(|c| c.get(name).copied())
    }

    #[must_use]
    pub fn get_variable(&self, name: &str) -> Option<&VariableInfo> {
        self.variables.get(name)
    }
}

/// Store for all scopes; root is at index 0.
#[derive(Debug)]
pub struct ScopeStore {
    scopes: Vec<Scope>,
    /// Class name -> its fields and methods (for this.x / `this.method()` type inference).
    class_members: HashMap<String, ClassMembers>,
    /// Root function name -> doc + complexity (from .sig).
    root_function_meta: HashMap<String, SigMeta>,
    /// Root global name -> doc + complexity (from .sig).
    root_global_meta: HashMap<String, SigMeta>,
}

impl Default for ScopeStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ScopeStore {
    #[must_use]
    pub fn new() -> Self {
        let mut store = Self {
            scopes: Vec::new(),
            class_members: HashMap::new(),
            root_function_meta: HashMap::new(),
            root_global_meta: HashMap::new(),
        };
        store.scopes.push(Scope::new_main());
        store
    }

    #[must_use]
    pub fn root_id(&self) -> ScopeId {
        ScopeId(0)
    }

    #[must_use]
    pub fn get(&self, id: ScopeId) -> Option<&Scope> {
        self.scopes.get(id.0)
    }

    pub fn get_mut(&mut self, id: ScopeId) -> Option<&mut Scope> {
        self.scopes.get_mut(id.0)
    }

    pub fn push(&mut self, kind: ScopeKind, parent: ScopeId) -> ScopeId {
        let id = ScopeId(self.scopes.len());
        self.scopes.push(Scope::new_child(kind, parent));
        id
    }

    /// Add a global name to the root (main) scope. Used when seeding from signature files.
    pub fn add_root_global(&mut self, name: String) {
        if let Some(root) = self.scopes.get_mut(0) {
            root.add_global(name);
        }
    }

    /// Add a global name with type to the root scope. Used when seeding from .sig.
    pub fn add_root_global_with_type(&mut self, name: String, ty: Type) {
        if let Some(root) = self.scopes.get_mut(0) {
            root.add_global_with_type(name, ty);
        }
    }

    /// Add a function to the root (main) scope. Used when seeding from signature files.
    /// Pass `Span::new(0, 0)` when no source span is available. Optional params (type?) give `min_arity` < `max_arity`.
    pub fn add_root_function(
        &mut self,
        name: String,
        min_arity: usize,
        max_arity: usize,
        span: Span,
    ) {
        if let Some(root) = self.scopes.get_mut(0) {
            root.add_function(name, min_arity, max_arity, span);
        }
    }

    /// Add a function with param/return types to the root scope. Used when seeding from .sig.
    pub fn add_root_function_with_types(
        &mut self,
        name: String,
        min_arity: usize,
        max_arity: usize,
        span: Span,
        param_types: Option<Vec<Type>>,
        return_type: Option<Type>,
    ) {
        if let Some(root) = self.scopes.get_mut(0) {
            root.add_function_with_types(
                name,
                min_arity,
                max_arity,
                span,
                param_types,
                return_type,
            );
        }
    }

    /// Set doc and complexity for a root function (from .sig `///` and `@complexity`).
    pub fn set_root_function_meta(&mut self, name: String, meta: SigMeta) {
        self.root_function_meta.insert(name, meta);
    }

    /// Set doc and complexity for a root global (from .sig).
    pub fn set_root_global_meta(&mut self, name: String, meta: SigMeta) {
        self.root_global_meta.insert(name, meta);
    }

    /// Get doc and complexity for a root function, if any.
    #[must_use]
    pub fn get_root_function_meta(&self, name: &str) -> Option<&SigMeta> {
        self.root_function_meta.get(name)
    }

    /// Get doc and complexity for a root global, if any.
    #[must_use]
    pub fn get_root_global_meta(&self, name: &str) -> Option<&SigMeta> {
        self.root_global_meta.get(name)
    }

    /// Add a class name to the root (main) scope. Used for built-ins (e.g. Class).
    /// Pass `Span::new(0, 0)` when no source span is available.
    pub fn add_root_class(&mut self, name: String, span: Span) {
        if let Some(root) = self.scopes.get_mut(0) {
            root.add_class(name, span);
        }
    }

    /// Register a class field for member type lookup (`this.field_name`). Visibility defaults to Public.
    pub fn add_class_field(
        &mut self,
        class_name: &str,
        field_name: String,
        ty: Type,
        visibility: MemberVisibility,
    ) {
        self.class_members
            .entry(class_name.to_string())
            .or_default()
            .fields
            .insert(field_name, (ty, visibility));
    }

    /// Register a class method for member type lookup (`this.method_name` returns function type). Visibility defaults to Public.
    pub fn add_class_method(
        &mut self,
        class_name: &str,
        method_name: String,
        param_types: Vec<Type>,
        return_type: Type,
        visibility: MemberVisibility,
    ) {
        self.class_members
            .entry(class_name.to_string())
            .or_default()
            .methods
            .insert(method_name, (param_types, return_type, visibility));
    }

    /// Register a static field (ClassName.staticField).
    pub fn add_class_static_field(
        &mut self,
        class_name: &str,
        field_name: String,
        ty: Type,
        visibility: MemberVisibility,
    ) {
        self.class_members
            .entry(class_name.to_string())
            .or_default()
            .static_fields
            .insert(field_name, (ty, visibility));
    }

    /// Register a static method (ClassName.staticMethod).
    pub fn add_class_static_method(
        &mut self,
        class_name: &str,
        method_name: String,
        param_types: Vec<Type>,
        return_type: Type,
        visibility: MemberVisibility,
    ) {
        self.class_members
            .entry(class_name.to_string())
            .or_default()
            .static_methods
            .insert(method_name, (param_types, return_type, visibility));
    }

    /// Type of a member (field or method) on a class instance. Returns None if unknown.
    #[must_use]
    pub fn get_class_member_type(&self, class_name: &str, member_name: &str) -> Option<Type> {
        let members = self.class_members.get(class_name)?;
        if let Some((ty, _)) = members.fields.get(member_name) {
            return Some(ty.clone());
        }
        if let Some((params, ret, _)) = members.methods.get(member_name) {
            return Some(Type::function(params.clone(), ret.clone()));
        }
        None
    }

    /// Type of a static member (ClassName.staticField or ClassName.staticMethod). Returns None if unknown.
    #[must_use]
    pub fn get_class_static_member_type(
        &self,
        class_name: &str,
        member_name: &str,
    ) -> Option<Type> {
        let members = self.class_members.get(class_name)?;
        if let Some((ty, _)) = members.static_fields.get(member_name) {
            return Some(ty.clone());
        }
        if let Some((params, ret, _)) = members.static_methods.get(member_name) {
            return Some(Type::function(params.clone(), ret.clone()));
        }
        None
    }

    /// Class members (instance and static fields/methods) for completion and member access.
    #[must_use]
    pub fn get_class_members(&self, class_name: &str) -> Option<&ClassMembers> {
        self.class_members.get(class_name)
    }

    /// Get the type of a function when used as a value, searching from current scope up to root.
    #[must_use]
    pub fn get_function_type_as_value(&self, current: ScopeId, name: &str) -> Option<Type> {
        let mut id = Some(current);
        while let Some(scope_id) = id {
            if let Some(scope) = self.get(scope_id) {
                if let Some(ty) = scope.get_function_type_as_value(name) {
                    return Some(ty);
                }
                id = scope.parent;
            } else {
                break;
            }
        }
        None
    }

    /// True if the root (main) scope has a class with this name (for fallback type inference).
    #[must_use]
    pub fn root_has_class(&self, name: &str) -> bool {
        self.get(ScopeId(0))
            .is_some_and(|scope| scope.has_class(name))
    }

    /// Resolve a name: look in current scope and parents; also check main's functions and classes.
    #[must_use]
    pub fn resolve(&self, current: ScopeId, name: &str) -> Option<ResolvedSymbol> {
        let mut id = Some(current);
        while let Some(scope_id) = id {
            let scope = self.get(scope_id)?;
            if let Some(v) = scope.get_variable(name) {
                return Some(ResolvedSymbol::Variable(v.clone()));
            }
            // Prefer class over global so that using a class name (e.g. PathManager.getCachedReachableCells) infers Class<T>.
            if scope.has_class(name) {
                return Some(ResolvedSymbol::Class(name.to_string()));
            }
            if scope.has_global(name) {
                return Some(ResolvedSymbol::Global(name.to_string()));
            }
            if let Some(arity) = scope.get_function_arity(name) {
                return Some(ResolvedSymbol::Function(name.to_string(), arity));
            }
            id = scope.parent;
        }
        None
    }
}

#[derive(Clone, Debug)]
pub enum ResolvedSymbol {
    Variable(VariableInfo),
    Global(String),
    Function(String, usize),
    Class(String),
}
