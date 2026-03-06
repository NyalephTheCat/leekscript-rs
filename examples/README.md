# LeekScript parser examples

Run any example with:

```bash
cargo run --example <name>
```

| Example | Description |
|---------|-------------|
| **parse_leek** | Full demo: token stream (Phase 1) and program parse (Phase 3). Includes include, var/typed var, global, functions with `->` return type, for/for-in, do-while, classes. |
| **example_types** | Type syntax: `integer x = 10`, `global integer g = 1`, params `integer a, integer b`, function `-> integer`, for-init and for-in with types. |
| **example_class** | Class with typed fields (`integer x`, `integer y`), constructor, and methods with return type at start (`public integer getX()`, `public static string name()`). |
| **example_expressions** | Expressions: intervals `1..10`, ternary, `as` cast, `instanceof`, map/set/object literals, arrow function, `new`. |
