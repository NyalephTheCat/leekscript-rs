//! Include preprocessor: expands `include(path)` by inlining file contents.
//!
//! Paths are resolved relative to the directory of the current file (or the current
//! working directory when parsing from stdin). Circular includes are detected and
//! reported as errors.

use std::path::{Path, PathBuf};

/// Error from the include preprocessor.
#[derive(Debug, Clone)]
pub enum IncludeError {
    /// File could not be read (e.g. not found, permission).
    Io(String),
    /// Circular include detected.
    CircularInclude { path: PathBuf },
    /// Invalid path (e.g. outside allowed base).
    InvalidPath(String),
}

impl std::fmt::Display for IncludeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IncludeError::Io(msg) => write!(f, "include: {msg}"),
            IncludeError::CircularInclude { path } => {
                write!(f, "circular include: {}", path.display())
            }
            IncludeError::InvalidPath(msg) => write!(f, "include path: {msg}"),
        }
    }
}

impl std::error::Error for IncludeError {}

/// Expand all `include("path")` or `include('path')` in `source` by reading each
/// file (relative to `base_path`) and replacing the include statement with the
/// file contents. Included files are recursively processed. Path is relative to
/// the directory of the current file; if `base_path` is `None` (e.g. stdin),
/// the current working directory is used.
pub fn expand_includes(
    source: &str,
    base_path: Option<&Path>,
) -> Result<String, IncludeError> {
    let base_dir = base_path
        .and_then(|p| p.parent())
        .unwrap_or_else(|| Path::new("."));
    let mut visited = std::collections::HashSet::new();
    expand_includes_impl(source, base_dir, &mut visited)
}

fn expand_includes_impl(
    source: &str,
    base_dir: &Path,
    visited: &mut std::collections::HashSet<PathBuf>,
) -> Result<String, IncludeError> {
    let mut out = String::new();
    let mut i = 0;
    let bytes = source.as_bytes();

    while i < bytes.len() {
        // Look for "include" as a word (start of token), then ( then string then ).
        if word_match(bytes, i, b"include") {
            let start = i;
            i += 7;
            i = skip_whitespace_and_comments(bytes, i);
            if i < bytes.len() && bytes[i] == b'(' {
                i += 1;
                i = skip_whitespace_and_comments(bytes, i);
                if let Some((path_str, end)) = parse_include_string(bytes, i) {
                    i = end;
                    i = skip_whitespace_and_comments(bytes, i);
                    if i < bytes.len() && bytes[i] == b')' {
                        i += 1;
                        // Optional semicolon
                        i = skip_whitespace_and_comments(bytes, i);
                        if i < bytes.len() && bytes[i] == b';' {
                            i += 1;
                        }
                        let resolved = resolve_path(base_dir, &path_str)?;
                        let content = std::fs::read_to_string(&resolved)
                            .map_err(|e| IncludeError::Io(e.to_string()))?;
                        let canonical = resolved
                            .canonicalize()
                            .map_err(|e| IncludeError::Io(e.to_string()))?;
                        if !visited.insert(canonical.clone()) {
                            return Err(IncludeError::CircularInclude { path: canonical });
                        }
                        let include_base = resolved.parent().unwrap_or(base_dir);
                        let expanded =
                            expand_includes_impl(&content, include_base, visited)?;
                        visited.remove(&canonical);
                        out.push_str(expanded.trim_end());
                        // Ensure we don't join two statements without a separator
                        if !expanded.trim_end().ends_with(';') && !expanded.trim_end().is_empty() {
                            out.push(';');
                        }
                        out.push('\n');
                        continue;
                    }
                }
            }
            // Not a valid include; fall through and copy from start
            i = start;
        }

        let ch = source[i..].chars().next().unwrap_or('\0');
        out.push(ch);
        i += ch.len_utf8();
    }

    Ok(out)
}

fn word_match(bytes: &[u8], i: usize, word: &[u8]) -> bool {
    if i + word.len() > bytes.len() {
        return false;
    }
    if bytes[i..i + word.len()] != *word {
        return false;
    }
    let after = i + word.len();
    if after < bytes.len() {
        let c = bytes[after];
        if c.is_ascii_alphanumeric() || c == b'_' {
            return false;
        }
    }
    let before_ok = i == 0 || {
        let c = bytes[i - 1];
        !c.is_ascii_alphanumeric() && c != b'_'
    };
    before_ok
}

fn skip_whitespace_and_comments(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i = next_char_boundary(bytes, i);
            }
            continue;
        }
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i = next_char_boundary(bytes, i);
            }
            if i + 1 < bytes.len() {
                i += 2;
            }
            continue;
        }
        break;
    }
    i
}

/// Parse a double- or single-quoted string starting at i; return (unescaped content, index after string).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_includes_inlines_file() {
        let dir = std::env::temp_dir().join("leekscript_include_test");
        let _ = std::fs::create_dir_all(&dir);
        let main_path = dir.join("main.leek");
        let lib_path = dir.join("lib.leek");
        std::fs::write(&lib_path, "var x = 42;\n").unwrap();
        std::fs::write(&main_path, "include(\"lib.leek\");\nreturn 0;\n").unwrap();
        let source = std::fs::read_to_string(&main_path).unwrap();
        let expanded = expand_includes(&source, Some(main_path.as_path())).unwrap();
        assert!(
            expanded.contains("var x = 42"),
            "expanded should contain included content: {:?}",
            expanded
        );
        assert!(
            expanded.contains("return 0"),
            "expanded should contain following content: {:?}",
            expanded
        );
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
        let result = expand_includes(&source, Some(a_path.as_path()));
        assert!(
            matches!(result, Err(IncludeError::CircularInclude { .. })),
            "expected CircularInclude: {:?}",
            result
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
