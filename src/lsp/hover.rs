//! Hover information for the LeekScript LSP.
//!
//! Resolves the symbol or expression at a position and builds markdown hover content
//! (type, kind, documentation from .sig or source).

use std::fmt::Write;

use sipha::line_index::LineIndex;
use sipha::red::SyntaxNode;
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range, Url};

use crate::analysis::{complexity_display_string, ResolvedSymbol, ScopeId, SigMeta, VariableKind};
use crate::document::{decl_span_for_name_span, RootSymbolKind};
use crate::syntax::Kind;
use crate::DocComment;
use crate::DocumentAnalysis;
use crate::Type;

/// Context for building links in doc comments (e.g. @file, @see). When present, file and symbol references become clickable.
struct DocLinkContext<'a> {
    analysis: &'a DocumentAnalysis,
    document_uri: &'a str,
}

/// Find the first character of a URL protocol in `s` starting at byte index `from`.
fn find_url_start(s: &str, from: usize) -> Option<(usize, &str)> {
    let rest = s.get(from..)?;
    if let Some(i) = rest.find("https://") {
        return Some((from + i, "https://"));
    }
    if let Some(i) = rest.find("http://") {
        return Some((from + i, "http://"));
    }
    if let Some(i) = rest.find("mailto:") {
        return Some((from + i, "mailto:"));
    }
    None
}

/// Return true if byte index `at` in `s` is inside the URL part of a markdown link (between `](` and `)`).
fn inside_markdown_link_url(s: &str, at: usize) -> bool {
    let before = match s.get(..at) {
        Some(b) => b,
        None => return false,
    };
    let link_start = match before.rfind("](") {
        Some(i) => i + 2,
        None => return false,
    };
    let after_link_start = match s.get(link_start..) {
        Some(a) => a,
        None => return false,
    };
    let close_paren = after_link_start.find(')').map(|i| link_start + i);
    match close_paren {
        Some(close) => at >= link_start && at < close,
        None => true,
    }
}

/// Replace whole-word occurrences of known symbols (classes, functions, globals) with markdown links to their definition.
/// Skips occurrences inside existing link URLs and when the symbol is part of a filename (e.g. Cell.leek).
fn linkify_symbols_in_text(s: &str, analysis: &DocumentAnalysis) -> String {
    let mut names: Vec<String> = analysis
        .definition_map
        .keys()
        .map(|(name, _)| name.clone())
        .collect();
    if let Some(ref sig_locs) = analysis.sig_definition_locations {
        for name in sig_locs.keys() {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
    }
    names.sort_by_key(|b| std::cmp::Reverse(b.len())); // longest first so "getCellX" before "getCell"
    let mut out = s.to_string();
    for name in names {
        if name.is_empty() {
            continue;
        }
        let mut search_from = 0;
        while let Some(pos) = out[search_from..].find(name.as_str()) {
            let start = search_from + pos;
            let end = start + name.len();
            if inside_markdown_link_url(&out, start) {
                search_from = end;
                continue;
            }
            let prev_ok = start == 0
                || !out[start - 1..]
                    .chars()
                    .next()
                    .is_none_or(|c| c.is_ascii_alphanumeric() || c == '_');
            let next_ch = out[end..].chars().next();
            let followed_by_parens = out.get(end..end + 2) == Some("()");
            let next_ok = next_ch.is_none()
                || (!next_ch.is_none_or(|c| c.is_ascii_alphanumeric() || c == '_')
                    && next_ch != Some('.')
                    && !followed_by_parens);
            if prev_ok && next_ok {
                if let Some((uri, line)) = definition_uri_and_line(analysis, &name) {
                    let link = format!("[{name}]({}#L{})", uri.as_str(), line + 1);
                    out.replace_range(start..end, &link);
                    search_from = start + link.len();
                } else {
                    search_from = end;
                }
            } else {
                search_from = end;
            }
        }
    }
    out
}

/// Turn URLs (http://, https://, mailto:) in text into markdown links.
fn linkify_urls_in_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pos = 0;
    loop {
        match find_url_start(s, pos) {
            Some((start, protocol)) => {
                out.push_str(&s[pos..start]);
                let after_protocol = start + protocol.len();
                let end = s[after_protocol..]
                    .find(|c: char| {
                        c.is_whitespace() || c == ')' || c == ']' || c == '"' || c == '>'
                    })
                    .map(|i| after_protocol + i)
                    .unwrap_or(s.len());
                let url = &s[start..end];
                let _ = write!(out, "[{url}]({url})");
                pos = end;
            }
            None => {
                out.push_str(&s[pos..]);
                break;
            }
        }
    }
    out
}

