//! Include handling: parse files and build a tree of (path, source, AST) with circular include detection.
//!
//! Paths are resolved relative to the directory of the current file (or the current
//! working directory when parsing from stdin). Circular includes are detected and
//! reported as errors. No source expansion: each file keeps its own AST.

use std::path::{Path, PathBuf};

use sipha::red::SyntaxNode;
use sipha::types::IntoSyntaxKind;

use crate::parse;
use crate::syntax::Kind;

/// Error from the include preprocessor.
#[derive(Debug, Clone)]
pub enum IncludeError {
    /// File could not be read (e.g. not found, permission). Message includes path and reason.
    Io(String),
    /// Circular include detected. `path` is the file that was included again; `included_from` is the file that requested it (when known).
    CircularInclude {
        path: PathBuf,
        /// File that contained the `include(...)` leading to the cycle.
        included_from: Option<PathBuf>,
    },
    /// Invalid path (e.g. outside allowed base).
    InvalidPath(String),
}

impl std::fmt::Display for IncludeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IncludeError::Io(msg) => write!(f, "include: {msg}"),
            IncludeError::CircularInclude { path, included_from } => {
                write!(f, "circular include: {}", path.display())?;
                if let Some(from) = included_from {
                    write!(f, " (included from {})", from.display())?;
                }
                Ok(())
            }
            IncludeError::InvalidPath(msg) => write!(f, "include path: {msg}"),
        }
    }
}

impl std::error::Error for IncludeError {}

/// One file in the include tree: path, source, parsed root, and parsed included files in order.
#[derive(Debug, Clone)]
pub struct IncludeTree {
    /// Resolved path of this file (empty if from stdin / no path).
    pub path: PathBuf,
    /// Full source of this file.
    pub source: String,
    /// Parsed AST (program root); None if parse failed or file is empty.
    pub root: Option<SyntaxNode>,
    /// Resolved path and subtree for each `include("...")` in order.
    pub includes: Vec<(PathBuf, IncludeTree)>,
}

impl IncludeTree {
    /// Root AST for a path within this tree (main file or an included file).
    #[must_use]
    pub fn root_for_path(&self, main_path: &Path, path: &Path) -> Option<&SyntaxNode> {
        if path == main_path {
            return self.root.as_ref();
        }
        for (p, child) in &self.includes {
            if p.as_path() == path {
                return child.root.as_ref();
            }
        }
        None
    }

    /// Source for a path within this tree (main file or an included file).
    #[must_use]
    pub fn source_for_path(&self, main_path: &Path, path: &Path) -> Option<&str> {
        if path == main_path {
            return Some(self.source.as_str());
        }
        for (p, child) in &self.includes {
            if p.as_path() == path {
                return Some(child.source.as_str());
            }
        }
        None
    }
}

/// Build the include tree: parse `source` as the main file, resolve each `include("path")`,
/// load and parse those files (with circular include detection), and return the tree.
///
/// If `base_path` is `None` (e.g. stdin), the current working directory is used to resolve includes.
pub fn build_include_tree(
    source: &str,
    base_path: Option<&Path>,
) -> Result<IncludeTree, IncludeError> {
    let base_dir = base_path
        .and_then(|p| p.parent())
        .unwrap_or_else(|| Path::new("."));
    let current_path = base_path.map(Path::to_path_buf).unwrap_or_else(PathBuf::new);
    let mut visited = std::collections::HashSet::new();
    build_include_tree_impl(source, base_dir, &current_path, &mut visited)
}

fn build_include_tree_impl(
    source: &str,
    base_dir: &Path,
    current_path: &PathBuf,
    visited: &mut std::collections::HashSet<PathBuf>,
) -> Result<IncludeTree, IncludeError> {
    let root = match parse(source) {
        Ok(Some(r)) => Some(r),
        Ok(None) => None,
        Err(_) => None,
    };

    let include_paths = root
        .as_ref()
        .map(|r| collect_include_paths(r, source))
        .unwrap_or_default();

    let mut includes = Vec::with_capacity(include_paths.len());
    for path_str in include_paths {
        let resolved = resolve_path(base_dir, &path_str)?;
        let content = std::fs::read_to_string(&resolved).map_err(|e| {
            let msg = match e.kind() {
                std::io::ErrorKind::NotFound => format!("file not found: {}", resolved.display()),
                std::io::ErrorKind::PermissionDenied => {
                    format!("permission denied: {}", resolved.display())
                }
                _ => format!("{}: {}", resolved.display(), e),
            };
            IncludeError::Io(msg)
        })?;
        let canonical = resolved.canonicalize().map_err(|e| {
            IncludeError::Io(format!("{}: {}", resolved.display(), e))
        })?;
        if !visited.insert(canonical.clone()) {
            return Err(IncludeError::CircularInclude {
                path: canonical,
                included_from: Some(current_path.clone()),
            });
        }
        let child_base = resolved.parent().unwrap_or(base_dir);
        let child_tree = build_include_tree_impl(&content, child_base, &resolved, visited)?;
        visited.remove(&canonical);
        includes.push((resolved, child_tree));
    }

    Ok(IncludeTree {
        path: current_path.clone(),
        source: source.to_string(),
        root,
        includes,
    })
}

