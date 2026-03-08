//! Find references for the LeekScript LSP.
//!
//! Finds all references to the symbol at a position in the current document (main file)
//! and in included files for root-level symbols (global, function, class).

use std::collections::HashSet;
use std::path::Path;

use sipha::line_index::LineIndex;
use sipha::red::SyntaxNode;
use tower_lsp::lsp_types::{Location, Position, Range, Url};

use crate::analysis::{
    class_decl_info, function_decl_info, param_name, scope_at_offset, var_decl_info, ResolvedSymbol,
};
use crate::document::RootSymbolKind;
use crate::syntax::Kind;
use crate::DocumentAnalysis;

fn path_to_uri(path: &Path) -> Option<Url> {
    Url::from_file_path(path).ok()
}

/// Convert a byte span to an LSP range using the given source and line index.
fn byte_span_to_range(source: &str, line_index: &LineIndex, start: u32, end: u32) -> Range {
    let (line_start, col_start) = crate::byte_offset_to_line_col_utf16(source, line_index, start);
    let (line_end, col_end) = crate::byte_offset_to_line_col_utf16(source, line_index, end);
    Range {
        start: Position {
            line: line_start,
            character: col_start,
        },
        end: Position {
            line: line_end,
            character: col_end,
        },
    }
}

/// True if the resolved symbol at a candidate position refers to the same symbol as the target.
fn symbol_matches(target: &ResolvedSymbol, resolved: Option<ResolvedSymbol>, _name: &str) -> bool {
    let Some(resolved) = resolved else {
        return false;
    };
    match (target, &resolved) {
        (ResolvedSymbol::Variable(v), ResolvedSymbol::Variable(v2)) => {
            v.name == v2.name && v.span.start == v2.span.start && v.span.end == v2.span.end
        }
        (ResolvedSymbol::Global(n), ResolvedSymbol::Global(n2)) => n == n2,
        (ResolvedSymbol::Function(n, _), ResolvedSymbol::Function(n2, _)) => n == n2,
        (ResolvedSymbol::Class(n), ResolvedSymbol::Class(n2)) => n == n2,
        _ => false,
    }
}

/// Find all references to the symbol at the given position in the current document.
///
/// Walks the main file's AST for identifier tokens, resolves each, and collects those that match
/// the symbol at the given position. When `include_declaration` is true, the definition location
/// is included in the results.
///
/// * `current_document_uri`: The URI of the document (main file) for building Locations.
#[must_use]
pub fn find_references(
    analysis: &DocumentAnalysis,
    position: Position,
    current_document_uri: Option<&str>,
    include_declaration: bool,
) -> Vec<Location> {
    let source = analysis.source.as_str();
    let line_index = &analysis.line_index;
    let byte_offset = match crate::line_col_utf16_to_byte(
        source,
        line_index,
        position.line,
        position.character,
    ) {
        Some(off) => off,
        None => return vec![],
    };
    let root = match analysis.root.as_ref() {
        Some(r) => r,
        None => return vec![],
    };
    let token = match root.token_at_offset(byte_offset) {
        Some(t) => t,
        None => return vec![],
    };
    if token.kind_as::<Kind>() != Some(Kind::TokIdent) {
        return vec![];
    }
    let target_symbol = match analysis.symbol_at_offset(byte_offset) {
        Some(s) => s,
        None => return vec![],
    };
    let target_name = token.text().to_string();

    let uri = match current_document_uri.and_then(|s| Url::parse(s).ok()) {
        Some(u) => u,
        None => return vec![],
    };

    let mut locations = Vec::new();

    if include_declaration {
        if let Some(decl_location) = definition_location_for_references(
            analysis,
            &target_symbol,
            &target_name,
            source,
            line_index,
            &uri,
        ) {
            locations.push(decl_location);
        }
    }

    for token in root.descendant_tokens() {
        if token.kind_as::<Kind>() != Some(Kind::TokIdent) {
            continue;
        }
        let name = token.text().to_string();
        if name != target_name {
            continue;
        }
        let range = token.text_range();
        let scope_id = scope_at_offset(&analysis.scope_extents, range.start);
        let resolved = analysis.scope_store.resolve(scope_id, &name);
        if symbol_matches(&target_symbol, resolved, &name) {
            let lsp_range = byte_span_to_range(source, line_index, range.start, range.end);
            locations.push(Location::new(uri.clone(), lsp_range));
        }
    }

    // For root-level symbols, also find references in included files.
    let def_info = match &target_symbol {
        ResolvedSymbol::Global(_) => {
            analysis.definition_span_for(&target_name, RootSymbolKind::Global)
        }
        ResolvedSymbol::Function(_, _) => {
            analysis.definition_span_for(&target_name, RootSymbolKind::Function)
        }
        ResolvedSymbol::Class(_) => {
            analysis.definition_span_for(&target_name, RootSymbolKind::Class)
        }
        ResolvedSymbol::Variable(_) => None,
    };
    if let (Some(ref tree), Some((ref def_path, def_start, def_end))) =
        (analysis.include_tree.as_ref(), def_info)
    {
        let kind = match &target_symbol {
            ResolvedSymbol::Global(_) => RootSymbolKind::Global,
            ResolvedSymbol::Function(_, _) => RootSymbolKind::Function,
            ResolvedSymbol::Class(_) => RootSymbolKind::Class,
            ResolvedSymbol::Variable(_) => unreachable!(),
        };
        for (path, child) in &tree.includes {
            let Some(ref incl_root) = child.root else {
                continue;
            };
            let incl_source = child.source.as_str();
            locations.extend(references_in_included_file(
                path,
                incl_root,
                incl_source,
                &target_name,
                kind,
                def_path,
                def_start,
                def_end,
            ));
        }
    }

    locations
}

