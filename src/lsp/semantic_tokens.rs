//! Semantic token types and computation for LSP-based syntax highlighting.
//!
//! Maps syntax tokens from the parse tree to LSP semantic token types (keyword, string,
//! number, operator, variable, etc.) so the editor can colorize via the LSP.

use std::collections::HashSet;

use sipha::line_index::LineIndex;
use sipha::red::{SyntaxElement, SyntaxNode, SyntaxToken};
use sipha::walk::{Visitor, WalkOptions, WalkResult};
use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
    SemanticTokensServerCapabilities,
};

use crate::analysis::{
    class_decl_info, function_decl_info, param_name, var_decl_info, VarDeclKind,
};
use crate::syntax::Kind;

/// Doxygen tag names we highlight (after @ or \).
const DOXYGEN_TAGS: &[&str] = &[
    "param",
    "return",
    "returns",
    "brief",
    "deprecated",
    "see",
    "since",
    "throws",
    "throw",
    "author",
    "version",
    "note",
    "warning",
    "todo",
    "code",
    "endcode",
    "pre",
    "post",
    "invariant",
    "class",
];

/// Segment of a comment: either a Doxygen tag (keyword) or plain comment text.
#[derive(Clone, Copy)]
struct CommentSegment {
    start: usize,
    end: usize,
    is_tag: bool,
}

fn doxygen_segments(text: &str) -> Vec<CommentSegment> {
    let mut raw = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let (is_tag, len) = if rest.starts_with('@') || rest.starts_with('\\') {
            let prefix_len = 1;
            let after = &rest[prefix_len..];
            let tag_match = DOXYGEN_TAGS.iter().find(|tag| {
                after.starts_with(*tag)
                    && (after.len() == tag.len()
                        || !after[tag.len()..]
                            .starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_'))
            });
            if let Some(tag) = tag_match {
                (true, prefix_len + tag.len())
            } else {
                let char_len = rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                (false, char_len)
            }
        } else {
            let char_len = rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            (false, char_len)
        };
        raw.push(CommentSegment {
            start: i,
            end: i + len,
            is_tag,
        });
        i += len;
    }
    let mut merged: Vec<CommentSegment> = Vec::new();
    for seg in raw {
        if let Some(last) = merged.last_mut() {
            if last.is_tag == seg.is_tag && last.end == seg.start {
                last.end = seg.end;
                continue;
            }
        }
        merged.push(seg);
    }
    merged
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenTypeKind {
    Keyword = 0,
    Modifier,
    String,
    Number,
    Operator,
    Comment,
    Type,
    Class,
    Function,
    Method,
    Parameter,
    Variable,
    Property,
}

impl TokenTypeKind {
    fn index(self) -> u32 {
        self as u32
    }

    fn to_semantic_token_type(self) -> SemanticTokenType {
        match self {
            Self::Keyword => SemanticTokenType::KEYWORD,
            Self::Modifier => SemanticTokenType::MODIFIER,
            Self::String => SemanticTokenType::STRING,
            Self::Number => SemanticTokenType::NUMBER,
            Self::Operator => SemanticTokenType::OPERATOR,
            Self::Comment => SemanticTokenType::COMMENT,
            Self::Type => SemanticTokenType::TYPE,
            Self::Class => SemanticTokenType::CLASS,
            Self::Function => SemanticTokenType::FUNCTION,
            Self::Method => SemanticTokenType::METHOD,
            Self::Parameter => SemanticTokenType::PARAMETER,
            Self::Variable => SemanticTokenType::VARIABLE,
            Self::Property => SemanticTokenType::PROPERTY,
        }
    }

    fn all() -> [Self; 13] {
        [
            Self::Keyword,
            Self::Modifier,
            Self::String,
            Self::Number,
            Self::Operator,
            Self::Comment,
            Self::Type,
            Self::Class,
            Self::Function,
            Self::Method,
            Self::Parameter,
            Self::Variable,
            Self::Property,
        ]
    }
}

/// Legend of token types and modifiers. The client uses this to map indices to theme colors.
#[must_use]
pub fn semantic_tokens_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TokenTypeKind::all()
            .iter()
            .map(|k| k.to_semantic_token_type())
            .collect(),
        token_modifiers: vec![
            SemanticTokenModifier::DECLARATION,
            SemanticTokenModifier::DEFINITION,
            SemanticTokenModifier::READONLY,
            SemanticTokenModifier::STATIC,
        ],
    }
}

