//! Job model, queue, output rules, publication and the undo journal.
//!
//! Pure filesystem rules; no Tauri, no child processes (`ARCHITECTURE.md` §2.3).
//!
//! - [`queue`] — admission, folder expansion, per-file state, batch progress
//!   (`U2`);
//! - [`outputs`] — the `SPEC.md` §5 output-rules engine: plan, convert, publish,
//!   journal (`C4`);
//! - [`publish`] — never-overwrite placement, per-directory claims, temp area
//!   (`C4`);
//! - [`journal`] — the on-disk undo journal and its verified delete (`C4`);
//! - [`hash`] — SHA-256 for the publication and undo guards (`C4`);
//! - [`paths`] — the cache root for the journal and job temp areas (`C4`).

pub mod hash;
pub mod journal;
pub mod outputs;
pub mod paths;
pub mod publish;
pub mod queue;
