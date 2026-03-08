//! Grammar for `LeekScript` **signature files** (stdlib / API declarations).
//!
//! This is a small DSL to declare function, class, and global signatures without
//! `LeekScript` bodies. Used to load standard library (and other) API definitions.
//!
//! ## Format (BNF-style)
//!
//! ```text
//! file           := (function_sig | class_sig | global_sig)*
//!
//! function_sig   := "function" ident "(" param_list ")" ["->" type_expr]
//! param_list     := [param ("," param)*]
//! param          := type_expr ident ["?"]   ("?" = argument can be omitted, not nullable)
//!
//! class_sig      := "class" ident ["extends" ident] "{" class_member* "}"
//! class_member   := method_sig | constructor_sig | field_sig
//! method_sig     := ["static"] ["public"|"private"|"protected"] type_expr ident "(" param_list ")"
//! constructor_sig:= "constructor" "(" param_list ")"
//! field_sig      := ["static"] ["final"] type_expr ident
//!
//! global_sig     := "global" type_expr ident
//!
//! type_expr      := type_optional ("|" type_optional)*
//! type_optional  := type_primary ["?"]
//! type_primary   := ident ["<" type_params ">"]
//! type_params    := "=>" type_expr
//!                  | type_expr ("," type_expr)*
//!                  | type_expr "=>" type_expr
//!                  | type_expr ("," type_expr)+ "=>" type_expr
//!                  | "(" type_expr ("," type_expr)* ")" "=>" type_expr
//! ```
//!
//! Types match `LeekScript`: `integer`, `real`, `string`, `boolean`, `void`, `any`,
//! `Array<T>`, `Map<K,V>`, `Set<T>`, `Function<P1, P2, ... => R>`, `(T1, T2) => R`, `T | U`, `T?`.
//! Shorthand: `Array`, `Map`, `Set` (no params) = `Array<any>`, `Map<any, any>`, `Set<any>`.
//! Function type: `Function< => ret>` (0 params) or `Function<a, b => ret>` (param types, then `=>`, then return type).
//!
//! **Documentation** (optional after each `function` or `global`): Doxygen-style block.
//! - `///` lines: one or more lines starting with `///` (supports @param, @return, @brief, @complexity, etc.).
//! - `/**` ... `*/` block: same Doxygen tags. Use `@complexity N` (1–13) for complexity.

use sipha::types::classes;
use sipha::types::CharClass;

use crate::syntax::Kind;

fn sig_kw(g: &mut sipha::builder::GrammarBuilder, kind: Kind, lit: &[u8]) {
    g.token(kind, |g| {
        g.literal(lit);
        g.neg_lookahead(|g| {
            g.class(classes::IDENT_CONT);
        });
    });
}

fn sig_ident_token(g: &mut sipha::builder::GrammarBuilder) {
    g.token(Kind::TokIdent, |g| {
        g.class(classes::IDENT_START);
        g.zero_or_more(|g| {
            g.class(classes::IDENT_CONT);
        });
    });
}

/// Signature-file trivia: whitespace and `//` line comments (not `///`, which is doc).
fn add_sig_ws(g: &mut sipha::builder::GrammarBuilder) {
    g.lexer_rule("sig_ws", |g| {
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
                        g.neg_lookahead(|g| {
                            g.byte(b'/');
                        });
                        g.zero_or_more(|g| {
                            g.neg_lookahead(|g| {
                                g.byte(b'\n');
                            });
                            g.class(CharClass::ANY);
                        });
                    });
                }),
            ]);
        });
    });
}

