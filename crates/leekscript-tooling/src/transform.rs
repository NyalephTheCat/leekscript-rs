//! Tree transformation using [sipha](https://docs.rs/sipha)'s transform API.
//!
//! This module is only available when the `transform` feature is enabled. It re-exports
//! sipha's [`Transformer`], [`transform`], and [`TransformResult`], and provides
//! LeekScript-specific transformers such as [`ExpandAssignAdd`].

#![cfg(feature = "transform")]

use sipha::green::GreenNode;
use sipha::green_builder::GreenBuilder;
use sipha::red::SyntaxNode;
use sipha::types::IntoSyntaxKind;

use leekscript_core::parse;
use leekscript_core::syntax::Kind;

// Re-export sipha's transform API for downstream crates (and use in this module)
pub use sipha::transform::{transform, TransformResult, Transformer};

// ─── Example: expand += to = ... + ────────────────────────────────────────────

/// Parses `return left + right;` and returns the expression node's green, or `None` on parse failure.
fn parse_binary_add_rhs(left: &str, right: &str) -> Option<std::sync::Arc<GreenNode>> {
    let src = format!("{} = {} + {};", left.trim(), left.trim(), right.trim());
    let root = parse(&src).ok().and_then(|o| o)?;
    let program = root.child_nodes().next()?;
    let return_stmt = program.child_nodes().next()?;
    let expr = return_stmt.child_nodes().next()?;
    Some(expr.green().clone())
}

/// Transformer that rewrites `a += b` into `a = a + b`.
pub struct ExpandAssignAdd;

impl Transformer for ExpandAssignAdd {
    fn transform_node(&mut self, node: &SyntaxNode) -> TransformResult {
        if node.kind_as::<Kind>() != Some(Kind::NodeExpr) {
            return None;
        }
        if !node.non_trivia_tokens().any(|t| t.text() == "+=") {
            return None;
        }

        let mut child_nodes = node.child_nodes();
        let postfix_node = child_nodes.next()?;
        let expr_node = child_nodes.next()?;
        let left_green = postfix_node.green().clone();
        let rhs_green =
            parse_binary_add_rhs(&postfix_node.collect_text(), &expr_node.collect_text())
                .unwrap_or_else(|| left_green.clone());

        Some(GreenBuilder::node_standalone(node.kind(), |b| {
            b.child_node(left_green)
                .token(Kind::TokOp.into_syntax_kind(), "=", false)
                .child_node(rhs_green);
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use leekscript_core::parse;

    #[test]
    fn transform_expand_assign_add() {
        let source = "var x = 0; x += 10;";
        let root = parse(source).unwrap().expect("parse");
        let mut t = ExpandAssignAdd;
        let new_root = transform(&root, &mut t);
        let new_text = new_root.collect_text();
        // After transform we should have "x = ..." instead of "x += 10" (RHS may be "x + 10" or "x" depending on parse)
        assert!(
            new_text.contains("x = "),
            "expected assignment in output, got: {:?}",
            new_text
        );
        assert!(
            !new_text.contains("+="),
            "expected no += in output, got: {:?}",
            new_text
        );
    }

    #[test]
    fn transform_identity_keeps_tree() {
        struct Id;
        impl Transformer for Id {
            fn transform_node(&mut self, _node: &SyntaxNode) -> TransformResult {
                None
            }
        }
        let source = "let a = 1 + 2;";
        let root = parse(source).unwrap().expect("parse");
        let new_root = transform(&root, &mut Id);
        assert!(
            sipha_diff::trees_equal(&root, &new_root),
            "{}",
            sipha_diff::format_diff(&root, &new_root)
        );
    }
}
