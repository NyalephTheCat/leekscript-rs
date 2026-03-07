//! Phase 1: token and `token_stream` parser rules.

use crate::syntax::Kind;

/// One token (any lexer token).
pub fn add_token_rule(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("token", |g: &mut sipha::builder::GrammarBuilder| {
        g.choices(vec![
            Box::new(|g| { g.call("string_lit"); }),
            Box::new(|g| { g.call("number_lit"); }),
            Box::new(|g| { g.call("arrow"); }),
            Box::new(|g| { g.call("dot_dot"); }),
            Box::new(|g| { g.call("dot"); }),
            Box::new(|g| { g.call("operator"); }),
            Box::new(|g| { g.call("bracket"); }),
            Box::new(|g| { g.call("special_lit"); }),
            Box::new(|g| { g.call("keyword_or_ident"); }),
        ]);
    });
}

/// Root: token stream (zero or more tokens).
pub fn add_token_stream(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("token_stream", |g| {
        g.node(Kind::NodeTokenStream, |g| {
            g.zero_or_more(|g| {
                g.call("token");
            });
        });
    });
}