/// Add lexer rules for signature file: trivia, keywords, ident, punctuation, doc, complexity.
fn add_sig_lexer(g: &mut sipha::builder::GrammarBuilder) {
    add_sig_ws(g);

    g.lexer_rule("sig_doc_line", |g| {
        g.token(Kind::TokSigDocLine, |g| {
            g.literal(b"///");
            g.zero_or_more(|g| {
                g.neg_lookahead(|g| {
                    g.byte(b'\n');
                });
                g.class(CharClass::ANY);
            });
        });
    });
    g.lexer_rule("sig_doc_block_comment", |g| {
        g.token(Kind::TokSigDocBlock, |g| {
            g.literal(b"/**");
            g.zero_or_more(|g| {
                g.choices(vec![
                    Box::new(|g| {
                        g.neg_lookahead(|g| {
                            g.byte(b'*');
                        });
                        g.class(CharClass::ANY);
                    }),
                    Box::new(|g| {
                        g.byte(b'*');
                        g.neg_lookahead(|g| {
                            g.byte(b'/');
                        });
                    }),
                ]);
            });
            g.literal(b"*/");
        });
    });

    g.lexer_rule("sig_kw_function", |g| {
        sig_kw(g, Kind::KwFunction, b"function");
    });
    g.lexer_rule("sig_kw_class", |g| sig_kw(g, Kind::KwClass, b"class"));
    g.lexer_rule("sig_kw_extends", |g| sig_kw(g, Kind::KwExtends, b"extends"));
    g.lexer_rule("sig_kw_constructor", |g| {
        sig_kw(g, Kind::KwConstructor, b"constructor");
    });
    g.lexer_rule("sig_kw_static", |g| sig_kw(g, Kind::KwStatic, b"static"));
    g.lexer_rule("sig_kw_public", |g| sig_kw(g, Kind::KwPublic, b"public"));
    g.lexer_rule("sig_kw_private", |g| sig_kw(g, Kind::KwPrivate, b"private"));
    g.lexer_rule("sig_kw_protected", |g| {
        sig_kw(g, Kind::KwProtected, b"protected");
    });
    g.lexer_rule("sig_kw_final", |g| sig_kw(g, Kind::KwFinal, b"final"));
    g.lexer_rule("sig_kw_global", |g| sig_kw(g, Kind::KwGlobal, b"global"));

    g.lexer_rule("sig_ident", |g| {
        g.choices(vec![
            Box::new(|g| {
                g.call("sig_kw_function");
            }),
            Box::new(|g| {
                g.call("sig_kw_class");
            }),
            Box::new(|g| {
                g.call("sig_kw_extends");
            }),
            Box::new(|g| {
                g.call("sig_kw_constructor");
            }),
            Box::new(|g| {
                g.call("sig_kw_static");
            }),
            Box::new(|g| {
                g.call("sig_kw_public");
            }),
            Box::new(|g| {
                g.call("sig_kw_private");
            }),
            Box::new(|g| {
                g.call("sig_kw_protected");
            }),
            Box::new(|g| {
                g.call("sig_kw_final");
            }),
            Box::new(|g| {
                g.call("sig_kw_global");
            }),
            Box::new(sig_ident_token),
        ]);
    });

    g.lexer_rule("sig_lparen", |g| {
        g.token(Kind::TokParenL, |g| {
            g.byte(b'(');
        });
    });
    g.lexer_rule("sig_rparen", |g| {
        g.token(Kind::TokParenR, |g| {
            g.byte(b')');
        });
    });
    g.lexer_rule("sig_lbrace", |g| {
        g.token(Kind::TokBraceL, |g| {
            g.byte(b'{');
        });
    });
    g.lexer_rule("sig_rbrace", |g| {
        g.token(Kind::TokBraceR, |g| {
            g.byte(b'}');
        });
    });
    g.lexer_rule("sig_comma", |g| {
        g.token(Kind::TokComma, |g| {
            g.byte(b',');
        });
    });
    g.lexer_rule("sig_colon", |g| {
        g.token(Kind::TokColon, |g| {
            g.byte(b':');
        });
    });
    g.lexer_rule("sig_arrow", |g| {
        g.choices(vec![
            Box::new(|g| {
                g.token(Kind::TokArrow, |g| {
                    g.literal(b"->");
                });
            }),
            Box::new(|g| {
                g.token(Kind::TokArrow, |g| {
                    g.literal(b"=>");
                });
            }),
        ]);
    });
    g.lexer_rule("sig_lt", |g| {
        g.token(Kind::TokOp, |g| {
            g.byte(b'<');
        });
    });
    g.lexer_rule("sig_gt", |g| {
        g.token(Kind::TokOp, |g| {
            g.byte(b'>');
        });
    });
    g.lexer_rule("sig_question", |g| {
        g.token(Kind::TokOp, |g| {
            g.byte(b'?');
        });
    });
    g.lexer_rule("sig_pipe", |g| {
        g.token(Kind::TokOp, |g| {
            g.byte(b'|');
        });
    });
}

