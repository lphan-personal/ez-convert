// Print or write the documentation blocks that the model tests assert.
//
// The matrix and the evidence index are generated from `src-tauri/src/model/`
// so the docs cannot drift from the code: `cargo test` compares them byte for
// byte and fails with "regenerate it" when they diverge. This driver is the
// "regenerate" half of that loop.
//
// Build + run (no cargo, no GTK required):
//
//     rustc --edition 2021 -o /tmp/model-tables scripts/model-tables.rs
//     /tmp/model-tables --write
//
// The generated-docs check in CI builds this driver with plain `rustc`, so it
// must not see the serde-backed settings modules — it includes the std-only
// detection core only. It writes the blocks in `docs/DETECTION.md` and the
// evidence table in `evidence/ROUTES.md`.

// The generator only needs the std-only detection core. `model/mod.rs` also
// declares the serde-backed settings/naming modules (`U2`), which plain `rustc`
// cannot compile without the crates — so it points at `model-core-mod.rs`, the
// std-only module root that `scripts/test-model.sh` also uses.
#[path = "model-core-mod.rs"]
mod model;

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // Run from the repository root; verified by requiring the docs to exist.
    let root = std::env::current_dir().expect("cwd");
    assert!(
        root.join("docs/DETECTION.md").is_file(),
        "run this from the repo root (docs/DETECTION.md not found under {})",
        root.display()
    );
    root
}

fn main() {
    let write = std::env::args().any(|a| a == "--write");
    if write {
        let root = repo_root();
        replace_block(
            &root.join("docs/DETECTION.md"),
            "matrix:begin",
            "matrix:end",
            &model::matrix::render_matrix_table(),
        );
        replace_block(
            &root.join("docs/DETECTION.md"),
            "offered:begin",
            "offered:end",
            &model::matrix::render_offered_table(),
        );
        replace_block(
            &root.join("docs/DETECTION.md"),
            "scope:begin",
            "scope:end",
            &model::matrix::render_spec_scope_table(),
        );
        replace_block(
            &root.join("evidence/ROUTES.md"),
            "routes:begin",
            "routes:end",
            &model::matrix::render_routes_table(),
        );
        println!("updated docs/DETECTION.md and evidence/ROUTES.md");
        return;
    }
    print!("{}", model::matrix::render_matrix_table());
}

fn replace_block(path: &Path, begin: &str, end: &str, body: &str) {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{}: {e} (the file must exist with its markers)", path.display()));
    let begin_tag = format!("<!-- {begin} -->");
    let end_tag = format!("<!-- {end} -->");
    let start = text
        .find(&begin_tag)
        .unwrap_or_else(|| panic!("{}: missing {begin_tag}", path.display()))
        + begin_tag.len();
    let stop = text[start..]
        .find(&end_tag)
        .unwrap_or_else(|| panic!("{}: missing {end_tag}", path.display()))
        + start;
    let updated = format!(
        "{}\n{}\n{}",
        &text[..start],
        body.trim_end(),
        &text[stop..]
    );
    fs::write(path, updated).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}
