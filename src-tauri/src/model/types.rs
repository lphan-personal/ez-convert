//! Canonical types shared by the queue, the naming rules and the window.
//!
//! Kept deliberately small: detection (`C1`) owns `Detected`, the matrix (`C1`)
//! owns the route table, and the output writer (`C4`) owns publication. The
//! window (`U2`) only needs the *shape* of a queued file plus the target list
//! that the matrix will eventually supply.

use std::path::PathBuf;

/// A file accepted into the queue.
///
/// `detected` is `None` until the detection step (`C1`) has run; the window
/// renders it as "checking type" rather than inventing a guess. `SPEC.md` §3
/// makes detection content-first, so no extension-based classification exists
/// here on purpose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputFile {
    pub path: PathBuf,
    pub size: u64,
}

/// Where an item sits in the per-file state machine (`ARCHITECTURE.md` §5.1).
///
/// `U2` renders these; the worker (`C4`) drives them. The variants are the ones
/// the window can show today — `Verifying` is included because `Done` must be
/// unreachable without a validator (§5.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemState {
    Queued,
    Preparing,
    Running,
    Verifying,
    Done,
    Failed(String),
    Skipped(String),
    Cancelled,
}

impl ItemState {
    /// Stable identifier for the UI (never localised text).
    pub fn code(&self) -> &'static str {
        match self {
            ItemState::Queued => "queued",
            ItemState::Preparing => "preparing",
            ItemState::Running => "running",
            ItemState::Verifying => "verifying",
            ItemState::Done => "done",
            ItemState::Failed(_) => "failed",
            ItemState::Skipped(_) => "skipped",
            ItemState::Cancelled => "cancelled",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ItemState::Done | ItemState::Failed(_) | ItemState::Skipped(_) | ItemState::Cancelled
        )
    }

    pub fn detail(&self) -> Option<&str> {
        match self {
            ItemState::Failed(why) | ItemState::Skipped(why) => Some(why.as_str()),
            _ => None,
        }
    }
}

/// Target formats the window may offer.
///
/// This is the *union of names* from `SPEC.md` §3/§9 so the settings editor can
/// express a wheel layout and the queue can show a chosen target. Which of them
/// is actually offered for a given input is the route matrix's decision
/// (`ARCHITECTURE.md` §0/§8, card `C1`) — a name here is not a promise that the
/// route is `Proven`.
pub const TARGET_FORMATS: &[&str] = &[
    "jpg", "png", "webp", "tiff", "avif", "gif", "bmp", "heic", "svg", "pdf", "mp3", "wav", "flac",
    "m4a", "aac", "ogg", "opus", "mp4", "mkv", "mov", "webm", "docx", "xlsx", "pptx", "rtf", "csv",
    "md", "html", "txt",
];

/// True when `target` is a name from the spec's format list.
/// Case-insensitive; the queue stores the lowercase form.
pub fn is_known_target(target: &str) -> bool {
    TARGET_FORMATS.contains(&target.to_ascii_lowercase().as_str())
}

/// Human label for a size in bytes (binary prefixes, one decimal).
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_names_are_lowercase_and_unique() {
        let mut seen = TARGET_FORMATS.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), TARGET_FORMATS.len(), "duplicate target name");
        assert!(TARGET_FORMATS.iter().all(|t| *t == t.to_ascii_lowercase()));
    }

    #[test]
    fn known_target_is_case_insensitive() {
        assert!(is_known_target("PNG"));
        assert!(is_known_target("pdf"));
        assert!(!is_known_target("klingon"));
    }

    #[test]
    fn every_queue_state_has_a_stable_code() {
        let states = [
            ItemState::Queued,
            ItemState::Preparing,
            ItemState::Running,
            ItemState::Verifying,
            ItemState::Done,
            ItemState::Failed("x".into()),
            ItemState::Skipped("y".into()),
            ItemState::Cancelled,
        ];
        let codes: Vec<&str> = states.iter().map(ItemState::code).collect();
        let mut unique = codes.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), codes.len(), "state codes must be unique");
        assert_eq!(ItemState::Failed("why".into()).detail(), Some("why"));
        assert!(!ItemState::Verifying.is_terminal());
    }

    #[test]
    fn human_size_is_readable() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KiB");
        assert_eq!(human_size(3 * 1024 * 1024), "3.0 MiB");
    }
}
