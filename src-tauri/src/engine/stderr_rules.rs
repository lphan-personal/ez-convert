//! `engine/stderr_rules.rs` — the "exit 0 is not success" table (§6.0.4).
//!
//! Measured on the real engine: a truncated JPEG decoded with **exit 0**, a
//! 1,878-byte PNG on disk and only `VIPS-WARNING` / `VipsJpeg: premature end of
//! JPEG image` on stderr (C2 discovery, `evidence/c2-media.md` §2 V9/D3). A
//! truncated PNG is louder (`pngload: end of stream`, exit 1) but still leaves a
//! **zero-byte** output file behind, which is why §6.0.4 rule 3 exists.
//!
//! Fatal patterns are matched case-sensitively against the engine's stderr;
//! patterns are lowercase-prefixed substrings copied from real transcripts.

/// `(pattern, why)` — a hit makes the run fatal even at exit 0.
pub const FATAL_PATTERNS: &[(&str, &str)] = &[
    ("tiff2vips:", "TIFF decode error (JBIG and friends)"),
    ("heifload:", "HEIF/AVIF decode error"),
    ("jpeg2vips:", "JPEG decode error"),
    ("VipsJpeg:", "JPEG decoder complaint"),
    ("pngload:", "PNG decode error"),
    ("webpload:", "WebP decode error"),
    ("gifload:", "GIF decode error"),
    ("svgload:", "SVG decode error"),
    ("tiffload:", "TIFF loader complaint"),
    ("VIPS-WARNING", "elided read errors during decode"),
    ("VipsForeignLoad:", "loader could not open or identify the file"),
    ("VipsForeignSave:", "saver could not write the target format"),
    ("unable to open for read", "input disappeared or is unreadable"),
    ("does not exist", "input disappeared or is unreadable"),
    ("end of stream", "truncated image stream"),
    ("premature end", "truncated image stream"),
    ("not a known file format", "no engine can write/read that format on this build"),
];

/// Deliberately NOT fatal: ffmpeg's normal progress chatter and the
/// `deprecated pixel format` migration notes that appear on valid conversions.
pub const IGNORED_PATTERNS: &[&str] = &["Deprecated pixel format used", "Last message repeated"];

/// First fatal pattern in `stderr`, or `None`.
pub fn first_fatal(stderr: &str) -> Option<&'static str> {
    if stderr.trim().is_empty() {
        return None;
    }
    let mut matched = None;
    for line in stderr.lines() {
        let line = line.trim();
        if IGNORED_PATTERNS.iter().any(|p| line.contains(p)) {
            continue;
        }
        if let Some((pattern, _)) = FATAL_PATTERNS.iter().find(|(p, _)| line.contains(p)) {
            matched = Some(*pattern);
            break;
        }
    }
    matched
}

/// Human-readable reason for the UI, capped so a pathological engine cannot
/// flood the error surface.
pub fn explain(pattern: &str) -> &'static str {
    FATAL_PATTERNS
        .iter()
        .find(|(p, _)| *p == pattern)
        .map(|(_, why)| *why)
        .unwrap_or("engine reported an error")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_truncated_jpeg_stderr_is_fatal() {
        let stderr = "\n(vips:96361): VIPS-WARNING **: 19:58:47.400: read gave 2 warnings\n\n\
                      (vips:96361): VIPS-WARNING **: 19:58:47.400: VipsJpeg: premature end of JPEG image\n";
        assert_eq!(first_fatal(stderr), Some("VIPS-WARNING"));
    }

    #[test]
    fn measured_truncated_png_stderr_is_fatal() {
        assert_eq!(first_fatal("pngload: end of stream"), Some("pngload:"));
    }

    #[test]
    fn measured_unknown_format_stderr_is_fatal() {
        assert_eq!(
            first_fatal("VipsForeignSave: \"o7.avif\" is not a known file format"),
            Some("VipsForeignSave:")
        );
    }

    #[test]
    fn clean_stderr_is_not_fatal() {
        assert_eq!(first_fatal(""), None);
        assert_eq!(first_fatal("  \n "), None);
    }

    #[test]
    fn ffmpeg_progress_chatter_is_not_fatal() {
        assert_eq!(
            first_fatal("Deprecated pixel format used, make sure you did set range correctly"),
            None
        );
    }

    #[test]
    fn every_pattern_has_an_explanation() {
        for (pattern, why) in FATAL_PATTERNS {
            assert!(!why.is_empty(), "{pattern} has no explanation");
            assert_eq!(explain(pattern), *why);
        }
    }
}
