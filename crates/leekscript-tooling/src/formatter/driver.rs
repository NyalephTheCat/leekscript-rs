//! Format driver: emits the syntax tree to a string (round-trip or canonical).
//!
//! Uses sipha's emit API for round-trip when no extras are requested. With
//! `canonical_format`, `parenthesize_expressions`, or `annotate_types`, uses a custom walk.

use sipha::emit::{syntax_root_to_string, EmitOptions};
use sipha::red::{SyntaxNode, SyntaxToken};
use sipha::types::{FromSyntaxKind, IntoSyntaxKind};
use sipha::walk::WalkOptions;

use crate::visitor::{walk, Visitor, WalkResult};
use leekscript_analysis::{analyze, analyze_with_signatures, TypeMapKey};
use leekscript_core::syntax::Kind;
use leekscript_core::Type;

use super::options::{BraceStyle, FormatterOptions, IndentStyle, SemicolonStyle};

/// Compound expression node kinds that get parentheses when `parenthesize_expressions` is true.
/// We wrap nodes that contain the *entire* expression in the AST:
/// - `NodeBinaryLevel`: one precedence level (add, mul, compare, etc.) with [left, op, right, ...].
/// - `NodeUnaryExpr`, `NodeExpr`, `NodeAsCast`, `NodeArray`, `NodeMap`, `NodeInterval` (full expr in one node).
/// We do NOT wrap `NodeBinaryExpr` (only [op, right]), `NodeMemberExpr`, `NodeCallExpr`, `NodeIndexExpr`.
fn is_expression_node(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::NodeBinaryLevel // full "left op right" for one precedence level
            | Kind::NodeUnaryExpr
            | Kind::NodeAsCast
            | Kind::NodeArray
            | Kind::NodeMap
            | Kind::NodeInterval
    )
}

/// Print the syntax tree to a string.
/// When `canonical_format`, `parenthesize_expressions`, or `annotate_types` is set, runs a custom walk;
/// otherwise uses sipha's emit API for round-trip.
#[must_use]
pub fn format(root: &SyntaxNode, options: &FormatterOptions) -> String {
    if options.canonical_format {
        return format_canonical(root, options);
    }
    if options.parenthesize_expressions || options.annotate_types {
        let type_map = if options.annotate_types {
            let result = if let Some(ref roots) = options.signature_roots {
                analyze_with_signatures(root, roots)
            } else {
                analyze(root)
            };
            result.type_map
        } else {
            std::collections::HashMap::<TypeMapKey, Type>::new()
        };
        format_with_extras(root, options, &type_map)
    } else {
        let emit_opts = EmitOptions {
            include_trivia: options.preserve_comments,
            skip_kind: Some(Kind::TokEof.into_syntax_kind()),
        };
        syntax_root_to_string(root, &emit_opts)
    }
}

/// Canonical format: normalize indentation, braces, semicolons. Trivia (comments/whitespace) is not emitted.
fn format_canonical(root: &SyntaxNode, options: &FormatterOptions) -> String {
    let mut driver = CanonicalFormatDriver {
        options,
        out: String::new(),
        indent_depth: 0,
        need_newline: false,
        statement_semicolon_stack: Vec::new(),
        last_token_ends_word: false,
    };
    let walk_opts = WalkOptions::full();
    let _ = walk(root, &mut driver, &walk_opts);
    driver.out
}

/// True if the token text looks like a word (ident/number) that could run into the next token.
fn token_ends_word(text: &str) -> bool {
    let c = text.chars().last().unwrap_or(' ');
    c.is_alphanumeric() || c == '_'
}

/// True if the token text starts like a word (ident/number).
fn token_starts_word(text: &str) -> bool {
    let c = text.chars().next().unwrap_or(' ');
    c.is_alphanumeric() || c == '_' || c == '"' || c == '\''
}

