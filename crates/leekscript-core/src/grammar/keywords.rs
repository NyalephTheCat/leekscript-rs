//! Keyword and identifier lexer rules (longest keyword first).
//!
//! Keywords use a word boundary (`neg_lookahead` `IDENT_CONT`) so that e.g. "initial"
//! is parsed as one identifier, not "in" + "itial".

use sipha::types::classes;

use crate::syntax::Kind;

/// Match a keyword only when not followed by an identifier character ([a-zA-Z0-9_]).
fn kw(g: &mut sipha::builder::GrammarBuilder, kind: Kind, lit: &[u8]) {
    g.token(kind, |g| {
        g.literal(lit);
        g.neg_lookahead(|g| {
            g.class(classes::IDENT_CONT);
        });
    });
}

/// Lexer rule that matches only "and" (for expression grammar; same as &&).
pub fn add_and_kw(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("and_kw", |g| {
        kw(g, Kind::KwAnd, b"and");
    });
}

/// Lexer rule that matches only "or" (for expression grammar; same as ||).
pub fn add_or_kw(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("or_kw", |g| {
        kw(g, Kind::KwOr, b"or");
    });
}

/// Lexer rule that matches only "xor" (for expression grammar).
pub fn add_xor_kw(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("xor_kw", |g| {
        kw(g, Kind::KwXor, b"xor");
    });
}

/// Lexer rule that matches only "abstract" (for `class_decl`).
pub fn add_abstract_kw(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("abstract_kw", |g| {
        kw(g, Kind::KwAbstract, b"abstract");
    });
}

/// Lexer rule that matches only "constructor" (for `class_decl`).
pub fn add_constructor_kw(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("constructor_kw", |g| {
        kw(g, Kind::KwConstructor, b"constructor");
    });
}

/// Lexer rule that matches only "extends" (for `class_decl`).
pub fn add_extends_kw(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("extends_kw", |g| {
        kw(g, Kind::KwExtends, b"extends");
    });
}

/// Lexer rule that matches only "static" (for class methods).
pub fn add_static_kw(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("static_kw", |g| {
        kw(g, Kind::KwStatic, b"static");
    });
}

/// Lexer rule that matches only "final" (for class fields).
pub fn add_final_kw(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("final_kw", |g| {
        kw(g, Kind::KwFinal, b"final");
    });
}

/// Lexer rule that matches only "public" (for class members).
pub fn add_public_kw(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("public_kw", |g| {
        kw(g, Kind::KwPublic, b"public");
    });
}

/// Lexer rule that matches only "private" (for class members).
pub fn add_private_kw(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("private_kw", |g| {
        kw(g, Kind::KwPrivate, b"private");
    });
}

/// Lexer rule that matches only "protected" (for class members).
pub fn add_protected_kw(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("protected_kw", |g| {
        kw(g, Kind::KwProtected, b"protected");
    });
}

/// Lexer rule that matches only "not" (unary, like !).
pub fn add_not_kw(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("not_kw", |g| {
        kw(g, Kind::KwNot, b"not");
    });
}

/// Lexer rule that matches only "as" (cast: expr as Type).
pub fn add_as_kw(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("as_kw", |g| {
        kw(g, Kind::KwAs, b"as");
    });
}

/// Lexer rule that matches only "in" (binary operator; word boundary so "instanceof" is not split).
pub fn add_in_kw(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("in_kw", |g| {
        kw(g, Kind::KwIn, b"in");
    });
}

/// Lexer rule that matches only "instanceof" (binary operator).
pub fn add_instanceof_kw(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("instanceof_kw", |g| {
        kw(g, Kind::KwInstanceof, b"instanceof");
    });
}