/// Server capability for semantic tokens (full document and range).
#[must_use]
pub fn semantic_tokens_provider() -> SemanticTokensServerCapabilities {
    SemanticTokensOptions {
        work_done_progress_options: Default::default(),
        legend: semantic_tokens_legend(),
        range: Some(true),
        full: Some(SemanticTokensFullOptions::Bool(true)),
    }
    .into()
}

const MOD_DECLARATION: u32 = 1 << 0;
#[allow(dead_code)]
const MOD_DEFINITION: u32 = 1 << 1;
const MOD_READONLY: u32 = 1 << 2;
#[allow(dead_code)]
const MOD_STATIC: u32 = 1 << 3;

struct DeclMap(Vec<(u32, u32, u32, u32)>);

impl DeclMap {
    fn new() -> Self {
        Self(Vec::new())
    }

    fn insert(&mut self, start: u32, end: u32, token_type: u32, modifiers: u32) {
        self.0.push((start, end, token_type, modifiers));
    }

    fn get(&self, start: u32, end: u32) -> Option<(u32, u32)> {
        self.0
            .iter()
            .find(|(s, e, _, _)| *s == start && *e == end)
            .map(|(_, _, ty, mod_)| (*ty, *mod_))
    }
}

fn is_type_name_token(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::TokIdent
            | Kind::KwAbstract
            | Kind::KwAnd
            | Kind::KwAs
            | Kind::KwBreak
            | Kind::KwClass
            | Kind::KwConst
            | Kind::KwContinue
            | Kind::KwDo
            | Kind::KwElse
            | Kind::KwFalse
            | Kind::KwFor
            | Kind::KwFunction
            | Kind::KwGlobal
            | Kind::KwIf
            | Kind::KwIn
            | Kind::KwInclude
            | Kind::KwLet
            | Kind::KwNew
            | Kind::KwNot
            | Kind::KwNull
            | Kind::KwOr
            | Kind::KwReturn
            | Kind::KwTrue
            | Kind::KwVar
            | Kind::KwWhile
            | Kind::KwXor
            | Kind::KwFinal
            | Kind::KwConstructor
            | Kind::KwExtends
            | Kind::KwStatic
            | Kind::KwPublic
            | Kind::KwPrivate
            | Kind::KwProtected
            | Kind::KwThis
            | Kind::KwSuper
            | Kind::KwInstanceof
            | Kind::KwTry
            | Kind::KwCatch
            | Kind::KwSwitch
            | Kind::KwCase
            | Kind::KwDefault
            | Kind::KwThrow
            | Kind::KwReserved
    )
}

fn member_expr_member_span(node: &SyntaxNode) -> Option<(u32, u32)> {
    if node.kind_as::<Kind>() != Some(Kind::NodeMemberExpr) {
        return None;
    }
    let mut saw_dot = false;
    for elem in node.children() {
        match elem {
            SyntaxElement::Token(t) => {
                if t.is_trivia() {
                    continue;
                }
                if t.text() == "." {
                    saw_dot = true;
                } else if saw_dot {
                    let r = t.text_range();
                    return Some((r.start, r.end));
                }
            }
            SyntaxElement::Node(_) => {}
        }
    }
    None
}

fn kind_to_semantic(kind: Kind) -> Option<(TokenTypeKind, u32)> {
    let (token_type, modifiers) = match kind {
        Kind::TokNumber => (TokenTypeKind::Number, 0u32),
        Kind::TokString => (TokenTypeKind::String, 0),
        Kind::TokIdent => (TokenTypeKind::Variable, 0),
        Kind::TokOp
        | Kind::TokArrow
        | Kind::TokDotDot
        | Kind::TokDot
        | Kind::TokColon
        | Kind::TokComma
        | Kind::TokSemi
        | Kind::TokParenL
        | Kind::TokParenR
        | Kind::TokBracketL
        | Kind::TokBracketR
        | Kind::TokBraceL
        | Kind::TokBraceR
        | Kind::TokLemnisate
        | Kind::TokPi => (TokenTypeKind::Operator, 0),
        Kind::KwAbstract
        | Kind::KwPublic
        | Kind::KwPrivate
        | Kind::KwProtected
        | Kind::KwFinal
        | Kind::KwStatic => (TokenTypeKind::Modifier, 0),
        Kind::KwClass
        | Kind::KwFunction
        | Kind::KwVar
        | Kind::KwGlobal
        | Kind::KwConst
        | Kind::KwLet
        | Kind::KwIf
        | Kind::KwElse
        | Kind::KwWhile
        | Kind::KwFor
        | Kind::KwDo
        | Kind::KwReturn
        | Kind::KwBreak
        | Kind::KwContinue
        | Kind::KwAnd
        | Kind::KwOr
        | Kind::KwNot
        | Kind::KwXor
        | Kind::KwIn
        | Kind::KwTrue
        | Kind::KwFalse
        | Kind::KwNull
        | Kind::KwNew
        | Kind::KwAs
        | Kind::KwInclude
        | Kind::KwExtends
        | Kind::KwConstructor
        | Kind::KwThis
        | Kind::KwSuper
        | Kind::KwInstanceof
        | Kind::KwTry
        | Kind::KwCatch
        | Kind::KwSwitch
        | Kind::KwCase
        | Kind::KwDefault
        | Kind::KwThrow
        | Kind::KwReserved => (TokenTypeKind::Keyword, 0),
        Kind::TriviaLineComment | Kind::TriviaBlockComment => (TokenTypeKind::Comment, 0),
        Kind::TriviaWs => return None,
        _ => (TokenTypeKind::Variable, 0),
    };
    Some((token_type, modifiers))
}

