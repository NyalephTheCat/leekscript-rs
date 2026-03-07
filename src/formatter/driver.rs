//! Format driver: emits the syntax tree to a string (round-trip).
//!
//! Uses sipha's emit API for a single canonical tree-to-string path when no extras are
//! requested. With `parenthesize_expressions` or `annotate_types`, uses a custom walk.

use sipha::emit::{syntax_root_to_string, EmitOptions};
use sipha::red::{SyntaxNode, SyntaxToken};
use sipha::types::{FromSyntaxKind, IntoSyntaxKind};
use sipha::walk::WalkOptions;

use crate::analysis::{analyze, analyze_with_signatures, TypeMapKey};
use crate::syntax::Kind;
use crate::types::Type;
use crate::visitor::{walk, Visitor, WalkResult};

use super::options::FormatterOptions;

/// Compound expression node kinds that get parentheses when `parenthesize_expressions` is true.
/// We wrap nodes that contain the *entire* expression in the AST:
/// - NodeBinaryLevel: one precedence level (add, mul, compare, etc.) with [left, op, right, ...].
/// - NodeUnaryExpr, NodeExpr, NodeAsCast, NodeArray, NodeMap, NodeInterval (full expr in one node).
/// We do NOT wrap NodeBinaryExpr (only [op, right]), NodeMemberExpr, NodeCallExpr, NodeIndexExpr.
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
/// When `parenthesize_expressions` or `annotate_types` is set, runs a custom walk that
/// may add parentheses and/or type comments; otherwise uses sipha's emit API.
#[must_use]
pub fn format(root: &SyntaxNode, options: &FormatterOptions) -> String {
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
    /// Depth in the tree (incremented on enter_node, decremented on leave_node).
    depth: usize,
    /// When to emit ")" for postfix chains (a.b.c → (a.b).c): (parent_depth, children_left_to_leave).
    postfix_close_stack: Vec<(usize, usize)>,
}

fn is_postfix_chain(node: &SyntaxNode) -> bool {
    let children: Vec<_> = node.child_nodes().collect();
    if children.len() < 2 {
        return false;
    }
    let first = children[0].kind_as::<Kind>();
    let first_is_suffix = matches!(first, Some(Kind::NodeMemberExpr) | Some(Kind::NodeCallExpr) | Some(Kind::NodeIndexExpr));
    if first_is_suffix {
        return false;
    }
    children[1..].iter().all(|c| {
        matches!(c.kind_as::<Kind>(), Some(Kind::NodeMemberExpr) | Some(Kind::NodeCallExpr) | Some(Kind::NodeIndexExpr))
    })
}

impl Visitor for FormatDriverWithExtras<'_> {
    fn enter_node(&mut self, node: &SyntaxNode) -> WalkResult {
        self.depth += 1;
        if self.options.parenthesize_expressions {
            if let Some(kind) = node.kind_as::<Kind>() {
                if kind == Kind::NodeBinaryLevel {
                    let has_binary = node
                        .child_nodes()
                        .any(|c| matches!(c.kind_as::<Kind>(), Some(Kind::NodeBinaryExpr) | Some(Kind::NodeInterval)));
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
            let kind_ok = node.kind_as::<Kind>().map_or(false, |k| {
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
            parenthesize_expressions: false,
            annotate_types: false,
            signature_roots: None,
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
        };
        let formatted = format(&root, &options);
        // Should add type comments for expressions (e.g. integer for literals and result)
        assert!(formatted.contains("/* integer */"));
    }
}