/// Keywords used in Phase 1 token grammar (no class, new).
const TOKEN_KEYWORDS: &[(Kind, &[u8])] = &[
    (Kind::KwFunction, b"function"),
    (Kind::KwReturn, b"return"),
    (Kind::KwGlobal, b"global"),
    (Kind::KwFalse, b"false"),
    (Kind::KwWhile, b"while"),
    (Kind::KwTrue, b"true"),
    (Kind::KwNull, b"null"),
    (Kind::KwVar, b"var"),
    (Kind::KwFor, b"for"),
    (Kind::KwIf, b"if"),
    (Kind::KwElse, b"else"),
    (Kind::KwLet, b"let"),
    (Kind::KwAnd, b"and"),
    (Kind::KwOr, b"or"),
    (Kind::KwXor, b"xor"),
    (Kind::KwConst, b"const"),
    (Kind::KwInclude, b"include"),
    (Kind::KwIn, b"in"),
    (Kind::KwInstanceof, b"instanceof"),
    (Kind::KwAs, b"as"),
    (Kind::KwNot, b"not"),
    (Kind::KwBreak, b"break"),
    (Kind::KwContinue, b"continue"),
    (Kind::KwDo, b"do"),
];

/// Extra keywords and reserved words for program grammar only.
const PROGRAM_EXTRA_KEYWORDS: &[(Kind, &[u8])] = &[
    (Kind::KwClass, b"class"),
    (Kind::KwNew, b"new"),
    (Kind::KwAbstract, b"abstract"),
    (Kind::KwFinal, b"final"),
    (Kind::KwConstructor, b"constructor"),
    (Kind::KwExtends, b"extends"),
    (Kind::KwStatic, b"static"),
    (Kind::KwPublic, b"public"),
    (Kind::KwPrivate, b"private"),
    (Kind::KwProtected, b"protected"),
    (Kind::KwThis, b"this"),
    (Kind::KwSuper, b"super"),
    (Kind::KwNot, b"not"),
    (Kind::KwAs, b"as"),
    (Kind::KwInstanceof, b"instanceof"),
    (Kind::KwTry, b"try"),
    (Kind::KwCatch, b"catch"),
    (Kind::KwSwitch, b"switch"),
    (Kind::KwCase, b"case"),
    (Kind::KwDefault, b"default"),
    (Kind::KwThrow, b"throw"),
    (Kind::KwReserved, b"await"),
    (Kind::KwReserved, b"export"),
    (Kind::KwReserved, b"import"),
    (Kind::KwReserved, b"goto"),
    (Kind::KwReserved, b"typeof"),
    (Kind::KwReserved, b"void"),
    (Kind::KwReserved, b"with"),
    (Kind::KwReserved, b"yield"),
    (Kind::KwReserved, b"finally"),
];

fn ident_choice(g: &mut sipha::builder::GrammarBuilder) {
    g.token(Kind::TokIdent, |g| {
        g.class(classes::IDENT_START);
        g.zero_or_more(|g| {
            g.class(classes::IDENT_CONT);
        });
    });
}

#[allow(clippy::type_complexity)]
fn keyword_choices(
    keywords: &[(Kind, &[u8])],
) -> Vec<Box<dyn FnOnce(&mut sipha::builder::GrammarBuilder)>> {
    keywords
        .iter()
        .map(|&(kind, lit)| {
            let bytes = lit.to_vec();
            Box::new(move |g: &mut sipha::builder::GrammarBuilder| kw(g, kind, &bytes))
                as Box<dyn FnOnce(&mut sipha::builder::GrammarBuilder)>
        })
        .collect()
}

/// Keywords + identifier for Phase 1 token grammar (no class, new).
pub fn add_keyword_or_ident_token(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule(
        "keyword_or_ident",
        |g: &mut sipha::builder::GrammarBuilder| {
            let mut choices = keyword_choices(TOKEN_KEYWORDS);
            choices.push(Box::new(ident_choice));
            g.choices(choices);
        },
    );
}

/// Keywords + identifier for program grammar (includes class, new, reserved words).
pub fn add_keyword_or_ident_program(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule(
        "keyword_or_ident",
        |g: &mut sipha::builder::GrammarBuilder| {
            let mut choices = keyword_choices(TOKEN_KEYWORDS);
            choices.extend(keyword_choices(PROGRAM_EXTRA_KEYWORDS));
            choices.push(Box::new(ident_choice));
            g.choices(choices);
        },
    );
}

/// Identifier only (no keywords); for expression grammar.
pub fn add_ident(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("ident", |g| {
        g.token(Kind::TokIdent, |g| {
            g.class(classes::IDENT_START);
            g.zero_or_more(|g| {
                g.class(classes::IDENT_CONT);
            });
        });
    });
}