/// One semantic token entry (byte range + type + modifiers) before delta encoding.
#[derive(Clone)]
struct TokenEntry {
    start: u32,
    end: u32,
    token_type: u32,
    token_modifiers_bitset: u32,
}

/// Splits any entry whose byte range contains a newline into one entry per line.
/// LSP semantic tokens are effectively single-line; clients only color the first line of a multi-line token.
fn split_entries_by_line(source: &str, entries: &[TokenEntry]) -> Vec<TokenEntry> {
    let mut out = Vec::new();
    for e in entries {
        let start = e.start as usize;
        let end = e.end as usize;
        if start >= end || !source[start..end].contains('\n') {
            out.push(e.clone());
            continue;
        }
        let mut pos = start;
        while pos < end {
            let line_end = source[pos..end].find('\n').map(|i| pos + i).unwrap_or(end);
            if line_end > pos {
                out.push(TokenEntry {
                    start: pos as u32,
                    end: line_end as u32,
                    token_type: e.token_type,
                    token_modifiers_bitset: e.token_modifiers_bitset,
                });
            }
            pos = line_end;
            if pos < end && source.as_bytes()[pos] == b'\n' {
                pos += 1;
            }
        }
    }
    out
}

/// Emit semantic tokens from comment text (with doxygen segment splitting) at a given byte offset.
/// Ensures that after every Doxygen tag, the following text (rest of line / until next tag) is
/// explicitly a comment segment so it is colored as comment.
fn comment_entries_from_text(comment_start: u32, text: &str) -> Vec<TokenEntry> {
    let segments = doxygen_segments(text);
    let comment_end = comment_start + text.len() as u32;
    let mut entries = Vec::new();
    for seg in &segments {
        if seg.start >= seg.end {
            continue;
        }
        let start = comment_start + seg.start as u32;
        let end = comment_start + seg.end as u32;
        let token_type = if seg.is_tag {
            TokenTypeKind::Keyword.index()
        } else {
            TokenTypeKind::Comment.index()
        };
        entries.push(TokenEntry {
            start,
            end,
            token_type,
            token_modifiers_bitset: 0,
        });
    }
    // Ensure there is a comment segment after every tag (rest of line / until next segment).
    // This guarantees text after @class, @brief, etc. is always marked as comment.
    let mut with_gaps = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        with_gaps.push(e.clone());
        if e.token_type == TokenTypeKind::Keyword.index() {
            let gap_start = e.end;
            let gap_end = if i + 1 < entries.len() {
                entries[i + 1].start
            } else {
                comment_end
            };
            if gap_end > gap_start {
                with_gaps.push(TokenEntry {
                    start: gap_start,
                    end: gap_end,
                    token_type: TokenTypeKind::Comment.index(),
                    token_modifiers_bitset: 0,
                });
            }
        }
    }
    with_gaps.sort_by_key(|e| e.start);
    with_gaps
}

/// Delta-encode a sorted list of token entries and append to `data`.
fn emit_entries(
    source: &str,
    line_index: &LineIndex,
    entries: &[TokenEntry],
    range: Option<(u32, u32)>,
    data: &mut Vec<SemanticToken>,
) {
    let mut prev_line = 0u32;
    let mut prev_char = 0u32;
    for e in entries {
        if let Some((byte_start, byte_end)) = range {
            if e.end <= byte_start || e.start >= byte_end {
                continue;
            }
        }
        let span_slice = &source[e.start as usize..e.end as usize];
        let length_utf16 = span_slice.encode_utf16().count() as u32;
        let (line, char) = line_index.line_col_utf16(source, e.start);
        let delta_line = line.saturating_sub(prev_line);
        let delta_start = if delta_line == 0 {
            char.saturating_sub(prev_char)
        } else {
            char
        };
        data.push(SemanticToken {
            delta_line,
            delta_start,
            length: length_utf16,
            token_type: e.token_type,
            token_modifiers_bitset: e.token_modifiers_bitset,
        });
        prev_line = line;
        // LSP: deltaStart is relative to the *start* of the previous token, not its end.
        prev_char = char;
    }
}

