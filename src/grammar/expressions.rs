//! Expression parser rules.
//!
//! Minimal (expression grammar): primary only.
//! Full (program grammar): precedence levels from primary up to assignment.

use crate::syntax::Kind;

// ─── Expression grammar (Phase 2): primary only ──────────────────────────────

/// Expression as primary only: number, string, ident, or ( expr ).
pub fn add_expr_minimal(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("expr", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeExpr, |g| {
            g.choices(vec![
                Box::new(|g| { g.call("number_lit"); }),
                Box::new(|g| { g.call("string_lit"); }),
                Box::new(|g| { g.call("ident"); }),
                Box::new(|g| {
                    g.call("lparen");
                    g.call("expr");
                    g.call("rparen");
                }),
            ]);
        });
    });
}

// ─── Program grammar: full expression precedence ───────────────────────────────

/// Primary: literals, ident (and contextual keywords this/class via `keyword_or_ident`), new, (expr), [array], {object}.
pub fn add_primary(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("primary", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodePrimaryExpr, |g| {
            g.choices(vec![
                Box::new(|g| { g.call("number_lit"); }),
                Box::new(|g| { g.call("string_lit"); }),
                Box::new(|g| { g.token(Kind::KwTrue, |g| { g.literal(b"true"); }); }),
                Box::new(|g| { g.token(Kind::KwFalse, |g| { g.literal(b"false"); }); }),
                Box::new(|g| { g.token(Kind::KwNull, |g| { g.literal(b"null"); }); }),
                Box::new(|g| { g.call("special_lit"); }),
                Box::new(|g| {
                    g.token(Kind::KwNew, |g| { g.literal(b"new"); });
                    g.call("ident");
                    g.optional(|g| {
                        g.call("lparen");
                        g.optional(|g| {
                            g.call("expr");
                            g.zero_or_more(|g| {
                                g.call("comma");
                                g.call("expr");
                            });
                        });
                        g.call("rparen");
                    });
                }),
                Box::new(|g| { g.call("keyword_or_ident"); }),
                Box::new(|g| {
                    g.node(Kind::NodeAnonFn, |g| {
                        g.call("lparen");
                        g.optional(|g| {
                            g.call("param");
                            g.zero_or_more(|g| {
                                g.call("comma");
                                g.call("param");
                            });
                        });
                        g.call("rparen");
                        g.call("arrow");
                        g.choices(vec![
                            Box::new(|g| { g.call("expr"); }),
                            Box::new(|g| { g.call("block"); }),
                        ]);
                    });
                }),
                Box::new(|g| {
                    g.call("lparen");
                    g.call("expr");
                    g.call("rparen");
                }),
                Box::new(|g| { g.call("bracket_literal"); }),
                Box::new(|g| {
                    g.node(Kind::NodeSet, |g| {
                        g.call("op_lt");
                        g.choices(vec![
                            Box::new(|g| { g.call("op_gt"); }),
                            Box::new(|g| {
                                g.call("expr");
                                g.zero_or_more(|g| {
                                    g.call("comma");
                                    g.call("expr");
                                });
                                g.call("op_gt");
                            }),
                        ]);
                    });
                }),
                Box::new(|g| {
                    g.call("lbrace");
                    g.optional(|g| {
                        g.call("object_pair");
                        g.zero_or_more(|g| {
                            g.call("comma");
                            g.call("object_pair");
                        });
                    });
                    g.call("rbrace");
                }),
            ]);
        });
    });
}

/// Interval literal: ] or [ on each side (closed/open). Four forms: ]a..b], ]a..b[, [a..b], [a..b[.
pub fn add_interval_literal(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("interval_literal", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeInterval, |g| {
            g.choices(vec![
                Box::new(|g| {
                    g.call("rbracket");
                    g.call("expr");
                    g.call("dot_dot");
                    g.call("expr");
                    g.call("rbracket");
                }),
                Box::new(|g| {
                    g.call("rbracket");
                    g.call("expr");
                    g.call("dot_dot");
                    g.call("expr");
                    g.call("lbracket");
                }),
                Box::new(|g| {
                    g.call("lbracket");
                    g.call("expr");
                    g.call("dot_dot");
                    g.call("expr");
                    g.call("rbracket");
                }),
                Box::new(|g| {
                    g.call("lbracket");
                    g.call("expr");
                    g.call("dot_dot");
                    g.call("expr");
                    g.call("lbracket");
                }),
            ]);
        });
    });
}

