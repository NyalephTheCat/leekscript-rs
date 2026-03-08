//! Scope extents: map from byte offset to `ScopeId` for LSP (go-to-def, references, completion).
//!
//! Delegates to [`sipha_analysis`] with a LeekScript-specific predicate for scope-creating nodes.

use sipha::red::SyntaxNode;

use leekscript_core::syntax::Kind;

use super::scope::ScopeId;

/// Build list of (`ScopeId`, extent) where extent is (`start_byte`, `end_byte`).
/// Root scope (ScopeId(0)) is always first with extent (0, `source_len`).
/// Remaining entries are in walk order, matching `scope_id_sequence`.
#[must_use]
pub fn build_scope_extents(
    root: &SyntaxNode,
    scope_id_sequence: &[ScopeId],
    source_len: usize,
) -> Vec<(ScopeId, (u32, u32))> {
    sipha_analysis::build_scope_extents(
        root,
        ScopeId(0),
        scope_id_sequence,
        source_len,
        is_scope_creating,
    )
}

fn is_scope_creating(node: &SyntaxNode) -> bool {
    matches!(
        node.kind_as::<Kind>(),
        Some(
            Kind::NodeBlock
                | Kind::NodeFunctionDecl
                | Kind::NodeClassDecl
                | Kind::NodeConstructorDecl
                | Kind::NodeWhileStmt
                | Kind::NodeForStmt
                | Kind::NodeForInStmt
                | Kind::NodeDoWhileStmt
        )
    )
}

/// Find the innermost scope containing the given byte offset.
/// When extents is empty (e.g. partial state), returns root scope ID so the LSP never panics.
#[must_use]
pub fn scope_at_offset(extents: &[(ScopeId, (u32, u32))], offset: u32) -> ScopeId {
    sipha_analysis::scope_at_offset(extents, offset, ScopeId(0))
}
