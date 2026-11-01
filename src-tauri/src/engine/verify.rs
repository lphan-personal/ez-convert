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

// ---------------------------------------------------------------------------
// Document outputs (§6.2 N3/N5, §6.0.5)
// ---------------------------------------------------------------------------
//
// No validator here extracts text. On Windows the `docx -> pdf` text layer is
// degraded for accented Latin (`cafe\x1c` for `café`) because the Cambria
// subset's `ToUnicode` CMap is missing 0x1C/0x1F — measured in two independent
// engines, see §6.2.4. A validator that read text would fail a correct PDF.

/// Refuse to load a monster produced by a buggy engine into memory.
const MAX_DOC_BYTES: u64 = 512 * 1024 * 1024;

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Count `/Type /Page` objects, falling back to the `/Count` of a `/Pages`
/// tree when the page dictionaries live inside compressed object streams.
pub fn count_pdf_pages(data: &[u8]) -> usize {
    let mut pages = 0usize;
    let mut i = 0usize;
    while let Some(pos) = find(&data[i..], b"/Type") {
        let mut j = i + pos + b"/Type".len();
        while j < data.len() && matches!(data[j], b' ' | b'\r' | b'\n' | b'\t') {
            j += 1;
        }
        if data[j..].starts_with(b"/Page") && data.get(j + 5) != Some(&b's') {
            pages += 1;
        }
        i += pos + b"/Type".len();
    }
    if pages > 0 {
        return pages;
    }
    // Fallback: the largest `/Count n` in the file (the root Pages node).
    let mut best = 0usize;
    let mut i = 0usize;
    while let Some(pos) = find(&data[i..], b"/Count") {
        let mut j = i + pos + b"/Count".len();
        while j < data.len() && matches!(data[j], b' ' | b'\r' | b'\n' | b'\t') {
            j += 1;
        }
        let mut value = 0usize;
        let mut digits = 0;
        while j < data.len() && data[j].is_ascii_digit() {
            value = value.saturating_mul(10).saturating_add((data[j] - b'0') as usize);
            j += 1;
            digits += 1;
        }
        if digits > 0 {
            best = best.max(value);
        }
        i += pos + b"/Count".len();
    }
    best
}

fn is_zip_with(data: &[u8], parts: &[&[u8]]) -> bool {
    data.starts_with(b"PK\x03\x04") && parts.iter().all(|p| find(data, p).is_some())
}

fn looks_like_text(data: &[u8]) -> bool {
    !data.contains(&0)
}

/// Structural validation for every document target this build writes.
///
/// This is the check that turns the measured `rc=0` silent no-op (§6.2 N3:
/// a text file renamed `.docx` becomes a valid-looking 12,388 B PDF) into a
/// failure instead of a saved file.
pub fn document_output(path: &Path, target: TargetFormat) -> Verdict {
    match exists_nonempty(path) {
        Verdict::Pass => {}
        failure => return failure,
    }
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => return Verdict::Fail(format!("{} is missing: {e}", path.display())),
    };
    if meta.len() > MAX_DOC_BYTES {
        return Verdict::Fail(format!(
            "{} is {} bytes, over the validator limit",
            path.display(),
            meta.len()
        ));
    }
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => return Verdict::Fail(format!("{} could not be read: {e}", path.display())),
    };
    match target {
        TargetFormat::Pdf => {
            if !data.starts_with(b"%PDF-") {
                return Verdict::Fail(format!(
                    "{} does not start with %PDF- (first bytes: {:?})",
                    path.display(),
                    String::from_utf8_lossy(&data[..data.len().min(16)])
                ));
            }
            let pages = count_pdf_pages(&data);
            if pages == 0 {
                return Verdict::Fail(format!("{} has no page objects", path.display()));
            }
            Verdict::Pass
        }
        TargetFormat::Docx => {
            if !is_zip_with(&data, &[b"[Content_Types].xml", b"word/document.xml"]) {
                return Verdict::Fail(format!(
                    "{} is not a docx container (zip + word/document.xml)",
                    path.display()
                ));
            }
            Verdict::Pass
        }
        TargetFormat::Xlsx => {
            if !is_zip_with(&data, &[b"[Content_Types].xml", b"xl/workbook.xml"]) {
                return Verdict::Fail(format!(
                    "{} is not an xlsx container (zip + xl/workbook.xml)",
                    path.display()
                ));
            }
            Verdict::Pass
        }
        TargetFormat::Pptx => {
            if !is_zip_with(&data, &[b"[Content_Types].xml", b"ppt/presentation.xml"]) {
                return Verdict::Fail(format!(
                    "{} is not a pptx container (zip + ppt/presentation.xml)",
                    path.display()
                ));
            }
            Verdict::Pass
        }
        TargetFormat::Html => {
            let head = &data[..data.len().min(8192)];
            let text = String::from_utf8_lossy(head).to_ascii_lowercase();
            if text.contains("<html") || text.contains("<!doctype html") {
                Verdict::Pass
            } else {
                Verdict::Fail(format!("{} has no html element", path.display()))
            }
        }
        TargetFormat::Md | TargetFormat::Txt => {
            if data.starts_with(b"%PDF-") {
                return Verdict::Fail(format!(
                    "{} is a PDF, not text — the engine wrote its input instead of converting it",
                    path.display()
                ));
            }
            if !looks_like_text(&data) || String::from_utf8_lossy(&data).trim().is_empty() {
                return Verdict::Fail(format!("{} is empty or binary text", path.display()));
            }
            Verdict::Pass
        }
        TargetFormat::Csv => {
            if data.starts_with(b"%PDF-") {
                return Verdict::Fail(format!("{} is a PDF, not csv", path.display()));
            }
            let text = String::from_utf8_lossy(&data);
            if !looks_like_text(&data) || text.trim().is_empty() {
                return Verdict::Fail(format!("{} is empty or binary", path.display()));
            }
            if !text.contains(',') && !text.contains('\n') && !text.contains('\t') {
                return Verdict::Fail(format!("{} has no field separator", path.display()));
            }
            Verdict::Pass
        }
        TargetFormat::Rtf => {
            // The measured trap for a container-free format: a text file named
            // `.rtf` is not an RTF, and an empty `{\rtf}` header is not a
            // document. LibreOffice writes `{\rtf1\ansi\deff…{\fonttbl…` and at
            // least one paragraph mark, so both are required.
            if !data.starts_with(b"{\\rtf") {
                return Verdict::Fail(format!(
                    "{} does not start with {{\\rtf (first bytes: {:?})",
                    path.display(),
                    String::from_utf8_lossy(&data[..data.len().min(16)])
                ));
            }
            let text = String::from_utf8_lossy(&data);
            if !text.contains("\\fonttbl") && !text.contains("\\par") && !text.contains("\\pard")
            {
                return Verdict::Fail(format!(
                    "{} has an RTF header but no document body",
                    path.display()
                ));
            }
            Verdict::Pass
        }
        other => Verdict::Fail(format!("no document validator for {}", other.as_str())),
    }
}

