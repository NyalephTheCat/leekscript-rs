//! Expression parser rules.
//!
//! Minimal (expression grammar): primary only.
//! Full (program grammar): precedence levels from primary up to assignment.
//! Uses sipha's precedence helpers (expr::left_assoc_infix_level, right_assoc_infix_level)
//! where applicable; levels that need a NodeBinaryLevel wrapper use a local helper.

use crate::syntax::Kind;
use sipha::expr;

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

// ─── Precedence helpers (wrap sipha's pattern in NodeBinaryLevel where needed) ──

/// Left-associative infix level wrapped in NodeBinaryLevel so the rule produces a single node.
/// Each (op, lower) pair is wrapped in NodeBinaryExpr with children [op, lower].
fn left_assoc_level_with_wrapper(
    g: &mut sipha::builder::GrammarBuilder,
    level_name: &'static str,
    lower_level_name: &'static str,
    ops: &[&'static str],
) {
    g.parser_rule(level_name, |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeBinaryLevel, |g| {
            g.call(lower_level_name);
            g.zero_or_more(move |g| {
                g.node(Kind::NodeBinaryExpr, |g| {
                    let mut choices: Vec<Box<dyn FnOnce(&mut sipha::builder::GrammarBuilder)>> =
                        Vec::new();
                    for op in ops {
                        let op = *op;
                        let lower = lower_level_name;
                        choices.push(Box::new(move |g| {
                            g.call(op);
                            g.call(lower);
                        }));
                    }
                    g.choices(choices);
                });
            });
        });
    });
}

/// Interval level: expr_add ( .. expr_interval )* with NodeInterval for each (.. rhs).
fn left_assoc_interval_level(g: &mut sipha::builder::GrammarBuilder) {
    g.parser_rule("expr_interval", |g: &mut sipha::builder::GrammarBuilder| {
        g.node(Kind::NodeBinaryLevel, |g| {
            g.call("expr_add");
            g.zero_or_more(|g| {
                g.node(Kind::NodeInterval, |g| {
                    g.call("dot_dot");
                    g.call("expr_interval");
                });
            });
        });
    });
}

/// Power: unary ** expr_power (right-associative) | unary (sipha precedence climbing).
pub fn add_expr_power(g: &mut sipha::builder::GrammarBuilder) {
    expr::right_assoc_infix_level(g, "expr_power", "unary", "op_power", &Kind::NodeBinaryExpr);
}

/// Mul: expr_power ( * / \ % expr_power )* with NodeBinaryLevel wrapper (sipha precedence + wrapper).
pub fn add_expr_mul(g: &mut sipha::builder::GrammarBuilder) {
    left_assoc_level_with_wrapper(
        g,
        "expr_mul",
        "expr_power",
        &["op_star", "op_slash", "op_backslash", "op_percent"],
    );
}

/// Add: expr_mul ( + - expr_mul )* (sipha precedence + wrapper).
pub fn add_expr_add(g: &mut sipha::builder::GrammarBuilder) {
    left_assoc_level_with_wrapper(g, "expr_add", "expr_mul", &["op_plus", "op_minus"]);
}

/// Interval: expr_add ( .. expr_interval )* (range); uses NodeInterval for each (.. rhs).
pub fn add_expr_interval(g: &mut sipha::builder::GrammarBuilder) {
    left_assoc_interval_level(g);
}

/// Compare: expr_interval ( < <= > >= expr_interval )* (sipha left_assoc_infix_level, no wrapper).
pub fn add_expr_compare(g: &mut sipha::builder::GrammarBuilder) {
    expr::left_assoc_infix_level(
        g,
        "expr_compare",
        "expr_interval",
        &["op_le", "op_ge", "op_lt", "op_gt"],
        &Kind::NodeBinaryExpr,
    );
}

/// Equality: expr_compare ( === !== == != expr_compare )* (sipha precedence + wrapper).
pub fn add_expr_equality(g: &mut sipha::builder::GrammarBuilder) {
    left_assoc_level_with_wrapper(
        g,
        "expr_equality",
        "expr_compare",
        &["op_strict_eq", "op_neq_or_strict", "op_eq"],
    );
}

/// In (membership): expr_equality ( in expr_equality )* (sipha left_assoc_infix_level, no wrapper).
pub fn add_expr_in(g: &mut sipha::builder::GrammarBuilder) {
    expr::left_assoc_infix_level(g, "expr_in", "expr_equality", &["in_kw"], &Kind::NodeBinaryExpr);
}

/// Instanceof: expr_in ( instanceof expr_equality )* (sipha precedence + wrapper).
pub fn add_expr_instanceof(g: &mut sipha::builder::GrammarBuilder) {
    left_assoc_level_with_wrapper(
        g,
        "expr_instanceof",
        "expr_in",
        &["instanceof_kw"],
    );
}

/// And: expr_instanceof ( && | and expr_instanceof )* (sipha precedence + wrapper).
pub fn add_expr_and(g: &mut sipha::builder::GrammarBuilder) {
    left_assoc_level_with_wrapper(
        g,
        "expr_and",
        "expr_instanceof",
        &["op_amp_amp", "and_kw"],
    );
}

/// Or: expr_and ( || | or expr_and )* (sipha precedence + wrapper).
pub fn add_expr_or(g: &mut sipha::builder::GrammarBuilder) {
    left_assoc_level_with_wrapper(g, "expr_or", "expr_and", &["op_pipe_pipe", "or_kw"]);
}

/// Xor: expr_or ( xor expr_or )* (sipha precedence + wrapper).
pub fn add_expr_xor(g: &mut sipha::builder::GrammarBuilder) {
    left_assoc_level_with_wrapper(g, "expr_xor", "expr_or", &["xor_kw"]);
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
