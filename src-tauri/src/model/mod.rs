//! Pure model layer: data + rules, no Tauri, no child processes.
//!
//! `ARCHITECTURE.md` §2.1 enforces the boundary mechanically: nothing under
//! `model/` or `job/` may import the Tauri crate. That is what lets these
//! modules be tested with `cargo test` on a runner with no window and no
//! webview session (and it is asserted by a test in `lib.rs`).

pub mod naming;
pub mod settings;
pub mod settings_schema;
pub mod types;