/// Extension a document target is written with.
pub fn document_extension(target: TargetFormat) -> Option<&'static str> {
    Some(match target {
        TargetFormat::Pdf => "pdf",
        TargetFormat::Docx => "docx",
        TargetFormat::Xlsx => "xlsx",
        TargetFormat::Csv => "csv",
        TargetFormat::Html => "html",
        TargetFormat::Md => "md",
        TargetFormat::Txt => "txt",
        TargetFormat::Rtf => "rtf",
        TargetFormat::Pptx => "pptx",
        _ => return None,
    })
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
        // Document targets: the structural validators in this module, not magic
        // bytes, because a docx/xlsx are containers and a md/txt have no magic.
        for t in [
            TargetFormat::Pdf,
            TargetFormat::Docx,
            TargetFormat::Xlsx,
            TargetFormat::Pptx,
            TargetFormat::Csv,
            TargetFormat::Html,
            TargetFormat::Md,
            TargetFormat::Txt,
        ] {
            assert!(
                document_extension(t).is_some(),
                "{t:?} has no document validator"
            );
        }
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("ezc-docverify-{}-{name}", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    const TINY_PDF: &[u8] = b"%PDF-1.7\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF\n";

    #[test]
    fn a_real_pdf_passes_and_counts_pages_without_extracting_text() {
        let p = tmp("tiny.pdf");
        std::fs::write(&p, TINY_PDF).unwrap();
        assert!(document_output(&p, TargetFormat::Pdf).is_pass());
        assert_eq!(count_pdf_pages(TINY_PDF), 1);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn the_measured_silent_garbage_is_a_failure_not_a_success() {
        // The shapes §6.2 N3 can leave behind: a "PDF" that is really the input
        // text, and a zero-byte file. Both must fail.
        let p = tmp("garbage.pdf");
        std::fs::write(&p, b"this is not a docx at all\n").unwrap();
        let v = document_output(&p, TargetFormat::Pdf);
        assert!(!v.is_pass());
        assert!(v.reason().unwrap().contains("%PDF-"), "{v:?}");

        let empty = tmp("empty.pdf");
        std::fs::write(&empty, b"").unwrap();
        assert!(!document_output(&empty, TargetFormat::Pdf).is_pass());
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(&empty);
    }

    #[test]
    fn page_count_falls_back_to_the_pages_tree_count() {
        // Page dictionaries inside an object stream: no `/Type /Page` outside
        // compression, but the root `/Type /Pages` still declares `/Count 3`.
        let packed = b"%PDF-1.7\n8 0 obj\n<< /Type /Pages /Kids [] /Count 3 >>\nendobj\n";
        assert_eq!(count_pdf_pages(packed), 3);
    }

    #[test]
    fn document_containers_are_checked_structurally() {
        use crate::model::test_support::fixture;
        let docx = fixture("sample.docx");
        assert!(
            document_output(&docx, TargetFormat::Docx).is_pass(),
            "the real docx fixture must pass its own validator"
        );
        // The same bytes are not an xlsx.
        assert!(!document_output(&docx, TargetFormat::Xlsx).is_pass());

        // The measured trap fixture: plain text named .docx.
        let fake = fixture("text_renamed.docx");
        assert!(!document_output(&fake, TargetFormat::Docx).is_pass());
        assert!(!document_output(&fake, TargetFormat::Pdf).is_pass());

        let csv = fixture("sample.csv");
        assert!(document_output(&csv, TargetFormat::Csv).is_pass());
        let html = fixture("sample.html");
        assert!(document_output(&html, TargetFormat::Html).is_pass());
        let md = fixture("sample.md");
        assert!(document_output(&md, TargetFormat::Md).is_pass());

        // RTF: header + body, and neither one alone is enough.
        let rtf = fixture("sample.rtf");
        assert!(
            document_output(&rtf, TargetFormat::Rtf).is_pass(),
            "the real rtf fixture must pass its own validator"
        );
        assert!(
            !document_output(&md, TargetFormat::Rtf).is_pass(),
            "markdown text must not pass as rtf"
        );
        let stub = tmp("stub.rtf");
        std::fs::write(&stub, b"{\\rtf1}").unwrap();
        assert!(
            !document_output(&stub, TargetFormat::Rtf).is_pass(),
            "an empty rtf header is not a document"
        );
        let _ = std::fs::remove_file(&stub);
    }
}
