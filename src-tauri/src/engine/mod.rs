//! `engine/` — the conversion engines, as sidecar processes.
//!
//! Layout (`ARCHITECTURE.md` §2.2, §6):
//!
//! - [`proc`] — the spawn contract: the only `Command` in the tree, built
//!   environment, timeouts, bounded capture, path admission (A1-A7);
//! - [`stderr_rules`] — "exit 0 is not success";
//! - [`vips`] — libvips argv for images (§6.1);
//! - [`ffmpeg`] — ffmpeg/ffprobe argv for audio, video, GIF (§6.3);
//! - [`plan`] — `(source, target, options)` → ordered steps, pure;
//! - [`verify`] — output validators (§6.0.5);
//! - [`convert`] — probe → plan → spawn → verify → tags, the worker's entry;
//! - [`tags`] — audio tag carry-over with `lofty`.
//!
//! Nothing here imports Tauri: the engine layer is testable on a headless box,
//! and `shell/ipc.rs` is what turns [`banner`] into a window command.
//!
//! The previous placeholder contract (`ConversionRequest`, `SUPPORTED_TARGETS`,
//! `ENGINE_BACKEND = "placeholder"`) is gone: targets now come from
//! `model::matrix` (evidence-gated), and requests are [`plan::Request`].

pub mod convert;
pub mod doc;
pub mod ffmpeg;
pub mod plan;
pub mod proc;
pub mod soffice;
pub mod stderr_rules;
pub mod tags;
pub mod verify;
pub mod vips;

/// What the shipped app bundles. A label, not a capability claim.
pub const ENGINE_BACKEND: &str = "libvips 8.18.7 + LGPL ffmpeg + LibreOffice 26.8.2 (sidecars)";

/// Version of the request/response contract. Bumped from 2 when the document
/// pipeline landed alongside the media planner.
pub const ENGINE_CONTRACT_VERSION: u32 = 3;

/// Human-readable engine banner for the window's diagnostics panel.
pub fn banner() -> String {
    format!(
        "{ENGINE_BACKEND} (contract v{ENGINE_CONTRACT_VERSION})"
    )
}

/// Engine directories resolved from the install root (`§6.7`), with the
/// `EZC_VIPS_DIR` / `EZC_FFMPEG_DIR` test overrides honoured.
pub fn engines_in(root: &std::path::Path) -> proc::EnginePaths {
    proc::EnginePaths::discover(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_names_the_real_engines_and_the_contract_version() {
        let b = banner();
        assert!(b.contains("libvips"));
        assert!(b.contains("ffmpeg"));
        assert!(b.contains(&format!("v{ENGINE_CONTRACT_VERSION}")));
    }

    #[test]
    fn the_placeholder_backend_is_gone() {
        assert!(!ENGINE_BACKEND.contains("placeholder"));
        assert_eq!(ENGINE_CONTRACT_VERSION, 3);
    }
}
