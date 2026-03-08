//! Loading of signature files (.sig) for stdlib and API definitions.
//!
//! Use [`load_signatures_from_dir`] or [`default_signature_roots`] to obtain
//! parsed signature roots for [`DocumentAnalysis`](crate::DocumentAnalysis) and
//! [`analyze_with_signatures`].

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use sipha::line_index::LineIndex;
use sipha::red::SyntaxNode;

use crate::parse_signatures;
use leekscript_core::syntax::Kind;

/// Default directory for .sig files when no explicit path is given.
/// Override with env var `LEEKSCRIPT_SIGNATURES_DIR`.
pub const DEFAULT_SIGNATURES_DIR: &str = "examples/signatures";

/// Load signature AST roots from a directory (all `*.sig` files).
#[must_use]
pub fn load_signatures_from_dir(dir: &Path) -> Vec<SyntaxNode> {
    let mut roots = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return roots;
    };
    let mut files: Vec<_> = entries
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "sig"))
        .collect();
    files.sort_by_key(|p| p.as_os_str().to_owned());
    for path in files {
        if let Ok(s) = fs::read_to_string(&path) {
            if let Ok(Some(node)) = parse_signatures(&s) {
                roots.push(node);
            }
        }
    }
    roots
}

/// If no signature path was given, use default locations: `LEEKSCRIPT_SIGNATURES_DIR` env var
/// if set, else `examples/signatures`, else `leekscript-rs/examples/signatures` (when run from
/// workspace root). Returns empty vec if no directory is found.
#[must_use]
pub fn default_signature_roots() -> Vec<SyntaxNode> {
    default_signature_roots_with_locations().0
}

/// Build a map from function/global name to (path, 0-based line) for one parsed .sig file.
#[must_use]
pub fn build_sig_definition_locations(
    path: PathBuf,
    source: &str,
    root: &SyntaxNode,
) -> HashMap<String, (PathBuf, u32)> {
    let mut out = HashMap::new();
    let line_index = LineIndex::new(source.as_bytes());
    let file_nodes: Vec<SyntaxNode> = if root.kind_as::<Kind>() == Some(Kind::NodeSigFile) {
        vec![root.clone()]
    } else {
        root.children()
            .filter_map(|c| c.as_node().cloned())
            .filter(|n| n.kind_as::<Kind>() == Some(Kind::NodeSigFile))
            .collect()
    };
    for file in file_nodes {
        for child in file.children().filter_map(|c| c.as_node().cloned()) {
            if child.kind_as::<Kind>() == Some(Kind::NodeSigFunction)
                || child.kind_as::<Kind>() == Some(Kind::NodeSigGlobal)
            {
                let name = child
                    .descendant_tokens()
                    .into_iter()
                    .find(|t| t.kind_as::<Kind>() == Some(Kind::TokIdent))
                    .map(|t| t.text().to_string());
                if let Some(name) = name {
                    let start = child.text_range().start;
                    let (line, _) = line_index.line_col_utf16(source, start);
                    out.insert(name, (path.clone(), line));
                }
            }
        }
    }
    out
}

/// Like [`default_signature_roots`] but also returns a map from function/global name to (path, 0-based line)
/// for hover/definition links into .sig files.
#[must_use]
pub fn default_signature_roots_with_locations() -> (Vec<SyntaxNode>, HashMap<String, (PathBuf, u32)>)
{
    let candidates: Vec<PathBuf> =
        if let Some(ref d) = std::env::var_os("LEEKSCRIPT_SIGNATURES_DIR") {
            vec![d.into()]
        } else {
            vec![
                PathBuf::from(DEFAULT_SIGNATURES_DIR),
                PathBuf::from("leekscript-rs/examples/signatures"),
            ]
        };
    for dir in candidates {
        if dir.is_dir() {
            let (roots, locations) = load_signatures_from_dir_with_locations(&dir);
            if !roots.is_empty() {
                return (roots, locations);
            }
        }
    }
    (Vec::new(), HashMap::new())
}

/// Load signature roots and a map from function/global name to (path, 0-based line) for link resolution.
#[must_use]
pub fn load_signatures_from_dir_with_locations(
    dir: &Path,
) -> (Vec<SyntaxNode>, HashMap<String, (PathBuf, u32)>) {
    let mut roots = Vec::new();
    let mut locations = HashMap::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return (roots, locations);
    };
    let mut files: Vec<_> = entries
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "sig"))
        .collect();
    files.sort_by_key(|p| p.as_os_str().to_owned());
    for path in files {
        if let Ok(s) = fs::read_to_string(&path) {
            if let Ok(Some(node)) = parse_signatures(&s) {
                let path_buf = path.to_path_buf();
                for (name, (_, line)) in build_sig_definition_locations(path_buf.clone(), &s, &node)
                {
                    locations.insert(name, (path_buf.clone(), line));
                }
                roots.push(node);
            }
        }
    }
    (roots, locations)
}
