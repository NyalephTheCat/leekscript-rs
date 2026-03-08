//! Operator and bracket lexer rules.
//!
//! Token grammar: single `operator`, `arrow`, `dot_dot`, `dot`, `bracket` rules.
//! Program grammar: separate rules per operator and bracket for expression parsing.

use crate::syntax::Kind;

// ─── Token grammar (Phase 1): longest-first operators and brackets ───────────

/// All operators as one rule (longest first); arrows and .. before .
pub fn add_operator(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("operator", |g: &mut sipha::builder::GrammarBuilder| {
        g.token(Kind::TokOp, |g: &mut sipha::builder::GrammarBuilder| {
            g.choices(vec![
                Box::new(|g| {
                    g.literal(b"<=>");
                }),
                Box::new(|g| {
                    g.literal(b"===");
                }),
                Box::new(|g| {
                    g.literal(b"!==");
                }),
                Box::new(|g| {
                    g.literal(b"**=");
                }),
                Box::new(|g| {
                    g.literal(b"<<<=");
                }),
                Box::new(|g| {
                    g.literal(b">>>=");
                }),
                Box::new(|g| {
                    g.literal(b"<<=");
                }),
                Box::new(|g| {
                    g.literal(b"<<<");
                }),
                Box::new(|g| {
                    g.literal(b"<<");
                }),
                Box::new(|g| {
                    g.literal(b"&&");
                }),
                Box::new(|g| {
                    g.literal(b"&=");
                }),
                Box::new(|g| {
                    g.literal(b"||");
                }),
                Box::new(|g| {
                    g.literal(b"|=");
                }),
                Box::new(|g| {
                    g.literal(b"++");
                }),
                Box::new(|g| {
                    g.literal(b"+=");
                }),
                Box::new(|g| {
                    g.literal(b"--");
                }),
                Box::new(|g| {
                    g.literal(b"-=");
                }),
                Box::new(|g| {
                    g.literal(b"**");
                }),
                Box::new(|g| {
                    g.literal(b"*=");
                }),
                Box::new(|g| {
                    g.literal(b"/=");
                }),
                Box::new(|g| {
                    g.literal(b"\\=");
                }),
                Box::new(|g| {
                    g.literal(b"%=");
                }),
                Box::new(|g| {
                    g.byte(b'\\');
                }),
                Box::new(|g| {
                    g.literal(b"==");
                }),
                Box::new(|g| {
                    g.literal(b"!=");
                }),
                Box::new(|g| {
                    g.literal(b"<=");
                }),
                Box::new(|g| {
                    g.literal(b">=");
                }),
                Box::new(|g| {
                    g.literal(b"^=");
                }),
                Box::new(|g| {
                    g.literal(b":");
                }),
                Box::new(|g| {
                    g.byte(b'&');
                }),
                Box::new(|g| {
                    g.byte(b'|');
                }),
                Box::new(|g| {
                    g.byte(b'+');
                }),
                Box::new(|g| {
                    g.byte(b'-');
                }),
                Box::new(|g| {
                    g.byte(b'*');
                }),
                Box::new(|g| {
                    g.byte(b'/');
                }),
                Box::new(|g| {
                    g.byte(b'%');
                }),
                Box::new(|g| {
                    g.byte(b'=');
                }),
                Box::new(|g| {
                    g.byte(b'!');
                }),
                Box::new(|g| {
                    g.byte(b'<');
                }),
                Box::new(|g| {
                    g.byte(b'>');
                }),
                Box::new(|g| {
                    g.byte(b'^');
                }),
                Box::new(|g| {
                    g.byte(b'~');
                }),
                Box::new(|g| {
                    g.byte(b'@');
                }),
                Box::new(|g| {
                    g.byte(b'?');
                }),
            ]);
        });
    });
}

/// Arrow token for return types and function types: `->` or `=>`.
/// Must be registered before single-char operators (e.g. op_minus, op_assign, op_gt)
/// so that `->` and `=>` are tokenized as one token, not as `-`+`>` or `=`+`>`.
pub fn add_arrow(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("arrow", |g| {
        g.token(Kind::TokArrow, |g| {
            g.choice(
                |g| {
                    g.literal(b"=>");
                },
                |g| {
                    g.literal(b"->");
                },
            );
        });
    });
}