/// Get (file URL, 0-based line) for a root-level symbol's definition, if resolvable (source or .sig).
fn definition_uri_and_line(analysis: &DocumentAnalysis, name: &str) -> Option<(Url, u32)> {
    if let Some(kind) = [
        RootSymbolKind::Class,
        RootSymbolKind::Function,
        RootSymbolKind::Global,
    ]
    .into_iter()
    .find(|&k| analysis.definition_span_for(name, k).is_some())
    {
        let (path, start, _end) = analysis.definition_span_for(name, kind)?;
        let source = if analysis.main_path.as_ref() == Some(&path) {
            analysis.source.as_str()
        } else {
            let tree = analysis.include_tree.as_ref()?;
            let main = analysis.main_path.as_ref()?;
            tree.source_for_path(main, &path)?
        };
        let line_index = LineIndex::new(source.as_bytes());
        let (line, _) = crate::byte_offset_to_line_col_utf16(source, &line_index, start);
        let path_resolved = std::fs::canonicalize(&path).ok().unwrap_or(path);
        let uri = Url::from_file_path(&path_resolved).ok()?;
        return Some((uri, line));
    }
    if let Some(ref sig_locs) = analysis.sig_definition_locations {
        if let Some((path, line)) = sig_locs.get(name) {
            let path_resolved = std::fs::canonicalize(path).ok().unwrap_or(path.clone());
            let uri = Url::from_file_path(&path_resolved).ok()?;
            return Some((uri, *line));
        }
    }
    None
}

/// Build a file:// URL for @file value (filename) relative to the document URI's directory.
fn file_uri_for_doc_file(document_uri: &str, file_name: &str) -> Option<Url> {
    let doc_url = Url::parse(document_uri).ok()?;
    if doc_url.scheme() != "file" {
        return None;
    }
    let base_path = doc_url.to_file_path().ok()?;
    let parent = base_path.parent()?;
    let resolved = parent.join(file_name);
    Url::from_file_path(&resolved).ok()
}

/// Get the documentation for a root-level symbol by resolving its definition and looking up the doc map.
/// Works for both definition and reference positions (e.g. hovering on `Obstacle` in `new Obstacle(1)`).
fn doc_for_symbol<'a>(
    analysis: &'a DocumentAnalysis,
    name: &str,
    kind: RootSymbolKind,
) -> Option<&'a DocComment> {
    let (path, name_start, name_end) = analysis.definition_span_for(name, kind)?;
    let root = if analysis.main_path.as_ref() == Some(&path) {
        analysis.root.as_ref()?
    } else {
        let main = analysis.main_path.as_ref()?;
        analysis
            .include_tree
            .as_ref()
            .and_then(|t| t.root_for_path(main, &path))?
    };
    let decl_span = decl_span_for_name_span(root, name_start, name_end)?;
    let doc_map = if analysis.main_path.as_ref() == Some(&path) {
        &analysis.doc_map
    } else {
        analysis
            .include_doc_maps
            .as_ref()
            .and_then(|m| m.get(&path))?
    };
    doc_map.get(&decl_span)
}

