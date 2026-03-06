//! Example: expression syntax — literals, operators, ternary, cast, intervals.
//!
//! Run: cargo run --example example_expressions

use leekscript_rs::parse;

fn run(name: &str, source: &str) {
    println!("═══ {} ═══", name);
    println!("  {}", source.trim().replace('\n', " "));
    match parse(source) {
        Ok(Some(_)) => println!("  ✓ OK\n"),
        Ok(None) => println!("  (empty)\n"),
        Err(e) => println!("  ✗ Error: {e}\n"),
    }
}

fn main() {
    run("Interval", "var r = 1..10;");
    run("Ternary", "var x = true ? 1 : 0;");
    run("Cast (as)", "var n = x as integer;");
    run("Instanceof", "var b = x instanceof Array;");
    run("Map literal", "var m = [1: \"a\", 2: \"b\"];");
    run("Set literal", "var s = <1, 2, 3>;");
    run("Object literal", "var o = { a: 1, b: 2 };");
    run("Anonymous function", "var f = (x, y) => x + y;");
    run("New", "var a = new Array();");
}
