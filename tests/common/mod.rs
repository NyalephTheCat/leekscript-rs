//! Shared helpers for integration tests.

use std::path::Path;

/// Read a fixture file from `tests/fixtures/<name>`.
pub fn read_fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e))
}