/// Bracket literal: [ e, ... ] array, interval, [ : ] empty map, [ k : v, ... ] map.
/// Tries array first so "[ [0,-1], ... ]" parses; then interval for [a..b] etc.
pub fn add_bracket_literal(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("bracket_literal", |g: &mut sipha::builder::GrammarBuilder| {
        g.choices(vec![
            Box::new(|g| {
                g.node(Kind::NodeArray, |g| {
                    g.call("lbracket");
                    g.optional(|g| {
                        g.call("expr_as");
                        g.zero_or_more(|g| {
                            g.call("comma");
                            g.call("expr_as");
                        });
                        g.optional(|g| { g.call("comma"); });
                    });
                    g.call("rbracket");
                });
            }),
            Box::new(|g| { g.call("interval_literal"); }),
            Box::new(|g| {
                g.node(Kind::NodeMap, |g| {
                    g.call("lbracket");
                    g.call("op_colon");
                    g.call("rbracket");
                });
            }),
            Box::new(|g| {
                g.node(Kind::NodeMap, |g| {
                    g.call("lbracket");
                    g.call("map_pair");
                    g.zero_or_more(|g| {
                        g.call("comma");
                        g.call("map_pair");
                    });
                    g.call("rbracket");
                });
            }),
        ]);
    });
}

/// One key : value pair inside a map literal.
pub fn add_map_pair(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("map_pair", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeMapPair, |g| {
            g.call("expr");
            g.call("op_colon");
            g.call("expr");
        });
    });
}

pub fn add_object_pair(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("object_pair", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeObjectPair, |g| {
            g.choices(vec![
                Box::new(|g| {
                    g.call("string_lit");
                    g.call("op_colon");
                    g.call("expr");
                }),
                Box::new(|g| {
                    g.call("keyword_or_ident");
                    g.call("op_colon");
                    g.call("expr");
                }),
            ]);
        });
    });
}

/// Postfix: primary ( call | index | member | ++ | -- )*
pub fn add_postfix(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("postfix", |g: &mut sipha::builder::GrammarBuilder| {
        g.call("primary");
        g.zero_or_more(|g| {
            g.choices(vec![
                Box::new(|g| {
                    g.node(Kind::NodeCallExpr, |g| {
                        g.call("lparen");
                        g.optional(|g| {
                            g.call("expr");
                            g.zero_or_more(|g| {
                                g.call("comma");
                                g.call("expr");
                            });
                        });
                        g.call("rparen");
                    });
                }),
                Box::new(|g| {
                    g.node(Kind::NodeIndexExpr, |g| {
                        g.call("lbracket");
                        g.call("expr");
                        g.optional(|g| {
                            g.call("op_colon");
                            g.call("expr");
                            g.optional(|g| {
                                g.call("op_colon");
                                g.call("expr");
                            });
                        });
                        g.call("rbracket");
                    });
                }),
                Box::new(|g| {
                    g.node(Kind::NodeMemberExpr, |g| {
                        g.call("dot");
                        g.call("keyword_or_ident");
                    });
                }),
                Box::new(|g| { g.call("op_plus_plus"); }),
                Box::new(|g| { g.call("op_minus_minus"); }),
                Box::new(|g| { g.call("op_bang_postfix"); }),
            ]);
        });
    });
}

/// Unary: - + ! ~ unary | postfix
pub fn add_unary(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("unary", |g: &mut sipha::builder::GrammarBuilder| {
        g.choices(vec![
            Box::new(|g| {
                g.node(Kind::NodeUnaryExpr, |g| {
                    g.call("op_minus");
                    g.call("unary");
                });
            }),
            Box::new(|g| {
                g.node(Kind::NodeUnaryExpr, |g| {
                    g.call("op_plus");
                    g.call("unary");
                });
            }),
            Box::new(|g| {
                g.node(Kind::NodeUnaryExpr, |g| {
                    g.call("op_bang");
                    g.call("unary");
                });
            }),
            Box::new(|g| {
                g.node(Kind::NodeUnaryExpr, |g| {
                    g.call("not_kw");
                    g.call("unary");
                });
            }),
            Box::new(|g| { g.call("postfix"); }),
        ]);
    });
}

/// Power: unary ** `expr_power` (right-associative) | unary
pub fn add_expr_power(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("expr_power", |g: &mut sipha::builder::GrammarBuilder| {
        g.choices(vec![
            Box::new(|g| {
                g.node(Kind::NodeBinaryExpr, |g| {
                    g.call("unary");
                    g.call("op_power");
                    g.call("expr_power");
                });
            }),
            Box::new(|g| { g.call("unary"); }),
        ]);
    });
}