/// Nodes after which we may need to emit a semicolon (optional in grammar).
fn is_statement_with_optional_semicolon(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::NodeVarDecl
            | Kind::NodeExprStmt
            | Kind::NodeReturnStmt
            | Kind::NodeBreakStmt
            | Kind::NodeContinueStmt
    )
}

/// Canonical format driver: emits tokens with normalized newlines and indentation.
struct CanonicalFormatDriver<'a> {
    options: &'a FormatterOptions,
    out: String,
    indent_depth: usize,
    need_newline: bool,
    /// Stack of "did we see semicolon for this statement" for optional-semicolon statements.
    statement_semicolon_stack: Vec<bool>,
    /// Last non-trivia token text, to decide if we need a space before the next token.
    last_token_ends_word: bool,
}

impl CanonicalFormatDriver<'_> {
    fn emit_indent(&mut self) {
        match self.options.indent_style {
            IndentStyle::Tabs => {
                for _ in 0..self.indent_depth {
                    self.out.push('\t');
                }
            }
            IndentStyle::Spaces(n) => {
                let spaces = n.max(1).min(8) as usize;
                for _ in 0..self.indent_depth {
                    for _ in 0..spaces {
                        self.out.push(' ');
                    }
                }
            }
        }
    }

    fn maybe_emit_newline_and_indent(&mut self) {
        if self.need_newline {
            self.out.push('\n');
            self.emit_indent();
            self.need_newline = false;
        }
    }
}

impl Visitor for CanonicalFormatDriver<'_> {
    fn enter_node(&mut self, node: &SyntaxNode) -> WalkResult {
        if let Some(kind) = node.kind_as::<Kind>() {
            if is_statement_with_optional_semicolon(kind) {
                self.statement_semicolon_stack.push(false);
            }
        }
        WalkResult::Continue(())
    }

    fn visit_token(&mut self, token: &SyntaxToken) -> WalkResult {
        let tok_kind = Kind::from_syntax_kind(token.kind());
        if tok_kind == Some(Kind::TokEof) {
            return WalkResult::Continue(());
        }
        if token.is_trivia() {
            return WalkResult::Continue(());
        }
        self.maybe_emit_newline_and_indent();

        let text = token.text();
        // Emit space between tokens that would otherwise run together (e.g. "var" "x" -> "var x").
        if self.last_token_ends_word && token_starts_word(text) {
            self.out.push(' ');
        }

        if tok_kind == Some(Kind::TokBraceR) {
            self.out.push('\n');
            if self.indent_depth > 0 {
                self.indent_depth -= 1;
            }
            self.emit_indent();
            self.out.push_str(text);
            self.last_token_ends_word = false;
            self.need_newline = true;
            return WalkResult::Continue(());
        }
        if tok_kind == Some(Kind::TokBraceL) {
            if self.options.brace_style == BraceStyle::NextLine {
                self.out.push('\n');
                self.emit_indent();
            }
            self.out.push_str(text);
            self.last_token_ends_word = false;
            self.indent_depth += 1;
            self.need_newline = true;
            return WalkResult::Continue(());
        }
        if tok_kind == Some(Kind::TokSemi) {
            if let Some(seen) = self.statement_semicolon_stack.last_mut() {
                *seen = true;
            }
            if self.options.semicolon_style == SemicolonStyle::Always {
                self.out.push_str(text);
            }
            self.last_token_ends_word = false;
            self.need_newline = true;
            return WalkResult::Continue(());
        }

        self.out.push_str(text);
        self.last_token_ends_word = token_ends_word(text);
        if text == "}" {
            self.need_newline = true;
        }
        WalkResult::Continue(())
    }

    fn leave_node(&mut self, node: &SyntaxNode) -> WalkResult {
        if let Some(kind) = node.kind_as::<Kind>() {
            if is_statement_with_optional_semicolon(kind) {
                if let Some(had_semi) = self.statement_semicolon_stack.pop() {
                    if self.options.semicolon_style == SemicolonStyle::Always && !had_semi {
                        self.maybe_emit_newline_and_indent();
                        self.out.push(';');
                        self.need_newline = true;
                    }
                }
            }
        }
        WalkResult::Continue(())
    }
}

