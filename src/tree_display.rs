//! Utilities to display the parser's syntax tree in a readable ASCII/Unicode tree format.
//!
//! Delegates to sipha's grammar-agnostic [`format_syntax_tree`](sipha::tree_display::format_syntax_tree)
//! with a kind-name callback for LeekScript syntax kinds.

use sipha::red::SyntaxNode;
use sipha::tree_display::format_syntax_tree as sipha_format_syntax_tree;

use crate::syntax;

/// Options for formatting the syntax tree (re-exported from sipha).
pub use sipha::tree_display::TreeDisplayOptions;

/// Format a syntax tree starting at `root` as a multi-line string.
///
/// Uses LeekScript kind names for node and token labels.
#[must_use]
pub fn format_syntax_tree(root: &SyntaxNode, options: &TreeDisplayOptions) -> String {
    sipha_format_syntax_tree(root, options, |k| syntax::kind_name(k).to_string())
}

/// Print the syntax tree to stdout.
pub fn print_syntax_tree(root: &SyntaxNode, options: &TreeDisplayOptions) {
    println!("{}", format_syntax_tree(root, options));
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
