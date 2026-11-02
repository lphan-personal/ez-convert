//! Shell layer: the only Tauri-aware modules (`ARCHITECTURE.md` §2.2).
//!
//! `geometry`, `accelerator`, `hotkey`, `grab` and `screen` are deliberately free of
//! Tauri (and, except `screen`, of any OS call) so the `U3` acceptance spike compiles
//! them unchanged: the code measured on the VM is the code that ships.

pub mod accelerator;
pub mod geometry;
pub mod grab;
pub mod hotkey;
pub mod ipc;
pub mod overlay;
pub mod probe;
pub mod screen;
pub mod summon;
pub mod tray;
