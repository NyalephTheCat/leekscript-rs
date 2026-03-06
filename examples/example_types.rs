//! Example: type syntax — typed vars, globals, params, function return type.
//!
//! Run: cargo run --example example_types

use leekscript_rs::parse;

fn run(name: &str, source: &str) {
    println!("═══ {} ═══", name);
    println!("{}\n", source.trim());
    match parse(source) {
        Ok(Some(_)) => println!("  ✓ Parse OK\n"),
        Ok(None) => println!("  (empty)\n"),
        Err(e) => println!("  ✗ Parse error: {e}\n"),
    }
}

fn main() {
    // Typed variable (type replaces var)
    run(
        "Typed variable: integer x = 10",
        r#"
        integer x = 10;
        integer y;
        "#,
    );

    // Global with type
    run(
        "Global with type: global integer g = 1",
        r#"
        global integer g = 1;
        global string name = "leek";
        "#,
    );

    // Function params: type then name
    run(
        "Function params: integer a, integer b",
        r#"
        function add(integer a, integer b) -> integer {
            return a + b;
        }
        "#,
    );

    // Function return type after arrow
    run(
        "Function return type: -> integer",
        r#"
        function id(integer x) -> integer { return x; }
        function greet(string s) -> string { return s; }
        "#,
    );

    // Optional return type (no arrow)
    run(
        "Function without return type",
        r#"
        function run() { return; }
        "#,
    );

    // For-init with typed variable
    run(
        "For with typed init: integer i = 0",
        r#"
        for (integer i = 0; i < 10; i = i + 1) {}
        "#,
    );

    // For-in with typed value variable
    run(
        "For-in with type: k : integer v in arr",
        r#"
        for (k : integer v in [1, 2, 3]) { }
        "#,
    );
}
