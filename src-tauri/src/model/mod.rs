//! Pure model layer: data + rules, no Tauri, no child processes.
//!
//! `ARCHITECTURE.md` §2.1/§2.2 enforces the boundary mechanically: nothing under
//! `model/` or `job/` may import the Tauri crate. That is what lets these
//! modules be tested with `cargo test` on a runner with no window and no
//! webview session (asserted by a test in `lib.rs` and by
//! `scripts/check-model-purity.sh`).
//!
//! - [`types`] — canonical types, targets, route status, evidence ids (`C1`)
//!   plus the queue-facing shapes the window renders (`U2`);
//! - [`detect`] — content-first detection, `ARCHITECTURE.md` §7 (`C1`);
//! - [`matrix`] — the target matrix that drives the wheel, §8 (`C1`);
//! - [`naming`], [`settings`], [`settings_schema`] — output rules (`C4`) and the
//!   settings schema the window is generated from (`U2`).

pub mod detect;
pub mod matrix;
pub mod naming;
pub mod settings;
pub mod settings_schema;
pub mod types;

#[cfg(test)]
pub(crate) mod test_support;