/// Build the definition location for the given symbol for use in references (include_declaration).
/// Returns the definition location even when it is in an included file.
fn definition_location_for_references(
    analysis: &DocumentAnalysis,
    symbol: &ResolvedSymbol,
    name: &str,
    source: &str,
    line_index: &LineIndex,
    uri: &Url,
) -> Option<Location> {
    match symbol {
        ResolvedSymbol::Variable(v) => {
            let range = byte_span_to_range(source, line_index, v.span.start, v.span.end);
            Some(Location::new(uri.clone(), range))
        }
        ResolvedSymbol::Global(_) => definition_location_for_root_kind(
            analysis,
            name,
            RootSymbolKind::Global,
            source,
            line_index,
            uri,
        ),
        ResolvedSymbol::Function(_, _) => definition_location_for_root_kind(
            analysis,
            name,
            RootSymbolKind::Function,
            source,
            line_index,
            uri,
        ),
        ResolvedSymbol::Class(_) => definition_location_for_root_kind(
            analysis,
            name,
            RootSymbolKind::Class,
            source,
            line_index,
            uri,
        ),
    }
}

/// Build definition Location for a root-level symbol, including when the definition is in an included file.
fn definition_location_for_root_kind(
    analysis: &DocumentAnalysis,
    name: &str,
    kind: RootSymbolKind,
    main_source: &str,
    main_line_index: &LineIndex,
    main_uri: &Url,
) -> Option<Location> {
    let (path, start, end) = analysis.definition_span_for(name, kind)?;
    let (range, uri) = if analysis.main_path.as_ref() == Some(&path) {
        let range = byte_span_to_range(main_source, main_line_index, start, end);
        (range, main_uri.clone())
    } else if let Some(ref tree) = analysis.include_tree {
        let main_path = analysis.main_path.as_ref()?;
        let src = tree.source_for_path(main_path, &path)?;
        let idx = LineIndex::new(src.as_bytes());
        let range = byte_span_to_range(src, &idx, start, end);
        let uri = path_to_uri(&path)?;
        (range, uri)
    } else {
        return None;
    };
    Some(Location::new(uri, range))
}

/// Collect (start, end) byte spans of all declarations of the given name in the file (variable, param, function, class, global).
fn declaration_name_spans(root: &SyntaxNode, name: &str) -> HashSet<(u32, u32)> {
    let mut spans = HashSet::new();
    for node in root.descendant_nodes() {
        if node.kind_as::<Kind>() == Some(Kind::NodeVarDecl) {
            if let Some(info) = var_decl_info(&node) {
                if info.name == name {
                    spans.insert((info.name_span.start, info.name_span.end));
                }
            }
        } else if node.kind_as::<Kind>() == Some(Kind::NodeFunctionDecl) {
            if let Some(info) = function_decl_info(&node) {
                if info.name == name {
                    spans.insert((info.name_span.start, info.name_span.end));
                }
            }
        } else if node.kind_as::<Kind>() == Some(Kind::NodeClassDecl) {
            if let Some(info) = class_decl_info(&node) {
                if info.name == name {
                    spans.insert((info.name_span.start, info.name_span.end));
                }
            }
        } else if node.kind_as::<Kind>() == Some(Kind::NodeParam) {
            if let Some((param_name_str, span)) = param_name(&node) {
                if param_name_str == name {
                    spans.insert((span.start, span.end));
                }
            }
        }
    }
    spans
}

/// Find reference locations in an included file for a root-level symbol.
fn references_in_included_file(
    path: &Path,
    root: &SyntaxNode,
    source: &str,
    target_name: &str,
    _kind: RootSymbolKind,
    def_path: &Path,
    def_start: u32,
    def_end: u32,
) -> Vec<Location> {
    let line_index = LineIndex::new(source.as_bytes());
    let uri = match path_to_uri(path) {
        Some(u) => u,
        None => return vec![],
    };
    let decl_spans = declaration_name_spans(root, target_name);
    let is_def_in_this_file = path == def_path;
    let mut locations = Vec::new();
    for token in root.descendant_tokens() {
        if token.kind_as::<Kind>() != Some(Kind::TokIdent) {
            continue;
        }
        let name = token.text().to_string();
        if name != target_name {
            continue;
        }
        let range = token.text_range();
        let (start, end) = (range.start, range.end);
        if is_def_in_this_file && start == def_start && end == def_end {
            continue;
        }
        if decl_spans.contains(&(start, end)) {
            continue;
        }
        let lsp_range = byte_span_to_range(source, &line_index, start, end);
        locations.push(Location::new(uri.clone(), lsp_range));
    }
    locations
}