/// Custom format pass: walk tree, emit tokens, optionally add parens and type comments.
fn format_with_extras(
    root: &SyntaxNode,
    options: &FormatterOptions,
    type_map: &std::collections::HashMap<TypeMapKey, Type>,
) -> String {
    let mut driver = FormatDriverWithExtras {
        options,
        type_map,
        out: String::new(),
        paren_stack: Vec::new(),
        depth: 0,
        postfix_close_stack: Vec::new(),
    };
    let walk_opts = WalkOptions::full();
    let _ = walk(root, &mut driver, &walk_opts);
    driver.out
}

/// Driver that emits tokens and optionally parentheses and type comments.
struct FormatDriverWithExtras<'a> {
    options: &'a FormatterOptions,
    type_map: &'a std::collections::HashMap<TypeMapKey, Type>,
    out: String,
    /// Stack of "did we emit open paren for this node" for matching close parens.
    paren_stack: Vec<bool>,
    /// Depth in the tree (incremented on `enter_node`, decremented on `leave_node`).
    depth: usize,
    /// When to emit ")" for postfix chains (a.b.c → (a.b).c): (`parent_depth`, `children_left_to_leave`).
    postfix_close_stack: Vec<(usize, usize)>,
}

fn is_postfix_chain(node: &SyntaxNode) -> bool {
    let children: Vec<_> = node.child_nodes().collect();
    if children.len() < 2 {
        return false;
    }
    let first = children[0].kind_as::<Kind>();
    let first_is_suffix = matches!(
        first,
        Some(Kind::NodeMemberExpr | Kind::NodeCallExpr | Kind::NodeIndexExpr)
    );
    if first_is_suffix {
        return false;
    }
    children[1..].iter().all(|c| {
        matches!(
            c.kind_as::<Kind>(),
            Some(Kind::NodeMemberExpr | Kind::NodeCallExpr | Kind::NodeIndexExpr)
        )
    })
}

