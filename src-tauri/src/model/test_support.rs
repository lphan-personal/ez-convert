//! Shared fixture/document lookup for the model tests.
//!
//! Two ways to run these tests, one resolver:
//!
//! - `cargo test` in `src-tauri`: `CARGO_MANIFEST_DIR` is `src-tauri`, so the
//!   repo root is its parent.
//! - `scripts/test-model.sh`: a generated harness compiles `types.rs`,
//!   `detect.rs`, `matrix.rs` and this file with plain rustc, because the Tauri
//!   shell cannot build on a host without GTK/WebKit. That harness has no cargo
//!   environment, so the script sets `EZC_REPO_ROOT` at compile time.

use std::path::{Path, PathBuf};

pub fn repo_root() -> PathBuf {
    if let Some(root) = option_env!("EZC_REPO_ROOT") {
        return PathBuf::from(root);
    }
    if let Some(manifest) = option_env!("CARGO_MANIFEST_DIR") {
        return Path::new(manifest)
            .parent()
            .expect("src-tauri must have a parent directory")
            .to_path_buf();
    }
    panic!(
        "cannot locate the repo root: run under `cargo test` or set EZC_REPO_ROOT \
         at compile time (scripts/test-model.sh does this)"
    );
}

pub fn fixture(name: &str) -> PathBuf {
    repo_root().join("fixtures").join(name)
}

pub fn read_fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixture(name)).unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

pub fn read_repo_file(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}