/// Collect include path strings from a program root (order of `include("...")` in the file).
fn collect_include_paths(root: &SyntaxNode, source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for node in root.find_all_nodes(Kind::NodeInclude.into_syntax_kind()) {
        if let Some(path) = include_path_from_node(&node, bytes) {
            out.push(path);
        }
    }
    out
}

/// Extract the path string from a NodeInclude (first TokString token, unquoted).
fn include_path_from_node(node: &SyntaxNode, source_bytes: &[u8]) -> Option<String> {
    let token = node
        .descendant_tokens()
        .into_iter()
        .find(|t| t.kind_as::<Kind>() == Some(Kind::TokString))?;
    let range = token.text_range();
    let start = range.start as usize;
    if start >= source_bytes.len() {
        return None;
    }
    parse_include_string(source_bytes, start).map(|(s, _)| s)
}

fn parse_include_string(bytes: &[u8], i: usize) -> Option<(String, usize)> {
    if i >= bytes.len() {
        return None;
    }
    let quote = bytes[i];
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let mut out = String::new();
    let mut j = i + 1;
    while j < bytes.len() {
        if bytes[j] == b'\\' && j + 1 < bytes.len() {
            match bytes[j + 1] {
                b'n' => out.push('\n'),
                b't' => out.push('\t'),
                b'r' => out.push('\r'),
                b'"' => out.push('"'),
                b'\'' => out.push('\''),
                b'\\' => out.push('\\'),
                b'u' if j + 5 < bytes.len() => {
                    let hex = std::str::from_utf8(&bytes[j + 2..j + 6]).ok()?;
                    let code = u32::from_str_radix(hex, 16).ok()?;
                    out.push(char::from_u32(code)?);
                    j += 4;
                }
                _ => out.push(bytes[j + 1] as char),
            }
            j += 2;
            continue;
        }
        if bytes[j] == quote {
            return Some((out, j + 1));
        }
        out.push(bytes[j] as char);
        j = next_char_boundary(bytes, j);
    }
    None
}

fn next_char_boundary(bytes: &[u8], i: usize) -> usize {
    if i >= bytes.len() {
        return bytes.len();
    }
    let b = bytes[i];
    if b < 128 {
        return i + 1;
    }
    let mut j = i + 1;
    while j < bytes.len() && (bytes[j] & 0xC0) == 0x80 {
        j += 1;
    }
    j
}

fn resolve_path(base_dir: &Path, path_str: &str) -> Result<PathBuf, IncludeError> {
    let path = Path::new(path_str);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(base_dir.join(path))
}

/// Flatten the tree into (path, source) for all files (root first, then includes depth-first).
pub fn all_files(tree: &IncludeTree) -> Vec<(PathBuf, &str)> {
    let mut out = vec![(tree.path.clone(), tree.source.as_str())];
    for (_, child) in &tree.includes {
        out.extend(all_files(child));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_tree_inlines_nothing_but_parses_includes() {
        let dir = std::env::temp_dir().join("leekscript_include_test");
        let _ = std::fs::create_dir_all(&dir);
        let main_path = dir.join("main.leek");
        let lib_path = dir.join("lib.leek");
        std::fs::write(&lib_path, "var x = 42;\n").unwrap();
        std::fs::write(&main_path, "include(\"lib.leek\");\nreturn 0;\n").unwrap();
        let source = std::fs::read_to_string(&main_path).unwrap();
        let tree = build_include_tree(&source, Some(main_path.as_path())).unwrap();
        assert!(tree.root.is_some(), "main should parse");
        assert_eq!(tree.includes.len(), 1, "one include");
        assert_eq!(tree.includes[0].0, lib_path);
        assert!(tree.includes[0].1.source.contains("var x = 42"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn circular_include_errors() {
        let dir = std::env::temp_dir().join("leekscript_circular_test");
        let _ = std::fs::create_dir_all(&dir);
        let a_path = dir.join("a.leek");
        let b_path = dir.join("b.leek");
        std::fs::write(&a_path, "include(\"b.leek\");\n").unwrap();
        std::fs::write(&b_path, "include(\"a.leek\");\n").unwrap();
        let source = std::fs::read_to_string(&a_path).unwrap();
        let result = build_include_tree(&source, Some(a_path.as_path()));
        assert!(
            matches!(result, Err(IncludeError::CircularInclude { .. })),
            "expected CircularInclude: {:?}",
            result
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
