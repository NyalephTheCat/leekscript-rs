//! Completion (autocomplete) for the LeekScript LSP.
//!
//! Provides general completion (variables, globals, functions, classes, keywords),
//! member completion (after `expr.`), include path completion (inside `include("...")`),
//! and optional constructor completion after `new `.
//! Supports completionItem/resolve for lazy documentation from .sig metadata.

use std::collections::HashSet;
use std::fmt::Write;
use std::path::PathBuf;

use sipha::red::{SyntaxElement, SyntaxNode};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse, MarkupContent,
    MarkupKind,
};

use crate::analysis::{complexity_display_string, scope_at_offset, ScopeId, ScopeStore, SigMeta};
use crate::document::DocumentAnalysis;
use crate::syntax::Kind;
use crate::DocComment;
use crate::Type;

/// Completion item data key for resolve: document uri where completion was requested.
pub const DATA_KEY_URI: &str = "uri";
/// Completion item data key: "function" | "global".
pub const DATA_KEY_TYPE: &str = "type";
/// Completion item data key: symbol name (function or global name).
pub const DATA_KEY_NAME: &str = "name";

/// Compute completion items at the given LSP position.
///
/// Returns `Some(CompletionResponse::Array(items))` or `Some(CompletionResponse::Array([]))`
/// when the document is available; returns `None` only when position cannot be mapped to byte offset.
/// Stores `document_uri` in resolvable items' data for completionItem/resolve.
#[must_use]
pub fn compute_completion(
    analysis: &DocumentAnalysis,
    params: &CompletionParams,
    document_uri: &str,
) -> Option<CompletionResponse> {
    let source = analysis.source.as_str();
    let line_index = &analysis.line_index;
    let position = params.text_document_position.position;

    let byte_offset =
        crate::line_col_utf16_to_byte(source, line_index, position.line, position.character)?;

    let prefix = word_prefix_before(source, line_index, position.line, position.character);

    let items = if let Some(ref root) = analysis.root {
        // Include path completion: cursor is inside the string of include("...")
        if let Some(include_items) = include_path_completion(analysis, root, byte_offset) {
            include_items
        } else if let Some(member_items) = member_completion(analysis, root, byte_offset, &prefix) {
            // Member completion: cursor is inside the member part of `receiver.member`
            member_items
        } else {
            // General completion: variables, globals, functions, classes, keywords; optional "new " class names
            let mut items = general_completion(analysis, root, byte_offset, &prefix, document_uri);
            if let Some(new_items) =
                new_keyword_class_completion(analysis, root, byte_offset, &prefix)
            {
                items.extend(new_items);
            }
            items
        }
    } else {
        // No parse tree: offer keywords only
        keyword_completion_items(&prefix)
    };

    Some(CompletionResponse::Array(items))
}

/// Resolve a completion item: fill detail and documentation from scope_store (e.g. .sig metadata).
///
/// Call with the ScopeStore from the document identified by `item.data["uri"]` when present.
/// If item.data does not contain resolvable keys or scope_store is None, returns the item unchanged.
#[must_use]
pub fn resolve_completion_item(
    mut item: CompletionItem,
    scope_store: Option<&ScopeStore>,
) -> CompletionItem {
    let Some(store) = scope_store else {
        return item;
    };
    let Some(data) = &item.data else {
        return item;
    };
    let Some(type_str) = data.get(DATA_KEY_TYPE).and_then(|v| v.as_str()) else {
        return item;
    };
    let Some(name) = data.get(DATA_KEY_NAME).and_then(|v| v.as_str()) else {
        return item;
    };

    let doc_markup = |meta: &SigMeta| -> tower_lsp::lsp_types::Documentation {
        tower_lsp::lsp_types::Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: sig_meta_to_markdown(meta),
        })
    };

    match type_str {
        "function" => {
            if let Some(meta) = store.get_root_function_meta(name) {
                if item.detail.is_none() {
                    item.detail = Some(format!("function {name}(…)"));
                }
                if item.documentation.is_none() {
                    item.documentation = Some(doc_markup(meta));
                }
            }
        }
        "global" => {
            if let Some(meta) = store.get_root_global_meta(name) {
                if item.documentation.is_none() {
                    item.documentation = Some(doc_markup(meta));
                }
            }
        }
        _ => {}
    }

    item
}

fn sig_meta_to_markdown(meta: &SigMeta) -> String {
    let mut md = String::new();
    if let Some(ref doc) = meta.doc {
        append_doc_comment(&mut md, doc);
    }
    if let Some(code) = meta.complexity {
        if !md.is_empty() {
            md.push_str("\n\n");
        }
        md.push_str("**Complexity:** ");
        md.push_str(complexity_display_string(code));
    }
    md
}

fn doc_sep(md: &mut String) {
    if !md.is_empty() {
        md.push_str("\n\n");
    }
}

