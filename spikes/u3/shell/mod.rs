//! Module root for the `U3` spike: the **shipped** shell modules, included by path so
//! the binary that runs on the VM is compiled from `src-tauri/src/shell/*.rs` itself.
//!
//! The paths are relative to this directory (`spikes/u3/shell/`), which must exist for
//! the `..` chain to resolve — hence a directory instead of an inline `mod`.

#[path = "../../../src-tauri/src/shell/geometry.rs"]
pub mod geometry;
#[path = "../../../src-tauri/src/shell/accelerator.rs"]
pub mod accelerator;
#[path = "../../../src-tauri/src/shell/hotkey.rs"]
pub mod hotkey;
#[path = "../../../src-tauri/src/shell/screen.rs"]
pub mod screen;
