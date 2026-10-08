//! What both suites in this crate need: where the repository is, and how to read
//! a file in it with the path in the failure.

use std::path::{Path, PathBuf};

/// The repository root, from this crate's own manifest directory.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root")
}

pub fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}
