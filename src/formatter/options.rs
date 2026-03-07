//! Formatter configuration options.
//!
//! Kept minimal for the pass-through printer. More options (semicolons, brace style, etc.)
//! can be added later when we support modifications.

use sipha::red::SyntaxNode;

/// Options for printing the syntax tree.
#[derive(Clone, Debug)]
pub struct FormatterOptions {
    /// Include trivia (whitespace, comments) in the output. If false, only semantic tokens are emitted.
    pub preserve_comments: bool,
    /// Wrap expressions in parentheses to make precedence explicit (e.g. `a + b + c * d` → `((a + b) + (c * d))`).
    pub parenthesize_expressions: bool,
    /// Add block comments `/* type */` with the inferred type after expressions and variables.
    pub annotate_types: bool,
    /// When `annotate_types` is true, use these signature roots (e.g. stdlib) so built-in function/global types are inferred.
    pub signature_roots: Option<Vec<SyntaxNode>>,
}

impl Default for FormatterOptions {
    fn default() -> Self {
        Self {
            preserve_comments: true,
            parenthesize_expressions: false,
            annotate_types: false,
            signature_roots: None,
        }
    }
}
