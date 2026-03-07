//! `LeekScript` tree printer.
//!
//! Prints the syntax tree by emitting tokens in source order without modifying the tree.
//! Use [`format`] with a root [`sipha::red::SyntaxNode`] and [`FormatterOptions`].
//! Modifications (e.g. semicolon insertion/removal) can be added later as separate passes.

mod driver;
mod options;

pub use driver::{format, FormatDriver};
pub use options::{BraceStyle, FormatterOptions, IndentStyle, SemicolonStyle};
