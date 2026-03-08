//! Built-in names used when no signature file is loaded.
//!
//! Only **class/type names** (e.g. `Array`, `Map`) are seeded here so that references
//! to these types resolve. For **functions and globals** (e.g. `getCell`, `getMP`), you must
//! supply `.sig` files (e.g. via `analyze_with_signatures` or the `--signatures` / `--stdlib-dir`
//! CLI options). Use the signature files under `examples/signatures/` or generate them from the
//! [LeekScript API](https://leekscript.com); see `examples/signatures/README.md`.

/// Built-in class/type names in LeekScript (language primitives only).
pub const BUILTIN_CLASS_NAMES: &[&str] = &["Class", "Object", "Array", "Map", "Set", "Interval"];
