// Module root for the std-only detection core, used by `scripts/model-tables.rs`
// (compiled with plain `rustc`, no cargo).
//
// `src-tauri/src/model/mod.rs` also declares the serde-backed settings, naming
// and schema modules (`U2`), which plain `rustc` cannot compile without the
// crates. This file mirrors only the std-only subset, so the generator and
// `scripts/test-model.sh` see exactly the same code cargo does — with `super::`
// paths intact, because the files are nested under `model` here too.

#[path = "../src-tauri/src/model/types.rs"]
pub mod types;
#[path = "../src-tauri/src/model/detect.rs"]
pub mod detect;
#[path = "../src-tauri/src/model/matrix.rs"]
pub mod matrix;
