//! LeekScript grammar built with sipha.
//!
//! Phase 1: token stream (lexer).
//! Phases 2–4: expressions, statements, top-level.

mod expressions;
mod keywords;
mod literals;
mod operators;
mod statements;
mod token_stream;
mod trivia;

use sipha::prelude::*;

/// Build the LeekScript grammar (Phase 1: token stream).
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
pub fn build_expression_grammar() -> sipha::builder::BuiltGraph {
    let mut g = GrammarBuilder::new();
    g.set_trivia_rule("ws");

    g.begin_rule("start");
    g.skip();
    g.call("expr");
    g.skip();
    g.end_of_input();
    g.accept();

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
/// Start rule is "start" → ws program ws eof.
pub fn build_program_grammar() -> sipha::builder::BuiltGraph {
    let mut g = GrammarBuilder::new();
    g.set_trivia_rule("ws");

    g.begin_rule("start");
    g.skip();
    g.call("program");
    g.skip();
    g.end_of_input();
    g.accept();

    trivia::add_ws(&mut g);
    literals::add_number_lit(&mut g);
    literals::add_string_lit(&mut g);
    literals::add_escape(&mut g);
    literals::add_special_lit(&mut g);
    keywords::add_ident(&mut g);
    keywords::add_xor_kw(&mut g);
    keywords::add_abstract_kw(&mut g);
    keywords::add_constructor_kw(&mut g);
    keywords::add_extends_kw(&mut g);
    keywords::add_static_kw(&mut g);
    keywords::add_final_kw(&mut g);
    keywords::add_public_kw(&mut g);
    keywords::add_private_kw(&mut g);
    keywords::add_protected_kw(&mut g);
    keywords::add_not_kw(&mut g);
    keywords::add_as_kw(&mut g);
    keywords::add_instanceof_kw(&mut g);
    operators::add_program_brackets(&mut g);
    operators::add_arrow(&mut g);
    operators::add_program_operators(&mut g);
    operators::add_op_colon(&mut g);
    keywords::add_keyword_or_ident_program(&mut g);

    statements::add_block(&mut g);
    expressions::add_bracket_literal(&mut g);
    expressions::add_map_pair(&mut g);
    expressions::add_primary(&mut g);
    expressions::add_object_pair(&mut g);
    expressions::add_postfix(&mut g);
    expressions::add_unary(&mut g);
    expressions::add_expr_power(&mut g);
    expressions::add_expr_mul(&mut g);
    expressions::add_expr_add(&mut g);
    expressions::add_expr_interval(&mut g);
    expressions::add_expr_compare(&mut g);
    expressions::add_expr_equality(&mut g);
    expressions::add_expr_instanceof(&mut g);
    expressions::add_expr_and(&mut g);
    expressions::add_expr_or(&mut g);
    expressions::add_expr_xor(&mut g);
    expressions::add_expr_ternary(&mut g);
    expressions::add_type_params(&mut g);
    expressions::add_type_primary(&mut g);
    expressions::add_type_optional(&mut g);
    expressions::add_type_expr(&mut g);
    expressions::add_expr_as(&mut g);
    expressions::add_expr(&mut g);

    statements::add_var_decl(&mut g);
    statements::add_global_decl(&mut g);
    statements::add_const_decl(&mut g);
    statements::add_let_decl(&mut g);
    statements::add_param(&mut g);
    statements::add_if_stmt(&mut g);
    statements::add_while_stmt(&mut g);
    statements::add_return_stmt(&mut g);
    statements::add_break_stmt(&mut g);
    statements::add_continue_stmt(&mut g);
    statements::add_expr_stmt(&mut g);
    statements::add_include_stmt(&mut g);
    statements::add_function_decl(&mut g);
    statements::add_class_method(&mut g);
    statements::add_constructor_decl(&mut g);
    statements::add_class_field(&mut g);
    statements::add_class_decl(&mut g);
    statements::add_for_init(&mut g);
    statements::add_for_stmt(&mut g);
    statements::add_for_in_stmt(&mut g);
    statements::add_do_while_stmt(&mut g);
    statements::add_statement(&mut g);
    statements::add_program(&mut g);

    g.finish().expect("program grammar must be valid")
}