impl Visitor for FormatDriverWithExtras<'_> {
    fn enter_node(&mut self, node: &SyntaxNode) -> WalkResult {
        self.depth += 1;
        if self.options.parenthesize_expressions {
            if let Some(kind) = node.kind_as::<Kind>() {
                if kind == Kind::NodeBinaryLevel {
                    let has_binary = node.child_nodes().any(|c| {
                        matches!(
                            c.kind_as::<Kind>(),
                            Some(Kind::NodeBinaryExpr | Kind::NodeInterval)
                        )
                    });
                    if has_binary {
                        self.out.push('(');
                        self.paren_stack.push(true);
                    } else if is_postfix_chain(node) {
                        // a.b.c → (a.b).c: wrap first segment, emit ")" after first suffix.
                        self.out.push('(');
                        self.paren_stack.push(false); // we close via postfix_close_stack
                        self.postfix_close_stack.push((self.depth, 2));
                    } else {
                        self.paren_stack.push(false);
                    }
                    return WalkResult::Continue(());
                }
                if is_expression_node(kind) {
                    self.out.push('(');
                    self.paren_stack.push(true);
                    return WalkResult::Continue(());
                }
            }
        }
        self.paren_stack.push(false);
        WalkResult::Continue(())
    }

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

    fn leave_node(&mut self, node: &SyntaxNode) -> WalkResult {
        // Postfix chain: emit ")" after we've left the first suffix (e.g. after .b in a.b.c).
        if let Some(&(parent_depth, _)) = self.postfix_close_stack.last() {
            if parent_depth == self.depth - 1 {
                let (_, k) = self.postfix_close_stack.pop().unwrap();
                if k == 1 {
                    self.out.push(')');
                } else {
                    self.postfix_close_stack.push((parent_depth, k - 1));
                }
            }
        }
        self.depth -= 1;
        let did_paren = self.paren_stack.pop().unwrap_or(false);
        if did_paren {
            self.out.push(')');
        }
        // Only emit type comments for node kinds we actually record in the type checker (avoids duplicates from wrapper nodes that share a child's span).
        if self.options.annotate_types {
            let kind_ok = node.kind_as::<Kind>().is_some_and(|k| {
                matches!(
                    k,
                    Kind::NodePrimaryExpr
                        | Kind::NodeBinaryExpr
                        | Kind::NodeUnaryExpr
                        | Kind::NodeCallExpr
                        | Kind::NodeMemberExpr
                        | Kind::NodeIndexExpr
                        | Kind::NodeVarDecl
                        | Kind::NodeAsCast
                )
            });
            if kind_ok {
                let span = node.text_range();
                let key = (span.start, span.end);
                if let Some(ty) = self.type_map.get(&key) {
                    self.out.push_str(" /* ");
                    self.out.push_str(&ty.for_annotation());
                    self.out.push_str(" */");
                }
            }
        }
        WalkResult::Continue(())
    }
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
    use leekscript_core::parse;

    use super::format;
    use crate::formatter::FormatterOptions;

    #[test]
    fn format_round_trip_parse() {
        let source = "return 1 + 2;";
        let root = parse(source).unwrap().expect("parse");
        let options = FormatterOptions::default();
        let formatted = format(&root, &options);
        let root2 = parse(&formatted).unwrap().expect("re-parse after format");
        assert!(
            root2.kind_as::<leekscript_core::syntax::Kind>()
                == Some(leekscript_core::syntax::Kind::NodeRoot)
        );
    }

    #[test]
    fn format_preserves_structure() {
        let source = "var x = 42";
        let root = parse(source).unwrap().expect("parse");
        let options = FormatterOptions {
            preserve_comments: true,
            parenthesize_expressions: false,
            annotate_types: false,
            signature_roots: None,
            ..FormatterOptions::default()
        };
        let formatted = format(&root, &options);
        assert!(!formatted.is_empty());
        assert!(formatted.contains("var"));
        assert!(formatted.contains("x"));
        assert!(formatted.contains("42"));
    }

    #[test]
    fn format_parenthesize_expressions() {
        let source = "return a + b + c * d;";
        let root = parse(source).unwrap().expect("parse");
        let options = FormatterOptions {
            preserve_comments: true,
            parenthesize_expressions: true,
            annotate_types: false,
            signature_roots: None,
            ..FormatterOptions::default()
        };
        let formatted = format(&root, &options);
        // Should add parentheses around compound expressions (exact shape depends on grammar associativity)
        assert!(
            formatted.contains('(') && formatted.contains(')'),
            "expected parentheses in output: {:?}",
            formatted
        );
    }

    #[test]
    fn format_annotate_types() {
        let source = "var x = 1 + 2";
        let root = parse(source).unwrap().expect("parse");
        let options = FormatterOptions {
            preserve_comments: true,
            parenthesize_expressions: false,
            annotate_types: true,
            signature_roots: None,
            ..FormatterOptions::default()
        };
        let formatted = format(&root, &options);
        // Should add type comments for expressions (e.g. integer for literals and result)
        assert!(formatted.contains("/* integer */"));
    }

    #[test]
    fn format_canonical_indent() {
        let source = "var x=1;function f(){return x;}";
        let root = parse(source).unwrap().expect("parse");
        let options = FormatterOptions {
            canonical_format: true,
            indent_style: super::IndentStyle::Tabs,
            semicolon_style: super::SemicolonStyle::Always,
            ..FormatterOptions::default()
        };
        let formatted = format(&root, &options);
        assert!(formatted.contains("var"));
        assert!(formatted.contains("x"));
        assert!(formatted.contains("1"));
        assert!(formatted.contains(';'));
        assert!(
            formatted.contains('\t'),
            "canonical format should use tabs: {:?}",
            formatted
        );
        assert!(formatted.contains("return"));
    }
}
