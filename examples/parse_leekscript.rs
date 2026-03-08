//! Example: how the parser handles different LeekScript code.
//!
//! Discovers `.leek` files in `examples/leekscript/valid/` (and `valid/expressions/`),
//! parses each and reports the result. Invalid snippets are read from `leekscript/invalid/`.
//!
//! Run: `cargo run --example parse_leekscript`

use std::fs;
use std::path::{Path, PathBuf};

use leekscript_rs::{
    parse, parse_error_to_miette, parse_expression, parse_recovering, print_syntax_tree,
    TreeDisplayOptions,
};

fn main() {
    // Use miette's graphical handler so parse errors show source snippets and spans.
    let _ = miette::set_hook(Box::new(
        |_| Box::new(miette::GraphicalReportHandler::new()),
    ));

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let leekscript_dir = manifest_dir.join("examples").join("leekscript");
    let valid_dir = leekscript_dir.join("valid");
    let invalid_dir = leekscript_dir.join("invalid");

    println!("═══ LeekScript parser examples ═══\n");
    println!(
        "Valid: {}  |  Invalid: {}\n",
        valid_dir.display(),
        invalid_dir.display()
    );

    // ─── Discover program snippets (all .leek in leekscript/valid/) ───────────
    let program_files = discover_leek_files(&valid_dir);
    let tree_display_file = program_files.first().cloned(); // show tree for first file

    for path in &program_files {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
        let show_tree = tree_display_file.as_ref() == Some(path);
        match fs::read_to_string(path) {
            Ok(source) => print_snippet(name, &source, path, show_tree),
            Err(e) => println!("─── {} ───\n  (read error: {})\n", name, e),
        }
    }

    // ─── Discover expression files (leekscript/valid/expressions/*.leek) ─────
    let expr_dir = valid_dir.join("expressions");
    println!("═══ Expression parser (single expression per file) ═══\n");
    if expr_dir.is_dir() {
        let expr_files = discover_leek_files(&expr_dir);
        for path in expr_files {
            match fs::read_to_string(&path) {
                Ok(source) => {
                    let source = source.trim();
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    match parse_expression(source) {
                        Ok(Some(_)) => println!("  {}  → OK", name),
                        Ok(None) => println!("  {}  → (empty)", name),
                        Err(e) => {
                            if let Some(report) = parse_error_to_miette(&e, source, name.as_ref()) {
                                eprintln!("  {}  → Error:\n{:?}", name, report);
                            } else {
                                println!("  {}  → Error: {}", name, e);
                            }
                        }
                    }
                }
                Err(e) => println!("  {}  → read error: {}", path.display(), e),
            }
        }
    } else {
        println!("  (no expressions/ directory)");
    }

    // ─── Invalid input: discover all .leek in leekscript/invalid/ ─────────────
    println!("\n═══ Invalid input (recovering parse) ═══\n");
    let invalid_files = discover_leek_files(&invalid_dir);
    for invalid_path in &invalid_files {
        if let Ok(source) = fs::read_to_string(invalid_path) {
            let short_name = invalid_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("invalid.leek");
            println!("File: {}", short_name);
            println!("Source: {:?}", source);
            match parse(&source) {
                Ok(Some(_)) => println!("  parse() succeeded (unexpected)"),
                Ok(None) => println!("  parse() returned None"),
                Err(e) => {
                    if let Some(report) = parse_error_to_miette(&e, &source, short_name) {
                        eprintln!("{:?}\n", report);
                    } else {
                        println!("  Error: {}", e);
                    }
                }
            }
            if let Err((partial, _e)) = parse_recovering(&source) {
                println!(
                    "  parse_recovering: consumed {} bytes (see diagnostic above)",
                    partial.consumed
                );
                if let Some(root) = partial.syntax_root(source.as_bytes()) {
                    println!("  Partial tree (structure only):");
                    print_syntax_tree(&root, &TreeDisplayOptions::structure_only());
                }
            }
            println!();
        }
    }
    if invalid_files.is_empty() {
        println!("  (no .leek files in invalid/)");
    }
}

/// Collect all `.leek` files in `dir`, sorted by name.
fn discover_leek_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(dir) else {
        return vec![];
    };
    let mut paths: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.extension().map_or(false, |ext| ext == "leek"))
        .collect();
    paths.sort_by_cached_key(|p| p.file_name().unwrap_or_default().to_owned());
    paths
}

fn print_snippet(name: &str, source: &str, path: &Path, show_tree: bool) {
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("<input>");
    println!("─── {} ({}) ───", name, filename);
    println!("  {}", source.trim().replace('\n', " "));
    // Use short filename in diagnostics so miette reports show e.g. "set_interval.leek" not full path.
    let diagnostic_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("<file>");
    match parse(source) {
        Ok(Some(root)) => {
            println!("  ✓ Parsed OK");
            if show_tree {
                print_syntax_tree(&root, &TreeDisplayOptions::structure_only());
            }
        }
        Ok(None) => println!("  (empty parse)"),
        Err(e) => {
            if let Some(report) = parse_error_to_miette(&e, source, diagnostic_name) {
                eprintln!("{:?}\n", report);
            } else {
                println!("  ✗ Error: {}", e);
            }
        }
    }
    println!();
}
