//! Inlay hints: by default only for `var x` / `global x` declarations when the inferred type is not `any`.

use std::collections::{HashMap, HashSet};

use sipha::line_index::LineIndex;
use sipha::red::{SyntaxElement, SyntaxNode};
use tower_lsp::lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position};

use crate::analysis::{primary_expr_new_constructor, var_decl_info, VarDeclKind};
use crate::syntax::Kind;
use crate::DocumentAnalysis;
use crate::Type;

/// Options for which inlay hints to compute.
#[derive(Clone, Debug, Default)]
pub struct InlayHintOptions {
    /// Show inferred type after expressions.
    pub expression_types: bool,
    /// Show type after variable/global identifiers.
    pub variable_types: bool,
    /// Show virtual parentheses around subexpressions to indicate precedence.
    pub parenthesis: bool,
}

impl InlayHintOptions {
    /// All hints enabled (debug view).
    #[must_use]
    pub const fn all() -> Self {
        Self {
            expression_types: true,
            variable_types: true,
            parenthesis: true,
        }
    }

    /// Only type-related hints.
    #[must_use]
    pub const fn types_only() -> Self {
        Self {
            expression_types: true,
            variable_types: true,
            parenthesis: false,
        }
    }
}

/// Compute inlay hints for the given document analysis.
///
/// By default only variable hints are shown: for declarations `var x` or `global x` when the
/// inferred type is not `any`. Other hint kinds (expression types, parenthesis) are gated by options.
///
/// `range` is an optional visible range in byte offsets (start, end). When set, only hints
/// that overlap this range are returned (reduces work for large files).
#[must_use]
pub fn compute_inlay_hints(
    analysis: &DocumentAnalysis,
    options: &InlayHintOptions,
    range: Option<(u32, u32)>,
) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    let source = analysis.source.as_str();
    let line_index = &analysis.line_index;

    let in_range = |byte_pos: u32| range.map_or(true, |(lo, hi)| byte_pos >= lo && byte_pos <= hi);

    if let Some(ref root) = analysis.root {
        // Variable/global declarations: show inferred type only when it is not `any`.
        if options.variable_types {
            hints.extend(variable_type_hints(
                &analysis.type_map,
                root,
                line_index,
                source,
                &in_range,
            ));
        }
        // Optional: structured hints (member access, new) and expression/parenthesis hints.
        let mut used_ends: HashSet<u32> = HashSet::new();
        if options.expression_types {
            let (member_new_hints, ends) =
                member_and_new_hints(analysis, root, line_index, source, &in_range);
            for end in ends {
                used_ends.insert(end);
            }
            hints.extend(member_new_hints);
        }
        if options.parenthesis {
            let (paren_hints, paren_used_ends) =
                parenthesis_hints(analysis, root, line_index, source, &in_range);
            for end in paren_used_ends {
                used_ends.insert(end);
            }
            hints.extend(paren_hints);
        }
        if options.expression_types {
            hints.extend(expression_type_hints(
                &analysis.type_map,
                line_index,
                source,
                &in_range,
                &used_ends,
            ));
        }
    }

    hints
}

/// Inlay hints for `var x` and `global x` declarations: show inferred type after the name only when it is not `any`.
fn variable_type_hints<F>(
    type_map: &HashMap<(u32, u32), Type>,
    root: &SyntaxNode,
    line_index: &LineIndex,
    source: &str,
    in_range: &F,
) -> Vec<InlayHint>
where
    F: Fn(u32) -> bool,
{
    let mut hints = Vec::new();
    for node in root.descendant_nodes() {
        if node.kind_as::<Kind>() != Some(Kind::NodeVarDecl) {
            continue;
        }
        let info = match var_decl_info(&node) {
            Some(i) => i,
            None => continue,
        };
        // Only var and global declarations (untyped by syntax); skip const, let, and typed decls.
        if info.kind != VarDeclKind::Var && info.kind != VarDeclKind::Global {
            continue;
        }
        let r = info.name_span;
        if !in_range(r.start) && !in_range(r.end) {
            continue;
        }
        let ty = match type_map.get(&(r.start, r.end)) {
            Some(t) => t,
            None => continue,
        };
        if matches!(ty, Type::Error | Type::Warning | Type::Any) {
            continue;
        }
        let label = ty.for_annotation();
        if label.is_empty() {
            continue;
        }
        let (line, col) = line_index.line_col_utf16(source, r.end);
        hints.push(InlayHint {
            position: Position {
                line,
                character: col,
            },
            label: InlayHintLabel::String(format!(": {label}")),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: Some(true),
            padding_right: Some(false),
            data: None,
        });
    }
    hints
}

