//! `LeekScript` grammar built with sipha.
//!
//! Phase 1: token stream (lexer).
//! Phases 2–4: expressions, statements, top-level.

mod expressions;
mod keywords;
mod literals;
mod operators;
mod signature;
mod statements;

pub use signature::build_signature_grammar;
mod token_stream;
mod trivia;

use sipha::prelude::*;

/// Build the `LeekScript` grammar (Phase 1: token stream).
#[must_use] 
pub fn build_grammar() -> sipha::builder::BuiltGraph {
    let mut g = GrammarBuilder::new();
    g.set_trivia_rule("ws");

    g.begin_rule("start");
    g.skip();
    g.call("token_stream");
    g.skip();
    g.end_of_input();
    g.accept();

    trivia::add_ws(&mut g);
    literals::add_string_lit(&mut g);
    literals::add_escape(&mut g);
    literals::add_number_lit_full(&mut g);
    operators::add_operator(&mut g);
    operators::add_arrow(&mut g);
    operators::add_dot_dot(&mut g);
    operators::add_dot(&mut g);
    operators::add_bracket(&mut g);
    keywords::add_keyword_or_ident_token(&mut g);
    literals::add_special_lit(&mut g);
    token_stream::add_token_rule(&mut g);
    token_stream::add_token_stream(&mut g);

    g.finish().expect("grammar must be valid")
}

/// Build a grammar that parses a single expression (Phase 2).
/// Start rule is "start" → ws expr ws eof.
#[must_use] 
pub fn build_expression_grammar() -> sipha::builder::BuiltGraph {
    let mut g = GrammarBuilder::new();
    g.set_trivia_rule("ws");

    g.begin_rule("start");
    g.skip();
    g.call("expr");
    g.skip();
    g.end_of_input();
    g.accept();

    g.allow_rule_cycles(true); // expr -> expr via ( expr ) is intentional; use with memo
    trivia::add_ws(&mut g);
    literals::add_number_lit(&mut g);
    literals::add_string_lit(&mut g);
    literals::add_escape(&mut g);
    keywords::add_ident(&mut g);
    operators::add_lparen_rparen(&mut g);
    expressions::add_expr_minimal(&mut g);

    g.finish().expect("expression grammar must be valid")
}

/// Build a grammar that parses a program (Phase 3/4: list of statements).
/// Start rule is "start" → node(NodeRoot, ws program ws eof) so the parse tree has a single root.
#[must_use] 
pub fn build_program_grammar() -> sipha::builder::BuiltGraph {
    let mut g = GrammarBuilder::new();
    g.set_trivia_rule("ws");

    g.begin_rule("start");
    g.node(crate::syntax::Kind::NodeRoot.into_syntax_kind(), |g| {
        g.skip();
        g.call("program");
        g.skip();
        g.end_of_input();
    });
    g.accept();

    g.allow_rule_cycles(true); // expr chain has intentional indirect recursion; use with memo
    trivia::add_ws(&mut g);
    add_literals_program(&mut g);
    add_keywords_program(&mut g);
    add_operators_program(&mut g);
    statements::add_block(&mut g); // before expressions (block used by anon fn, etc.)
    add_expressions(&mut g);
    add_statements(&mut g);

    g.finish().expect("program grammar must be valid")
}

fn add_literals_program(g: &mut sipha::builder::GrammarBuilder) {
    literals::add_number_lit(g);
    literals::add_string_lit(g);
    literals::add_escape(g);
    literals::add_special_lit(g);
}

fn add_keywords_program(g: &mut sipha::builder::GrammarBuilder) {
    keywords::add_ident(g);
    keywords::add_and_kw(g);
    keywords::add_or_kw(g);
    keywords::add_xor_kw(g);
    keywords::add_abstract_kw(g);
    keywords::add_constructor_kw(g);
    keywords::add_extends_kw(g);
    keywords::add_static_kw(g);
    keywords::add_final_kw(g);
    keywords::add_public_kw(g);
    keywords::add_private_kw(g);
    keywords::add_protected_kw(g);
    keywords::add_not_kw(g);
    keywords::add_as_kw(g);
    keywords::add_in_kw(g);
    keywords::add_instanceof_kw(g);
}

fn add_operators_program(g: &mut sipha::builder::GrammarBuilder) {
    operators::add_program_brackets(g);
    operators::add_arrow(g);
    operators::add_program_operators(g);
    operators::add_op_colon(g);
    keywords::add_keyword_or_ident_program(g);
}

fn add_expressions(g: &mut sipha::builder::GrammarBuilder) {
    expressions::add_interval_literal(g);
    expressions::add_bracket_literal(g);
    expressions::add_map_pair(g);
    expressions::add_primary(g);
    expressions::add_object_pair(g);
    expressions::add_postfix(g);
    expressions::add_unary(g);
    expressions::add_expr_power(g);
    expressions::add_expr_mul(g);
    expressions::add_expr_add(g);
    expressions::add_expr_interval(g);
    expressions::add_expr_compare(g);
    expressions::add_expr_equality(g);
    expressions::add_expr_in(g);
    expressions::add_expr_instanceof(g);
    expressions::add_expr_and(g);
    expressions::add_expr_or(g);
    expressions::add_expr_xor(g);
    expressions::add_expr_ternary(g);
    expressions::add_type_params(g);
    expressions::add_type_primary(g);
    expressions::add_type_optional(g);
    expressions::add_type_expr(g);
    expressions::add_expr_as(g);
    expressions::add_expr(g);
}

fn add_statements(g: &mut sipha::builder::GrammarBuilder) {
    statements::add_var_decl(g);
    statements::add_global_decl(g);
    statements::add_const_decl(g);
    statements::add_let_decl(g);
    statements::add_param(g);
    statements::add_if_stmt(g);
    statements::add_while_stmt(g);
    statements::add_return_stmt(g);
    statements::add_break_stmt(g);
    statements::add_continue_stmt(g);
    statements::add_expr_stmt(g);
    statements::add_include_stmt(g);
    statements::add_function_decl(g);
    statements::add_class_method(g);
    statements::add_constructor_decl(g);
    statements::add_class_field(g);
    statements::add_class_decl(g);
    statements::add_for_init(g);
    statements::add_for_stmt(g);
    statements::add_for_in_stmt(g);
    statements::add_do_while_stmt(g);
    statements::add_statement(g);
    statements::add_program(g);
}
