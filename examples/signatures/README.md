# LeekScript signature files

These files describe **signatures only** (functions, classes, globals) for use when loading the standard library or other API definitions. The format is **not** valid LeekScript source; it is a small DSL parsed by `parse_signatures()`.

## Generated stdlib files

From the **parsing** repo root (parent of `leekscript-rs`), generate the standard library signatures from the JSON definitions:

```bash
python3 scripts/gen_stdlib_sigs.py
```

This writes:

- **`stdlib_functions.sig`** — all functions from `functions.json` (one `function name(params) -> returnType` per line).
- **`stdlib_constants.sig`** — all constants from `constants.json` (one `global type name` per line).

Regenerate whenever the JSON sources change.

## Usage

```rust
use leekscript_rs::parse_signatures;

let src = std::fs::read_to_string("stdlib_functions.sig")?;
let root = parse_signatures(&src)?.expect("root");
// root is NodeSigFile; walk children for NodeSigFunction, NodeSigClass, NodeSigGlobal
```

## Format summary

- **Functions:** `function name(param_list) [-> return_type]`
- **Classes:** `class Name [extends Base] { members }`
- **Globals:** `global type name`
- **Params:** `type paramName` (e.g. `real x`, `Array<integer> ids`). Omittable args: `type paramName?` (e.g. `real entity?` for `getWeapon`); the `?` means "argument can be omitted", not nullable (`type?` = type|null).
- **Types:** Same as LeekScript — `integer`, `real`, `string`, `Array<T>`, `Map<K,V>`, `(T1, T2) => R`, `T | U`, `T?`. Shorthand: `Array`, `Map`, `Set` (no type params) mean `Array<any>`, `Map<any, any>`, `Set<any>`. Function type: `Function< => ret>` (0 params) or `Function<a, b => ret>` (param types, then `=>`, then return type). Spaces around `=>` optional.

See `src/grammar/signature.rs` for the full BNF and grammar.
