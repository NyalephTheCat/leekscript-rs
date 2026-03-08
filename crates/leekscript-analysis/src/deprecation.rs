//! Deprecation pass: emits deprecation diagnostics for deprecated syntax (e.g. === and !==).

use sipha::error::SemanticDiagnostic;
use sipha::red::{SyntaxElement, SyntaxNode};
use sipha::walk::{Visitor, WalkResult};

use leekscript_core::syntax::Kind;

/// Collects deprecation diagnostics by walking the syntax tree.
pub struct DeprecationChecker {
    pub diagnostics: Vec<SemanticDiagnostic>,
}

impl DeprecationChecker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }
}

impl Default for DeprecationChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl Visitor for DeprecationChecker {
    fn enter_node(&mut self, node: &SyntaxNode) -> WalkResult {
        if node.kind_as::<Kind>() != Some(Kind::NodeBinaryExpr) {
            return WalkResult::Continue(());
        }
        for child in node.children() {
            let SyntaxElement::Token(t) = child else {
                continue;
            };
            if t.kind_as::<Kind>() != Some(Kind::TokOp) || t.is_trivia() {
                continue;
            }
            let text = t.text();
            if text == "===" {
                self.diagnostics.push(
                    SemanticDiagnostic::deprecation(
                        t.text_range(),
                        "strict equality operator `===` is deprecated; use `==` instead",
                    )
                    .with_code("deprecated_strict_eq"),
                );
            } else if text == "!==" {
                self.diagnostics.push(
                    SemanticDiagnostic::deprecation(
                        t.text_range(),
                        "strict inequality operator `!==` is deprecated; use `!=` instead",
                    )
                    .with_code("deprecated_strict_neq"),
                );
            }
        }
        WalkResult::Continue(())
    }
}
