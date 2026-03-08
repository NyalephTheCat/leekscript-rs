//! Semantic analysis: scope building and validation.
//!
//! **Signature files (.sig):** When `.sig` files are provided (e.g. stdlib), they are parsed
//! with the signature grammar, then type expressions in each `function`/`global` declaration
//! are turned into `Type` via `parse_type_expr` and seeded into the root scope. The type checker
//! uses these for inference (e.g. call return types, global variable types). Use
//! `analyze_with_signatures(program_root, signature_roots)` so that built-in names resolve and
//! types are inferred from the .sig definitions.

mod builtins;
mod deprecation;
mod error;
mod node_helpers;
mod scope;
mod scope_builder;
mod scope_extents;
mod signature_loader;
mod type_checker;
mod type_expr;
mod validator;

pub use error::AnalysisError;
pub use node_helpers::{
    binary_expr_rhs, call_argument_count, call_argument_node, class_decl_info, class_field_info,
    class_member_visibility, function_decl_info, member_expr_member_name,
    member_expr_receiver_name, param_name, primary_expr_new_constructor,
    primary_expr_resolvable_name, var_decl_info, ClassDeclInfo, FunctionDeclInfo, VarDeclInfo,
    VarDeclKind,
};
pub use scope::{
    complexity_display_string, MemberVisibility, ResolvedSymbol, Scope, ScopeId, ScopeKind,
    ScopeStore, SigMeta, VariableInfo, VariableKind,
};
pub use scope_builder::{seed_scope_from_program, ScopeBuilder};
pub use scope_extents::{build_scope_extents, scope_at_offset};
pub use type_checker::{TypeChecker, TypeMapKey};
pub use type_expr::{find_type_expr_child, parse_type_expr, TypeExprResult};
pub use validator::Validator;

use sipha::error::SemanticDiagnostic;
use sipha::red::SyntaxNode;
use sipha::types::Span;
use sipha::walk::WalkOptions;
use std::collections::HashMap;

use leekscript_core::Type;

/// Options for running analysis (single entry point for program or document).
#[derive(Default)]
pub struct AnalyzeOptions<'a> {
    /// When set, scope is seeded from included files and (if present) signature_roots; then the main program is analyzed.
    pub include_tree: Option<&'a leekscript_core::IncludeTree>,
    /// When set, root scope is seeded with these signature roots (e.g. from `parse_signatures()`).
    pub signature_roots: Option<&'a [SyntaxNode]>,
}

/// Run analysis with the given options. Dispatches to `analyze`, `analyze_with_signatures`, or `analyze_with_include_tree` as appropriate.
#[must_use]
pub fn analyze_with_options(
    program_root: &SyntaxNode,
    options: &AnalyzeOptions<'_>,
) -> AnalysisResult {
    if let Some(tree) = options.include_tree {
        let sigs = options.signature_roots.unwrap_or(&[]);
        return analyze_with_include_tree(tree, sigs);
    }
    if let Some(sigs) = options.signature_roots {
        return analyze_with_signatures(program_root, sigs);
    }
    analyze(program_root)
}

/// Result of running scope building and validation.
#[derive(Debug)]
pub struct AnalysisResult {
    pub diagnostics: Vec<SemanticDiagnostic>,
    pub scope_store: ScopeStore,
    /// Map from expression span (start, end) to inferred type (for formatter type annotations).
    pub type_map: std::collections::HashMap<TypeMapKey, Type>,
    /// Scope IDs in walk order (for LSP: compute scope at offset from scope-extent list).
    pub scope_id_sequence: Vec<ScopeId>,
}

impl AnalysisResult {
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == sipha::error::Severity::Error)
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.has_errors()
    }
}

/// Run validation, type checking, and deprecation passes after scope building. Returns merged diagnostics and type map.
fn run_pipeline(
    program_root: &SyntaxNode,
    store: &ScopeStore,
    scope_id_sequence: &[ScopeId],
    options: &WalkOptions,
) -> (Vec<SemanticDiagnostic>, HashMap<TypeMapKey, Type>) {
    let mut validator = Validator::new(store, scope_id_sequence);
    let _ = program_root.walk(&mut validator, options);

    let mut type_checker = TypeChecker::new(store, program_root);
    let _ = program_root.walk(&mut type_checker, options);

    let mut deprecation_checker = deprecation::DeprecationChecker::new();
    let _ = program_root.walk(&mut deprecation_checker, options);

    let mut diagnostics = validator.diagnostics;
    diagnostics.extend(type_checker.diagnostics);
    diagnostics.extend(deprecation_checker.diagnostics);

    (diagnostics, type_checker.type_map)
}

