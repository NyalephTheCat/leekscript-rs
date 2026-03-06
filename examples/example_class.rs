//! Example: class with typed fields and methods (return type at start).
//!
//! Run: cargo run --example example_class

use leekscript_rs::parse;

fn main() {
    let source = r#"
class Point {
    integer x = 0;
    integer y = 0;

    constructor(integer x, integer y) {
        this.x = x;
        this.y = y;
    }

    public integer getX() {
        return this.x;
    }

    public integer getY() {
        return this.y;
    }

    public static string name() {
        return "Point";
    }
}
"#;

    println!("═══ Class: typed fields and methods (return type at start) ═══\n");
    println!("{}", source);

    match parse(source) {
        Ok(Some(root)) => {
            use leekscript_rs::syntax;

            let kind = root.kind();
            let children: Vec<_> = root.child_nodes().collect();
            let program_root = if kind == syntax::SYNTHETIC_ROOT && !children.is_empty() {
                children.into_iter().next().unwrap()
            } else {
                root
            };
            let stmts: Vec<_> = program_root.child_nodes().collect();
            println!("  ✓ Parsed {} top-level statement(s)", stmts.len());
            for (i, stmt) in stmts.iter().enumerate() {
                let name = syntax::kind_name(stmt.kind());
                let text = stmt.collect_text();
                let preview = if text.len() > 60 { format!("{}...", &text[..57]) } else { text };
                println!("    {}. {}  {}", i + 1, name, preview.replace('\n', " "));
            }
        }
        Ok(None) => println!("  (empty)"),
        Err(e) => println!("  ✗ Parse error: {e}"),
    }
}
