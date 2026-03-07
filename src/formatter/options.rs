//! Formatter configuration options.
//!
//! Supports round-trip (preserve layout) and canonical formatting (normalize indentation, braces, semicolons).

use sipha::red::SyntaxNode;

/// Indentation style for canonical formatting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum IndentStyle {
    /// Use tab characters.
    #[default]
    Tabs,
    /// Use the given number of spaces per indent level.
    Spaces(u32),
}

/// Brace placement for blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BraceStyle {
    /// Opening brace on same line as keyword/expression: `if (x) {`.
    #[default]
    SameLine,
    /// Opening brace on next line.
    NextLine,
}

/// Semicolon style for statement ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SemicolonStyle {
    /// Ensure semicolon after every statement (add if missing).
    #[default]
    Always,
    /// Omit optional semicolons (emit only when required by grammar).
    Omit,
}

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
    /// When true, normalize layout: indentation, brace style, semicolons. Ignores source whitespace/comment layout.
    pub canonical_format: bool,
    /// Indentation for canonical format (tabs or N spaces).
    pub indent_style: IndentStyle,
    /// Brace placement for canonical format.
    pub brace_style: BraceStyle,
    /// Semicolon placement for canonical format.
    pub semicolon_style: SemicolonStyle,
}

impl Default for FormatterOptions {
    fn default() -> Self {
        Self {
            preserve_comments: true,
            parenthesize_expressions: false,
            annotate_types: false,
            signature_roots: None,
            canonical_format: false,
            indent_style: IndentStyle::default(),
            brace_style: BraceStyle::default(),
            semicolon_style: SemicolonStyle::default(),
        }
    }
}
