//! Trivia rules (whitespace and comments).

use sipha::prelude::*;
use sipha::types::classes;

use crate::syntax::Kind;

/// Adds the shared whitespace/comment lexer rule `ws`.
pub fn add_ws(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("ws", |g| {
        g.zero_or_more(|g| {
            g.choices(vec![
                Box::new(|g| {
                    g.trivia(Kind::TriviaWs, |g| {
                        g.class(classes::WHITESPACE);
                    });
                }),
                Box::new(|g| {
                    g.trivia(Kind::TriviaLineComment, |g| {
                        g.literal(b"//");
                        g.zero_or_more(|g| {
                            g.neg_lookahead(|g| {
                                g.byte(b'\n');
                            });
                            g.class(CharClass::ANY);
                        });
                    });
                }),
                Box::new(|g| {
                    g.trivia(Kind::TriviaBlockComment, |g| {
                        g.literal(b"/*");
                        g.zero_or_more(|g| {
                            g.neg_lookahead(|g| {
                                g.literal(b"*/");
                            });
                            g.class(CharClass::ANY);
                        });
                        g.literal(b"*/");
                    });
                }),
            ]);
        });
    });
}
