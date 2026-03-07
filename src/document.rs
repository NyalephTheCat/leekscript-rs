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

use crate::analysis::{
    analyze, analyze_with_include_tree, analyze_with_signatures,
    class_decl_info, function_decl_info, var_decl_info,
    build_scope_extents, scope_at_offset, ResolvedSymbol, ScopeId, ScopeStore, VarDeclKind,
};
use sipha_analysis::collect_definitions;
use crate::doc_comment::{build_doc_map, DocComment};
use crate::parser::{parse, parse_error_to_diagnostics, parse_recovering_multi};
use crate::preprocess::{build_include_tree, IncludeTree};
use crate::syntax::Kind;
use crate::types::Type;

/// Kind of root-level symbol for definition map (matches scope seeding order).
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub enum RootSymbolKind {
    Class,
    Function,
    Global,
}

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

/// Build (name, kind) -> (path, start, end) for root-level symbols (includes first, then main).
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
            return class_decl_info(node).map(|info| (info.name, RootSymbolKind::Class, info.name_span));
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

/// Find the declaration node span (for doc_map lookup) that contains the given name span.
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
    None
}

/// Build map class_name -> superclass_name from the AST (for visibility: subclass can see protected).
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
    pub type_map: HashMap<(u32, u32), crate::types::Type>,
    pub scope_store: ScopeStore,
    /// Scope extent (ScopeId, (start_byte, end_byte)) for scope_at_offset.
    pub scope_extents: Vec<(ScopeId, (u32, u32))>,
    /// (name, kind) -> (path, start_byte, end_byte) for root-level symbols.
    pub definition_map: HashMap<(String, RootSymbolKind), (PathBuf, u32, u32)>,
    /// Map from declaration (start_byte, end_byte) to parsed Doxygen-style documentation.
    pub doc_map: HashMap<(u32, u32), DocComment>,
    /// When Some (with include_tree), doc_map per included file path.
    pub include_doc_maps: Option<HashMap<PathBuf, HashMap<(u32, u32), DocComment>>>,
    /// Class name -> superclass name (for visibility: subclass can see protected).
    pub class_super: HashMap<String, String>,
}

impl DocumentAnalysis {
    /// Run parsing and analysis for the given source.
    ///
    /// When `main_path` is `Some` and `build_include_tree` succeeds, uses the include tree and
    /// analyzes with included files and `signature_roots`. Otherwise parses a single file (or uses
    /// `existing_root` when provided, e.g. from incremental reparse) and optionally uses
    /// `signature_roots` for analysis. Parse errors are appended to `diagnostics` when the main
    /// program root is missing.
    #[must_use]
    pub fn new(
        source: &str,
        main_path: Option<&Path>,
        signature_roots: &[SyntaxNode],
        existing_root: Option<SyntaxNode>,
    ) -> Self {
        let mut diagnostics = Vec::new();
        let mut type_map = HashMap::new();
        let mut scope_store = ScopeStore::new();
        let mut scope_extents = vec![];
        let mut definition_map = HashMap::new();
        let mut doc_map = HashMap::new();
        let mut include_doc_maps: Option<HashMap<PathBuf, HashMap<(u32, u32), DocComment>>> = None;
        let mut include_tree: Option<IncludeTree> = None;
        let mut main_path_buf: Option<PathBuf> = main_path.map(Path::to_path_buf);
        let mut root: Option<SyntaxNode> = None;
        let mut source_owned = source.to_string();

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
                            diagnostics.extend(parse_error_to_diagnostics(&parse_err, &tree.source));
                        }
                    }
                    definition_map = build_definition_map(&tree, path);
                    doc_map = tree
                        .root
                        .as_ref()
                        .map(build_doc_map)
                        .unwrap_or_default();
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
                        existing_root,
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
                    existing_root,
                    &mut diagnostics,
                    &mut type_map,
                    &mut scope_store,
                    &mut scope_extents,
                    &mut root,
                );
            }
        }

        if doc_map.is_empty() && root.is_some() && include_tree.is_none() {
            doc_map = build_doc_map(root.as_ref().unwrap());
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
        }
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
        self.definition_map
            .get(&(name.to_string(), kind))
            .cloned()
    }
}

/// Max parse errors to collect in recovery mode before stopping.
const PARSE_RECOVERY_MAX_ERRORS: usize = 64;

fn single_file_analysis(
    source: &str,
    signature_roots: &[SyntaxNode],
    existing_root: Option<SyntaxNode>,
    diagnostics: &mut Vec<sipha::error::SemanticDiagnostic>,
    type_map: &mut HashMap<(u32, u32), crate::types::Type>,
    scope_store: &mut ScopeStore,
    scope_extents: &mut Vec<(ScopeId, (u32, u32))>,
    root: &mut Option<SyntaxNode>,
) {
    let parsed = if let Some(r) = existing_root {
        Some(r)
    } else {
        match parse_recovering_multi(source, PARSE_RECOVERY_MAX_ERRORS) {
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