pub fn add_dot_dot(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("dot_dot", |g| {
        g.token(Kind::TokDotDot, |g| {
            g.literal(b"..");
        });
    });
}

pub fn add_dot(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("dot", |g| {
        g.token(Kind::TokDot, |g| {
            g.byte(b'.');
        });
    });
}

pub fn add_bracket(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("bracket", |g| {
        g.choices(vec![
            Box::new(|g| {
                g.token(Kind::TokParenL, |g| {
                    g.byte(b'(');
                });
            }),
            Box::new(|g| {
                g.token(Kind::TokParenR, |g| {
                    g.byte(b')');
                });
            }),
            Box::new(|g| {
                g.token(Kind::TokBracketL, |g| {
                    g.byte(b'[');
                });
            }),
            Box::new(|g| {
                g.token(Kind::TokBracketR, |g| {
                    g.byte(b']');
                });
            }),
            Box::new(|g| {
                g.token(Kind::TokBraceL, |g| {
                    g.byte(b'{');
                });
            }),
            Box::new(|g| {
                g.token(Kind::TokBraceR, |g| {
                    g.byte(b'}');
                });
            }),
            Box::new(|g| {
                g.token(Kind::TokComma, |g| {
                    g.byte(b',');
                });
            }),
            Box::new(|g| {
                g.token(Kind::TokSemi, |g| {
                    g.byte(b';');
                });
            }),
            Box::new(|g| {
                g.token(Kind::TokColon, |g| {
                    g.byte(b':');
                });
            }),
        ]);
    });
}

// ─── Expression grammar: just parens ───────────────────────────────────────────

/// Left and right parens only (for expression grammar).
pub fn add_lparen_rparen(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("lparen", |g| {
        g.token(Kind::TokParenL, |g| {
            g.byte(b'(');
        });
    });
    g.lexer_rule("rparen", |g| {
        g.token(Kind::TokParenR, |g| {
            g.byte(b')');
        });
    });
}

// ─── Program grammar: separate rules for expression parsing ───────────────────

/// Single brackets and punctuation used by program/expression grammar.
/// Note: `dot_dot` ("..") must be defined before dot (".") so interval parses correctly.
pub fn add_program_brackets(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("lparen", |g| {
        g.token(Kind::TokParenL, |g| {
            g.byte(b'(');
        });
    });
    g.lexer_rule("rparen", |g| {
        g.token(Kind::TokParenR, |g| {
            g.byte(b')');
        });
    });
    g.lexer_rule("lbracket", |g| {
        g.token(Kind::TokBracketL, |g| {
            g.byte(b'[');
        });
    });
    g.lexer_rule("rbracket", |g| {
        g.token(Kind::TokBracketR, |g| {
            g.byte(b']');
        });
    });
    g.lexer_rule("lbrace", |g| {
        g.token(Kind::TokBraceL, |g| {
            g.byte(b'{');
        });
    });
    g.lexer_rule("rbrace", |g| {
        g.token(Kind::TokBraceR, |g| {
            g.byte(b'}');
        });
    });
    g.lexer_rule("comma", |g| {
        g.token(Kind::TokComma, |g| {
            g.byte(b',');
        });
    });
    g.lexer_rule("semicolon", |g| {
        g.token(Kind::TokSemi, |g| {
            g.byte(b';');
        });
    });
    g.lexer_rule("dot_dot", |g| {
        g.token(Kind::TokDotDot, |g| {
            g.literal(b"..");
        });
    });
    g.lexer_rule("dot", |g| {
        g.token(Kind::TokDot, |g| {
            g.byte(b'.');
        });
    });
}

