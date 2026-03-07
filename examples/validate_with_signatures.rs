//! Example: parse a program, run semantic analysis with optional .sig files, and print diagnostics.
//!
//! Run: `cargo run --example validate_with_signatures [file.leek]`
//! With no argument, validates a small inline snippet. Pass a path to a .leek file to validate it.

use std::env;
use std::fs;
use std::path::Path;

#[allow(unused_imports)]
use leekscript_rs::{analyze, analyze_with_signatures, parse, parse_signatures};

fn main() {
    let source = if let Some(path) = env::args().nth(1) {
        let p = Path::new(&path);
        fs::read_to_string(p).unwrap_or_else(|e| {
            eprintln!("read {}: {}", p.display(), e);
            std::process::exit(1);
        })
    } else {
        // Default: validate a small snippet that uses a built-in style API (no .sig needed for this one).
        r#"
            function add(integer a, integer b) -> integer { return a + b; }
            integer x = add(1, 2);
            return x;
        "#
        .to_string()
    };

    let root = match parse(&source) {
        Ok(Some(r)) => r,
        Ok(None) => {
            eprintln!("Parse returned None (empty or no root)");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Parse error: {}", e);
            std::process::exit(1);
        }
    };

    // Option A: analyze without .sig (built-in class names only)
    let result = analyze(&root);

    // Option B: with signature files (e.g. stdlib) — uncomment and set paths as needed:
    // let sig_src = fs::read_to_string("examples/signatures/stdlib_functions.sig").unwrap();
    // let sig_root = parse_signatures(&sig_src).unwrap().expect("sig parse");
    // let result = analyze_with_signatures(&root, &[sig_root]);

    if result.is_valid() {
        println!("Validation OK (no errors)");
        if !result.diagnostics.is_empty() {
            for d in &result.diagnostics {
                println!("  [{:?}] {}", d.severity, d.message);
            }
        }
    } else {
        println!("Validation failed ({} diagnostic(s)):", result.diagnostics.len());
        for d in &result.diagnostics {
            println!("  [{:?}] {} (span {}..{})", d.severity, d.message, d.span.start, d.span.end);
        }
        std::process::exit(1);
    }
}
