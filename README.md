# leekscript-rs

A [LeekScript](https://leekscript.com) parser implemented in Rust using [sipha](https://github.com/...) (PEG parser with green/red syntax trees).

## Status

- **Phase 1 (lexer)** — Done: token stream (keywords, identifiers, numbers, strings, operators, brackets, comments). Use `parse_tokens()`.
- **Phase 2** — Done: primary expressions (number, string, identifier, parenthesized). Use `parse_expression()`.
- **Phase 3** — Minimal: statement = expression; program = list of statements. Use `parse()`.
- **Phase 4** — Minimal: program root; full top-level (functions, classes, includes) can be added incrementally.

## Usage

```rust
use leekscript_rs::{parse, parse_tokens};

// Token stream only (Phase 1)
let out = parse_tokens("var x = 42")?;
let root = out.syntax_root("var x = 42".as_bytes()).unwrap();

// Full parse (currently same as token stream)
let root = parse("return 1 + 2")??;
```

## Example

```bash
cargo run -p leekscript-rs --example parse_leek
```

## Tests

```bash
cargo test -p leekscript-rs
```

## Reference

Grammar and token set are aligned with the [LeekScript Java compiler](https://github.com/leek-wars/leekscript) (lexer in `LexicalParser.java`, token types in `TokenType.java`).
