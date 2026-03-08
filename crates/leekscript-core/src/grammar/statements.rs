//! Statement and program parser rules.

use crate::syntax::Kind;

/// Sync rule for error recovery: matches `;`, `}`, or a statement-start keyword.
/// Used by [`recover_until`] so that after a failed statement we skip until the next safe point.
pub fn add_statement_sync(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule(
        "statement_sync",
        |g: &mut sipha::builder::GrammarBuilder| {
            g.skip();
            g.choices(vec![
                Box::new(|g| {
                    g.call("semicolon");
                }),
                Box::new(|g| {
                    g.call("rbrace");
                }),
                Box::new(|g| {
                    g.keyword(Kind::KwVar, b"var");
                }),
                Box::new(|g| {
                    g.keyword(Kind::KwReturn, b"return");
                }),
                Box::new(|g| {
                    g.keyword(Kind::KwIf, b"if");
                }),
                Box::new(|g| {
                    g.keyword(Kind::KwWhile, b"while");
                }),
                Box::new(|g| {
                    g.keyword(Kind::KwFunction, b"function");
                }),
                Box::new(|g| {
                    g.keyword(Kind::KwClass, b"class");
                }),
                Box::new(|g| {
                    g.keyword(Kind::KwFor, b"for");
                }),
                Box::new(|g| {
                    g.keyword(Kind::KwDo, b"do");
                }),
                Box::new(|g| {
                    g.keyword(Kind::KwBreak, b"break");
                }),
                Box::new(|g| {
                    g.keyword(Kind::KwContinue, b"continue");
                }),
                Box::new(|g| {
                    g.keyword(Kind::KwGlobal, b"global");
                }),
                Box::new(|g| {
                    g.keyword(Kind::KwConst, b"const");
                }),
                Box::new(|g| {
                    g.keyword(Kind::KwLet, b"let");
                }),
                Box::new(|g| {
                    g.keyword(Kind::KwInclude, b"include");
                }),
            ]);
        },
    );
}

pub fn add_block(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("block", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeBlock, |g| {
            g.call("lbrace");
            g.zero_or_more(|g| {
                g.call("statement");
            });
            g.call("rbrace");
        });
    });
}

/// Var decl: `var x = 10` (untyped) or `integer x = 10` (type replaces var — no "var" keyword).
pub fn add_var_decl(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("var_decl", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeVarDecl, |g| {
            g.choices(vec![
                Box::new(|g| {
                    g.keyword(Kind::KwVar, b"var");
                    g.call("keyword_or_ident");
                    g.optional(|g| {
                        g.call("op_assign");
                        g.call("expr");
                    });
                }),
                Box::new(|g| {
                    g.call("type_expr");
                    g.call("keyword_or_ident");
                    g.optional(|g| {
                        g.call("op_assign");
                        g.call("expr");
                    });
                }),
            ]);
            g.optional(|g| {
                g.call("semicolon");
            });
        });
    });
}

/// Global decl: `global integer x = 10` or `global integer x` — try with initializer first, then one optional semicolon.
pub fn add_global_decl(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("global_decl", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeVarDecl, |g| {
            g.keyword(Kind::KwGlobal, b"global");
            g.optional(|g| {
                g.call("type_expr");
            });
            g.call("keyword_or_ident");
            g.choices(vec![
                Box::new(|g| {
                    g.call("op_assign");
                    g.call("expr_as");
                }),
                Box::new(|_g| {}),
            ]);
            g.optional(|g| {
                g.call("semicolon");
            });
        });
    });
}

/// Const decl: `const name = expr` (no typed form).
pub fn add_const_decl(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("const_decl", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeVarDecl, |g| {
            g.keyword(Kind::KwConst, b"const");
            g.call("keyword_or_ident");
            g.optional(|g| {
                g.call("op_assign");
                g.call("expr");
            });
            g.optional(|g| {
                g.call("semicolon");
            });
        });
    });
}

/// Let decl: `let name = expr` (no typed form).
pub fn add_let_decl(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("let_decl", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeVarDecl, |g| {
            g.keyword(Kind::KwLet, b"let");
            g.call("keyword_or_ident");
            g.optional(|g| {
                g.call("op_assign");
                g.call("expr");
            });
            g.optional(|g| {
                g.call("semicolon");
            });
        });
    });
}

pub fn add_if_stmt(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("if_stmt", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeIfStmt, |g| {
            g.keyword(Kind::KwIf, b"if");
            g.call("lparen");
            g.call("expr");
            g.call("rparen");
            g.call("statement");
            g.optional(|g| {
                g.keyword(Kind::KwElse, b"else");
                g.call("statement");
            });
        });
    });
}

pub fn add_while_stmt(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("while_stmt", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeWhileStmt, |g| {
            g.keyword(Kind::KwWhile, b"while");
            g.call("lparen");
            g.call("expr");
            g.call("rparen");
            g.call("statement");
        });
    });
}