/// Type params: `< => R >` | `< T >` | `< K , V >` | `< T => R >` | `< T , V , ... => R >`
fn add_sig_type_params(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("sig_type_params", |g| {
        g.node(Kind::NodeTypeParams, |g| {
            g.choices(vec![
                // Zero params: => ret>
                Box::new(|g| {
                    g.call("sig_arrow");
                    g.call("sig_type_expr");
                    g.no_skip(|g| {
                        g.call("sig_gt");
                    });
                }),
                // One or more params (then optional => return)
                Box::new(|g| {
                    g.call("sig_type_expr");
                    g.choices(vec![
                        Box::new(|g| {
                            g.no_skip(|g| {
                                g.call("sig_gt");
                            });
                        }),
                        Box::new(|g| {
                            g.call("sig_comma");
                            g.call("sig_type_expr");
                            g.no_skip(|g| {
                                g.call("sig_gt");
                            });
                        }),
                        Box::new(|g| {
                            g.call("sig_arrow");
                            g.call("sig_type_expr");
                            g.no_skip(|g| {
                                g.call("sig_gt");
                            });
                        }),
                        Box::new(|g| {
                            g.call("sig_comma");
                            g.call("sig_type_expr");
                            g.zero_or_more(|g| {
                                g.call("sig_comma");
                                g.call("sig_type_expr");
                            });
                            g.call("sig_arrow");
                            g.call("sig_type_expr");
                            g.no_skip(|g| {
                                g.call("sig_gt");
                            });
                        }),
                    ]);
                }),
            ]);
        });
    });
}

/// `type_primary`: ident or ident `<` `type_params` `>`
fn add_sig_type_primary(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("sig_type_primary", |g| {
        g.call("sig_ident");
        g.optional(|g| {
            g.call("sig_lt");
            g.call("sig_type_params");
        });
    });
}

/// `type_optional`: `type_primary` `?`?
fn add_sig_type_optional(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("sig_type_optional", |g| {
        g.call("sig_type_primary");
        g.optional(|g| {
            g.call("sig_question");
        });
    });
}

/// `type_expr`: `type_optional` ( `|` `type_optional` )*
fn add_sig_type_expr(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("sig_type_expr", |g| {
        g.node(Kind::NodeTypeExpr, |g| {
            g.call("sig_type_optional");
            g.zero_or_more(|g| {
                g.call("sig_pipe");
                g.call("sig_type_optional");
            });
        });
    });
}

/// param: `type_expr` ident ["?"]  — "?" after name means argument can be omitted (not type|null)
fn add_sig_param(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("sig_param", |g| {
        g.node(Kind::NodeSigParam, |g| {
            g.call("sig_type_expr");
            g.call("sig_ident");
            g.optional(|g| {
                g.call("sig_question");
            });
        });
    });
}

/// `param_list`: [param ("," param)*]
fn add_sig_param_list(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("sig_param_list", |g| {
        g.optional(|g| {
            g.call("sig_param");
            g.zero_or_more(|g| {
                g.call("sig_comma");
                g.call("sig_param");
            });
        });
    });
}

/// `function_sig`: "function" ident "(" `param_list` ")" ["->" `type_expr`]
fn add_sig_function(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("sig_function", |g| {
        g.node(Kind::NodeSigFunction, |g| {
            g.call("sig_kw_function");
            g.call("sig_ident");
            g.call("sig_lparen");
            g.call("sig_param_list");
            g.call("sig_rparen");
            g.optional(|g| {
                g.call("sig_arrow");
                g.call("sig_type_expr");
            });
        });
    });
}

/// `constructor_sig`: "constructor" "(" `param_list` ")"
fn add_sig_constructor(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("sig_constructor", |g| {
        g.node(Kind::NodeSigConstructor, |g| {
            g.call("sig_kw_constructor");
            g.call("sig_lparen");
            g.call("sig_param_list");
            g.call("sig_rparen");
        });
    });
}