/// Structured hints for member access `(receiver : Type).member : Type` and
/// `new (Class : Class<C>)(args) : Result`. Returns hints and the set of end offsets
/// used so we don't add duplicate generic type hints there.
fn member_and_new_hints<F>(
    analysis: &DocumentAnalysis,
    root: &SyntaxNode,
    line_index: &LineIndex,
    source: &str,
    in_range: &F,
) -> (Vec<InlayHint>, HashSet<u32>)
where
    F: Fn(u32) -> bool,
{
    let mut hints = Vec::new();
    let mut used_ends = HashSet::new();

    for node in root.descendant_nodes() {
        if node.kind_as::<Kind>() == Some(Kind::NodeMemberExpr) {
            if let Some(h) = member_expr_hints(analysis, &node, line_index, source, in_range) {
                used_ends.insert(h.receiver_end);
                used_ends.insert(h.member_end);
                if let Some(open) = h.open {
                    hints.push(open);
                }
                hints.push(h.receiver_type);
                hints.push(h.member_type);
            }
        }
        if node.kind_as::<Kind>() == Some(Kind::NodePrimaryExpr) {
            if let Some(h) = new_expr_hints(analysis, &node, line_index, source, in_range) {
                used_ends.insert(h.class_end);
                used_ends.insert(h.call_end);
                if let Some(open) = h.open {
                    hints.push(open);
                }
                hints.push(h.class_type);
                hints.extend(h.arg_hints);
                hints.push(h.result_type);
            }
        }
        if node.kind_as::<Kind>() == Some(Kind::NodeCallExpr) {
            if let Some(h) = call_expr_hint(analysis, root, &node, line_index, source, in_range) {
                used_ends.insert(h.call_end);
                hints.push(h.result_type);
            }
        }
    }

    (hints, used_ends)
}

/// True if the node is a single token (e.g. one identifier or keyword).
fn is_single_token_node(node: &SyntaxNode) -> bool {
    let Some(ft) = node.first_token() else {
        return false;
    };
    let Some(lt) = node.last_token() else {
        return false;
    };
    ft.text_range() == lt.text_range()
}

struct MemberHints {
    open: Option<InlayHint>,
    receiver_type: InlayHint,
    member_type: InlayHint,
    receiver_end: u32,
    member_end: u32,
}

