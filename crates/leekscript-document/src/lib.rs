//! Document-level analysis: single entry point for parsing, analysis, definition map, and doc maps.
//!
//! Use [`DocumentAnalysis::new`] to run parsing and analysis (with optional include tree and
//! signature roots) and get diagnostics, type map, scope store, definition map, doc maps, and
//! class hierarchy in one place.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sipha::line_index::LineIndex;
use sipha::red::SyntaxNode;
use sipha::types::IntoSyntaxKind;

use leekscript_analysis::{
    analyze, analyze_with_include_tree, analyze_with_signatures, build_scope_extents,
    class_decl_info, function_decl_info, scope_at_offset, var_decl_info, ResolvedSymbol, ScopeId,
    ScopeStore, VarDeclKind,
};
use leekscript_core::doc_comment::{build_doc_map, DocComment};
use leekscript_core::syntax::Kind;
use leekscript_core::{
    build_include_tree, parse, parse_error_to_diagnostics, parse_recovering_multi, IncludeTree,
    Type,
};
use sipha_analysis::collect_definitions;

/// Kind of root-level symbol for definition map (matches scope seeding order).
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub enum RootSymbolKind {
    Class,
    Function,
    Global,
}

fn is_top_level(node: &SyntaxNode, root: &SyntaxNode) -> bool {
    for anc in node.ancestors(root) {
        if let Some(Kind::NodeBlock | Kind::NodeFunctionDecl | Kind::NodeClassDecl) =
            anc.kind_as::<Kind>()
        {
            return false;
        }
    }
    true
}

/// Build (name, kind) -> (path, start, end) for root-level symbols (includes first, then main).
#[must_use]
pub fn build_definition_map(
    tree: &IncludeTree,
    main_path: &Path,
) -> HashMap<(String, RootSymbolKind), (PathBuf, u32, u32)> {
    let mut roots: Vec<(PathBuf, SyntaxNode)> = tree
        .includes
        .iter()
        .filter_map(|(path, child)| child.root.as_ref().map(|r| (path.clone(), r.clone())))
        .collect();
    if let Some(ref root) = tree.root {
        roots.push((main_path.to_path_buf(), root.clone()));
    }
    collect_definitions(&roots, |node, root| {
        if !is_top_level(node, root) {
            return None;
        }
        if node.kind_as::<Kind>() == Some(Kind::NodeClassDecl) {
            return class_decl_info(node)
                .map(|info| (info.name, RootSymbolKind::Class, info.name_span));
        }
        if node.kind_as::<Kind>() == Some(Kind::NodeFunctionDecl) {
            return function_decl_info(node)
                .map(|info| (info.name, RootSymbolKind::Function, info.name_span));
        }
        if node.kind_as::<Kind>() == Some(Kind::NodeVarDecl) {
            if let Some(info) = var_decl_info(node) {
                if info.kind == VarDeclKind::Global {
                    return Some((info.name, RootSymbolKind::Global, info.name_span));
                }
            }
        }
        None
    })
}

/// Find the declaration node span (for `doc_map` lookup) that contains the given name span.
#[must_use]
pub fn decl_span_for_name_span(
    root: &SyntaxNode,
    name_start: u32,
    name_end: u32,
) -> Option<(u32, u32)> {
    for node in root.find_all_nodes(Kind::NodeClassDecl.into_syntax_kind()) {
        if let Some(info) = class_decl_info(&node) {
            if info.name_span.start == name_start && info.name_span.end == name_end {
                let r = node.text_range();
                return Some((r.start, r.end));
            }
        }
    }
    for node in root.find_all_nodes(Kind::NodeFunctionDecl.into_syntax_kind()) {
        if let Some(info) = function_decl_info(&node) {
            if info.name_span.start == name_start && info.name_span.end == name_end {
                let r = node.text_range();
                return Some((r.start, r.end));
            }
        }
    }
    for node in root.find_all_nodes(Kind::NodeVarDecl.into_syntax_kind()) {
        if !is_top_level(&node, root) {
            continue;
        }
        if let Some(info) = var_decl_info(&node) {
            if info.kind == VarDeclKind::Global
                && info.name_span.start == name_start
                && info.name_span.end == name_end
            {
                let r = node.text_range();
                return Some((r.start, r.end));
            }
        }
    }
    None
}

/// Build map `class_name` -> `superclass_name` from the AST (for visibility: subclass can see protected).
#[must_use]
pub fn build_class_super(root: Option<&SyntaxNode>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let root = match root {
        Some(r) => r,
        None => return map,
    };
    for node in root.find_all_nodes(Kind::NodeClassDecl.into_syntax_kind()) {
        if let Some(info) = class_decl_info(&node) {
            if let Some(super_name) = info.super_class {
                map.insert(info.name, super_name);
            }
        }
    }
    map
}

