//! Generic visitor over the parsed `LeekScript` syntax tree.
//!
//! Re-exports sipha's tree walk ([`Visitor`], [`WalkOptions`], [`walk`], [`WalkResult`]).
//! Implement [`Visitor`] and call [`walk`] (or `root.walk(...)`) with your visitor and options.

// Re-export so callers can use one import from leekscript_rs.
pub use sipha::walk::{Visitor, WalkOptions, WalkResult};

/// Walks the tree starting at `root` with the given visitor and options.
///
/// Returns `ControlFlow::Break(())` if the visitor requested early termination.
pub fn walk(
    root: &sipha::red::SyntaxNode,
    visitor: &mut impl sipha::walk::Visitor,
    options: &WalkOptions,
) -> WalkResult {
    root.walk(visitor, options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    struct CountNodes {
        nodes: usize,
        tokens: usize,
    }

    impl Visitor for CountNodes {
        fn enter_node(&mut self, _node: &sipha::red::SyntaxNode) -> WalkResult {
            self.nodes += 1;
            WalkResult::Continue(())
        }
        fn visit_token(&mut self, _token: &sipha::red::SyntaxToken) -> WalkResult {
            self.tokens += 1;
            WalkResult::Continue(())
        }
    }

    #[test]
    fn visitor_walks_program() {
        let source = "var x = 1; function f() { return x; }";
        let root = parse(source).unwrap().expect("parse");
        let mut counter = CountNodes {
            nodes: 0,
            tokens: 0,
        };
        let _ = walk(&root, &mut counter, &WalkOptions::default());
        assert!(counter.nodes > 0, "should visit nodes");
        assert!(counter.tokens > 0, "should visit tokens");
    }

    #[test]
    fn nodes_only_visits_no_tokens() {
        let source = "let a = 2;";
        let root = parse(source).unwrap().expect("parse");
        let mut counter = CountNodes {
            nodes: 0,
            tokens: 0,
        };
        let _ = walk(&root, &mut counter, &WalkOptions::nodes_only());
        assert!(counter.nodes > 0);
        assert_eq!(counter.tokens, 0);
    }
}
