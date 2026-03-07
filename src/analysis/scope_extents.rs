//! Scope extents: map from byte offset to ScopeId for LSP (go-to-def, references, completion).
//!
//! Walks the tree in the same order as ScopeBuilder and records (ScopeId, extent) so that
//! the LSP can find the innermost scope containing a given offset.

use sipha::red::SyntaxNode;
use sipha::walk::{Visitor, WalkOptions, WalkResult};

use crate::syntax::Kind;

use super::scope::ScopeId;

/// Build list of (ScopeId, extent) where extent is (start_byte, end_byte).
/// Root scope (ScopeId(0)) is always first with extent (0, source_len).
/// Remaining entries are in walk order, matching scope_id_sequence.
#[must_use]
pub fn build_scope_extents(
    root: &SyntaxNode,
    scope_id_sequence: &[ScopeId],
    source_len: usize,
) -> Vec<(ScopeId, (u32, u32))> {
    let mut extents = vec![(ScopeId(0), (0u32, source_len as u32))];
    let mut index = 0usize;
    let mut visitor = ScopeExtentVisitor {
        scope_id_sequence,
        extents: &mut extents,
        index: &mut index,
    };
    let options = WalkOptions::nodes_only();
    let _ = root.walk(&mut visitor, &options);
    extents
}

/// Find the innermost scope containing the given byte offset.
#[must_use]
pub fn scope_at_offset(extents: &[(ScopeId, (u32, u32))], offset: u32) -> ScopeId {
    let mut best: Option<(ScopeId, u32)> = None;
    for (scope_id, (start, end)) in extents {
        if *start <= offset && offset < *end {
            let len = end - start;
            if best.map_or(true, |(_, best_len)| len < best_len) {
                best = Some((*scope_id, len));
            }
        }
    }
    best.map(|(id, _)| id).unwrap_or(ScopeId(0))
}

struct ScopeExtentVisitor<'a> {
    scope_id_sequence: &'a [ScopeId],
    extents: &'a mut Vec<(ScopeId, (u32, u32))>,
    index: &'a mut usize,
}

impl Visitor for ScopeExtentVisitor<'_> {
    fn enter_node(&mut self, node: &SyntaxNode) -> WalkResult {
        let kind = match node.kind_as::<Kind>() {
            Some(k) => k,
            None => return WalkResult::Continue(()),
        };
        let scope_creating = matches!(
            kind,
            Kind::NodeBlock
                | Kind::NodeFunctionDecl
                | Kind::NodeClassDecl
                | Kind::NodeWhileStmt
                | Kind::NodeForStmt
                | Kind::NodeForInStmt
                | Kind::NodeDoWhileStmt
        );
        if scope_creating {
            if let Some(&scope_id) = self.scope_id_sequence.get(*self.index) {
                let range = node.text_range();
                self.extents.push((scope_id, (range.start, range.end)));
            }
            *self.index += 1;
        }
        WalkResult::Continue(())
    }
}