/// Options for building a [`DocumentAnalysis`].
///
/// Use [`DocumentAnalysis::new_with_options`] to run parsing and analysis with these options.
/// Keeps the API extensible (e.g. `max_parse_errors`, "skip analysis") without breaking callers.
#[derive(Default)]
pub struct DocumentAnalysisOptions<'a> {
    /// Source code of the main document.
    pub source: &'a str,
    /// When set and [`build_include_tree`](leekscript_core::build_include_tree) succeeds, analysis uses the include tree.
    pub main_path: Option<&'a Path>,
    /// Parsed signature roots (e.g. from [`parse_signatures`](leekscript_core::parse_signatures)) to seed the scope.
    pub signature_roots: &'a [SyntaxNode],
    /// Previous syntax root for incremental reparse; when `Some`, parsing reuses it when applicable.
    pub existing_root: Option<SyntaxNode>,
    /// Max parse errors to collect in recovery mode (default: 64).
    pub max_parse_errors: Option<usize>,
    /// When set, function/global name -> (path, 0-based line) from .sig files for hover links (e.g. getCellX, getCellY).
    pub sig_definition_locations: Option<&'a HashMap<String, (PathBuf, u32)>>,
}

/// Result of document-level analysis: source, AST, diagnostics, scope, types, definition map, doc maps, class hierarchy.
#[derive(Debug)]
pub struct DocumentAnalysis {
    /// Main document source (what the client has).
    pub source: String,
    /// Line index for the main document.
    pub line_index: LineIndex,
    /// Program root (main file); None if parse failed.
    pub root: Option<SyntaxNode>,
    /// When Some, the document has includes; tree holds main + included files.
    pub include_tree: Option<IncludeTree>,
    /// Path of the main document (when file URI), for go-to-def on include.
    pub main_path: Option<PathBuf>,
    /// Semantic and parse diagnostics.
    pub diagnostics: Vec<sipha::error::SemanticDiagnostic>,
    /// Map from expression span (start, end) to inferred type.
    pub type_map: HashMap<(u32, u32), leekscript_core::Type>,
    pub scope_store: ScopeStore,
    /// Scope extent (`ScopeId`, (`start_byte`, `end_byte`)) for `scope_at_offset`.
    pub scope_extents: Vec<(ScopeId, (u32, u32))>,
    /// (name, kind) -> (path, `start_byte`, `end_byte`) for root-level symbols.
    pub definition_map: HashMap<(String, RootSymbolKind), (PathBuf, u32, u32)>,
    /// Map from declaration (`start_byte`, `end_byte`) to parsed Doxygen-style documentation.
    pub doc_map: HashMap<(u32, u32), DocComment>,
    /// When Some (with `include_tree`), `doc_map` per included file path.
    #[allow(clippy::type_complexity)]
    pub include_doc_maps: Option<HashMap<PathBuf, HashMap<(u32, u32), DocComment>>>,
    /// Class name -> superclass name (for visibility: subclass can see protected).
    pub class_super: HashMap<String, String>,
    /// When Some, function/global name -> (path, 0-based line) from .sig files for hover/definition links.
    pub sig_definition_locations: Option<HashMap<String, (PathBuf, u32)>>,
}