fn append_doc_comment(md: &mut String, doc: &DocComment) {
    if let Some(ref brief) = doc.brief {
        doc_sep(md);
        md.push_str(brief);
    }
    if !doc.description.is_empty() && doc.brief.as_deref() != Some(doc.description.as_str()) {
        doc_sep(md);
        md.push_str(doc.description.trim());
    }
    for (param_name, param_desc) in &doc.params {
        doc_sep(md);
        let _ = write!(md, "- **{param_name}**: {param_desc}");
    }
    if let Some(ref ret) = doc.returns {
        doc_sep(md);
        let _ = write!(md, "**Returns:** {ret}");
    }
    if let Some(ref dep) = doc.deprecated {
        doc_sep(md);
        let _ = write!(md, "**Deprecated:** {dep}");
    }
    if let Some(code) = doc.complexity {
        doc_sep(md);
        md.push_str("**Complexity:** ");
        md.push_str(complexity_display_string(code));
    }
}

fn word_prefix_before(
    source: &str,
    line_index: &sipha::line_index::LineIndex,
    line: u32,
    character: u32,
) -> String {
    let Some(prefix) = crate::line_prefix_utf16(source, line_index, line, character) else {
        return String::new();
    };
    prefix
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

/// Completion for include path: when cursor is inside the string argument of `include("...")`,
/// suggest .leek files and subdirectories under the document's directory.
fn include_path_completion(
    analysis: &DocumentAnalysis,
    root: &SyntaxNode,
    byte_offset: u32,
) -> Option<Vec<CompletionItem>> {
    let node = root.node_at_offset(byte_offset)?;
    let include_node = node
        .ancestors(root)
        .into_iter()
        .find(|n| n.kind_as::<Kind>() == Some(Kind::NodeInclude))?;
    // Find the string token (first TokString in this include node)
    let string_token = include_node
        .descendant_tokens()
        .into_iter()
        .find(|t| t.kind_as::<Kind>() == Some(Kind::TokString))?;
    let range = string_token.text_range();
    // Cursor must be inside the string content (between the quotes)
    let quote_start = range.start as usize;
    if byte_offset <= range.start + 1 || byte_offset >= range.end.saturating_sub(1) {
        return None;
    }
    let source = analysis.source.as_str();
    let path_prefix = source
        .get((quote_start + 1)..(byte_offset as usize))
        .unwrap_or("");
    let base_dir: PathBuf = analysis
        .main_path
        .as_ref()
        .and_then(|p| p.as_path().parent().map(PathBuf::from))?;
    let (list_dir, file_prefix, dir_prefix): (PathBuf, &str, String) =
        if let Some(slash) = path_prefix.rfind('/') {
            let dir_part = &path_prefix[..slash];
            let file_prefix = &path_prefix[slash + 1..];
            let dir_prefix = format!("{dir_part}/");
            (base_dir.join(dir_part), file_prefix, dir_prefix)
        } else {
            (base_dir, path_prefix, String::new())
        };
    let read_dir = std::fs::read_dir(&list_dir).ok()?;
    let mut items = Vec::new();
    for entry in read_dir.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }
        let (label, prefix_match): (String, bool) = if entry.path().is_dir() {
            let label = format!("{name_str}/");
            let match_prefix = file_prefix.is_empty()
                || name_str.starts_with(file_prefix)
                || file_prefix == name_str;
            (label, match_prefix)
        } else if name_str.ends_with(".leek") {
            let label = name_str.to_string();
            let match_prefix = file_prefix.is_empty() || name_str.starts_with(file_prefix);
            (label, match_prefix)
        } else {
            continue;
        };
        if !prefix_match {
            continue;
        }
        let full_label = format!("{dir_prefix}{label}");
        items.push(CompletionItem {
            label: full_label.clone(),
            kind: Some(if label.ends_with('/') {
                CompletionItemKind::FOLDER
            } else {
                CompletionItemKind::FILE
            }),
            detail: Some("include path".to_string()),
            insert_text: Some(full_label),
            ..Default::default()
        });
    }
    if items.is_empty() {
        return None;
    }
    Some(items)
}

