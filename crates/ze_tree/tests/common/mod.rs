//! Load reference files generated from ez-tree by `tools/golden`.

#![allow(dead_code)]

use std::path::PathBuf;

use serde_json::Value;

/// Resolves a fixture-relative path, such as `"options/default.json"`, from
/// the crate directory rather than the current working directory.
pub fn fixture_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel)
}

/// Reads a UTF-8 fixture using a path relative to `tests/fixtures/`.
///
/// # Panics
///
/// Panics with the file path if the fixture cannot be read.
pub fn fixture_text(rel: &str) -> String {
    let path = fixture_path(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Reads JSON from a path relative to `tests/fixtures/`.
///
/// # Panics
///
/// Panics if the file cannot be read or does not contain valid JSON.
pub fn fixture_json(rel: &str) -> Value {
    serde_json::from_str(&fixture_text(rel)).unwrap_or_else(|e| panic!("parsing {rel}: {e}"))
}
