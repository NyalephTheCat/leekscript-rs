//! Scope model for LeekScript: block chain with variables, globals, functions, classes.

use sipha::types::Span;
use std::collections::HashMap;

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariableKind {
    Local,
    Global,
    Parameter,
}

/// Single scope: variables and optional main-only data.
#[derive(Debug)]
pub struct Scope {
    pub kind: ScopeKind,
    pub parent: Option<ScopeId>,
    variables: HashMap<String, VariableInfo>,
    /// Main scope only: names declared as global.
    globals: Option<std::collections::HashSet<String>>,
    /// Main scope only: user function name -> list of (min_arity, max_arity, span) for overloads and default params.
    functions: Option<HashMap<String, Vec<(usize, usize, Span)>>>,
    /// Main scope only: class name -> first declaration span.
    classes: Option<HashMap<String, Span>>,
}

impl Scope {
    pub fn new_main() -> Self {
        Self {
            kind: ScopeKind::Main,
            parent: None,
            variables: HashMap::new(),
            globals: Some(std::collections::HashSet::new()),
            functions: Some(HashMap::new()),
            classes: Some(HashMap::new()),
        }
    }

    pub fn new_child(kind: ScopeKind, parent: ScopeId) -> Self {
        Self {
            kind,
            parent: Some(parent),
            variables: HashMap::new(),
            globals: None,
            functions: None,
            classes: None,
        }
    }

    pub fn add_variable(&mut self, info: VariableInfo) {
        self.variables.insert(info.name.clone(), info);
    }

    pub fn has_variable(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }

    /// Add a global name (main scope only).
    pub fn add_global(&mut self, name: String) {
        if let Some(g) = &mut self.globals {
            g.insert(name);
        }
    }

    /// Add a user function (main scope only). Supports overloads and default params via (min_arity, max_arity).
    pub fn add_function(&mut self, name: String, min_arity: usize, max_arity: usize, span: Span) {
        if let Some(f) = &mut self.functions {
            f.entry(name).or_default().push((min_arity, max_arity, span));
        }
    }

    /// Add a class name (main scope only). Keeps first declaration; span is for duplicate reporting.
    pub fn add_class(&mut self, name: String, span: Span) {
        if let Some(c) = &mut self.classes {
            c.entry(name).or_insert(span);
        }
    }

    pub fn has_global(&self, name: &str) -> bool {
        self.globals.as_ref().map_or(false, |g| g.contains(name))
    }

    /// Whether any overload/default set for this name accepts the given argument count.
    pub fn function_accepts_arity(&self, name: &str, arity: usize) -> bool {
        self.functions.as_ref().map_or(false, |f| {
            f.get(name).map_or(false, |ranges| {
                ranges
                    .iter()
                    .any(|(min, max, _)| *min <= arity && arity <= *max)
            })
        })
    }

    /// First declaration span for a function in this scope (for duplicate diagnostic).
    pub fn get_function_first_span(&self, name: &str) -> Option<Span> {
        self.functions
            .as_ref()
            .and_then(|f| f.get(name).and_then(|v| v.first()).map(|(_, _, s)| *s))
    }

    /// Span of an existing overload with the same (min_arity, max_arity), if any (for duplicate same-signature).
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
                    .find(|(min, max, _)| *min == min_arity && *max == max_arity)
                    .map(|(_, _, s)| *s)
            })
        })
    }

    /// Legacy: single arity (for resolve symbol). Returns first range's max if any.
    pub fn get_function_arity(&self, name: &str) -> Option<usize> {
        self.functions
            .as_ref()
            .and_then(|f| f.get(name).and_then(|v| v.first()).map(|(_, max, _)| *max))
    }

    pub fn has_class(&self, name: &str) -> bool {
        self.classes.as_ref().map_or(false, |c| c.contains_key(name))
    }

    /// First declaration span for a class in this scope (for duplicate diagnostic).
    pub fn get_class_first_span(&self, name: &str) -> Option<Span> {
        self.classes.as_ref().and_then(|c| c.get(name).copied())
    }

    pub fn get_variable(&self, name: &str) -> Option<&VariableInfo> {
        self.variables.get(name)
    }
}

/// Store for all scopes; root is at index 0.
#[derive(Debug)]
pub struct ScopeStore {
    scopes: Vec<Scope>,
}

impl ScopeStore {
    pub fn new() -> Self {
        let mut store = Self {
            scopes: Vec::new(),
        };
        store.scopes.push(Scope::new_main());
        store
    }

    pub fn root_id(&self) -> ScopeId {
        ScopeId(0)
    }

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

    /// Add a function to the root (main) scope. Used when seeding from signature files.
    /// Pass `Span::new(0, 0)` when no source span is available. Optional params (type?) give min_arity < max_arity.
    pub fn add_root_function(&mut self, name: String, min_arity: usize, max_arity: usize, span: Span) {
        if let Some(root) = self.scopes.get_mut(0) {
            root.add_function(name, min_arity, max_arity, span);
        }
    }

    /// Add a class name to the root (main) scope. Used for built-ins (e.g. Class).
    /// Pass `Span::new(0, 0)` when no source span is available.
    pub fn add_root_class(&mut self, name: String, span: Span) {
        if let Some(root) = self.scopes.get_mut(0) {
            root.add_class(name, span);
        }
    }

    /// Resolve a name: look in current scope and parents; also check main's functions and classes.
    pub fn resolve(&self, current: ScopeId, name: &str) -> Option<ResolvedSymbol> {
        let mut id = Some(current);
        while let Some(scope_id) = id {
            let scope = self.get(scope_id)?;
            if let Some(v) = scope.get_variable(name) {
                return Some(ResolvedSymbol::Variable(v.clone()));
            }
            if scope.has_global(name) {
                return Some(ResolvedSymbol::Global(name.to_string()));
            }
            if let Some(arity) = scope.get_function_arity(name) {
                return Some(ResolvedSymbol::Function(name.to_string(), arity));
            }
            if scope.has_class(name) {
                return Some(ResolvedSymbol::Class(name.to_string()));
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