impl DocumentAnalysis {
    /// Run parsing and analysis from options.
    ///
    /// When `main_path` is set and [`build_include_tree`](leekscript_core::build_include_tree) succeeds, uses
    /// the include tree and analyzes with included files and `signature_roots`. Otherwise parses a
    /// single file (or uses `existing_root` when provided) and optionally uses `signature_roots`.
    #[must_use]
    pub fn new_with_options(options: &DocumentAnalysisOptions<'_>) -> Self {
        let DocumentAnalysisOptions {
            source,
            main_path,
            signature_roots,
            existing_root,
            max_parse_errors,
            sig_definition_locations,
        } = options;
        let max_errors = max_parse_errors.unwrap_or(PARSE_RECOVERY_MAX_ERRORS);
        let mut diagnostics = Vec::new();
        let mut type_map = HashMap::new();
        let mut scope_store = ScopeStore::new();
        let mut scope_extents = vec![];
        let mut definition_map = HashMap::new();
        let mut doc_map = HashMap::new();
        #[allow(clippy::type_complexity)]
        let mut include_doc_maps: Option<HashMap<PathBuf, HashMap<(u32, u32), DocComment>>> = None;
        let mut include_tree: Option<IncludeTree> = None;
        let mut main_path_buf: Option<PathBuf> = main_path.map(Path::to_path_buf);
        let mut root: Option<SyntaxNode> = None;
        let mut source_owned = (*source).to_string();

        match main_path {
            Some(path) => match build_include_tree(source, Some(path)) {
                Ok(tree) => {
                    let result = analyze_with_include_tree(&tree, signature_roots);
                    diagnostics = result.diagnostics;
                    type_map = result.type_map;
                    scope_store = result.scope_store;
                    let len = tree.source.len() as u32;
                    scope_extents = match &tree.root {
                        Some(r) => build_scope_extents(r, &result.scope_id_sequence, len as usize),
                        _ => vec![(ScopeId(0), (0, len))],
                    };
                    if tree.root.is_none() {
                        if let Err(parse_err) = parse(&tree.source) {
                            diagnostics
                                .extend(parse_error_to_diagnostics(&parse_err, &tree.source));
                        }
                    }
                    definition_map = build_definition_map(&tree, path);
                    doc_map = tree.root.as_ref().map(build_doc_map).unwrap_or_default();
                    let mut inc_doc = HashMap::new();
                    for (p, child) in &tree.includes {
                        if let Some(ref inc_root) = child.root {
                            inc_doc.insert(p.clone(), build_doc_map(inc_root));
                        }
                    }
                    include_doc_maps = Some(inc_doc);
                    include_tree = Some(tree.clone());
                    main_path_buf = Some(path.to_path_buf());
                    source_owned = tree.source.clone();
                    root = tree.root.clone();
                }
                Err(_) => {
                    single_file_analysis(
                        source,
                        signature_roots,
                        existing_root.clone(),
                        max_errors,
                        &mut diagnostics,
                        &mut type_map,
                        &mut scope_store,
                        &mut scope_extents,
                        &mut root,
                    );
                }
            },
            None => {
                single_file_analysis(
                    source,
                    signature_roots,
                    existing_root.clone(),
                    max_errors,
                    &mut diagnostics,
                    &mut type_map,
                    &mut scope_store,
                    &mut scope_extents,
                    &mut root,
                );
            }
        }

        if doc_map.is_empty() && include_tree.is_none() {
            if let Some(r) = root.as_ref() {
                doc_map = build_doc_map(r);
            }
        }

        let class_super = build_class_super(root.as_ref());

        let line_index = LineIndex::new(source_owned.as_bytes());

        Self {
            source: source_owned,
            line_index,
            root,
            include_tree,
            main_path: main_path_buf,
            diagnostics,
            type_map,
            scope_store,
            scope_extents,
            definition_map,
            doc_map,
            include_doc_maps,
            class_super,
            sig_definition_locations: sig_definition_locations.cloned(),
        }
    }

    /// Run parsing and analysis for the given source.
    ///
    /// Convenience wrapper around [`Self::new_with_options`]. When `main_path` is `Some` and
    /// `build_include_tree` succeeds, uses the include tree; otherwise parses a single file (or uses
    /// `existing_root` when provided, e.g. from incremental reparse).
    #[must_use]
    pub fn new(
        source: &str,
        main_path: Option<&Path>,
        signature_roots: &[SyntaxNode],
        existing_root: Option<SyntaxNode>,
        sig_definition_locations: Option<HashMap<String, (PathBuf, u32)>>,
    ) -> Self {
        Self::new_with_options(&DocumentAnalysisOptions {
            source,
            main_path,
            signature_roots,
            existing_root,
            max_parse_errors: None,
            sig_definition_locations: sig_definition_locations.as_ref(),
        })
    }

    /// Resolve the symbol at the given byte offset (e.g. variable, function, class, global).
    /// Returns `None` if there is no root, no token at offset, or the identifier does not resolve.
    #[must_use]
    pub fn symbol_at_offset(&self, byte_offset: u32) -> Option<ResolvedSymbol> {
        let root = self.root.as_ref()?;
        let token = root.token_at_offset(byte_offset)?;
        if token.kind_as::<Kind>() != Some(Kind::TokIdent) {
            return None;
        }
        let name = token.text().to_string();
        let scope_id = scope_at_offset(&self.scope_extents, byte_offset);
        self.scope_store.resolve(scope_id, &name)
    }

    /// Type at the given byte offset. Looks up the node at offset in the type map, then walks
    /// ancestors until a type is found.
    #[must_use]
    pub fn type_at_offset(&self, byte_offset: u32) -> Option<Type> {
        let root = self.root.as_ref()?;
        let node = root.node_at_offset(byte_offset)?;
        let range = node.text_range();
        let key = (range.start, range.end);
        self.type_map.get(&key).cloned().or_else(|| {
            for anc in node.ancestors(root) {
                let r = anc.text_range();
                if let Some(t) = self.type_map.get(&(r.start, r.end)) {
                    return Some(t.clone());
                }
            }
            None
        })
    }