fn member_expr_hints<F>(
    analysis: &DocumentAnalysis,
    node: &SyntaxNode,
    line_index: &LineIndex,
    source: &str,
    in_range: &F,
) -> Option<MemberHints>
where
    F: Fn(u32) -> bool,
{
    let receiver = node.first_child_node()?;
    let receiver_span = receiver.text_range();
    if !in_range(receiver_span.start) && !in_range(receiver_span.end) {
        return None;
    }
    let receiver_ty = analysis.type_at_offset(receiver_span.start)?;
    if matches!(receiver_ty, Type::Error | Type::Warning) {
        return None;
    }
    let mut after_dot = false;
    let mut member_end = 0u32;
    for elem in node.children() {
        match &elem {
            SyntaxElement::Token(t) if !t.is_trivia() => {
                if t.text() == "." {
                    after_dot = true;
                } else if after_dot {
                    let r = t.text_range();
                    member_end = r.end;
                    break;
                }
            }
            _ => {}
        }
    }
    if member_end == 0 || !in_range(member_end) {
        return None;
    }
    let node_span = node.text_range();
    let whole_ty = analysis.type_map.get(&(node_span.start, node_span.end))?;
    if matches!(whole_ty, Type::Error | Type::Warning) {
        return None;
    }
    let (line_r_start, col_r_start) = line_index.line_col_utf16(source, receiver_span.start);
    let (line_r_end, col_r_end) = line_index.line_col_utf16(source, receiver_span.end);
    let (line_m_end, col_m_end) = line_index.line_col_utf16(source, member_end);
    let receiver_label = receiver_ty.for_annotation();
    let whole_label = whole_ty.for_annotation();
    if receiver_label.is_empty() || whole_label.is_empty() {
        return None;
    }
    let single = is_single_token_node(&receiver);
    Some(MemberHints {
        open: if single {
            None
        } else {
            Some(InlayHint {
                position: Position {
                    line: line_r_start,
                    character: col_r_start,
                },
                label: InlayHintLabel::String("(".to_string()),
                kind: None,
                text_edits: None,
                tooltip: None,
                padding_left: Some(false),
                padding_right: Some(false),
                data: None,
            })
        },
        receiver_type: InlayHint {
            position: Position {
                line: line_r_end,
                character: col_r_end,
            },
            label: InlayHintLabel::String(if single {
                format!(" : {receiver_label}")
            } else {
                format!(" : {receiver_label})")
            }),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: Some(true),
            padding_right: Some(false),
            data: None,
        },
        member_type: InlayHint {
            position: Position {
                line: line_m_end,
                character: col_m_end,
            },
            label: InlayHintLabel::String(format!(" : {whole_label}")),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: Some(true),
            padding_right: Some(true),
            data: None,
        },
        receiver_end: receiver_span.end,
        member_end,
    })
}

struct NewExprHints {
    open: Option<InlayHint>,
    class_type: InlayHint,
    arg_hints: Vec<InlayHint>,
    result_type: InlayHint,
    class_end: u32,
    call_end: u32,
}

