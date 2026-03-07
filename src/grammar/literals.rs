//! Literal lexer rules: strings, numbers, escape sequences, special (∞, π).

use sipha::prelude::*;
use sipha::types::classes;

use crate::syntax::Kind;

/// String literal (single or double quoted).
pub fn add_string_lit(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("string_lit", |g| {
        g.token(Kind::TokString, |g| {
            g.choice(
                |g| {
                    g.byte(b'"');
                    g.zero_or_more(|g| {
                        g.neg_lookahead(|g| { g.byte(b'"'); });
                        g.choice(
                            |g| { g.call("escape"); },
                            |g| { g.class(CharClass::ANY); },
                        );
                    });
                    g.byte(b'"');
                },
                |g| {
                    g.byte(b'\'');
                    g.zero_or_more(|g| {
                        g.neg_lookahead(|g| { g.byte(b'\''); });
                        g.choice(
                            |g| { g.call("escape"); },
                            |g| { g.class(CharClass::ANY); },
                        );
                    });
                    g.byte(b'\'');
                },
            );
        });
    });
}

/// Escape sequence (used by string_lit).
pub fn add_escape(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("escape", |g| {
        g.byte(b'\\');
        g.choice(
            |g| {
                g.class(
                    CharClass::from_chars(b"\"'\\nrt"),
                );
            },
            |g| {
                g.byte(b'u');
                g.repeat(4.., |g| { g.class(classes::HEX_DIGIT); });
            },
        );
    });
}

/// Number literal without exponent (for expression and program grammars).
pub fn add_number_lit(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("number_lit", |g| {
        g.token(Kind::TokNumber, |g| {
            g.optional(|g| { g.byte(b'-'); });
            g.choice(
                |g| { g.byte(b'0'); },
                |g| {
                    g.byte_range(b'1', b'9');
                    g.zero_or_more(|g| { g.class(classes::DIGIT); });
                },
            );
            g.optional(|g| {
                g.byte(b'.');
                g.choices(vec![
                    Box::new(|g| { g.one_or_more(|g| { g.class(classes::DIGIT); }); }),
                    // Trailing "." (e.g. "1.") but not when followed by "." (so "1..10" parses as 1 .. 10)
                    Box::new(|g| {
                        g.neg_lookahead(|g| { g.class(classes::DIGIT); });
                        g.neg_lookahead(|g| { g.byte(b'.'); });
                    }),
                ]);
            });
        });
    });
}

/// Number literal with optional exponent (for Phase 1 token grammar).
pub fn add_number_lit_full(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("number_lit", |g| {
        g.token(Kind::TokNumber, |g| {
            g.optional(|g| { g.byte(b'-'); });
            g.choice(
                |g| { g.byte(b'0'); },
                |g| {
                    g.byte_range(b'1', b'9');
                    g.zero_or_more(|g| { g.class(classes::DIGIT); });
                },
            );
            g.optional(|g| {
                g.byte(b'.');
                g.choices(vec![
                    Box::new(|g| { g.one_or_more(|g| { g.class(classes::DIGIT); }); }),
                    // Trailing "." but not when followed by "." (so "1..10" parses as range)
                    Box::new(|g| {
                        g.neg_lookahead(|g| { g.class(classes::DIGIT); });
                        g.neg_lookahead(|g| { g.byte(b'.'); });
                    }),
                ]);
            });
            g.optional(|g| {
                g.choice(|g| { g.byte(b'e'); }, |g| { g.byte(b'E'); });
                g.optional(|g| { g.choice(|g| { g.byte(b'+'); }, |g| { g.byte(b'-'); }); });
                g.one_or_more(|g| { g.class(classes::DIGIT); });
            });
        });
    });
}

/// Special tokens ∞ and π (Phase 1 token grammar only).
pub fn add_special_lit(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("special_lit", |g| {
        g.choice(
            |g| { g.token(Kind::TokLemnisate, |g| { g.char('∞'); }); },
            |g| { g.token(Kind::TokPi, |g| { g.char('π'); }); },
        );
    });
}
