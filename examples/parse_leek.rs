//! Example: parse LeekScript source (token stream or full parse).
//!
//! Displays token stream (Phase 1) and program parse tree (Phase 3).

use leekscript_rs::syntax;
use leekscript_rs::syntax::Kind;
use leekscript_rs::{parse, parse_tokens, print_syntax_tree, TreeDisplayOptions};
use sipha::red::SyntaxNode;
use sipha::types::IntoSyntaxKind;

/// Returns the syntax node whose tokens we should print: the token_stream child if present, otherwise the root.
fn token_stream_node(root: Option<&SyntaxNode>) -> Option<SyntaxNode> {
    let root = root?;
    if root.kind_as::<Kind>() == Some(Kind::NodeTokenStream) {
        Some(root.clone())
    } else {
        root.find_node(Kind::NodeTokenStream.into_syntax_kind())
    }
}

fn main() {
    // Program: include, function (-> return type), var vs typed (integer x = 10), global integer, for (integer i), etc.
    let source = r#"
        include("lib.leek");
        var PI = 3;
        integer x = 10;
        global integer g = 1;
        var name = "leek";
        function sum(integer a, integer b) -> integer { return a + b; }
        var n = 0;
        var msg = "hello";
        var ids = [];
        var scores;
        var opt = null;
        var numOrNull = null;
        function add(a, b) { return a + b; }
        function first(arr) { return arr[0]; }
        var x = 42;
        var r = 1..10;
        var arr = [1, 2, 3];
        var mapVar = ['a': 1, 'b': 2];
        var setVar = <1, 2, 3>;
        if (true xor false) { x = 0; }
        for (var i = 0; i < 10; i = i + 1) { x = sum(x, i); }
        for (var k in r) { x = x + k; }
        do { x = x - 1; } while (x > 0);
    "#;

    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│ Input                                                        │");
    println!("└─────────────────────────────────────────────────────────────┘");
    println!("{source}");
    println!();

    // ─── Phase 1: Token stream ─────────────────────────────────────────────
    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│ Phase 1: Token stream (lexer)                               │");
    println!("└─────────────────────────────────────────────────────────────┘");
    println!();

    match parse_tokens(source) {
        Ok(out) => {
            println!("  Parsed bytes: {}", out.consumed);
            let root = out.syntax_root(source.as_bytes());
            let ts_node = token_stream_node(root.as_ref());
            let show_all = ts_node.is_some();
            let node = ts_node.or_else(|| root.clone());
            if let Some(n) = node {
                let tokens: Vec<_> = n.non_trivia_tokens().collect();
                println!("  Semantic tokens: {}\n", tokens.len());
                let limit = if show_all { tokens.len() } else { 24 };
                for (i, tok) in tokens.iter().enumerate().take(limit) {
                    let name = syntax::kind_name(tok.kind());
                    println!("    {:2}. {:12} {:?}", i + 1, name, tok.text());
                }
                if tokens.len() > limit {
                    println!("    ... and {} more", tokens.len() - limit);
                }
            } else {
                println!("  No syntax root.");
            }
        }
        Err(e) => eprintln!("  Parse error: {e}"),
    }

    println!();
    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│ Phase 3: Program (syntax tree)                               │");
    println!("└─────────────────────────────────────────────────────────────┘");
    println!();

    match parse(source) {
        Ok(Some(root)) => {
            let opts = TreeDisplayOptions::default();
            print_syntax_tree(&root, &opts);
        }
        Ok(None) => println!("  No root (empty tree)."),
        Err(e) => eprintln!("  Parse error: {e}"),
    }
}