    /// Definition span for a root-level symbol: `(path, start_byte, end_byte)`.
    /// Returns `None` if the name/kind is not in the definition map.
    #[must_use]
    pub fn definition_span_for(
        &self,
        name: &str,
        kind: RootSymbolKind,
    ) -> Option<(PathBuf, u32, u32)> {
        self.definition_map.get(&(name.to_string(), kind)).cloned()
    }

    /// Build minimal document state with only source and line index (no parse/analysis).
    /// Used by the LSP to update the document buffer immediately on `did_change` so that
    /// subsequent changes are applied to the correct base; analysis overwrites this when it completes.
    #[must_use]
    pub fn minimal(source: String) -> Self {
        let line_index = LineIndex::new(source.as_bytes());
        let len = source.len() as u32;
        Self {
            source,
            line_index,
            root: None,
            include_tree: None,
            main_path: None,
            diagnostics: Vec::new(),
            type_map: HashMap::new(),
            scope_store: ScopeStore::new(),
            scope_extents: vec![(ScopeId(0), (0, len))],
            definition_map: HashMap::new(),
            doc_map: HashMap::new(),
            include_doc_maps: None,
            class_super: HashMap::new(),
            sig_definition_locations: None,
        }
    }

    /// Like [`Self::minimal`] but keeps the given root so the next incremental reparse can reuse it.
    /// Use when reparse succeeded and analysis will run async; keeps the tree available for the next edit.
    #[must_use]
    pub fn minimal_with_root(source: String, root: SyntaxNode) -> Self {
        let line_index = LineIndex::new(source.as_bytes());
        let len = source.len() as u32;
        Self {
            source,
            line_index,
            root: Some(root),
            include_tree: None,
            main_path: None,
            diagnostics: Vec::new(),
            type_map: HashMap::new(),
            scope_store: ScopeStore::new(),
            scope_extents: vec![(ScopeId(0), (0, len))],
            definition_map: HashMap::new(),
            doc_map: HashMap::new(),
            include_doc_maps: None,
            class_super: HashMap::new(),
            sig_definition_locations: None,
        }
    }

    /// Build document state from source using parse only (no semantic analysis).
    /// Use when full analysis panics so the LSP can still provide syntax highlighting and basic features.
    #[must_use]
    pub fn from_parse_only(source: &str) -> Self {
        let mut diagnostics = Vec::new();
        let root = match parse_recovering_multi(source, PARSE_RECOVERY_MAX_ERRORS) {
            Ok(output) => output.syntax_root(source.as_bytes()),
            Err(recover) => {
                for parse_err in &recover.errors {
                    diagnostics.extend(parse_error_to_diagnostics(parse_err, source));
                }
                recover.partial.syntax_root(source.as_bytes())
            }
        };
        let source_owned = source.to_string();
        let line_index = LineIndex::new(source_owned.as_bytes());
        let scope_extents = vec![(ScopeId(0), (0, source.len() as u32))];
        let doc_map = root.as_ref().map(build_doc_map).unwrap_or_default();
        let class_super = build_class_super(root.as_ref());
        Self {
            source: source_owned,
            line_index,
            root,
            include_tree: None,
            main_path: None,
            diagnostics,
            type_map: HashMap::new(),
            scope_store: ScopeStore::new(),
            scope_extents,
            definition_map: HashMap::new(),
            doc_map,
            include_doc_maps: None,
            class_super,
            sig_definition_locations: None,
        }
    }
}

/// Max parse errors to collect in recovery mode before stopping.
const PARSE_RECOVERY_MAX_ERRORS: usize = 64;

