//! Example: parse LeekScript and print the syntax tree.
//!
//! Run with: `cargo run --example tree [path to .leek file]`
//! Or pipe source: `echo 'var x = 1;' | cargo run --example tree`

use leekscript_rs::{parse, TreeDisplayOptions};
use std::env;
use std::io::{self, Read};
use std::process::ExitCode;

fn main() -> ExitCode {
    let source = match env::args().nth(1) {
        Some(path) => {
            match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to read {}: {}", path, e);
                    return ExitCode::FAILURE;
                }
            }
        }
        None => {
            let mut s = String::new();
            if io::stdin().read_to_string(&mut s).is_err() || s.is_empty() {
                // Default demo source
                s = "var x = 1 + 2;\nfunction f() { return x; }\n".to_string();
            }
            s
        }
    };

    match parse(&source) {
        Ok(Some(root)) => {
            let opts = TreeDisplayOptions::default();
            leekscript_rs::print_syntax_tree(&root, &opts);
            ExitCode::SUCCESS
        }
        Ok(None) => {
            eprintln!("Empty parse result.");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("Parse error: {}", e);
            ExitCode::FAILURE
        }
    }
}
