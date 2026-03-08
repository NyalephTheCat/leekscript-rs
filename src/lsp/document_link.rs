//! Document links for `include("path")` — click on the path to open the included file.

use std::path::{Path, PathBuf};

use tower_lsp::lsp_types::{DocumentLink, Range, Url};

use crate::preprocess::collect_include_path_ranges;
use crate::DocumentAnalysis;

/// Build a file URL for an included path by resolving relative to the document's directory.
/// Uses `document_uri` so the target URL matches the client's URI format (e.g. same scheme for remote workspaces).
fn target_uri_for_include(
    document_uri: Option<&str>,
    path_str: &str,
    main_path: Option<&PathBuf>,
) -> Option<Url> {
    let resolved = if let Some(uri_str) = document_uri {
        if let Ok(doc_url) = Url::parse(uri_str) {
            if doc_url.scheme() == "file" {
                if let Ok(doc_path) = doc_url.to_file_path() {
                    if let Some(base_dir) = doc_path.parent() {
                        let r = base_dir.join(path_str);
                        if r.is_absolute() {
                            Some(r)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                // Non-file scheme (e.g. vscode-remote): build target by path manipulation.
                let segments: Vec<_> = doc_url
                    .path_segments()
                    .map(|s| s.collect::<Vec<_>>())
                    .unwrap_or_default();
                if segments.is_empty() {
                    None
                } else {
                    let mut new_segments: Vec<String> = segments[..segments.len() - 1]
                        .iter()
                        .map(|s| (*s).to_string())
                        .collect();
                    for part in path_str.split('/').filter(|s| !s.is_empty()) {
                        new_segments.push(part.to_string());
                    }
                    let new_path = format!("/{}", new_segments.join("/"));
                    let mut target = doc_url.clone();
                    target.set_path(&new_path);
                    return Some(target);
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    let resolved = resolved.or_else(|| {
        let base_dir = main_path
            .map(PathBuf::as_path)
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new("."));
        let r = base_dir.join(path_str);
        if r.is_absolute() {
            Some(r)
        } else {
            None
        }
    })?;

    // Use canonical path when the file exists so the client opens the real path (helps with symlinks).
    let path_for_url = std::fs::canonicalize(&resolved).ok().unwrap_or(resolved);
    Url::from_file_path(&path_for_url).ok()
}

/// Convert a byte span to an LSP range using the given source and line index.
fn byte_span_to_range(
    source: &str,
    line_index: &sipha::line_index::LineIndex,
    start: u32,
    end: u32,
) -> Range {
    let (line_start, col_start) = crate::byte_offset_to_line_col_utf16(source, line_index, start);
    let (line_end, col_end) = crate::byte_offset_to_line_col_utf16(source, line_index, end);
    Range {
        start: tower_lsp::lsp_types::Position {
            line: line_start,
            character: col_start,
        },
        end: tower_lsp::lsp_types::Position {
            line: line_end,
            character: col_end,
        },
    }
}

/// Compute document links for all `include("path")` strings in the document.
/// Each link's range covers the path string; the target is the resolved file URI.
///
/// `document_uri` should be the URI of the current document (e.g. from the LSP request).
/// It is used to resolve relative include paths and to build target URLs in the same format the client expects.
#[must_use]
pub fn compute_document_links(
    analysis: &DocumentAnalysis,
    document_uri: Option<&str>,
) -> Vec<DocumentLink> {
    let root = match &analysis.root {
        Some(r) => r,
        None => return vec![],
    };
    let source = analysis.source.as_str();
    let ranges = collect_include_path_ranges(root, source);
    let main_path = analysis.main_path.as_ref();

    let mut links = Vec::with_capacity(ranges.len());
    for (start, end, path_str) in ranges {
        let target = match target_uri_for_include(document_uri, &path_str, main_path) {
            Some(u) => u,
            None => continue,
        };
        let range = byte_span_to_range(source, &analysis.line_index, start, end);
        links.push(DocumentLink {
            range,
            target: Some(target),
            tooltip: Some("Open included file".to_string()),
            data: None,
        });
    }
    links
}