#[allow(clippy::too_many_arguments)]
fn single_file_analysis(
    source: &str,
    signature_roots: &[SyntaxNode],
    existing_root: Option<SyntaxNode>,
    max_parse_errors: usize,
    diagnostics: &mut Vec<sipha::error::SemanticDiagnostic>,
    type_map: &mut HashMap<(u32, u32), leekscript_core::Type>,
    scope_store: &mut ScopeStore,
    scope_extents: &mut Vec<(ScopeId, (u32, u32))>,
    root: &mut Option<SyntaxNode>,
) {
    let parsed = if let Some(r) = existing_root {
        Some(r)
    } else {
        match parse_recovering_multi(source, max_parse_errors) {
            Ok(output) => output.syntax_root(source.as_bytes()),
            Err(recover) => {
                for parse_err in &recover.errors {
                    diagnostics.extend(parse_error_to_diagnostics(parse_err, source));
                }
                recover.partial.syntax_root(source.as_bytes())
            }
        }
    };

    if let Some(ref r) = parsed {
        let result = if signature_roots.is_empty() {
            analyze(r)
        } else {
            analyze_with_signatures(r, signature_roots)
        };
        diagnostics.extend(result.diagnostics);
        *type_map = result.type_map;
        *scope_store = result.scope_store;
        *scope_extents = build_scope_extents(r, &result.scope_id_sequence, source.len());
        *root = Some(r.clone());
    } else {
        *scope_extents = vec![(ScopeId(0), (0, source.len() as u32))];
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use leekscript_core::{build_include_tree, parse};

    use super::{
        build_class_super, build_definition_map, decl_span_for_name_span, DocumentAnalysis,
        RootSymbolKind,
    };

    #[test]
    fn build_definition_map_main_only() {
        let source = r#"
global integer x = 1;
function f(integer a) -> integer { return a; }
class C { }
"#;
        let main_path = Path::new("main.leek");
        let tree = build_include_tree(source, Some(main_path)).expect("build_include_tree");
        let map = build_definition_map(&tree, main_path);
        assert!(map.contains_key(&("x".to_string(), RootSymbolKind::Global)));
        assert!(map.contains_key(&("f".to_string(), RootSymbolKind::Function)));
        assert!(map.contains_key(&("C".to_string(), RootSymbolKind::Class)));
        for (_, (path, start, end)) in &map {
            assert_eq!(path, main_path);
            assert!(end > start);
        }
    }

    #[test]
    fn build_class_super_single_inheritance() {
        let source = "class Child extends Parent { }";
        let root = parse(source).unwrap().expect("parse");
        let map = build_class_super(Some(&root));
        assert_eq!(map.get("Child"), Some(&"Parent".to_string()));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn build_class_super_none_for_no_extends() {
        let source = "class C { }";
        let root = parse(source).unwrap().expect("parse");
        let map = build_class_super(Some(&root));
        assert!(map.is_empty());
    }

    #[test]
    fn build_class_super_none_root() {
        let map = build_class_super(None);
        assert!(map.is_empty());
    }

    #[test]
    fn document_analysis_empty_source() {
        let analysis = DocumentAnalysis::new("", None, &[], None, None);
        assert_eq!(analysis.source, "");
        assert!(!analysis.scope_extents.is_empty());
    }

    #[test]
    fn document_analysis_incomplete_syntax_does_not_panic() {
        let source = "var x = ";
        let analysis = DocumentAnalysis::new(source, None, &[], None, None);
        let _ = &analysis.source;
        let _ = &analysis.scope_extents;
        let _ = &analysis.scope_store;
    }

    #[test]
    fn document_analysis_unclosed_brace_does_not_panic() {
        let source = "function f() { return 1; ";
        let analysis = DocumentAnalysis::new(source, None, &[], None, None);
        let _ = &analysis.diagnostics;
        let _ = &analysis.scope_extents;
    }

    #[test]
    fn document_analysis_symbol_at_offset_no_root_returns_none() {
        let analysis = DocumentAnalysis::new("", None, &[], None, None);
        assert!(analysis.symbol_at_offset(0).is_none());
    }

    #[test]
    fn document_analysis_type_at_offset_no_root_returns_none() {
        let analysis = DocumentAnalysis::new("", None, &[], None, None);
        assert!(analysis.type_at_offset(0).is_none());
    }

    #[test]
    fn decl_span_for_name_span_class() {
        let source = "class Foo { }";
        let root = parse(source).unwrap().expect("parse");
        // "Foo" is at offset 6, length 3
        let decl_span = decl_span_for_name_span(&root, 6, 9);
        assert!(decl_span.is_some());
        let (start, end) = decl_span.unwrap();
        assert!(end > start);
        assert!(start <= 6 && end >= 9);
    }

    #[test]
    fn decl_span_for_name_span_function() {
        let source = "function bar() { }";
        let root = parse(source).unwrap().expect("parse");
        // "bar" is at offset 9, length 3
        let decl_span = decl_span_for_name_span(&root, 9, 12);
        assert!(decl_span.is_some());
        let (start, end) = decl_span.unwrap();
        assert!(end > start);
    }

    #[test]
    fn decl_span_for_name_span_unknown_returns_none() {
        let source = "var x = 1;";
        let root = parse(source).unwrap().expect("parse");
        let decl_span = decl_span_for_name_span(&root, 4, 5); // "x" - var decl not class/function name
        assert!(decl_span.is_none());
    }
}