/// Visitor that collects semantic token entries in document order, including comments (trivia).
/// Builds declaration map, type spans, and method/property spans during the walk via enter_node/leave_node.
struct SemanticTokensVisitor<'a> {
    entries: Vec<TokenEntry>,
    decl_map: DeclMap,
    type_spans: HashSet<(u32, u32)>,
    method_spans: HashSet<(u32, u32)>,
    property_spans: HashSet<(u32, u32)>,
    /// Root of the tree; used for MemberExpr ancestor/sibling checks.
    root: &'a SyntaxNode,
    /// Depth of TypeExpr nesting so visit_token can record type name spans.
    type_expr_depth: u32,
}

impl<'a> SemanticTokensVisitor<'a> {
    fn new(root: &'a SyntaxNode) -> Self {
        Self {
            entries: Vec::new(),
            decl_map: DeclMap::new(),
            type_spans: HashSet::new(),
            method_spans: HashSet::new(),
            property_spans: HashSet::new(),
            root,
            type_expr_depth: 0,
        }
    }

    fn push_entries_for_token(&mut self, token: &SyntaxToken) {
        let r = token.text_range();
        let Some(kind) = token.kind_as::<Kind>() else {
            return;
        };
        if kind == Kind::TriviaLineComment || kind == Kind::TriviaBlockComment {
            let comment_entries = comment_entries_from_text(r.start, token.text());
            self.entries.extend(comment_entries);
            return;
        }
        if kind == Kind::TriviaWs {
            return;
        }
        let (mut token_type, mut token_modifiers_bitset) = match kind_to_semantic(kind) {
            Some((ty, mods)) => (ty.index(), mods),
            None => return,
        };
        if kind == Kind::TokIdent {
            if let Some((ty, mods)) = self.decl_map.get(r.start, r.end) {
                token_type = ty;
                token_modifiers_bitset = mods;
            } else if self.type_spans.contains(&(r.start, r.end)) {
                token_type = TokenTypeKind::Type.index();
                token_modifiers_bitset = 0;
            } else if self.method_spans.contains(&(r.start, r.end)) {
                token_type = TokenTypeKind::Method.index();
                token_modifiers_bitset = 0;
            } else if self.property_spans.contains(&(r.start, r.end)) {
                token_type = TokenTypeKind::Property.index();
                token_modifiers_bitset = 0;
            }
        } else if is_type_name_token(kind) && self.type_spans.contains(&(r.start, r.end)) {
            token_type = TokenTypeKind::Type.index();
            token_modifiers_bitset = 0;
        }
        self.entries.push(TokenEntry {
            start: r.start,
            end: r.end,
            token_type,
            token_modifiers_bitset,
        });
    }
}

