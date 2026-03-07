//! Formatter configuration options.
//!
//! Kept minimal for the pass-through printer. More options (semicolons, brace style, etc.)
//! can be added later when we support modifications.

/// Options for printing the syntax tree.
///
// Currently only controls whether trivia (whitespace, comments) is included in the output.
#[derive(Clone, Debug)]
pub struct FormatterOptions {
    /// Include trivia (whitespace, comments) in the output. If false, only semantic tokens are emitted.
    pub preserve_comments: bool,
}

impl Default for FormatterOptions {
    fn default() -> Self {
        Self {
            preserve_comments: true,
        }
    }
}