/// Build scope from the tree and run validation. Returns diagnostics and the scope store.
///
/// Pass order: (1) `ScopeBuilder` runs first and builds the scope store and scope ID sequence.
/// (2) Validator and (3) `TypeChecker` then use that store so resolution and type checking see the same scopes.
#[must_use]
pub fn analyze(root: &SyntaxNode) -> AnalysisResult {
    let options = WalkOptions::nodes_only();
    let mut builder = ScopeBuilder::new();
    let _ = root.walk(&mut builder, &options);

    for name in builtins::BUILTIN_CLASS_NAMES {
        builder
            .store
            .add_root_class((*name).to_string(), Span::new(0, 0));
    }

    let (diagnostics, type_map) =
        run_pipeline(root, &builder.store, &builder.scope_id_sequence, &options);

    AnalysisResult {
        diagnostics,
        scope_store: builder.store,
        type_map,
        scope_id_sequence: builder.scope_id_sequence,
    }
}

/// Seed the root scope from parsed signature file(s). Each element of `signature_roots`
/// should be the root node returned by `parse_signatures()` (may be a wrapper or `NodeSigFile`).
pub fn seed_scope_from_signatures(store: &mut ScopeStore, signature_roots: &[SyntaxNode]) {
    signature_loader::seed_scope_from_signatures(store, signature_roots);
}

/// Analyze the main file of an include tree: seed scope from included files and signatures, then analyze the main AST.
#[must_use]
pub fn analyze_with_include_tree(
    tree: &leekscript_core::IncludeTree,
    signature_roots: &[SyntaxNode],
) -> AnalysisResult {
    let program_root = match &tree.root {
        Some(r) => r.clone(),
        None => {
            return AnalysisResult {
                diagnostics: Vec::new(),
                scope_store: ScopeStore::new(),
                type_map: std::collections::HashMap::new(),
                scope_id_sequence: Vec::new(),
            };
        }
    };
    let options = WalkOptions::nodes_only();
    let mut store = ScopeStore::new();
    seed_scope_from_signatures(&mut store, signature_roots);
    for (_, child) in &tree.includes {
        if let Some(ref root) = child.root {
            seed_scope_from_program(&mut store, root);
        }
    }
    for name in builtins::BUILTIN_CLASS_NAMES {
        store.add_root_class((*name).to_string(), Span::new(0, 0));
    }
    let mut builder = ScopeBuilder::with_store(store);
    let _ = program_root.walk(&mut builder, &options);

    let mut validator = Validator::new(&builder.store, &builder.scope_id_sequence);
    let _ = program_root.walk(&mut validator, &options);

    let mut type_checker = TypeChecker::new(&builder.store, &program_root);
    let _ = program_root.walk(&mut type_checker, &options);

    let mut deprecation_checker = deprecation::DeprecationChecker::new();
    let _ = program_root.walk(&mut deprecation_checker, &options);

    let mut diagnostics = validator.diagnostics;
    diagnostics.extend(type_checker.diagnostics);
    diagnostics.extend(deprecation_checker.diagnostics);

    let type_map = type_checker.type_map.clone();
    AnalysisResult {
        diagnostics,
        scope_store: builder.store,
        type_map,
        scope_id_sequence: builder.scope_id_sequence,
    }
}

/// Analyze the program with the root scope pre-seeded from signature files (e.g. stdlib constants and functions).
/// This allows references to global constants and built-in functions to resolve without errors.
/// Also seeds built-in type/class names (e.g. `Class`, `bool`) so that common `LeekScript` code validates.
#[must_use]
pub fn analyze_with_signatures(
    program_root: &SyntaxNode,
    signature_roots: &[SyntaxNode],
) -> AnalysisResult {
    let options = WalkOptions::nodes_only();
    let mut store = ScopeStore::new();
    seed_scope_from_signatures(&mut store, signature_roots);
    for name in builtins::BUILTIN_CLASS_NAMES {
        store.add_root_class((*name).to_string(), Span::new(0, 0));
    }
    let mut builder = ScopeBuilder::with_store(store);
    let _ = program_root.walk(&mut builder, &options);

    let (diagnostics, type_map) = run_pipeline(
        program_root,
        &builder.store,
        &builder.scope_id_sequence,
        &options,
    );

    AnalysisResult {
        diagnostics,
        scope_store: builder.store,
        type_map,
        scope_id_sequence: builder.scope_id_sequence,
    }
}

#[cfg(test)]
mod tests;