/// Compute hover information at the given LSP position (UTF-16 line/character).
///
/// Returns `None` if the position cannot be resolved to a byte offset, there is no
/// parse tree, or the token at the position has no hover content (e.g. whitespace).
///
/// When `document_uri` is provided, @file and @see in doc comments become clickable links.
#[must_use]
pub fn compute_hover(
    analysis: &DocumentAnalysis,
    position: Position,
    document_uri: Option<&str>,
) -> Option<Hover> {
    let source = analysis.source.as_str();
    let line_index = &analysis.line_index;

    let byte_offset =
        crate::line_col_utf16_to_byte(source, line_index, position.line, position.character)?;

    let root = analysis.root.as_ref()?;
    let token = root.token_at_offset(byte_offset)?;
    let token_range = token.text_range();
    let range = lsp_range(source, line_index, token_range.start, token_range.end);

    let kind = token.kind_as::<Kind>();

    // Identifier: resolve symbol and show type + doc
    if kind == Some(Kind::TokIdent) {
        let name = token.text().to_string();
        if let Some(symbol) = analysis.symbol_at_offset(byte_offset) {
            let ty = analysis.type_at_offset(byte_offset);
            let contents = hover_contents_for_symbol(
                analysis,
                root,
                &name,
                token_range.start,
                token_range.end,
                &symbol,
                ty.as_ref(),
                document_uri,
            );
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: contents,
                }),
                range: Some(range),
            });
        }
        // Identifier that didn't resolve: show type if available and/or doc from declaration (e.g. method names)
        let mut value = String::new();
        if let Some(ty) = analysis.type_at_offset(byte_offset) {
            if !matches!(ty, Type::Error | Type::Warning) {
                value.push_str(&format!("`{}`", ty.for_annotation()));
            }
        }
        // Methods and other decls (e.g. in same file): doc_map is keyed by decl node span; find by name span
        if let Some(decl_span) = decl_span_for_name_span(root, token_range.start, token_range.end) {
            if let Some(doc) = analysis.doc_map.get(&decl_span) {
                let link_ctx = document_uri.map(|uri| DocLinkContext {
                    analysis,
                    document_uri: uri,
                });
                append_doc_comment(&mut value, doc, link_ctx);
            }
        }
        if !value.is_empty() {
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value,
                }),
                range: Some(range),
            });
        }
        return None;
    }

    // Keywords: short descriptions
    if let Some(kw) = kind {
        let desc = match kw {
            Kind::KwThis => "Reference to the current instance.",
            Kind::KwNull => "The null value.",
            Kind::KwTrue | Kind::KwFalse => "Boolean literal.",
            Kind::KwNew => "Constructor call.",
            _ => return None,
        };
        let ty = analysis.type_at_offset(byte_offset);
        let mut value = desc.to_string();
        if let Some(t) = ty {
            if !matches!(t, Type::Error | Type::Warning) {
                let _ = write!(value, "\n\n`{}`", t.for_annotation());
            }
        }
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: Some(range),
        });
    }

    // Literals (number, string): show type only
    if kind == Some(Kind::TokNumber) || kind == Some(Kind::TokString) {
        if let Some(ty) = analysis.type_at_offset(byte_offset) {
            if !matches!(ty, Type::Error | Type::Warning) {
                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("`{}`", ty.for_annotation()),
                    }),
                    range: Some(range),
                });
            }
        }
    }

    None
}