/// Mul: `expr_power` ( * / \ % `expr_power` )*
pub fn add_expr_mul(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("expr_mul", |g: &mut sipha::builder::GrammarBuilder| {
        g.call("expr_power");
        g.zero_or_more(|g| {
            g.node(Kind::NodeBinaryExpr, |g| {
                g.choices(vec![
                    Box::new(|g| { g.call("op_star"); }),
                    Box::new(|g| { g.call("op_slash"); }),
                    Box::new(|g| { g.call("op_backslash"); }),
                    Box::new(|g| { g.call("op_percent"); }),
                ]);
                g.node_with_field(Kind::NodeExpr, "rhs", |g| { g.call("expr_power"); });
            });
        });
    });
}

/// Add: `expr_mul` ( + - `expr_mul` )*
pub fn add_expr_add(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("expr_add", |g: &mut sipha::builder::GrammarBuilder| {
        g.call("expr_mul");
        g.zero_or_more(|g| {
            g.node(Kind::NodeBinaryExpr, |g| {
                g.choices(vec![
                    Box::new(|g| { g.call("op_plus"); }),
                    Box::new(|g| { g.call("op_minus"); }),
                ]);
                g.call("expr_mul");
            });
        });
    });
}

/// Interval: `expr_add` ( .. `expr_interval` )* (range, e.g. 1..10)
pub fn add_expr_interval(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("expr_interval", |g: &mut sipha::builder::GrammarBuilder| {
        g.call("expr_add");
        g.zero_or_more(|g| {
            g.node(Kind::NodeInterval, |g| {
                g.call("dot_dot");
                g.call("expr_interval");
            });
        });
    });
}

/// Compare: `expr_interval` ( < <= > >= `expr_interval` )*
pub fn add_expr_compare(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("expr_compare", |g: &mut sipha::builder::GrammarBuilder| {
        g.call("expr_interval");
        g.zero_or_more(|g| {
            g.node(Kind::NodeBinaryExpr, |g| {
                g.choices(vec![
                    Box::new(|g| { g.call("op_le"); }),
                    Box::new(|g| { g.call("op_ge"); }),
                    Box::new(|g| { g.call("op_lt"); }),
                    Box::new(|g| { g.call("op_gt"); }),
                ]);
                g.call("expr_interval");
            });
        });
    });
}

/// Equality: `expr_compare` ( === !== == != `expr_compare` )*
pub fn add_expr_equality(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("expr_equality", |g: &mut sipha::builder::GrammarBuilder| {
        g.call("expr_compare");
        g.zero_or_more(|g| {
            g.node(Kind::NodeBinaryExpr, |g| {
                g.choices(vec![
                    Box::new(|g| { g.call("op_strict_eq"); }),
                    Box::new(|g| { g.call("op_neq_or_strict"); }),
                    Box::new(|g| { g.call("op_eq"); }),
                ]);
                g.call("expr_compare");
            });
        });
    });
}

/// In (membership): `expr_equality` ( in `expr_equality` )*
pub fn add_expr_in(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("expr_in", |g: &mut sipha::builder::GrammarBuilder| {
        g.call("expr_equality");
        g.zero_or_more(|g| {
            g.node(Kind::NodeBinaryExpr, |g| {
                g.call("in_kw");
                g.call("expr_equality");
            });
        });
    });
}

/// Instanceof: `expr_in` ( instanceof `expr_equality` )*
pub fn add_expr_instanceof(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("expr_instanceof", |g: &mut sipha::builder::GrammarBuilder| {
        g.call("expr_in");
        g.zero_or_more(|g| {
            g.node(Kind::NodeBinaryExpr, |g| {
                g.call("instanceof_kw");
                g.call("expr_equality");
            });
        });
    });
}

/// And: `expr_instanceof` ( ( && | and ) `expr_instanceof` )*
pub fn add_expr_and(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("expr_and", |g: &mut sipha::builder::GrammarBuilder| {
        g.call("expr_instanceof");
        g.zero_or_more(|g| {
            g.node(Kind::NodeBinaryExpr, |g| {
                g.choices(vec![
                    Box::new(|g| { g.call("op_amp_amp"); }),
                    Box::new(|g| { g.call("and_kw"); }),
                ]);
                g.call("expr_instanceof");
            });
        });
    });
}

/// Or: `expr_and` ( ( || | or ) `expr_and` )*
pub fn add_expr_or(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("expr_or", |g: &mut sipha::builder::GrammarBuilder| {
        g.call("expr_and");
        g.zero_or_more(|g| {
            g.node(Kind::NodeBinaryExpr, |g| {
                g.choices(vec![
                    Box::new(|g| { g.call("op_pipe_pipe"); }),
                    Box::new(|g| { g.call("or_kw"); }),
                ]);
                g.call("expr_and");
            });
        });
    });
}

