//! Format driver: emits the syntax tree to a string (round-trip).
//!
//! Uses sipha's emit API for a single canonical tree-to-string path. Pass-through
//! printer: no reformatting, no semicolon insertion/removal.

use sipha::emit::{syntax_root_to_string, EmitOptions};
use sipha::red::{SyntaxNode, SyntaxToken};
use sipha::types::{FromSyntaxKind, IntoSyntaxKind};

use crate::syntax::Kind;
use crate::visitor::{Visitor, WalkResult};

use super::options::FormatterOptions;

/// Print the syntax tree to a string using sipha's emit API (not [`FormatDriver`]).
/// Tokens are emitted in source order with no modifications. Skips the EOF sentinel token;
/// trivia is included only when `options.preserve_comments` is true.
/// Use [`FormatDriver`] with [`walk`](crate::walk) for custom walk-based formatting.
#[must_use] 
pub fn format(root: &SyntaxNode, options: &FormatterOptions) -> String {
    let emit_opts = EmitOptions {
        include_trivia: options.preserve_comments,
        skip_kind: Some(Kind::TokEof.into_syntax_kind()),
    };
    syntax_root_to_string(root, &emit_opts)
}

/// Driver that writes the syntax tree to a string by visiting tokens.
/// You can use [`format`] directly, or build a custom flow with `walk(root, &mut driver, &opts)`.
pub struct FormatDriver<'a> {
    options: &'a FormatterOptions,
    out: String,
}

impl<'a> FormatDriver<'a> {
    #[must_use] 
    pub fn new(options: &'a FormatterOptions) -> Self {
        Self {
            options,
            out: String::new(),
        }
    }

    #[must_use] 
    pub fn into_string(self) -> String {
        self.out
    }
}

impl Visitor for FormatDriver<'_> {
    fn visit_token(&mut self, token: &SyntaxToken) -> WalkResult {
        if Kind::from_syntax_kind(token.kind()) == Some(Kind::TokEof) {
            return WalkResult::Continue(());
        }
        if token.is_trivia() && !self.options.preserve_comments {
            return WalkResult::Continue(());
        }
        self.out.push_str(token.text());
        WalkResult::Continue(())
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::parse;

    use super::format;
    use crate::formatter::FormatterOptions;

    #[test]
    fn format_round_trip_parse() {
        let source = "return 1 + 2;";
        let root = parse(source).unwrap().expect("parse");
        let options = FormatterOptions::default();
        let formatted = format(&root, &options);
        let root2 = parse(&formatted).unwrap().expect("re-parse after format");
        assert!(root2.kind_as::<crate::syntax::Kind>() == Some(crate::syntax::Kind::NodeRoot));
    }

    #[test]
    fn format_preserves_structure() {
        let source = "var x = 42";
        let root = parse(source).unwrap().expect("parse");
        let options = FormatterOptions {
            preserve_comments: true,
        };
        let formatted = format(&root, &options);
        assert!(!formatted.is_empty());
        assert!(formatted.contains("var"));
        assert!(formatted.contains("x"));
        assert!(formatted.contains("42"));
    }
}