/// `method_sig`: ["static"] ["public"|"private"|"protected"] `type_expr` ident "(" `param_list` ")"
fn add_sig_method(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("sig_method", |g| {
        g.node(Kind::NodeSigMethod, |g| {
            g.optional(|g| {
                g.call("sig_kw_static");
            });
            g.optional(|g| {
                g.choices(vec![
                    Box::new(|g| {
                        g.call("sig_kw_public");
                    }),
                    Box::new(|g| {
                        g.call("sig_kw_private");
                    }),
                    Box::new(|g| {
                        g.call("sig_kw_protected");
                    }),
                ]);
            });
            g.call("sig_type_expr");
            g.call("sig_ident");
            g.call("sig_lparen");
            g.call("sig_param_list");
            g.call("sig_rparen");
        });
    });
}

/// `field_sig`: ["static"] ["final"] `type_expr` ident
fn add_sig_field(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("sig_field", |g| {
        g.node(Kind::NodeSigField, |g| {
            g.optional(|g| {
                g.call("sig_kw_static");
            });
            g.optional(|g| {
                g.call("sig_kw_final");
            });
            g.call("sig_type_expr");
            g.call("sig_ident");
        });
    });
}

/// `class_member`: `method_sig` | `constructor_sig` | `field_sig`
fn add_sig_class_member(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("sig_class_member", |g| {
        g.choices(vec![
            Box::new(|g| {
                g.call("sig_method");
            }),
            Box::new(|g| {
                g.call("sig_constructor");
            }),
            Box::new(|g| {
                g.call("sig_field");
            }),
        ]);
    });
}

/// `class_sig`: "class" ident ["extends" ident] "{" `class_member`* "}"
fn add_sig_class(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("sig_class", |g| {
        g.node(Kind::NodeSigClass, |g| {
            g.call("sig_kw_class");
            g.call("sig_ident");
            g.optional(|g| {
                g.call("sig_kw_extends");
                g.call("sig_ident");
            });
            g.call("sig_lbrace");
            g.zero_or_more(|g| {
                g.call("sig_class_member");
            });
            g.call("sig_rbrace");
        });
    });
}

/// `global_sig`: "global" `type_expr` ident
fn add_sig_global(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("sig_global", |g| {
        g.node(Kind::NodeSigGlobal, |g| {
            g.call("sig_kw_global");
            g.call("sig_type_expr");
            g.call("sig_ident");
        });
    });
}

/// Doc block: either `/**` ... `*/` or one or more `///` lines (Doxygen-style; may include @param, @return, @complexity).
fn add_sig_doc_block(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("sig_doc_block", |g| {
        g.node(Kind::NodeSigDocBlock, |g| {
            g.choices(vec![
                Box::new(|g| {
                    g.call("sig_doc_block_comment");
                }),
                Box::new(|g| {
                    g.call("sig_doc_line");
                    g.zero_or_more(|g| {
                        g.call("sig_doc_line");
                    });
                }),
            ]);
        });
    });
}

/// Top-level item: `function_sig` [`doc_block`]? | `global_sig` [`doc_block`]? | `class_sig`
fn add_sig_item(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("sig_item", |g| {
        g.choices(vec![
            Box::new(|g| {
                g.call("sig_function");
                g.optional(|g| {
                    g.call("sig_doc_block");
                });
            }),
            Box::new(|g| {
                g.call("sig_global");
                g.optional(|g| {
                    g.call("sig_doc_block");
                });
            }),
            Box::new(|g| {
                g.call("sig_class");
            }),
        ]);
    });
}

/// file: `sig_item`*
fn add_sig_file(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("sig_file", |g| {
        g.node(Kind::NodeSigFile, |g| {
            g.zero_or_more(|g| {
                g.call("sig_item");
            });
        });
    });
}