/// Xor: `expr_or` ( xor `expr_xor` )*
pub fn add_expr_xor(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("expr_xor", |g: &mut sipha::builder::GrammarBuilder| {
        g.call("expr_or");
        g.zero_or_more(|g| {
            g.node(Kind::NodeBinaryExpr, |g| {
                g.call("xor_kw");
                g.call("expr_xor");
            });
        });
    });
}

/// Ternary: `expr_xor` ? expr : `expr_ternary` | `expr_xor`
pub fn add_expr_ternary(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("expr_ternary", |g: &mut sipha::builder::GrammarBuilder| {
        g.choices(vec![
            Box::new(|g| {
                g.node(Kind::NodeExpr, |g| {
                    g.call("expr_xor");
                    g.call("op_question");
                    g.call("expr");
                    g.call("op_colon");
                    g.call("expr_ternary");
                });
            }),
            Box::new(|g| { g.call("expr_xor"); }),
        ]);
    });
}

// ─── Type expression (for annotations and casts) ─────────────────────────────

/// Type params: `< => R >` | `< T >` | `< K , V >` | `< T => R >` | `< T , V , ... => R >`
/// Shorthand: bare Array, Map, Set, Interval (no `<...>`) = Array<any>, Map<any, any>, etc.
/// Function: Function< => ret> (0 params) or Function<a, b => ret>.
pub fn add_type_params(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("type_params", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeTypeParams, |g| {
            g.choices(vec![
                // Zero params: => ret>
                Box::new(|g| {
                    g.call("arrow");
                    g.call("type_expr");
                    g.no_skip(|g| { g.call("op_gt"); });
                }),
                // One or more params (then optional => return)
                Box::new(|g| {
                    g.call("type_expr");
                    g.choices(vec![
                        Box::new(|g| { g.no_skip(|g| { g.call("op_gt"); }); }),
                        Box::new(|g| {
                            g.call("comma");
                            g.call("type_expr");
                            g.no_skip(|g| { g.call("op_gt"); });
                        }),
                        Box::new(|g| {
                            g.call("arrow");
                            g.call("type_expr");
                            g.no_skip(|g| { g.call("op_gt"); });
                        }),
                        Box::new(|g| {
                            g.call("comma");
                            g.call("type_expr");
                            g.zero_or_more(|g| {
                                g.call("comma");
                                g.call("type_expr");
                            });
                            g.call("arrow");
                            g.call("type_expr");
                            g.no_skip(|g| { g.call("op_gt"); });
                        }),
                    ]);
                }),
            ]);
        });
    });
}

/// Type primary: ident or ident `<` `type_params` (`type_params` already ends with `>`).
/// Bare Array, Map, Set, Interval (no `<...>`) are valid shorthands for Array<any>, Map<any, any>, etc.
pub fn add_type_primary(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("type_primary", |g: &mut sipha::builder::GrammarBuilder| {
        g.call("keyword_or_ident");
        g.optional(|g| {
            g.call("op_lt");
            g.call("type_params");
        });
    });
}

/// Type optional: `type_primary` `?`
pub fn add_type_optional(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("type_optional", |g: &mut sipha::builder::GrammarBuilder| {
        g.call("type_primary");
        g.optional(|g| { g.call("op_question"); });
    });
}

/// Type expr: `type_optional` ( `|` `type_optional` )*
pub fn add_type_expr(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("type_expr", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeTypeExpr, |g| {
            g.call("type_optional");
            g.zero_or_more(|g| {
                g.call("op_pipe");
                g.call("type_optional");
            });
        });
    });
}

/// As cast: `expr_ternary` ( as `type_expr` )*
pub fn add_expr_as(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("expr_as", |g: &mut sipha::builder::GrammarBuilder| {
        g.call("expr_ternary");
        g.zero_or_more(|g| {
            g.node(Kind::NodeAsCast, |g| {
                g.call("as_kw");
                g.call("type_expr");
            });
        });
    });
}

/// Expr: assignment (postfix = expr) | `expr_as`
pub fn add_expr(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("expr", |g: &mut sipha::builder::GrammarBuilder| {
        g.choices(vec![
            Box::new(|g| {
                g.node(Kind::NodeExpr, |g| {
                    g.call("postfix");
                    g.call("op_assign");
                    g.call("expr");
                });
            }),
            Box::new(|g| {
                g.call("expr_as");
            }),
        ]);
    });
}