impl Visitor for SemanticTokensVisitor<'_> {
    fn enter_node(&mut self, node: &SyntaxNode) -> WalkResult {
        match node.kind_as::<Kind>() {
            Some(Kind::NodeClassDecl) => {
                if let Some(info) = class_decl_info(node) {
                    self.decl_map.insert(
                        info.name_span.start,
                        info.name_span.end,
                        TokenTypeKind::Class.index(),
                        MOD_DECLARATION,
                    );
                }
            }
            Some(Kind::NodeFunctionDecl) => {
                if let Some(info) = function_decl_info(node) {
                    self.decl_map.insert(
                        info.name_span.start,
                        info.name_span.end,
                        TokenTypeKind::Function.index(),
                        MOD_DECLARATION,
                    );
                }
            }
            Some(Kind::NodeVarDecl) => {
                if let Some(info) = var_decl_info(node) {
                    let readonly = matches!(info.kind, VarDeclKind::Const | VarDeclKind::Let);
                    let mods = MOD_DECLARATION | if readonly { MOD_READONLY } else { 0 };
                    self.decl_map.insert(
                        info.name_span.start,
                        info.name_span.end,
                        TokenTypeKind::Variable.index(),
                        mods,
                    );
                }
            }
            Some(Kind::NodeParam) => {
                if let Some((_, span)) = param_name(node) {
                    self.decl_map.insert(
                        span.start,
                        span.end,
                        TokenTypeKind::Parameter.index(),
                        MOD_DECLARATION,
                    );
                }
            }
            Some(Kind::NodeTypeExpr) => {
                self.type_expr_depth = self.type_expr_depth.saturating_add(1);
            }
            Some(Kind::NodeMemberExpr) => {
                if let Some((start, end)) = member_expr_member_span(node) {
                    let ancestors = node.ancestors(self.root);
                    let parent = match ancestors.first() {
                        Some(p) => p.clone(),
                        None => {
                            self.property_spans.insert((start, end));
                            return WalkResult::Continue(());
                        }
                    };
                    let siblings: Vec<SyntaxNode> = parent.child_nodes().collect();
                    let pos = match siblings
                        .iter()
                        .position(|s| s.text_range() == node.text_range())
                    {
                        Some(p) => p,
                        None => {
                            self.property_spans.insert((start, end));
                            return WalkResult::Continue(());
                        }
                    };
                    let next_sibling = siblings.get(pos + 1);
                    if next_sibling.and_then(|n| n.kind_as::<Kind>()).as_ref()
                        == Some(&Kind::NodeCallExpr)
                    {
                        self.method_spans.insert((start, end));
                    } else {
                        self.property_spans.insert((start, end));
                    }
                }
            }
            _ => {}
        }
        WalkResult::Continue(())
    }

    fn leave_node(&mut self, node: &SyntaxNode) -> WalkResult {
        if node.kind_as::<Kind>() == Some(Kind::NodeTypeExpr) {
            self.type_expr_depth = self.type_expr_depth.saturating_sub(1);
        }
        WalkResult::Continue(())
    }

    fn visit_token(&mut self, token: &SyntaxToken) -> WalkResult {
        if self.type_expr_depth > 0 {
            if let Some(kind) = token.kind_as::<Kind>() {
                if is_type_name_token(kind) && !token.is_trivia() {
                    let r = token.text_range();
                    self.type_spans.insert((r.start, r.end));
                }
            }
        }
        self.push_entries_for_token(token);
        WalkResult::Continue(())
    }
}

fn compute_semantic_tokens_impl(
    source: &str,
    line_index: &LineIndex,
    root: &SyntaxNode,
    range: Option<(u32, u32)>,
) -> SemanticTokens {
    let mut visitor = SemanticTokensVisitor::new(root);
    // Full walk: visit nodes (enter_node/leave_node for decl map, type spans, method/property) and tokens (visit_token for emission).
    let _ = root.walk(&mut visitor, &WalkOptions::full());
    let entries = split_entries_by_line(source, &visitor.entries);
    let mut data = Vec::new();
    emit_entries(source, line_index, &entries, range, &mut data);
    SemanticTokens {
        result_id: None,
        data,
    }
}

/// Compute semantic tokens from token stream only (no program tree).
/// Use when the program parse failed or produced no root, so the LSP can still provide
/// basic keyword/string/number/comment/operator highlighting.
#[must_use]
pub fn compute_semantic_tokens_fallback(
    source: &str,
    line_index: &LineIndex,
    range: Option<(u32, u32)>,
) -> SemanticTokens {
    if let Ok(out) = crate::parse_tokens(source) {
        if let Some(root) = out.syntax_root(source.as_bytes()) {
            let mut visitor = SemanticTokensVisitor::new(&root);
            let _ = root.walk(&mut visitor, &WalkOptions::full());
            let entries = split_entries_by_line(source, &visitor.entries);
            let mut data = Vec::new();
            emit_entries(source, line_index, &entries, range, &mut data);
            return SemanticTokens {
                result_id: None,
                data,
            };
        }
    }
    // No root at all: nothing to walk; return empty tokens.
    SemanticTokens {
        result_id: None,
        data: Vec::new(),
    }
}

/// Compute full-document semantic tokens for the given root and source.
#[must_use]
pub fn compute_semantic_tokens(
    source: &str,
    line_index: &LineIndex,
    root: &SyntaxNode,
) -> SemanticTokens {
    compute_semantic_tokens_impl(source, line_index, root, None)
}

/// Compute semantic tokens for tokens overlapping [byte_start, byte_end).
#[must_use]
pub fn compute_semantic_tokens_range(
    source: &str,
    line_index: &LineIndex,
    root: &SyntaxNode,
    byte_start: u32,
    byte_end: u32,
) -> SemanticTokens {
    compute_semantic_tokens_impl(source, line_index, root, Some((byte_start, byte_end)))
}