/// Assignment and binary operators for program grammar.
pub fn add_program_operators(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("op_assign", |g| {
        g.token(Kind::TokOp, |g| {
            g.choices(vec![
                Box::new(|g| {
                    g.literal(b"=");
                }),
                Box::new(|g| {
                    g.literal(b"+=");
                }),
                Box::new(|g| {
                    g.literal(b"-=");
                }),
                Box::new(|g| {
                    g.literal(b"*=");
                }),
                Box::new(|g| {
                    g.literal(b"/=");
                }),
                Box::new(|g| {
                    g.literal(b"\\=");
                }),
                Box::new(|g| {
                    g.literal(b"%=");
                }),
            ]);
        });
    });
    g.lexer_rule("op_plus", |g| {
        g.token(Kind::TokOp, |g| {
            g.byte(b'+');
        });
    });
    g.lexer_rule("op_minus", |g| {
        g.token(Kind::TokOp, |g| {
            g.byte(b'-');
        });
    });
    g.lexer_rule("op_star", |g| {
        g.token(Kind::TokOp, |g| {
            g.byte(b'*');
        });
    });
    g.lexer_rule("op_slash", |g| {
        g.token(Kind::TokOp, |g| {
            g.byte(b'/');
        });
    });
    g.lexer_rule("op_backslash", |g| {
        g.token(Kind::TokOp, |g| {
            g.byte(b'\\');
        });
    });
    g.lexer_rule("op_percent", |g| {
        g.token(Kind::TokOp, |g| {
            g.byte(b'%');
        });
    });
    g.lexer_rule("op_strict_eq", |g| {
        g.token(Kind::TokOp, |g| {
            g.literal(b"===");
        });
    });
    // Matches "!==" or "!=" (longer first via optional third '=') so PEG doesn't consume "!=" before trying "!==".
    g.lexer_rule("op_neq_or_strict", |g| {
        g.token(Kind::TokOp, |g| {
            g.literal(b"!=");
            g.optional(|g| {
                g.byte(b'=');
            });
        });
    });
    g.lexer_rule("op_strict_neq", |g| {
        g.token(Kind::TokOp, |g| {
            g.literal(b"!==");
        });
    });
    g.lexer_rule("op_eq", |g| {
        g.token(Kind::TokOp, |g| {
            g.literal(b"==");
        });
    });
    g.lexer_rule("op_neq", |g| {
        g.token(Kind::TokOp, |g| {
            g.literal(b"!=");
        });
    });
    g.lexer_rule("op_lt", |g| {
        g.token(Kind::TokOp, |g| {
            g.byte(b'<');
        });
    });
    g.lexer_rule("op_le", |g| {
        g.token(Kind::TokOp, |g| {
            g.literal(b"<=");
        });
    });
    g.lexer_rule("op_gt", |g| {
        g.token(Kind::TokOp, |g| {
            g.byte(b'>');
        });
    });
    g.lexer_rule("op_ge", |g| {
        g.token(Kind::TokOp, |g| {
            g.literal(b">=");
        });
    });
    g.lexer_rule("op_question", |g| {
        g.token(Kind::TokOp, |g| {
            g.byte(b'?');
        });
    });
    g.lexer_rule("op_bang", |g| {
        g.token(Kind::TokOp, |g| {
            g.byte(b'!');
        });
    });
    // Postfix "!" only when not part of "!=" or "!==" (so "!=" is parsed as binary op, not postfix "!" + "=").
    g.lexer_rule("op_bang_postfix", |g| {
        g.token(Kind::TokOp, |g| {
            g.byte(b'!');
            g.neg_lookahead(|g| {
                g.byte(b'=');
            });
        });
    });
    g.lexer_rule("op_at", |g| {
        g.token(Kind::TokOp, |g| {
            g.byte(b'@');
        });
    });
    g.lexer_rule("op_amp", |g| {
        g.token(Kind::TokOp, |g| {
            g.byte(b'&');
        });
    });
    g.lexer_rule("op_pipe", |g| {
        g.token(Kind::TokOp, |g| {
            g.byte(b'|');
        });
    });
    g.lexer_rule("op_amp_amp", |g| {
        g.token(Kind::TokOp, |g| {
            g.literal(b"&&");
        });
    });
    g.lexer_rule("op_pipe_pipe", |g| {
        g.token(Kind::TokOp, |g| {
            g.literal(b"||");
        });
    });
    g.lexer_rule("op_power", |g| {
        g.token(Kind::TokOp, |g| {
            g.literal(b"**");
        });
    });
    g.lexer_rule("op_plus_plus", |g| {
        g.token(Kind::TokOp, |g| {
            g.literal(b"++");
        });
    });
    g.lexer_rule("op_minus_minus", |g| {
        g.token(Kind::TokOp, |g| {
            g.literal(b"--");
        });
    });
}

pub fn add_op_colon(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("op_colon", |g| {
        g.token(Kind::TokColon, |g| {
            g.byte(b':');
        });
    });
}