/// Check if cursor is inside the member part of a NodeMemberExpr (after the dot).
fn member_completion(
    analysis: &DocumentAnalysis,
    root: &SyntaxNode,
    byte_offset: u32,
    prefix: &str,
) -> Option<Vec<CompletionItem>> {
    let node = root.node_at_offset(byte_offset)?;
    let member_expr = node
        .ancestors(root)
        .into_iter()
        .find(|n| n.kind_as::<Kind>() == Some(Kind::NodeMemberExpr))?;
    // Cursor must be after the dot (in the member token or empty after dot)
    let receiver = member_expr.first_child_node()?;
    let receiver_span = receiver.text_range();
    if byte_offset <= receiver_span.end {
        return None;
    }
    let mut after_dot = false;
    let mut member_start = receiver_span.end;
    for elem in member_expr.children() {
        match &elem {
            SyntaxElement::Token(t) if !t.is_trivia() => {
                if t.text() == "." {
                    after_dot = true;
                } else if after_dot {
                    let r = t.text_range();
                    member_start = r.start;
                    break;
                }
            }
            _ => {}
        }
    }
    if !after_dot {
        member_start = receiver_span.end + 1; // just after "."
    }
    if byte_offset < member_start {
        return None;
    }

    let receiver_ty = analysis.type_at_offset(receiver_span.start)?;
    if matches!(receiver_ty, Type::Error | Type::Warning) {
        return None;
    }

    let class_name = match &receiver_ty {
        Type::Instance(name) => name.as_str(),
        Type::Class(Some(name)) => name.as_str(),
        _ => return None,
    };

    let members = analysis.scope_store.get_class_members(class_name)?;
    let mut items = Vec::new();
    let is_static = matches!(&receiver_ty, Type::Class(Some(_)));

    if is_static {
        for (name, (ty, _vis)) in &members.static_fields {
            if prefix.is_empty() || name.starts_with(prefix) {
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::FIELD),
                    detail: Some(ty.for_annotation()),
                    ..Default::default()
                });
            }
        }
        for (name, (_params, ret, _vis)) in &members.static_methods {
            if prefix.is_empty() || name.starts_with(prefix) {
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::METHOD),
                    detail: Some(format!("{}(…) -> {}", name, ret.for_annotation())),
                    ..Default::default()
                });
            }
        }
    } else {
        for (name, (ty, _vis)) in &members.fields {
            if prefix.is_empty() || name.starts_with(prefix) {
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::FIELD),
                    detail: Some(ty.for_annotation()),
                    ..Default::default()
                });
            }
        }
        for (name, (_params, ret, _vis)) in &members.methods {
            if prefix.is_empty() || name.starts_with(prefix) {
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::METHOD),
                    detail: Some(format!("{}(…) -> {}", name, ret.for_annotation())),
                    ..Default::default()
                });
            }
        }
    }

    Some(items)
}

fn general_completion(
    analysis: &DocumentAnalysis,
    _root: &SyntaxNode,
    byte_offset: u32,
    prefix: &str,
    document_uri: &str,
) -> Vec<CompletionItem> {
    let scope_id = scope_at_offset(&analysis.scope_extents, byte_offset);
    let store = &analysis.scope_store;
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    // Variables in scope chain (inner wins)
    let mut id = Some(scope_id);
    while let Some(sid) = id {
        if let Some(scope) = store.get(sid) {
            for name in scope.variable_names() {
                if seen.insert(name.clone()) && (prefix.is_empty() || name.starts_with(prefix)) {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::VARIABLE),
                        detail: Some("variable".to_string()),
                        ..Default::default()
                    });
                }
            }
            id = scope.parent;
        } else {
            break;
        }
    }

    // Root scope: globals, functions, classes
    if let Some(root_scope) = store.get(ScopeId(0)) {
        if let Some(names) = root_scope.global_names() {
            for name in names {
                if seen.insert(name.clone()) && (prefix.is_empty() || name.starts_with(prefix)) {
                    let detail = root_scope
                        .get_global_type(&name)
                        .map(|t| t.for_annotation())
                        .unwrap_or_else(|| "global".to_string());
                    let data = resolvable_data(document_uri, "global", &name);
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::VARIABLE),
                        detail: Some(detail),
                        data,
                        ..Default::default()
                    });
                }
            }
        }
        if let Some(names) = root_scope.function_names() {
            for name in names {
                if seen.insert(name.clone()) && (prefix.is_empty() || name.starts_with(prefix)) {
                    let data = resolvable_data(document_uri, "function", &name);
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::FUNCTION),
                        detail: Some(format!("function {name}(…)")),
                        data,
                        ..Default::default()
                    });
                }
            }
        }
        if let Some(names) = root_scope.class_names() {
            for name in names {
                if seen.insert(name.clone()) && (prefix.is_empty() || name.starts_with(prefix)) {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::CLASS),
                        detail: Some("class".to_string()),
                        ..Default::default()
                    });
                }
            }
        }
    }

    items.extend(keyword_completion_items(prefix));
    items
}

fn resolvable_data(document_uri: &str, type_str: &str, name: &str) -> Option<serde_json::Value> {
    Some(serde_json::json!({
        DATA_KEY_URI: document_uri,
        DATA_KEY_TYPE: type_str,
        DATA_KEY_NAME: name,
    }))
}

fn new_keyword_class_completion(
    analysis: &DocumentAnalysis,
    _root: &SyntaxNode,
    byte_offset: u32,
    prefix: &str,
) -> Option<Vec<CompletionItem>> {
    if !prefix.is_empty() {
        return None;
    }
    let source = analysis.source.as_str();
    if byte_offset < 4 {
        return None;
    }
    let before = source.get((byte_offset - 4) as usize..byte_offset as usize)?;
    if before != "new " && before != "new\t" {
        return None;
    }
    let mut items = Vec::new();
    if let Some(root_scope) = analysis.scope_store.get(ScopeId(0)) {
        if let Some(names) = root_scope.class_names() {
            for name in names {
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::CLASS),
                    detail: Some("class".to_string()),
                    ..Default::default()
                });
            }
        }
    }
    Some(items)
}

fn keyword_completion_items(prefix: &str) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    for kw in crate::syntax::KEYWORDS {
        if prefix.is_empty() || kw.starts_with(prefix) {
            items.push(CompletionItem {
                label: (*kw).to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("keyword".to_string()),
                ..Default::default()
            });
        }
    }
    items
}