pub fn add_return_stmt(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("return_stmt", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeReturnStmt, |g| {
            g.keyword(Kind::KwReturn, b"return");
            g.optional(|g| {
                g.call("expr");
            });
            g.optional(|g| {
                g.call("semicolon");
            });
        });
    });
}

pub fn add_break_stmt(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("break_stmt", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeBreakStmt, |g| {
            g.keyword(Kind::KwBreak, b"break");
            g.optional(|g| {
                g.call("semicolon");
            });
        });
    });
}

pub fn add_continue_stmt(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("continue_stmt", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeContinueStmt, |g| {
            g.keyword(Kind::KwContinue, b"continue");
            g.optional(|g| {
                g.call("semicolon");
            });
        });
    });
}

pub fn add_expr_stmt(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("expr_stmt", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeExprStmt, |g| {
            g.call("expr");
            g.optional(|g| {
                g.call("semicolon");
            });
        });
    });
}

pub fn add_include_stmt(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("include_stmt", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeInclude, |g| {
            g.keyword(Kind::KwInclude, b"include");
            g.call("lparen");
            g.call("string_lit");
            g.call("rparen");
            g.optional(|g| {
                g.call("semicolon");
            });
        });
    });
}

/// Param: `integer x` (type then name) or untyped `x`; optional @ and default.
pub fn add_param(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("param", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeParam, |g| {
            g.optional(|g| {
                g.call("op_at");
            });
            g.choices(vec![
                Box::new(|g| {
                    g.call("type_expr");
                    g.call("keyword_or_ident");
                }),
                Box::new(|g| {
                    g.call("keyword_or_ident");
                }),
            ]);
            g.optional(|g| {
                g.call("op_assign");
                g.call("expr");
            });
        });
    });
}

/// Top-level function: return type at end after arrow — `function a() -> integer {}`.
pub fn add_function_decl(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("function_decl", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeFunctionDecl, |g| {
            g.keyword(Kind::KwFunction, b"function");
            g.call("keyword_or_ident");
            g.call("lparen");
            g.optional(|g| {
                g.call("param");
                g.zero_or_more(|g| {
                    g.call("comma");
                    g.call("param");
                });
            });
            g.call("rparen");
            g.optional(|g| {
                g.call("arrow");
                g.call("type_expr");
            });
            g.call("block");
        });
    });
}

pub fn add_constructor_decl(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule(
        "constructor_decl",
        |g: &mut sipha::builder::GrammarBuilder| {
            g.node(Kind::NodeConstructorDecl, |g| {
                g.call("constructor_kw");
                g.call("lparen");
                g.optional(|g| {
                    g.call("param");
                    g.zero_or_more(|g| {
                        g.call("comma");
                        g.call("param");
                    });
                });
                g.call("rparen");
                g.call("block");
            });
        },
    );
}

/// Class method: optional return type at start — `public static integer a()` or `public update()` (no "function" keyword).
pub fn add_class_method(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("class_method", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeFunctionDecl, |g| {
            g.choices(vec![
                Box::new(|g| {
                    g.call("type_expr");
                    g.call("keyword_or_ident");
                }),
                Box::new(|g| {
                    g.call("keyword_or_ident");
                }),
            ]);
            g.call("lparen");
            g.optional(|g| {
                g.call("param");
                g.zero_or_more(|g| {
                    g.call("comma");
                    g.call("param");
                });
            });
            g.call("rparen");
            g.call("block");
        });
    });
}

/// Class field: [static] [final] ( `type_expr` `keyword_or_ident` | `keyword_or_ident` ) [= expr] ;
/// Matches `LeekScript` Java endClassMember: optional type then name (method vs field by ( vs =).
pub fn add_class_field(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("class_field", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeClassField, |g| {
            g.optional(|g| {
                g.call("static_kw");
            });
            g.optional(|g| {
                g.call("final_kw");
            });
            g.choices(vec![
                Box::new(|g| {
                    g.call("type_expr");
                    g.call("keyword_or_ident");
                }),
                Box::new(|g| {
                    g.call("keyword_or_ident");
                }),
            ]);
            g.optional(|g| {
                g.call("op_assign");
                g.call("expr");
            });
            g.optional(|g| {
                g.call("semicolon");
            });
        });
    });
}

pub fn add_class_decl(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("class_decl", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeClassDecl, |g| {
            g.keyword(Kind::KwClass, b"class");
            g.call("keyword_or_ident");
            g.optional(|g| {
                g.call("extends_kw");
                g.call("keyword_or_ident");
            });
            g.call("lbrace");
            g.zero_or_more(|g| {
                g.optional(|g| {
                    g.choices(vec![
                        Box::new(|g| {
                            g.call("public_kw");
                        }),
                        Box::new(|g| {
                            g.call("private_kw");
                        }),
                        Box::new(|g| {
                            g.call("protected_kw");
                        }),
                    ]);
                });
                g.choices(vec![
                    Box::new(|g| {
                        g.optional(|g| {
                            g.call("static_kw");
                        });
                        // g.optional(|g| { g.call("abstract_kw"); });
                        g.call("class_method");
                    }),
                    Box::new(|g| {
                        g.call("constructor_decl");
                    }),
                    Box::new(|g| {
                        g.call("class_field");
                    }),
                ]);
            });
            g.call("rbrace");
        });
    });
}

