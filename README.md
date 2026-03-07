# leekscript-rs

A [LeekScript](https://leekscript.com) parser implemented in Rust using [sipha](https://github.com/...) (PEG parser with green/red syntax trees).

## Status

- **Phase 1 (lexer)** — Done: token stream (keywords, identifiers, numbers, strings, operators, brackets, comments). Use `parse_tokens()`.
- **Phase 2** — Done: primary expressions (number, string, identifier, parenthesized). Use `parse_expression()`.
- **Phase 3** — Done: program = list of statements (var/global/const/let, if/while/for/do-while, return/break/continue, blocks, expression statements). Use `parse()`.
- **Phase 4** — Done: top-level statements include `include`, function declarations, and class declarations; program root is a single node with statement children.

## CLI

The `leekscript` binary supports format and validate (and more to come):

```bash
# Format from stdin to stdout
cargo run --bin leekscript -- format

# Format a file in place
cargo run --bin leekscript -- format --in-place script.leek

# Check if formatting would change (exit 1 if so)
cargo run --bin leekscript -- format --check script.leek

# Validate syntax and run semantic analysis (scopes, types, deprecations)
cargo run --bin leekscript -- validate script.leek

# Canonical format: normalize indentation, braces, semicolons
cargo run --bin leekscript -- format --canonical script.leek
```

Format: by default prints the syntax tree as-is (round-trip). Use `--canonical` to normalize layout (indent, brace style, semicolons). Use `--preserve-comments` (default) to include comments and whitespace. See `leekscript format --help`.

## Library usage

```rust
use leekscript_rs::{parse, parse_tokens, format, FormatterOptions};

// Token stream only (Phase 1)
let out = parse_tokens("var x = 42")?;
let root = out.syntax_root("var x = 42".as_bytes()).unwrap();

// Full parse
let root = parse("return 1 + 2")?.expect("root");

// Format
let options = FormatterOptions::default();
let formatted = format(&root, &options);
```

## Example

```bash
cargo run -p leekscript-rs --example parse_leekscript
```

## Tests

```bash
cargo test -p leekscript-rs
```

## Reference

Grammar and token set are aligned with the [LeekScript Java compiler](https://github.com/leek-wars/leekscript) (lexer in `LexicalParser.java`, token types in `TokenType.java`).
