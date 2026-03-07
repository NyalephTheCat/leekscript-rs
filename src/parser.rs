//! Parser entry points: build grammar once and parse source.

use std::sync::OnceLock;

use sipha::engine::{Engine, ParseError, ParseOutput};
pub use sipha::incremental::TextEdit;
use sipha::incremental::reparse as sipha_reparse;
use sipha::insn::ParseGraph;
use sipha::parsed_doc::ParsedDoc;
use sipha::red::SyntaxNode;

use crate::grammar::{
    build_expression_grammar, build_grammar, build_program_grammar, build_signature_grammar,
};

type BuiltAndGraph = (sipha::builder::BuiltGraph, ParseGraph);

fn cache_grammar<F>(storage: &'static OnceLock<BuiltAndGraph>, build: F) -> &'static BuiltAndGraph
where
    F: FnOnce() -> sipha::builder::BuiltGraph,
{
    storage.get_or_init(|| {
        let built = build();
        let graph = built.as_graph();
        (built, graph)
    })
}

macro_rules! cached_grammar_fn {
    ($name:ident, $build:ident) => {
        fn $name() -> &'static BuiltAndGraph {
            static STORAGE: OnceLock<BuiltAndGraph> = OnceLock::new();
            cache_grammar(&STORAGE, $build)
        }
    };
}

cached_grammar_fn!(token_stream_built_and_graph, build_grammar);
cached_grammar_fn!(expression_built_and_graph, build_expression_grammar);
cached_grammar_fn!(program_built_and_graph, build_program_grammar);
cached_grammar_fn!(signature_built_and_graph, build_signature_grammar);

/// Single place for engine creation and parse; returns raw `ParseOutput`.
fn run_parse(
    source: &str,
    get_graph: fn() -> &'static BuiltAndGraph,
) -> Result<ParseOutput, ParseError> {
    let (_, graph) = get_graph();
    let mut engine = Engine::new().with_memo();
    engine.parse(graph, source.as_bytes())
}

fn parse_to_syntax_root(
    source: &str,
    get_graph: fn() -> &'static BuiltAndGraph,
) -> Result<Option<SyntaxNode>, ParseError> {
    let out = run_parse(source, get_graph)?;
    Ok(out.syntax_root(source.as_bytes()))
}

fn parse_to_output(
    source: &str,
    get_graph: fn() -> &'static BuiltAndGraph,
) -> Result<ParseOutput, ParseError> {
    run_parse(source, get_graph)
}

/// Parse source as a token stream (Phase 1 lexer).
///
/// Returns the sipha parse output; use `.syntax_root(source.as_bytes())` to get
/// the root syntax node, or `.tree_events` for the raw event list.
pub fn parse_tokens(source: &str) -> Result<ParseOutput, ParseError> {
    run_parse(source, token_stream_built_and_graph)
}

/// Parse source as a program (Phase 3/4: list of statements).
///
/// Returns the program root node (`NODE_ROOT` with statement children).
/// For token stream only, use [`parse_tokens`].
pub fn parse(source: &str) -> Result<Option<SyntaxNode>, ParseError> {
    parse_to_syntax_root(source, program_built_and_graph)
}

/// Parse source as a single expression (Phase 2).
///
/// Uses a dedicated expression grammar (primary: number, string, identifier, parenthesized expr).
pub fn parse_expression(source: &str) -> Result<Option<SyntaxNode>, ParseError> {
    parse_to_syntax_root(source, expression_built_and_graph)
}

/// Parse source as a signature file (function/class/global declarations only).
///
/// Returns the root node (`NodeSigFile`) whose children are sig items.
/// Use for loading stdlib or other API signature definitions.
pub fn parse_signatures(source: &str) -> Result<Option<SyntaxNode>, ParseError> {
    parse_to_syntax_root(source, signature_built_and_graph)
}

/// Parse source and return a [`ParsedDoc`]: source bytes, line index, and syntax root.
///
/// Use this when you need offset-to-line/column, [`ParsedDoc::node_at_offset`],
/// [`ParsedDoc::token_at_offset`], or formatted diagnostics. Returns `None` if
/// the parse produced no or invalid tree events.
pub fn parse_to_doc(source: &str) -> Result<Option<ParsedDoc>, ParseError> {
    let out = parse_to_output(source, program_built_and_graph)?;
    Ok(ParsedDoc::new(source.as_bytes().to_vec(), &out))
}

/// Parse in recovering mode: on failure, returns the partial output and the error.
///
/// Returns `Ok(out)` on full success; `Err((partial, e))` on failure, with
/// `partial` containing tree events and `consumed` up to the error position.
/// Use `partial.syntax_root(source.as_bytes())` to try to build a partial tree
/// (may be `None` if events are not well-nested). Use for IDE or multi-error reporting.
pub fn parse_recovering(source: &str) -> Result<ParseOutput, (ParseOutput, ParseError)> {
    let (_, graph) = program_built_and_graph();
    let mut engine = Engine::new().with_memo();
    engine.parse_recovering(graph, source.as_bytes())
}

/// Literal table for the program grammar (used for parsing full programs).
///
/// Use with [`parse_error_to_miette`] so that "expected literal#n" in diagnostics
/// is resolved to the actual token text (e.g. `"var"`, `"function"`).
#[must_use] 
pub fn program_literals() -> &'static sipha::insn::LiteralTable {
    &program_built_and_graph().1.literals
}