fn new_expr_hints<F>(
    analysis: &DocumentAnalysis,
    node: &SyntaxNode,
    line_index: &LineIndex,
    source: &str,
    in_range: &F,
) -> Option<NewExprHints>
where
    F: Fn(u32) -> bool,
{
    let (_class_name, _arg_count) = primary_expr_new_constructor(node)?;
    let elements: Vec<SyntaxElement> = node
        .children()
        .filter(|e| match e {
            SyntaxElement::Token(t) => !t.is_trivia(),
            SyntaxElement::Node(_) => true,
        })
        .collect();
    let class_token = match elements.get(1) {
        Some(SyntaxElement::Token(t)) if t.kind_as::<Kind>() == Some(Kind::TokIdent) => t,
        _ => return None,
    };
    let class_span = class_token.text_range();
    if !in_range(class_span.start) && !in_range(class_span.end) {
        return None;
    }
    let class_ty = analysis.type_at_offset(class_span.start)?;
    let class_ty = match &class_ty {
        Type::Class(Some(_)) => class_ty,
        _ => return None,
    };
    let result_ty = analysis
        .type_map
        .get(&(node.text_range().start, node.text_range().end))?;
    if matches!(result_ty, Type::Error | Type::Warning) {
        return None;
    }
    let call_end = node.text_range().end;
    let (line_c_end, col_c_end) = line_index.line_col_utf16(source, class_span.end);
    let (line_call_end, col_call_end) = line_index.line_col_utf16(source, call_end);
    let class_label = class_ty.for_annotation();
    let result_label = result_ty.for_annotation();
    if class_label.is_empty() || result_label.is_empty() {
        return None;
    }
    let mut arg_hints = Vec::new();
    for arg_node in node
        .child_nodes()
        .filter(|n| n.kind_as::<Kind>() == Some(Kind::NodeExpr))
    {
        let arg_span = arg_node.text_range();
        if let Some(ty) = analysis.type_at_offset(arg_span.start) {
            if matches!(ty, Type::Error | Type::Warning) {
                continue;
            }
            let (line_a, col_a) = line_index.line_col_utf16(source, arg_span.end);
            if in_range(arg_span.end) {
                let label = ty.for_annotation();
                if !label.is_empty() {
                    arg_hints.push(InlayHint {
                        position: Position {
                            line: line_a,
                            character: col_a,
                        },
                        label: InlayHintLabel::String(format!(" : {label}")),
                        kind: Some(InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip: None,
                        padding_left: Some(true),
                        padding_right: Some(true),
                        data: None,
                    });
                }
            }
        }
    }
    // Class name is always a single token — no parens around it.
    Some(NewExprHints {
        open: None,
        class_type: InlayHint {
            position: Position {
                line: line_c_end,
                character: col_c_end,
            },
            label: InlayHintLabel::String(format!(" : {class_label}")),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: Some(true),
            padding_right: Some(true),
            data: None,
        },
        arg_hints,
        result_type: InlayHint {
            position: Position {
                line: line_call_end,
                character: col_call_end,
            },
            label: InlayHintLabel::String(format!(" : {result_label}")),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: Some(true),
            padding_right: Some(true),
            data: None,
        },
        class_end: class_span.end,
        call_end,
    })
}

/// Hint for a plain function call (not `new`): show the call's inferred return type after the closing paren.
struct CallExprHint {
    call_end: u32,
    result_type: InlayHint,
}

fn call_expr_hint<F>(
    analysis: &DocumentAnalysis,
    root: &SyntaxNode,
    node: &SyntaxNode,
    line_index: &LineIndex,
    source: &str,
    in_range: &F,
) -> Option<CallExprHint>
where
    F: Fn(u32) -> bool,
{
    // Skip when this call is part of `new Foo(...)` — that is handled by new_expr_hints.
    if let Some(parent) = node.ancestors(root).into_iter().next() {
        if parent.kind_as::<Kind>() == Some(Kind::NodePrimaryExpr)
            && primary_expr_new_constructor(&parent).is_some()
        {
            return None;
        }
    }
    let range = node.text_range();
    let result_ty = analysis.type_map.get(&(range.start, range.end))?;
    if matches!(result_ty, Type::Error | Type::Warning) {
        return None;
    }
    let label = result_ty.for_annotation();
    if label.is_empty() {
        return None;
    }
    let call_end = range.end;
    if !in_range(call_end) {
        return None;
    }
    let (line, col) = line_index.line_col_utf16(source, call_end);
    Some(CallExprHint {
        call_end,
        result_type: InlayHint {
            position: Position {
                line,
                character: col,
            },
            label: InlayHintLabel::String(format!(": {label}")),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: Some(false),
            padding_right: Some(true),
            data: None,
        },
    })
}

/// One type hint per "outermost" expression end; skip positions already used by member/new hints.
fn expression_type_hints<F>(
    type_map: &HashMap<(u32, u32), Type>,
    line_index: &LineIndex,
    source: &str,
    in_range: &F,
    used_ends: &HashSet<u32>,
) -> Vec<InlayHint>
where
    F: Fn(u32) -> bool,
{
    let mut by_end: HashMap<u32, (u32, &Type)> = HashMap::new();
    for ((start, end), ty) in type_map {
        if matches!(ty, Type::Error | Type::Warning) {
            continue;
        }
        if used_ends.contains(end) {
            continue;
        }
        match by_end.get(end) {
            Some((prev_start, _)) if *prev_start <= *start => continue,
            _ => {}
        }
        by_end.insert(*end, (*start, ty));
    }

    let mut hints = Vec::new();
    for (end, (_, ty)) in by_end {
        if !in_range(end) {
            continue;
        }
        let (line, col) = line_index.line_col_utf16(source, end);
        let label = ty.for_annotation();
        if label.is_empty() {
            continue;
        }
        hints.push(InlayHint {
            position: Position {
                line,
                character: col,
            },
            label: InlayHintLabel::String(format!(": {label}")),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: Some(false),
            padding_right: Some(true),
            data: None,
        });
    }
    hints
}

/// Virtual parentheses for binary expressions:
/// - When RHS is NodeBinaryLevel (nested level): wrap the RHS only — e.g. `a + (b * c)`.
/// - When RHS is NodeBinaryExpr (op + right only): wrap the whole expression so the type
///   is shown on the closing paren — e.g. `(cell == null): boolean`.
/// Returns hints and the set of end offsets where the type was shown on the paren (so expression_type_hints skips them).
fn parenthesis_hints<F>(
    analysis: &DocumentAnalysis,
    root: &SyntaxNode,
    line_index: &LineIndex,
    source: &str,
    in_range: &F,
) -> (Vec<InlayHint>, HashSet<u32>)
where
    F: Fn(u32) -> bool,
{
    let mut hints = Vec::new();
    let mut used_ends = HashSet::new();
    for node in root.descendant_nodes() {
        if node.kind_as::<Kind>() != Some(Kind::NodeBinaryLevel) {
            continue;
        }
        let child_nodes: Vec<SyntaxNode> = node.child_nodes().collect();
        if child_nodes.len() < 2 {
            continue;
        }
        let rhs = child_nodes.last().unwrap();
        let rhs_kind = rhs.kind_as::<Kind>();
        // Span to wrap: whole expression for NodeBinaryExpr (e.g. cell == null), else just RHS (e.g. b * c).
        let (wrap_start, wrap_end) = match rhs_kind {
            Some(Kind::NodeBinaryLevel) => {
                let r = rhs.text_range();
                (r.start, r.end)
            }
            Some(Kind::NodeBinaryExpr) => {
                let r = node.text_range();
                (r.start, r.end)
            }
            _ => continue,
        };
        if !in_range(wrap_start) && !in_range(wrap_end) {
            continue;
        }
        let start_byte = wrap_start as usize;
        if start_byte >= source.len() {
            continue;
        }
        let first_char = source[start_byte..].chars().next();
        if first_char == Some('(') {
            continue;
        }
        let (line_start, col_start) = line_index.line_col_utf16(source, wrap_start);
        let (line_end, col_end) = line_index.line_col_utf16(source, wrap_end);
        hints.push(InlayHint {
            position: Position {
                line: line_start,
                character: col_start,
            },
            label: InlayHintLabel::String("(".to_string()),
            kind: None,
            text_edits: None,
            tooltip: None,
            padding_left: Some(false),
            padding_right: Some(false),
            data: None,
        });
        // Show type on the closing parenthesis when we have a type for this span.
        let (close_label, has_type) = analysis
            .type_map
            .get(&(wrap_start, wrap_end))
            .and_then(|ty| {
                if matches!(ty, Type::Error | Type::Warning) {
                    None
                } else {
                    let label = ty.for_annotation();
                    if label.is_empty() {
                        None
                    } else {
                        Some((format!("): {label}"), true))
                    }
                }
            })
            .unwrap_or_else(|| (")".to_string(), false));
        if has_type {
            used_ends.insert(wrap_end);
        }
        hints.push(InlayHint {
            position: Position {
                line: line_end,
                character: col_end,
            },
            label: InlayHintLabel::String(close_label),
            kind: if has_type {
                Some(InlayHintKind::TYPE)
            } else {
                None
            },
            text_edits: None,
            tooltip: None,
            padding_left: Some(false),
            padding_right: Some(false),
            data: None,
        });
    }
    (hints, used_ends)
}
