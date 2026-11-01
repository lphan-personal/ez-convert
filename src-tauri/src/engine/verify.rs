//! `engine/verify.rs` — output validators (`ARCHITECTURE.md` §6.0.5).
//!
//! `Done` is unreachable without one of these passing. Two are pure (magic
//! bytes, ffprobe text parsing); the rest read the file through the engine that
//! claims to have written it (`vipsheader`, `ffprobe`), which is also how a
//! truncated or zero-byte output is caught — measured: a failed vips load leaves
//! a **0-byte** file behind, so "the file exists" proves nothing.

use std::path::Path;

use crate::model::detect;
use crate::model::types::{CanonicalType, TargetFormat};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail(String),
}

impl Verdict {
    pub fn is_pass(&self) -> bool {
        matches!(self, Verdict::Pass)
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            Verdict::Pass => None,
            Verdict::Fail(r) => Some(r),
        }
    }
}

/// Family a target format belongs to, for magic-byte comparison.
fn expected_type(target: TargetFormat) -> Option<CanonicalType> {
    Some(match target {
        TargetFormat::Jpg => CanonicalType::Jpeg,
        TargetFormat::Png => CanonicalType::Png,
        TargetFormat::Webp => CanonicalType::WebP,
        TargetFormat::Tiff => CanonicalType::Tiff,
        TargetFormat::Avif => CanonicalType::Avif,
        TargetFormat::Gif => CanonicalType::Gif,
        TargetFormat::Mp3 => CanonicalType::Mp3,
        TargetFormat::Wav => CanonicalType::Wav,
        TargetFormat::Flac => CanonicalType::Flac,
        TargetFormat::M4a => CanonicalType::M4a,
        TargetFormat::Aac => CanonicalType::Aac,
        TargetFormat::Ogg => CanonicalType::Ogg,
        TargetFormat::Opus => CanonicalType::Opus,
        TargetFormat::Mp4 => CanonicalType::Mp4,
        TargetFormat::Mkv => CanonicalType::Mkv,
        TargetFormat::Mov => CanonicalType::Mov,
        TargetFormat::Webm => CanonicalType::WebM,
        _ => return None,
    })
}

/// §6.0.4 rule 3: missing, empty or unreadable output is a failure even when the
/// engine exited 0.
pub fn exists_nonempty(path: &Path) -> Verdict {
    match std::fs::metadata(path) {
        Ok(m) if m.len() > 0 => Verdict::Pass,
        Ok(_) => Verdict::Fail(format!("{} is zero bytes", path.display())),
        Err(e) => Verdict::Fail(format!("{} is missing: {e}", path.display())),
    }
}

/// §6.0.5: magic bytes must match the target family. Content beats extension —
/// the same rule detection uses on input.
pub fn magic_matches(path: &Path, target: TargetFormat) -> Verdict {
    let Some(expected) = expected_type(target) else {
        return Verdict::Fail(format!("no validator for {}", target.as_str()));
    };
    let detected = detect::detect_path(path);
    if detected.ty == expected {
        Verdict::Pass
    } else {
        Verdict::Fail(format!(
            "{} detected as {:?}, expected {:?}",
            path.display(),
            detected.ty,
            expected
        ))
    }
}

/// One `vipsheader -f <field>` answer → a positive integer.
pub fn parse_header_dimension(stdout: &str) -> Option<u32> {
    stdout
        .lines()
        .find_map(|l| l.trim().parse::<u32>().ok())
        .filter(|d| *d > 0)
}

/// Facts parsed out of `ffprobe -of default=nw=1` output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProbeFacts {
    pub codec_name: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub sample_fmt: Option<String>,
    pub bits_per_raw_sample: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub r_frame_rate: Option<(u32, u32)>,
    pub avg_frame_rate: Option<(u32, u32)>,
    /// From `format=duration`, seconds.
    pub duration_ms: Option<u64>,
    pub has_audio: bool,
    pub has_video: bool,
}

fn parse_rate(value: &str) -> Option<(u32, u32)> {
    let (num, den) = value.split_once('/')?;
    let num: u32 = num.parse().ok()?;
    let den: u32 = den.parse().ok()?;
    if den == 0 {
        return None;
    }
    Some((num, den))
}

/// Parse the flat `key=value` probe output. Unknown keys are ignored on purpose:
/// ffprobe adds fields between releases and we only trust what we named.
pub fn parse_probe(stdout: &str) -> ProbeFacts {
    let mut facts = ProbeFacts::default();
    for line in stdout.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "codec_name" => {
                facts.codec_name = Some(value.to_string());
                // A first audio/video stream is selected by the caller; this
                // flag records which kind it was (see `parse_probe_kinds`).
            }
            "sample_rate" => facts.sample_rate = value.parse().ok(),
            "channels" => facts.channels = value.parse().ok(),
            "sample_fmt" => facts.sample_fmt = Some(value.to_string()),
            "bits_per_raw_sample" => facts.bits_per_raw_sample = value.parse().ok(),
            "width" => facts.width = value.parse().ok(),
            "height" => facts.height = value.parse().ok(),
            "r_frame_rate" => facts.r_frame_rate = parse_rate(value),
            "avg_frame_rate" => facts.avg_frame_rate = parse_rate(value),
            "duration" => {
                facts.duration_ms = value
                    .parse::<f64>()
                    .ok()
                    .map(|d| (d * 1000.0).round() as u64)
            }
            "codec_type" => match value {
                "audio" => facts.has_audio = true,
                "video" => facts.has_video = true,
                _ => {}
            },
            _ => {}
        }
    }
    facts
}