/// Rule names for the program grammar (used for diagnostics).
///
/// Use with [`parse_error_to_miette`] so that "expected rule#n" shows as the rule name.
#[must_use] 
pub fn program_rule_names() -> &'static [&'static str] {
    program_built_and_graph().1.rule_names
}

/// Reparse after a text edit, reusing unchanged parts of the tree.
///
/// Takes the previous source, the previous syntax root (from [`parse`]), and an edit.
/// Returns the new syntax root, or `None` if the new parse produced no root.
/// Use for incremental updates in editors or formatters.
pub fn reparse(
    old_source: &str,
    old_root: &SyntaxNode,
    edit: &TextEdit,
) -> Result<Option<SyntaxNode>, ParseError> {
    let (_, graph) = program_built_and_graph();
    let mut engine = Engine::new().with_memo();
    sipha_reparse(
        &mut engine,
        graph,
        old_source.as_bytes(),
        old_root,
        edit,
    )
}

/// Expected labels for the program grammar (used for diagnostics).
#[must_use] 
pub fn program_expected_labels() -> &'static [&'static str] {
    program_built_and_graph().1.expected_labels
}

/// Convert a parse error into a [`miette::Report`] with source snippet and resolved literals.
///
/// Uses the **program** grammar's literal and rule-name tables so that expected
/// tokens and rules show as readable text (e.g. `"var"`, `statement`). Returns
/// `None` for [`ParseError::BadGraph`](sipha::engine::ParseError::BadGraph).
///
/// Use when the error came from [`parse`]. For [`parse_expression`] or
/// [`parse_tokens`], use the corresponding graph literals via sipha directly
/// if you need miette reports.
#[must_use] 
pub fn parse_error_to_miette(
    e: &ParseError,
    source: &str,
    filename: &str,
) -> Option<miette::Report> {
    e.to_miette_report(
        source,
        filename,
        Some(program_literals()),
        Some(program_rule_names()),
        Some(program_expected_labels()),
    )
}

#[cfg(test)]
mod tests {
    use sipha::red::SyntaxElement;

    use crate::syntax::Kind;

    use super::{parse, parse_expression, parse_tokens, reparse};

    #[test]
    fn parse_tokens_valid() {
        let out = parse_tokens("var x = 42").unwrap();
        let root = out.syntax_root("var x = 42".as_bytes());
        assert!(root.is_some(), "token stream should produce a root");
    }

    #[test]
    fn parse_tokens_invalid() {
        let result = parse_tokens("'unterminated string");
        assert!(result.is_err(), "unterminated string should fail");
    }

    #[test]
    fn parse_expression_valid() {
        let root = parse_expression("1").unwrap();
        assert!(root.is_some(), "simple expression should parse");
    }

    #[test]
    fn parse_expression_invalid() {
        let result = parse_expression("1 + ");
        assert!(result.is_err() || result.as_ref().ok().and_then(|r| r.as_ref()).is_none());
    }

    #[test]
    fn parse_valid_program() {
        let root = parse("return 1 + 2").unwrap().expect("root");
        assert_eq!(root.kind_as::<Kind>(), Some(Kind::NodeRoot));
        let node_children: Vec<_> = root
            .children()
            .filter_map(|c| match c {
                SyntaxElement::Node(n) => Some(n),
                _ => None,
            })
            .collect();
        assert!(!node_children.is_empty(), "root should have statement children");
        assert_eq!(
            node_children[0].kind_as::<Kind>(),
            Some(Kind::NodeReturnStmt),
            "first statement should be return"
        );
    }

    #[test]
    fn parse_invalid_program() {
        // Unclosed brace or invalid token sequence should fail.
        let result = parse("return (");
        assert!(result.is_err(), "invalid program should return parse error");
    }

    #[test]
    fn assert_parse_sexp() {
        use sipha_diff::{assert_parse_eq, syntax_node_to_sexp, SexpOptions};

        let opts = SexpOptions {
            kind_to_name: Some(|k| Some(crate::syntax::kind_name(k))),
            ..SexpOptions::semantic_only()
        };
        let root = parse_expression("1").unwrap().expect("root");
        let expected = syntax_node_to_sexp(&root, &opts);
        assert_parse_eq(parse_expression("1"), "1", &expected, &opts);
        assert!(expected.contains("EXPR"), "readable kind names in S-expression");
    }

    #[test]
    fn reparse_after_edit() {
        let old = "var x = 1;";
        let root = parse(old).unwrap().expect("root");
        let edit = super::TextEdit {
            start: 8,
            end: 9,
            new_text: b"2".to_vec(),
        };
        let new_root = reparse(old, &root, &edit).unwrap();
        let new_root = new_root.expect("reparse should yield root");
        let new_text = new_root.collect_text();
        assert!(new_text.contains("2"), "edited content in reparsed tree: {:?}", new_text);
    }

    #[test]
    fn binary_expr_rhs_field() {
        use sipha::types::IntoSyntaxKind;

        use crate::analysis::binary_expr_rhs;

        let root = parse("3 * 4;").unwrap().expect("root");
        let binary = root
            .find_node(Kind::NodeBinaryExpr.into_syntax_kind())
            .expect("should have binary expr");
        let rhs = binary_expr_rhs(&binary).expect("rhs field on mul-level binary");
        assert_eq!(rhs.collect_text().trim(), "4", "named field rhs should be right operand");
    }
}