fn lsp_range(
    source: &str,
    line_index: &sipha::line_index::LineIndex,
    start: u32,
    end: u32,
) -> Range {
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

#[allow(clippy::too_many_arguments)]
fn hover_contents_for_symbol(
    analysis: &DocumentAnalysis,
    _root: &SyntaxNode,
    _name: &str,
    _name_start: u32,
    _name_end: u32,
    symbol: &ResolvedSymbol,
    inferred_type: Option<&Type>,
    document_uri: Option<&str>,
) -> String {
    let mut md = String::new();

    let scope_id = analysis
        .scope_extents
        .first()
        .map(|(id, _)| *id)
        .unwrap_or(ScopeId(0));

    match symbol {
        ResolvedSymbol::Variable(v) => {
            let kind_label = match v.kind {
                VariableKind::Local => "variable",
                VariableKind::Parameter => "parameter",
                VariableKind::Global => "global",
            };
            let ty = inferred_type
                .or(v.declared_type.as_ref())
                .map(|t| t.for_annotation())
                .unwrap_or_else(|| "any".to_string());
            md.push_str(&format!("**{kind_label}** `{ty}`"));
            // When variable type is a class, show that class's documentation on hover.
            if let Some(t) = inferred_type.or(v.declared_type.as_ref()) {
                let class_name = match t {
                    Type::Instance(n) => Some(n.as_str()),
                    Type::Class(Some(n)) => Some(n.as_str()),
                    _ => None,
                };
                if let Some(name) = class_name {
                    if let Some(doc) = doc_for_symbol(analysis, name, RootSymbolKind::Class) {
                        append_doc_comment(
                            &mut md,
                            doc,
                            document_uri.map(|uri| DocLinkContext {
                                analysis,
                                document_uri: uri,
                            }),
                        );
                    }
                }
            }
        }

        ResolvedSymbol::Global(name_str) => {
            let ty = analysis
                .scope_store
                .get(ScopeId(0))
                .and_then(|scope| scope.get_global_type(name_str))
                .map(|t| t.for_annotation())
                .or_else(|| inferred_type.map(|t| t.for_annotation()))
                .unwrap_or_else(|| "any".to_string());
            md.push_str(&format!("**global** `{ty}`"));
            if let Some(meta) = analysis.scope_store.get_root_global_meta(name_str) {
                append_sig_meta(
                    &mut md,
                    meta,
                    document_uri.map(|uri| DocLinkContext {
                        analysis,
                        document_uri: uri,
                    }),
                );
            } else if let Some(doc) = doc_for_symbol(analysis, name_str, RootSymbolKind::Global) {
                append_doc_comment(
                    &mut md,
                    doc,
                    document_uri.map(|uri| DocLinkContext {
                        analysis,
                        document_uri: uri,
                    }),
                );
            }
        }

        ResolvedSymbol::Function(name_str, _arity) => {
            let scope_ty = analysis
                .scope_store
                .get_function_type_as_value(scope_id, name_str);
            let ty = inferred_type.or(scope_ty.as_ref());
            if let Some(t) = ty {
                if !matches!(t, Type::Error | Type::Warning) {
                    md.push_str(&format!("**function** `{}`", t.for_annotation()));
                } else {
                    md.push_str(&format!("**function** `{name_str}`"));
                }
            } else {
                md.push_str(&format!("**function** `{name_str}`"));
            }
            if let Some(meta) = analysis.scope_store.get_root_function_meta(name_str) {
                append_sig_meta(
                    &mut md,
                    meta,
                    document_uri.map(|uri| DocLinkContext {
                        analysis,
                        document_uri: uri,
                    }),
                );
            } else if let Some(doc) = doc_for_symbol(analysis, name_str, RootSymbolKind::Function) {
                append_doc_comment(
                    &mut md,
                    doc,
                    document_uri.map(|uri| DocLinkContext {
                        analysis,
                        document_uri: uri,
                    }),
                );
            }
        }

        ResolvedSymbol::Class(class_name) => {
            let super_name = analysis.class_super.get(class_name);
            md.push_str("**class** ");
            md.push_str(class_name);
            if let Some(super_name) = super_name {
                let super_link = document_uri.and_then(|_| {
                    definition_uri_and_line(analysis, super_name).map(|(uri, line)| {
                        format!(" *extends* [{super_name}]({}#L{})", uri.as_str(), line + 1)
                    })
                });
                if let Some(link) = super_link {
                    md.push_str(&link);
                } else {
                    let _ = write!(md, " *extends* `{super_name}`");
                }
            }
            if let Some(doc) = doc_for_symbol(analysis, class_name, RootSymbolKind::Class) {
                append_doc_comment(
                    &mut md,
                    doc,
                    document_uri.map(|uri| DocLinkContext {
                        analysis,
                        document_uri: uri,
                    }),
                );
            }
        }
    }

    md
}

fn append_sig_meta(md: &mut String, meta: &SigMeta, link_context: Option<DocLinkContext<'_>>) {
    if let Some(ref doc) = meta.doc {
        append_doc_comment(md, doc, link_context);
    }
    if let Some(code) = meta.complexity {
        if !md.is_empty() {
            md.push_str("\n\n");
        }
        md.push_str("**Complexity:** ");
        md.push_str(complexity_display_string(code));
    }
}

fn doc_sep(md: &mut String) {
    if !md.is_empty() {
        md.push_str("\n\n");
    }
}

fn append_doc_comment(md: &mut String, doc: &DocComment, link_context: Option<DocLinkContext<'_>>) {
    let linkify = |s: &str| {
        let s = if let Some(ref ctx) = link_context {
            linkify_symbols_in_text(s, ctx.analysis)
        } else {
            s.to_string()
        };
        linkify_urls_in_text(&s)
    };

    if let Some(ref brief) = doc.brief {
        doc_sep(md);
        md.push_str(&linkify(brief));
    }
    if !doc.description.is_empty() && doc.brief.as_deref() != Some(doc.description.as_str()) {
        doc_sep(md);
        md.push_str(&linkify(doc.description.trim()));
    }
    if let Some(ref details) = doc.details {
        if !details.is_empty() {
            doc_sep(md);
            md.push_str(&linkify(details.trim()));
        }
    }
    for (param_name, param_desc) in &doc.params {
        doc_sep(md);
        let _ = write!(md, "- **{param_name}**: {}", linkify(param_desc));
    }
    if let Some(ref ret) = doc.returns {
        doc_sep(md);
        let _ = write!(md, "**Returns:** {}", linkify(ret));
    }
    for retval in &doc.retvals {
        doc_sep(md);
        let _ = write!(md, "**Retval:** {}", linkify(retval));
    }
    if let Some(ref dep) = doc.deprecated {
        doc_sep(md);
        let _ = write!(md, "**Deprecated:** {}", linkify(dep));
    }
    if !doc.see.is_empty() {
        doc_sep(md);
        let see_links: Vec<String> = doc
            .see
            .iter()
            .map(|s| {
                let s = s.trim();
                if let Some(ref ctx) = link_context {
                    if let Some((uri, line)) = definition_uri_and_line(ctx.analysis, s) {
                        return format!("[{s}]({}#L{})", uri.as_str(), line + 1);
                    }
                }
                format!("`{s}`")
            })
            .collect();
        let _ = write!(md, "**See:** {}", see_links.join(", "));
    }
    if let Some(ref since) = doc.since {
        doc_sep(md);
        let _ = write!(md, "**Since:** {}", linkify(since));
    }
    for note in &doc.notes {
        doc_sep(md);
        let _ = write!(md, "*Note:* {}", linkify(note));
    }
    for warning in &doc.warnings {
        doc_sep(md);
        let _ = write!(md, "**Warning:** {}", linkify(warning));
    }
    if let Some(ref author) = doc.author {
        doc_sep(md);
        let _ = write!(md, "**Author:** {}", linkify(author));
    }
    if let Some(ref version) = doc.version {
        doc_sep(md);
        let _ = write!(md, "**Version:** {}", linkify(version));
    }
    for exc in &doc.exceptions {
        doc_sep(md);
        let _ = write!(md, "**Exception:** {}", linkify(exc));
    }
    if let Some(ref pre) = doc.pre {
        doc_sep(md);
        let _ = write!(md, "**Pre:** {}", linkify(pre));
    }
    if let Some(ref post) = doc.post {
        doc_sep(md);
        let _ = write!(md, "**Post:** {}", linkify(post));
    }
    for (title, content) in &doc.sections {
        doc_sep(md);
        if title.is_empty() {
            if content.contains('\n') || content.contains("//") || content.contains('{') {
                let _ = write!(md, "```\n{content}\n```");
            } else {
                md.push_str(&linkify(content));
            }
        } else {
            let _ = write!(md, "**{title}**\n\n");
            if content.contains('\n') || content.contains("//") || content.contains('{') {
                let _ = write!(md, "```\n{content}\n```");
            } else {
                md.push_str(&linkify(content));
                md.push('\n');
            }
        }
    }
    if let Some(code) = doc.complexity {
        doc_sep(md);
        md.push_str("**Complexity:** ");
        md.push_str(complexity_display_string(code));
    }
    if let Some(ref date) = doc.date {
        doc_sep(md);
        let _ = write!(md, "**Date:** {}", linkify(date));
    }
    if let Some(ref file) = doc.file {
        doc_sep(md);
        if let Some(ref ctx) = link_context {
            if let Some(uri) = file_uri_for_doc_file(ctx.document_uri, file) {
                let _ = write!(md, "**File:** [{file}]({})", uri.as_str());
            } else {
                let _ = write!(md, "**File:** `{file}`");
            }
        } else {
            let _ = write!(md, "**File:** `{file}`");
        }
    }
    if let Some(ref class_name) = doc.class_name {
        doc_sep(md);
        let _ = write!(md, "**Class:** `{class_name}`");
    }
    if let Some(ref copyright) = doc.copyright {
        doc_sep(md);
        let _ = write!(md, "**Copyright:** {}", linkify(copyright));
    }
    if let Some(ref license) = doc.license {
        doc_sep(md);
        let _ = write!(md, "**License:** {}", linkify(license));
    }
    for todo in &doc.todos {
        doc_sep(md);
        let _ = write!(md, "*Todo:* {}", linkify(todo));
    }
    for inv in &doc.invariants {
        doc_sep(md);
        let _ = write!(md, "**Invariant:** {}", linkify(inv));
    }
}