/// A video/audio output is valid when the probe reports the right stream kind
/// and a duration above a floor: a 1-frame GIF at 25 fps is 40 ms, so 20 ms is
/// the smallest honest floor.
pub fn probe_is_valid(facts: &ProbeFacts, want_video: bool) -> Verdict {
    if want_video && !facts.has_video {
        return Verdict::Fail("no video stream in the output".into());
    }
    if !want_video && !facts.has_audio {
        return Verdict::Fail("no audio stream in the output".into());
    }
    match facts.duration_ms {
        Some(d) if d >= 20 => Verdict::Pass,
        Some(d) => Verdict::Fail(format!("output duration is {d} ms")),
        None => Verdict::Fail("ffprobe reported no duration".into()),
    }
}

/// `r_frame_rate` and `avg_frame_rate` disagree ⇒ the stream is variable.
/// Measured control: a CFR transcode reports the same value for both, an
/// unfixed VFR transcode reports `30/1` and `2040/89`.
pub fn is_vfr(facts: &ProbeFacts) -> bool {
    match (facts.r_frame_rate, facts.avg_frame_rate) {
        (Some(r), Some(a)) => {
            // Compare as floats: 2040/89 vs 30/1 differ by more than 1%.
            let rf = r.0 as f64 / r.1 as f64;
            let af = a.0 as f64 / a.1 as f64;
            (rf - af).abs() > rf * 0.01
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_output_fails() {
        let p = std::env::temp_dir().join("ezc-verify-absent.png");
        assert!(!exists_nonempty(&p).is_pass());
    }

    #[test]
    fn zero_byte_output_fails_even_though_it_exists() {
        let p = std::env::temp_dir().join(format!("ezc-zero-{}.png", std::process::id()));
        std::fs::write(&p, b"").unwrap();
        let v = exists_nonempty(&p);
        assert!(!v.is_pass());
        assert!(v.reason().unwrap().contains("zero bytes"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn magic_must_match_the_target_not_the_extension() {
        let dir = std::env::temp_dir();
        let p = dir.join(format!("ezc-magic-{}.png", std::process::id()));
        // Real JPEG bytes under a .png name: the PNG target must reject it.
        std::fs::write(&p, [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F']).unwrap();
        assert!(!magic_matches(&p, TargetFormat::Png).is_pass());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn probe_text_is_parsed_without_a_json_dependency() {
        let text = "codec_name=h264\nwidth=320\nheight=240\nr_frame_rate=30/1\n\
                    avg_frame_rate=30/1\nduration=2.966667\ncodec_type=video\n";
        let f = parse_probe(text);
        assert_eq!(f.codec_name.as_deref(), Some("h264"));
        assert_eq!(f.width, Some(320));
        assert_eq!(f.r_frame_rate, Some((30, 1)));
        assert_eq!(f.duration_ms, Some(2967));
        assert!(f.has_video && !f.has_audio);
        assert!(!is_vfr(&f));
    }

    #[test]
    fn measured_vfr_control_is_detected() {
        // Real ffprobe output of the unfixed VFR transcode (evidence §4 V9).
        let vfr = parse_probe("r_frame_rate=30/1\navg_frame_rate=2040/89\n");
        assert!(is_vfr(&vfr));
        let cfr = parse_probe("r_frame_rate=30/1\navg_frame_rate=30/1\n");
        assert!(!is_vfr(&cfr));
    }

    #[test]
    fn probe_validator_requires_the_stream_kind_and_a_duration() {
        let audio = parse_probe("codec_name=mp3\nsample_rate=44100\nduration=0.25\ncodec_type=audio\n");
        assert!(probe_is_valid(&audio, false).is_pass());
        assert!(!probe_is_valid(&audio, true).is_pass());
        let silent = parse_probe("codec_name=h264\ncodec_type=video\n");
        assert!(!probe_is_valid(&silent, true).is_pass());
    }

    #[test]
    fn header_dimension_parses_vipsheader_output() {
        assert_eq!(parse_header_dimension("400\n"), Some(400));
        assert_eq!(parse_header_dimension("vipsheader: nope\n"), None);
        assert_eq!(parse_header_dimension("0\n"), None);
    }

    #[test]
    fn every_target_we_write_has_a_validator() {
        for t in [
            TargetFormat::Jpg,
            TargetFormat::Png,
            TargetFormat::Webp,
            TargetFormat::Tiff,
            TargetFormat::Avif,
            TargetFormat::Gif,
            TargetFormat::Mp3,
            TargetFormat::Wav,
            TargetFormat::Flac,
            TargetFormat::M4a,
            TargetFormat::Aac,
            TargetFormat::Ogg,
            TargetFormat::Opus,
            TargetFormat::Mp4,
            TargetFormat::Mkv,
            TargetFormat::Mov,
            TargetFormat::Webm,
        ] {
            assert!(expected_type(t).is_some(), "{t:?} has no validator");
        }
    }
}
