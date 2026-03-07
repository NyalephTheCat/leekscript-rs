//! Parser entry points: build grammar once and parse source.

use std::sync::OnceLock;

use sipha::engine::{Engine, ParseError, ParseOutput, RecoverMultiResult};
pub use sipha::incremental::TextEdit;
use sipha::incremental::reparse as sipha_reparse;
use sipha::insn::ParseGraph;
use sipha::parsed_doc::ParsedDoc;
use sipha::red::SyntaxNode;
use sipha::types::Span;

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

/// Parse in multi-error recovery mode: on statement failures, skip to the next sync point
/// (e.g. `;`, `}`, or statement-start keyword) and continue, collecting up to `max_errors` errors.
///
/// Requires the program grammar to use [`recover_until`](sipha::builder::GrammarBuilder::recover_until)
/// (used for `program` and `block`). Returns `Ok(output)` on full success; `Err(RecoverMultiResult { partial, errors })`
/// when at least one parse error was collected. Use the partial output's syntax root for a best-effort
/// tree and convert each error to diagnostics for IDE or batch reporting.
pub fn parse_recovering_multi(
    source: &str,
    max_errors: usize,
) -> Result<ParseOutput, RecoverMultiResult> {
    let (_, graph) = program_built_and_graph();
    let mut engine = Engine::new().with_memo();
    engine.parse_recovering_multi(graph, source.as_bytes(), max_errors)
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

/// Convert a parse error into semantic diagnostics for LSP or other tooling.
///
/// Returns a single-element vec for [`ParseError::NoMatch`] (with message from
/// the program grammar's literals/rule names) or [`ParseError::BadGraph`].
/// Use when the main program failed to parse so that diagnostics include the
/// parse error without duplicating conversion logic in the LSP.
#[must_use]
pub fn parse_error_to_diagnostics(parse_err: &ParseError, source: &str) -> Vec<sipha::error::SemanticDiagnostic> {
    let source_bytes = source.as_bytes();
    let line_index = sipha::line_index::LineIndex::new(source_bytes);
    let (span, message) = match parse_err {
        ParseError::NoMatch(diag) => {
            let message = diag.format_with_source(
                source_bytes,
                &line_index,
                Some(program_literals()),
                Some(program_rule_names()),
                Some(program_expected_labels()),
            );
            (Span::new(diag.furthest, diag.furthest), message)
        }
        ParseError::BadGraph => (
            Span::new(0, 0),
            "malformed parse graph".to_string(),
        ),
    };
    vec![sipha::error::SemanticDiagnostic {
        span,
        message,
        severity: sipha::error::Severity::Error,
        code: Some("parse_error".to_string()),
        file_id: None,
    }]
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
    use sipha::types::IntoSyntaxKind;

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

    // ─── Parser edge cases: malformed or ambiguous inputs ────────────────────

    #[test]
    fn parse_edge_unterminated_double_quote_string() {
        let result = parse(r#"return "hello"#);
        assert!(result.is_err(), "unterminated double-quote string should fail");
    }

    #[test]
    fn parse_edge_unterminated_single_quote_string() {
        let result = parse("return 'x");
        assert!(result.is_err(), "unterminated single-quote string should fail");
    }

    #[test]
    fn parse_edge_empty_input() {
        let result = parse("");
        assert!(result.is_ok(), "empty input should not panic");
        // Empty input may return None or Some(empty root) depending on grammar.
        let _ = result.unwrap();
    }

    #[test]
    fn parse_edge_only_whitespace() {
        let result = parse("   \n\t  ");
        assert!(result.is_ok());
    }

    #[test]
    fn parse_edge_incomplete_binary_op() {
        let result = parse("return 1 + ");
        assert!(result.is_err(), "incomplete expression after + should fail");
    }

    #[test]
    fn parse_edge_unclosed_paren() {
        let result = parse("return (1 + 2");
        assert!(result.is_err(), "unclosed parenthesis should fail");
    }

    #[test]
    fn parse_edge_unclosed_brace() {
        let result = parse("function f() { return 1;");
        // Parser may fail or recover; we only check it doesn't panic.
        let _ = result;
    }

    #[test]
    fn parse_edge_odd_operator_sequence() {
        let result = parse("return 1 * * 2;");
        // Grammar may reject or accept; we lock in that we don't panic.
        let _ = result;
    }

    #[test]
    fn parse_edge_recovery_produces_partial_tree() {
        use super::parse_recovering_multi;
        let source = "var x = 1; return ( ; var y = 2;";
        let out = parse_recovering_multi(source, 5);
        // Recovery returns Ok(ParseOutput) when parse succeeds, or Err with .partial and .errors.
        // When Err, partial result should still yield a syntax root for downstream use.
        if let Err(err) = &out {
            assert!(
                err.partial.syntax_root(source.as_bytes()).is_some(),
                "recovery Err should yield partial syntax root"
            );
        }
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
    fn parse_recovering_multi_collects_multiple_errors() {
        use super::parse_recovering_multi;

        // Two invalid statements: "return (" and "var x = " — recovery skips to next sync point.
        let source = "return ( ; var x = ";
        let result = parse_recovering_multi(source, 10);
        let err = result.expect_err("recovery should return Err with collected errors");
        assert!(
            err.errors.len() >= 2,
            "expected at least 2 parse errors, got {}",
            err.errors.len()
        );
    }

    #[test]
    fn parse_error_to_miette_produces_report() {
        use super::parse_error_to_miette;

        let source = "return (";
        let err = parse(source).unwrap_err();
        let filename = "test.leek";
        let report = parse_error_to_miette(&err, source, filename);
        assert!(report.is_some(), "NoMatch parse error should produce a miette report");
        let report = report.unwrap();
        let report_str = format!("{report:?}");
        assert!(
            report_str.contains("expected") || report_str.contains("test.leek"),
            "report should contain expected tokens or filename: {:?}",
            report_str
        );
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

    // ─── Function declaration forms (plan: ensure all shapes parse) ───────────

    fn assert_parse_function_decl(source: &str, test_name: &str) {
        let root = parse(source).unwrap().expect(test_name);
        let funcs = root.find_all_nodes(Kind::NodeFunctionDecl.into_syntax_kind());
        assert!(!funcs.is_empty(), "{}: expected at least one NodeFunctionDecl in {:?}", test_name, source);
    }

    #[test]
    fn parse_function_untyped_params_no_return() {
        assert_parse_function_decl("function a(b, c) {}", "untyped params, no return");
    }

    #[test]
    fn parse_function_untyped_params_arrow_return() {
        assert_parse_function_decl("function a(b, c) -> void {}", "untyped params, -> void");
    }

    #[test]
    fn parse_function_untyped_params_fat_arrow_return() {
        assert_parse_function_decl("function a(b, c) => void {}", "untyped params, => void");
    }

    #[test]
    fn parse_function_mixed_params_fat_arrow_return() {
        assert_parse_function_decl("function a(integer b, c) => void {}", "mixed params, => void");
    }

    #[test]
    fn parse_function_no_params() {
        assert_parse_function_decl("function a() {}", "no params");
    }

    #[test]
    fn parse_function_typed_params_arrow_return() {
        assert_parse_function_decl("function a(integer x, integer y) -> integer {}", "typed params, -> integer");
    }

    // ─── Function type in type position (Function<...>) ────────────────────────

    #[test]
    fn parse_program_with_function_type_two_args_return() {
        let source = "var f = null as Function<integer, integer => void>;";
        let root = parse(source).unwrap().expect("Function<integer, integer => void>");
        let type_exprs = root.find_all_nodes(Kind::NodeTypeExpr.into_syntax_kind());
        assert!(!type_exprs.is_empty(), "expected NodeTypeExpr for Function<...> type");
    }

    #[test]
    fn parse_program_with_function_type_zero_params() {
        let source = "var f = null as Function< => void>;";
        let root = parse(source).unwrap().expect("Function< => void>");
        let type_exprs = root.find_all_nodes(Kind::NodeTypeExpr.into_syntax_kind());
        assert!(!type_exprs.is_empty(), "expected NodeTypeExpr for Function< => void>");
    }

    #[test]
    fn parse_program_with_function_type_one_param() {
        let source = "var f = null as Function<integer => void>;";
        let root = parse(source).unwrap().expect("Function<integer => void>");
        let type_exprs = root.find_all_nodes(Kind::NodeTypeExpr.into_syntax_kind());
        assert!(!type_exprs.is_empty(), "expected NodeTypeExpr for Function<integer => void>");
    }

    #[test]
    fn parse_program_with_function_type_three_params() {
        let source = "var f = null as Function<integer, string, real => boolean>;";
        let root = parse(source).unwrap().expect("Function<integer, string, real => boolean>");
        let type_exprs = root.find_all_nodes(Kind::NodeTypeExpr.into_syntax_kind());
        assert!(!type_exprs.is_empty(), "expected NodeTypeExpr for Function<...>");
    }
}