/// For init: `var i = 0` | `let i = 0` | `integer i = 0` (type replaces var) | expr.
pub fn add_for_init(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("for_init", |g: &mut sipha::builder::GrammarBuilder| {
        g.choices(vec![
            Box::new(|g| {
                g.keyword(Kind::KwVar, b"var");
                g.call("keyword_or_ident");
                g.optional(|g| {
                    g.call("op_assign");
                    g.call("expr");
                });
            }),
            Box::new(|g| {
                g.keyword(Kind::KwLet, b"let");
                g.call("keyword_or_ident");
                g.optional(|g| {
                    g.call("op_assign");
                    g.call("expr");
                });
            }),
            Box::new(|g| {
                g.call("type_expr");
                g.call("keyword_or_ident");
                g.optional(|g| {
                    g.call("op_assign");
                    g.call("expr");
                });
            }),
            Box::new(|g| {
                g.call("expr");
            }),
        ]);
    });
}

pub fn add_for_stmt(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("for_stmt", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeForStmt, |g| {
            g.keyword(Kind::KwFor, b"for");
            g.call("lparen");
            g.optional(|g| {
                g.call("for_init");
            });
            g.optional(|g| {
                g.call("semicolon");
            });
            g.optional(|g| {
                g.call("expr");
            });
            g.optional(|g| {
                g.call("semicolon");
            });
            g.optional(|g| {
                g.call("expr");
            });
            g.call("rparen");
            g.call("statement");
        });
    });
}

/// For-in: for ( [`type_expr`] [var] key [ : [`type_expr`] [var] valueVar ] in expr ) statement.
/// Matches `LeekScript` Java: optional type then key, or key : optional type then value name.
pub fn add_for_in_stmt(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("for_in_stmt", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeForInStmt, |g| {
            g.keyword(Kind::KwFor, b"for");
            g.call("lparen");
            g.optional(|g| {
                g.call("type_expr");
            });
            g.optional(|g| {
                g.keyword(Kind::KwVar, b"var");
            });
            g.call("keyword_or_ident"); // key
            g.optional(|g| {
                g.call("op_colon");
                g.optional(|g| {
                    g.call("type_expr");
                });
                g.optional(|g| {
                    g.keyword(Kind::KwVar, b"var");
                });
                g.call("keyword_or_ident"); // value variable name
            });
            g.keyword(Kind::KwIn, b"in");
            g.call("expr");
            g.call("rparen");
            g.call("statement");
        });
    });
}

pub fn add_do_while_stmt(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("do_while_stmt", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeDoWhileStmt, |g| {
            g.keyword(Kind::KwDo, b"do");
            g.call("block");
            g.keyword(Kind::KwWhile, b"while");
            g.call("lparen");
            g.call("expr");
            g.call("rparen");
            g.optional(|g| {
                g.call("semicolon");
            });
        });
    });
}

pub fn add_statement(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("statement", |g: &mut sipha::builder::GrammarBuilder| {
        g.choices(vec![
            Box::new(|g| {
                g.call("return_stmt");
            }),
            Box::new(|g| {
                g.call("break_stmt");
            }),
            Box::new(|g| {
                g.call("continue_stmt");
            }),
            Box::new(|g| {
                g.call("include_stmt");
            }),
            Box::new(|g| {
                g.call("function_decl");
            }),
            Box::new(|g| {
                g.call("class_decl");
            }),
            Box::new(|g| {
                g.call("for_in_stmt");
            }),
            Box::new(|g| {
                g.call("for_stmt");
            }),
            Box::new(|g| {
                g.call("do_while_stmt");
            }),
            Box::new(|g| {
                g.call("block");
            }),
            Box::new(|g| {
                g.call("global_decl");
            }),
            Box::new(|g| {
                g.call("const_decl");
            }),
            Box::new(|g| {
                g.call("let_decl");
            }),
            Box::new(|g| {
                g.call("var_decl");
            }),
            Box::new(|g| {
                g.call("if_stmt");
            }),
            Box::new(|g| {
                g.call("while_stmt");
            }),
            Box::new(|g| {
                g.call("expr_stmt");
            }),
        ]);
    });
}

pub fn add_program(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("program", |g: &mut sipha::builder::GrammarBuilder| {
        g.zero_or_more(|g| {
            g.recover_until("statement_sync", |g| {
                g.call("statement");
            });
        });
    });
}