/// Build the signature file grammar.
/// Start rule: `start` → `sig_ws` `sig_file` `sig_ws` eof.
#[must_use]
pub fn build_signature_grammar() -> sipha::builder::BuiltGraph {
    let mut g = sipha::builder::GrammarBuilder::new();
    g.set_trivia_rule("sig_ws");
    g.allow_rule_cycles(true); // sig_type_expr → sig_type_params → sig_type_expr

    g.begin_rule("start");
    g.skip();
    g.call("sig_file");
    g.skip();
    g.end_of_input();
    g.accept();

    add_sig_lexer(&mut g);
    add_sig_doc_block(&mut g);
    add_sig_type_params(&mut g);
    add_sig_type_primary(&mut g);
    add_sig_type_optional(&mut g);
    add_sig_type_expr(&mut g);
    add_sig_param(&mut g);
    add_sig_param_list(&mut g);
    add_sig_function(&mut g);
    add_sig_constructor(&mut g);
    add_sig_method(&mut g);
    add_sig_field(&mut g);
    add_sig_class_member(&mut g);
    add_sig_class(&mut g);
    add_sig_global(&mut g);
    add_sig_item(&mut g);
    add_sig_file(&mut g);

    g.finish().expect("signature grammar must be valid")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use sipha::red::SyntaxElement;

    use crate::parser::parse_signatures;
    use crate::syntax::Kind;

    #[test]
    fn parse_example_signature_file() {
        let src = r#"
            function abs(real x) -> real
            function min(integer a, integer b) -> integer
            global integer MAX_CELLS
            function withCallback(Function<Array, integer => string> fn) -> void
            function noArg(Function< => boolean> pred) -> void
            class Cell extends Entity {
                constructor(integer w, integer h)
                integer getX()
                static Cell create(integer w, integer h)
            }
        "#;
        let result = parse_signatures(src).expect("parse should not error");
        let root = result.expect("parse should produce a root");
        // Should have at least one child (function, class, or global)
        assert!(
            root.children().next().is_some(),
            "expected at least one sig item"
        );
    }

    #[test]
    fn parse_signature_with_doxygen_doc() {
        let src = r#"
            function abs(real|integer number) -> integer
            /// Returns the absolute value of number.
            /// @param number The value to convert
            /// @return The absolute value
            /// @complexity 1

            function contains(string string, string search) -> boolean
            /** Returns true if string contains search.
             *  @param string The haystack
             *  @param search The needle
             *  @return true if found
             *  @complexity 4
             */
        "#;
        let result = parse_signatures(src).expect("parse should not error");
        let root = result.expect("parse should produce a root");
        let file = root
            .children()
            .find_map(|c| match &c {
                SyntaxElement::Node(n) if n.kind_as::<Kind>() == Some(Kind::NodeSigFile) => {
                    Some(n.clone())
                }
                _ => None,
            })
            .or_else(|| {
                root.children().find_map(|c| match &c {
                    SyntaxElement::Node(n) => Some(n.clone()),
                    _ => None,
                })
            });
        let file = file.expect("sig file node");
        let nodes: Vec<_> = file.children().collect();
        assert!(nodes.len() >= 2, "expected at least 2 items");
        let has_doc = nodes.iter().any(|e| {
            matches!(e, SyntaxElement::Node(n) if n.kind_as::<Kind>() == Some(Kind::NodeSigDocBlock))
        });
        assert!(has_doc, "expected at least one NodeSigDocBlock");
    }

    #[test]
    fn parse_generated_stdlib_signatures() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let sig_dir = manifest_dir.join("examples").join("signatures");
        let mut parsed = 0;
        for name in ["stdlib_functions.sig", "stdlib_constants.sig"] {
            let path = sig_dir.join(name);
            let src = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(_) => continue, // skip if not generated (run scripts/gen_stdlib_sigs.py from repo root)
            };
            let result = parse_signatures(&src).expect("parse should not error");
            let root = result.expect("parse should produce a root");
            assert!(
                root.children().next().is_some(),
                "{} should have sig items",
                name
            );
            parsed += 1;
        }
        // If generator was run, we should have parsed at least one file
        if parsed == 0 {
            eprintln!("hint: run python3 scripts/gen_stdlib_sigs.py from repo root to generate stdlib *.sig");
        }
    }
}
