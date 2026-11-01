//! Placeholder conversion-engine surface.
//!
//! This module defines the request/response contract the wheel UI binds to,
//! but performs no I/O and shells out to nothing. The S-series spikes
//! (S1/S2/S3) decide which backend this eventually wraps — bundled ffmpeg
//! sidecar, native OS codecs, or a hybrid — and replace the placeholder
//! functions while keeping the contract stable.

/// Backend the skeleton pretends to use. Spikes must not depend on this value.
pub const ENGINE_BACKEND: &str = "placeholder";

/// Version of the request/response contract. Bump on breaking shape changes.
pub const ENGINE_CONTRACT_VERSION: u32 = 1;

/// Target formats the skeleton advertises to the wheel.
///
/// Purely declarative: nothing here transcodes anything yet.
pub const SUPPORTED_TARGETS: &[&str] = &[
    "mp4", "webm", "mkv", "mov", "mp3", "wav", "flac", "gif", "png", "jpg", "webp", "pdf",
];

/// A single conversion job as the wheel would submit it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversionRequest {
    pub input: String,
    pub target_format: String,
}

impl ConversionRequest {
    pub fn new(input: impl Into<String>, target_format: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            // Normalise so `MP4`, ` mp4 ` and `mp4` are the same target.
            target_format: target_format.into().trim().to_ascii_lowercase(),
        }
    }

    /// Cheap pre-flight validation. No filesystem access.
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.input.trim().is_empty() {
            return Err(EngineError::EmptyInput);
        }
        if self.target_format.is_empty() {
            return Err(EngineError::EmptyTargetFormat);
        }
        if !SUPPORTED_TARGETS.contains(&self.target_format.as_str()) {
            return Err(EngineError::UnsupportedTargetFormat(
                self.target_format.clone(),
            ));
        }
        Ok(())
    }
}

/// Why a request was rejected before any work started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    EmptyInput,
    EmptyTargetFormat,
    UnsupportedTargetFormat(String),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::EmptyInput => write!(f, "input path is empty"),
            EngineError::EmptyTargetFormat => write!(f, "target format is empty"),
            EngineError::UnsupportedTargetFormat(fmt_) => {
                write!(f, "unsupported target format: {fmt_}")
            }
        }
    }
}

/// Stack-allocated, display-ready engine banner.
pub fn banner() -> String {
    format!(
        "{ENGINE_BACKEND} (contract v{ENGINE_CONTRACT_VERSION}, {} targets)",
        SUPPORTED_TARGETS.len()
    )
}

/// Tauri command the placeholder window calls on load.
#[tauri::command]
pub fn engine_version() -> String {
    banner()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_reports_backend_and_contract_version() {
        let b = banner();
        assert!(b.contains(ENGINE_BACKEND), "banner missing backend: {b}");
        assert!(
            b.contains(&format!("v{ENGINE_CONTRACT_VERSION}")),
            "banner missing contract version: {b}"
        );
        assert!(
            b.contains(&SUPPORTED_TARGETS.len().to_string()),
            "banner missing target count: {b}"
        );
    }

    #[test]
    fn tauri_command_matches_banner() {
        assert_eq!(engine_version(), banner());
    }

    #[test]
    fn target_format_is_trimmed_and_lowercased() {
        let req = ConversionRequest::new("clip.mov", "  MP4 ");
        assert_eq!(req.target_format, "mp4");
        assert_eq!(req.validate(), Ok(()));
    }

    #[test]
    fn every_advertised_target_validates() {
        for target in SUPPORTED_TARGETS {
            let req = ConversionRequest::new("in.mov", *target);
            assert_eq!(req.validate(), Ok(()), "target {target} rejected");
        }
    }

    #[test]
    fn empty_input_is_rejected() {
        let req = ConversionRequest::new("   ", "mp4");
        assert_eq!(req.validate(), Err(EngineError::EmptyInput));
    }

    #[test]
    fn empty_target_format_is_rejected() {
        let req = ConversionRequest::new("in.mov", "   ");
        assert_eq!(req.validate(), Err(EngineError::EmptyTargetFormat));
    }

    #[test]
    fn unknown_target_format_is_rejected_with_its_name() {
        let req = ConversionRequest::new("in.mov", "klingon");
        assert_eq!(
            req.validate(),
            Err(EngineError::UnsupportedTargetFormat("klingon".into()))
        );
        assert!(req.validate().unwrap_err().to_string().contains("klingon"));
    }

    #[test]
    fn supported_targets_are_unique_and_lowercase() {
        let mut seen = SUPPORTED_TARGETS.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), SUPPORTED_TARGETS.len(), "duplicate target");
        assert!(
            SUPPORTED_TARGETS.iter().all(|t| *t == t.to_ascii_lowercase()),
            "targets must be lowercase"
        );
    }
}
