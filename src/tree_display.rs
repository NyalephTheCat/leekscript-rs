//! Utilities to display the parser's syntax tree in a readable ASCII/Unicode tree format.

use std::collections::HashSet;
use std::sync::Arc;

use sipha::red::{SyntaxElement, SyntaxNode, SyntaxToken};

use crate::syntax;

/// Options for formatting the syntax tree.
#[derive(Clone, Debug)]
pub struct TreeDisplayOptions {
    /// Include leaf tokens in the tree (otherwise only nodes are shown).
    pub show_tokens: bool,
    /// When showing tokens, include trivia (whitespace, comments).
    pub show_trivia: bool,
    /// Maximum length of token text to display; longer text is truncated with "...".
    pub max_token_text_len: usize,
    /// Indent string for each level (e.g. "  " or "│ ").
    pub indent: String,
}

impl Default for TreeDisplayOptions {
    fn default() -> Self {
        Self {
            show_tokens: true,
            show_trivia: false,
            max_token_text_len: 40,
            indent: "  ".to_string(),
        }
    }
}

impl TreeDisplayOptions {
    /// Structure-only: only syntax nodes, no tokens.
    pub fn structure_only() -> Self {
        Self {
            show_tokens: false,
            ..Self::default()
        }
    }

    /// Full tree: nodes and all tokens including trivia.
    pub fn full() -> Self {
        Self {
            show_trivia: true,
            ..Self::default()
        }
    }
}

/// Identity for cycle detection: (green node pointer, offset).
fn node_id(node: &SyntaxNode) -> (usize, u32) {
    (
        Arc::as_ptr(node.green()) as usize,
        node.offset(),
    )
}

/// Format a syntax tree starting at `root` as a multi-line string.
///
/// If `root` is a synthetic root (single wrapper node), it is unwrapped so the
/// first child is used as the displayed root. Detects cycles and prints `[cycle]`
/// to avoid infinite recursion.
pub fn format_syntax_tree(root: &SyntaxNode, options: &TreeDisplayOptions) -> String {
    let root = unwrap_synthetic_root(root);
    let mut out = String::new();
    let mut visited = HashSet::new();
    format_node(root, options, "", true, &mut out, &mut visited);
    out
}

/// Print the syntax tree to stdout.
pub fn print_syntax_tree(root: &SyntaxNode, options: &TreeDisplayOptions) {
    println!("{}", format_syntax_tree(root, options));
}

fn unwrap_synthetic_root(root: &SyntaxNode) -> SyntaxNode {
    if root.kind() == syntax::SYNTHETIC_ROOT {
        root.child_nodes().next().unwrap_or_else(|| root.clone())
    } else {
        root.clone()
    }
}

fn format_node(
    node: SyntaxNode,
    options: &TreeDisplayOptions,
    prefix: &str,
    is_last: bool,
    out: &mut String,
    visited: &mut HashSet<(usize, u32)>,
) {
    let id = node_id(&node);
    if !visited.insert(id) {
        let kind_str = syntax::kind_name(node.kind());
        let connector = if is_last { "└── " } else { "├── " };
        out.push_str(prefix);
        out.push_str(connector);
        out.push_str(kind_str);
        out.push_str(" [cycle]\n");
        return;
    }

    let kind_str = syntax::kind_name(node.kind());
    let connector = if is_last { "└── " } else { "├── " };
    out.push_str(prefix);
    out.push_str(connector);
    out.push_str(kind_str);
    out.push('\n');

    let new_prefix = if is_last {
        format!("{}    ", prefix)
    } else {
        format!("{}│   ", prefix)
    };

    if options.show_tokens {
        let elements: Vec<SyntaxElement> = node.children().collect();
        let visible: Vec<(SyntaxElement, bool)> = {
            let visible: Vec<_> = elements
                .into_iter()
                .filter(|e| {
                    !matches!(e, SyntaxElement::Token(t) if t.is_trivia() && !options.show_trivia)
                })
                .collect();
            let n = visible.len();
            visible
                .into_iter()
                .enumerate()
                .map(|(i, e)| (e, i == n.saturating_sub(1)))
                .collect()
        };
        for (elem, is_last) in visible {
            match elem {
                SyntaxElement::Node(n) => format_node(n, options, &new_prefix, is_last, out, visited),
                SyntaxElement::Token(t) => format_token(&t, options, &new_prefix, is_last, out),
            }
        }
    } else {
        let child_nodes: Vec<SyntaxNode> = node.child_nodes().collect();
        let last_idx = child_nodes.len().saturating_sub(1);
        for (i, child) in child_nodes.into_iter().enumerate() {
            format_node(child, options, &new_prefix, i == last_idx, out, visited);
        }
    }

    visited.remove(&id);
}

fn format_token(
    token: &SyntaxToken,
    options: &TreeDisplayOptions,
    prefix: &str,
    is_last: bool,
    out: &mut String,
) {
    let kind_str = syntax::kind_name(token.kind());
    let connector = if is_last { "└── " } else { "├── " };
    out.push_str(prefix);
    out.push_str(connector);
    out.push_str(kind_str);
    let text = token.text();
    if !text.is_empty() {
        let display = if text.len() > options.max_token_text_len {
            format!("{:?}...", &text[..options.max_token_text_len])
        } else {
            format!("{:?}", text)
        };
        out.push(' ');
        out.push_str(&display);
    }
    if token.is_trivia() {
        out.push_str(" (trivia)");
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_default() {
        let opts = TreeDisplayOptions::default();
        assert!(opts.show_tokens);
        assert!(!opts.show_trivia);
    }

    #[test]
    fn options_structure_only() {
        let opts = TreeDisplayOptions::structure_only();
        assert!(!opts.show_tokens);
    }

    #[test]
    fn options_full() {
        let opts = TreeDisplayOptions::full();
        assert!(opts.show_trivia);
    }
}
