//! Q3 — the end-to-end suite: **one automated run per advertised direction**.
//!
//! Every direction the wheel can offer (`model::matrix`, status `Proven`) and
//! every document direction the LibreOffice pipeline advertises
//! (`engine::soffice::proven_routes`) is executed here against real engines and
//! real fixture files, through the same functions the job worker calls
//! (`engine::convert::convert`, `engine::doc::convert`). Nothing in this file is
//! a list of directions maintained by hand: if the matrix gains a route, this
//! suite grows a row, and if a route's fixture is missing the suite fails.
//!
//! Per direction the suite asserts three things:
//!
//! 1. **existence** — the output file is there and is not empty;
//! 2. **content sanity** — a property a broken conversion cannot fake: decoded
//!    dimensions and a non-blank mean for images, codec + sample rate + duration
//!    for audio, codec + geometry + constant frame rate for video, page count /
//!    container parts / a source marker string for documents;
//! 3. **metadata policy** — `SPEC.md` §5: EXIF/ICC kept by default and gone with
//!    the strip toggle (images), audio tags carried when kept and absent when
//!    stripped. Where the policy cannot apply to a direction (a GIF has no ICC
//!    field; video has no declared metadata field; a document's own properties
//!    are not the policy's) the row says `meta=n/a: <reason>` instead of
//!    pretending a check ran.
//!
//! It also proves the harness **can fail**: `controls()` plants faults into each
//! assertion family (missing output, empty file, wrong container, wrong
//! dimensions, wrong codec, short duration, a text file named `.docx`, a PDF
//! with no pages, metadata that survived a strip, metadata that was dropped by a
//! keep) and requires every planted fault to be reported. A control that is not
//! caught is a `FAIL` in the summary and a non-zero exit — a green run is only
//! green because the checks bite.
//!
//! `--inject=drop-output` deletes the first converted output before it is
//! checked, so CI can also see the real direction loop go red and the process
//! exit 1. It exists for exactly one purpose: to prove the gate is wired.
//!
//! Usage (normally through `scripts/e2e.sh`):
//!   ezc-q3-e2e <fixtures-dir> <work-dir> [--media-only|--docs-only]
//!              [--strict] [--inject=drop-output]
//!
//! Environment: `EZC_VIPS_DIR`, `EZC_FFMPEG_DIR`, `EZC_ENGINE_LIB_DIR`,
//! `EZC_VIPS_PLUGIN_DIR`, `EZC_SOFFICE_DIR` (same overrides as the engine's own
//! discovery), `EZC_E2E_REPORT` (where `report.tsv` / `report.json` go).

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::engine::convert::{self, Job};
use crate::engine::doc::{self, DocJob};
use crate::engine::ffmpeg;
use crate::engine::plan::{self, Arg, Expect, Options, PathRef, Plan, Source, Step};
use crate::engine::proc::{self, EnginePaths, Program};
use crate::engine::verify;
use crate::model::matrix;
use crate::model::types::{CanonicalType, Engine, RouteStatus, TargetFormat, ALL_CANONICAL_TYPES};

// ---------------------------------------------------------------------------
// Row model
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Pass,
    Fail,
    Skip,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Pass => "PASS",
            Kind::Fail => "FAIL",
            Kind::Skip => "SKIP",
        }
    }
}

struct Row {
    family: &'static str,
    direction: String,
    kind: Kind,
    detail: String,
    metadata: String,
    bytes: u64,
    ms: u128,
}

struct Suite {
    paths: EnginePaths,
    fix: PathBuf,
    work: PathBuf,
    rows: Vec<Row>,
    pass: usize,
    fail: usize,
    skip: usize,
    controls_pass: usize,
    controls_fail: usize,
    meta_sources: Vec<(CanonicalType, PathBuf)>,
    /// Fault-injection switch, see the module docs.
    inject: Option<String>,
    injected: bool,
    strict: bool,
}

impl Suite {
    fn new(paths: EnginePaths, fix: PathBuf, work: PathBuf, strict: bool, inject: Option<String>) -> Self {
        Self {
            paths,
            fix,
            work,
            rows: Vec::new(),
            pass: 0,
            fail: 0,
            skip: 0,
            controls_pass: 0,
            controls_fail: 0,
            meta_sources: Vec::new(),
            inject,
            injected: false,
            strict,
        }
    }

    fn fixture(&self, name: &str) -> PathBuf {
        self.fix.join(name)
    }

    fn out_dir(&self) -> PathBuf {
        self.work.join("out")
    }

    fn out(&self, name: &str) -> PathBuf {
        let dir = self.out_dir();
        let _ = std::fs::create_dir_all(&dir);
        dir.join(name)
    }

    fn tmp(&self, name: &str) -> PathBuf {
        self.work.join("tmp").join(name)
    }

    // -- record -------------------------------------------------------------

    fn record(&mut self, family: &'static str, direction: String, kind: Kind, detail: String, metadata: String, bytes: u64, ms: u128) {
        match kind {
            Kind::Pass => self.pass += 1,
            Kind::Fail => self.fail += 1,
            Kind::Skip => self.skip += 1,
        }
        println!(
            "{:<4}  {:<8}  {:<26}  meta={:<22}  {:>8} B  {:>5} ms  {}",
            kind.label(),
            family,
            direction,
            metadata,
            bytes,
            ms,
            detail
        );
        self.rows.push(Row {
            family,
            direction,
            kind,
            detail,
            metadata,
            bytes,
            ms,
        });
    }

    fn ok(&mut self, family: &'static str, direction: String, detail: String, meta: String, bytes: u64, ms: u128) {
        self.record(family, direction, Kind::Pass, detail, meta, bytes, ms);
    }

    fn bad(&mut self, family: &'static str, direction: String, detail: String, ms: u128) {
        self.record(family, direction, Kind::Fail, detail, String::new(), 0, ms);
    }

    fn skipped(&mut self, family: &'static str, direction: String, why: String, ms: u128) {
        self.record(family, direction, Kind::Skip, why, String::new(), 0, ms);
    }

    /// `--inject=drop-output`: make the first converted output disappear before
    /// it is checked, so the gate can be observed failing for real.
    fn maybe_inject(&mut self, output: &Path) {
        if self.injected || self.inject.is_none() {
            return;
        }
        if self.inject.as_deref() == Some("drop-output") {
            if output.is_file() {
                let _ = std::fs::remove_file(output);
                self.injected = true;
                println!("      INJECT drop-output: removed {} before the check", output.display());
            }
        }
    }

    // -- engine probes ------------------------------------------------------

    /// Run one engine tool through the spawn contract and return stdout.
    fn tool(&self, program: Program, args: Vec<Arg>, input: &Path) -> Result<String, String> {
        let plan = Plan {
            steps: vec![Step {
                engine: Engine::Vips,
                program,
                args,
                reads: Source::Input,
                writes: crate::engine::plan::Output::None,
                purpose: "q3 probe",
            }],
            expect: Expect::Image {
                target: TargetFormat::Png,
            },
            tag_merge: None,
            notes: vec![],
            temp_exts: vec![],
        };
        let probe = self.work.join("probe");
        let _ = std::fs::create_dir_all(&probe);
        let invocations = plan.resolve(input, input, &probe).map_err(|e| e.to_string())?;
        let outcome = proc::run(&self.paths, &invocations[0]).map_err(|e| e.to_string())?;
        if let Some(reason) = outcome.is_fatal() {
            return Err(format!("{}: {reason}", program.name()));
        }
        Ok(outcome.stdout)
    }

    /// `vipsheader -a <path>`: width/height/bands/ICC/EXIF presence.
    fn header(&self, path: &Path) -> Result<String, String> {
        self.tool(
            Program::VipsHeader,
            vec![
                Arg::Word("-a"),
                Arg::Path(PathRef::Input, Default::default()),
            ],
            path,
        )
    }

    /// `vips avg <path>`: the mean pixel value, which is 0.0 for a blank image
    /// and non-zero for anything with content.
    fn mean(&self, path: &Path) -> Result<f64, String> {
        let text = self.tool(
            Program::Vips,
            vec![
                Arg::Word("avg"),
                Arg::Path(PathRef::Input, Default::default()),
            ],
            path,
        )?;
        text.trim()
            .parse::<f64>()
            .map_err(|_| format!("vips avg printed {:?}", text.trim()))
    }

    fn probe(&self, path: &Path, video: bool) -> Result<verify::ProbeFacts, String> {
        let args = if video {
            ffmpeg::build_probe_video()
        } else {
            ffmpeg::build_probe_audio()
        };
        let text = self.tool(Program::Ffprobe, args, path)?;
        Ok(verify::parse_probe(&text))
    }

    /// `ffprobe`'s raw tag text — used to prove a tag write really landed in the
    /// container instead of only being reported by the library that wrote it.
    fn tags(&self, path: &Path) -> String {
        self.tool(
            Program::Ffprobe,
            vec![
                Arg::Word("-v"),
                Arg::Word("error"),
                Arg::Word("-show_entries"),
                Arg::Word("format_tags"),
                Arg::Word("-show_entries"),
                Arg::Word("stream_tags"),
                Arg::Word("-of"),
                Arg::Word("default=nw=1"),
                Arg::Path(PathRef::Input, Default::default()),
            ],
            path,
        )
        .unwrap_or_default()
    }

    fn still_facts(&self, path: &Path) -> Option<StillFacts> {
        let header = self.header(path).ok()?;
        StillFacts::parse(&header)
    }
}

// ---------------------------------------------------------------------------
// Expectations and the checks (free functions: the controls call them too)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct StillFacts {
    width: u32,
    height: u32,
    icc: bool,
    orientation: u32,
}

impl StillFacts {
    /// `vipsheader -a` lines: `width: 64`, `height: 48`,
    /// `icc-profile-data: …`, `exif-ifd0-Orientation: 6`.
    fn parse(header: &str) -> Option<Self> {
        let field = |key: &str| -> Option<&str> {
            header
                .lines()
                .find_map(|line| line.trim().strip_prefix(key))
                .map(|value| value.trim())
        };
        let width: u32 = field("width:")?.parse().ok()?;
        let height: u32 = field("height:")?.parse().ok()?;
        Some(Self {
            width,
            height,
            icc: header.contains("icc-profile-data"),
            orientation: orientation_of(header).unwrap_or(1),
        })
    }

    /// What the output's geometry must be. vips autorots and then re-encodes, so
    /// an EXIF orientation of 5–8 swaps the axes — except for HEIF sources,
    /// where libheif has already applied `irot` before vips sees the pixels
    /// (measured: `heic -> jpg, 200x400`).
    fn expected(&self, from: CanonicalType) -> (u32, u32) {
        let swap = (5..=8).contains(&self.orientation)
            && !matches!(from, CanonicalType::Heic | CanonicalType::Avif);
        if swap {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }
}

struct Observed {
    header: String,
    mean: Option<f64>,
}

/// The EXIF orientation tag as a number. vips prints the value with its type,
/// `6 (6, rational)` or `6 (6, short)`, so only the leading integer counts.
fn orientation_of(header: &str) -> Option<u32> {
    let value = header
        .lines()
        .find_map(|line| line.trim().strip_prefix("exif-ifd0-Orientation:"))?
        .trim();
    let digits: String = value.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// Does this target container have an ICC field at all? Measured in
/// `spikes/c2/EVIDENCE.md` §1: jpg/png/webp/tiff keep the profile, avif carries
/// it by contract (`SPEC.md` §9.3 D1), gif has no ICC chunk.
fn carries_icc(to: TargetFormat) -> bool {
    matches!(
        to,
        TargetFormat::Jpg | TargetFormat::Png | TargetFormat::Webp | TargetFormat::Tiff | TargetFormat::Avif
    )
}

/// Existence + container + geometry + "the picture is not blank".
fn check_still(
    out: &Path,
    target: TargetFormat,
    expect: (u32, u32),
    observed: &Observed,
) -> Result<String, String> {
    let exists = verify::exists_nonempty(out);
    if !exists.is_pass() {
        return Err(format!(
            "output missing or empty: {}",
            exists.reason().unwrap_or("absent")
        ));
    }
    let magic = verify::magic_matches(out, target);
    if !magic.is_pass() {
        return Err(format!(
            "container is not {}: {}",
            target.as_str(),
            magic.reason().unwrap_or("wrong family")
        ));
    }
    let facts = StillFacts::parse(&observed.header)
        .ok_or_else(|| format!("cannot decode the output: {:?}", observed.header.lines().next()))?;
    if (facts.width, facts.height) != expect {
        return Err(format!(
            "geometry {}x{} but the source says {}x{}",
            facts.width, facts.height, expect.0, expect.1
        ));
    }
    let mean = match observed.mean {
        Some(mean) => mean,
        None => return Err("the output could not be measured (vips avg failed)".into()),
    };
    if !(0.5..=254.5).contains(&mean) {
        return Err(format!("output is blank (mean pixel {mean:.2})"));
    }
    Ok(format!(
        "{}x{} mean={:.1} {}B",
        facts.width,
        facts.height,
        mean,
        std::fs::metadata(out).map(|m| m.len()).unwrap_or(0)
    ))
}

/// The `SPEC.md` §5 policy for one image direction, in whichever direction the
/// mode asks for.
///
/// ICC is the stable, measurable half: it is carried by jpg/png/webp/tiff/avif
/// and gone after `strip`. EXIF is checked differently on purpose: vips rewrites
/// the block when it autorots (measured: "orientation baked, Orientation
/// rewritten to 1"), so the property worth asserting is that a stale orientation
/// never survives — not that the blob's presence is unchanged.
fn check_metadata(
    mode: &str,
    expect_icc: bool,
    source_orientation: u32,
    observed_icc: bool,
    observed_orientation: Option<u32>,
) -> Result<String, String> {
    if mode == "strip" {
        if observed_icc || observed_orientation.is_some() {
            return Err(format!(
                "strip mode left metadata behind (icc={observed_icc} exif-orientation={observed_orientation:?})"
            ));
        }
        return Ok("stripped".into());
    }
    if observed_icc != expect_icc {
        return Err(format!(
            "keep mode ICC is {observed_icc} but this container should carry it = {expect_icc}"
        ));
    }
    if (5..=8).contains(&source_orientation) {
        match observed_orientation {
            Some(1) | None => {}
            other => {
                return Err(format!(
                    "keep mode kept a stale EXIF orientation {other:?}: the source was Orientation={source_orientation} and the pixels are already rotated"
                ))
            }
        }
    }
    Ok(format!(
        "icc={} exif={}",
        if observed_icc { "kept" } else { "none" },
        match observed_orientation {
            Some(o) => format!("orientation-{o}"),
            None => "none".into(),
        }
    ))
}

struct AudioExpect {
    to: TargetFormat,
    duration_ms: u64,
}

/// The codec `ffmpeg::AudioEncode` picks for a target, as `ffprobe` names it.
fn expected_audio_codec(to: TargetFormat) -> &'static str {
    match to {
        TargetFormat::Mp3 => "mp3",
        TargetFormat::Wav => "pcm_",
        TargetFormat::Flac => "flac",
        TargetFormat::M4a => "aac",
        TargetFormat::Aac => "aac",
        TargetFormat::Ogg => "vorbis",
        TargetFormat::Opus => "opus",
        _ => "",
    }
}

fn check_audio(facts: &verify::ProbeFacts, expect: &AudioExpect) -> Result<String, String> {
    let codec = facts.codec_name.clone().unwrap_or_default();
    let want = expected_audio_codec(expect.to);
    let codec_ok = if want.ends_with('_') {
        codec.starts_with(want)
    } else {
        codec == want
    };
    if !codec_ok {
        return Err(format!("codec {codec:?}, expected {want:?}"));
    }
    let rate = facts.sample_rate.unwrap_or(0);
    if rate < 8000 {
        return Err(format!("sample rate {rate} Hz"));
    }
    if facts.channels.unwrap_or(0) == 0 {
        return Err("no channels".into());
    }
    let ms = facts.duration_ms.unwrap_or(0);
    let tolerance = (expect.duration_ms / 10).max(200);
    let drift = (ms as i64 - expect.duration_ms as i64).abs() as u64;
    if drift > tolerance {
        return Err(format!(
            "duration {ms} ms vs source {} ms (tolerance {tolerance})",
            expect.duration_ms
        ));
    }
    Ok(format!("{codec} {rate} Hz {} ch {ms} ms", facts.channels.unwrap_or(0)))
}

struct VideoExpect {
    to: TargetFormat,
    width: u32,
    height: u32,
    duration_ms: u64,
}

fn expected_video_codec(to: TargetFormat) -> &'static str {
    match to {
        TargetFormat::Mp4 | TargetFormat::Mkv | TargetFormat::Mov => "h264",
        TargetFormat::Webm => "vp9",
        TargetFormat::Gif => "gif",
        _ => "",
    }
}

fn check_video(facts: &verify::ProbeFacts, expect: &VideoExpect) -> Result<String, String> {
    if !facts.has_video {
        return Err("no video stream in the output".into());
    }
    let codec = facts.codec_name.clone().unwrap_or_default();
    let want = expected_video_codec(expect.to);
    if codec != want {
        return Err(format!("codec {codec:?}, expected {want:?}"));
    }
    let valid = verify::probe_is_valid(facts, true);
    if !valid.is_pass() {
        return Err(valid.reason().unwrap_or("invalid stream").to_string());
    }
    if verify::is_vfr(facts) && expect.to != TargetFormat::Gif {
        // GIF has no frame-rate field of its own: the encoder writes per-frame
        // delays, so `r_frame_rate` vs `avg_frame_rate` disagree by design. The
        // constant-rate rule (ARCHITECTURE §6.3) applies to the video containers.
        return Err(format!(
            "variable frame rate: {:?} vs {:?}",
            facts.r_frame_rate, facts.avg_frame_rate
        ));
    }
    let (w, h) = (facts.width.unwrap_or(0), facts.height.unwrap_or(0));
    if expect.to == TargetFormat::Gif {
        // GIF output is scaled to `gif_max_width`; it must keep the aspect.
        if w == 0 || w > expect.width.max(1) || (w as f64 - expect.width as f64).abs() > 1.0 {
            return Err(format!("gif width {w}, source {}", expect.width));
        }
        let want_h = (expect.height as f64 * w as f64 / expect.width.max(1) as f64).round() as u32;
        if (h as i64 - want_h as i64).abs() > 2 {
            return Err(format!("gif {w}x{h}, aspect of {}x{}", expect.width, expect.height));
        }
    } else if (w, h) != (expect.width, expect.height) {
        return Err(format!(
            "geometry {w}x{h}, source {}x{}",
            expect.width, expect.height
        ));
    }
    let ms = facts.duration_ms.unwrap_or(0);
    let tolerance = (expect.duration_ms / 5).max(250);
    if (ms as i64 - expect.duration_ms as i64).abs() as u64 > tolerance {
        return Err(format!("duration {ms} ms vs source {} ms", expect.duration_ms));
    }
    Ok(format!("{codec} {w}x{h} {ms} ms"))
}

fn read_text(path: &Path) -> String {
    String::from_utf8_lossy(&std::fs::read(path).unwrap_or_default()).into_owned()
}

/// Structural + content checks for one document direction. `verify::` is the
/// same validator the pipeline gates `Done` on; the marker checks below add the
/// things only a real conversion produces (a heading that came from the source,
/// the measured `<stem>-<sheet>.csv` naming, page counts).
/// The source marker a fixture is known to carry, for the targets that are
/// extraction-grade text. `sample.docx` and `sample.doc` are the two fixtures
/// with `MARKER-ALPHA` in their body; the HTML/RTF/CSV fixtures are minimal by
/// design, so the check for them is structural only (and saying so is the point:
/// a marker assertion that cannot hold is not an assertion).
fn source_marker(from: CanonicalType) -> Option<&'static str> {
    match from {
        CanonicalType::Docx | CanonicalType::Doc => Some("MARKER-ALPHA"),
        _ => None,
    }
}

fn check_document(
    path: &Path,
    target: TargetFormat,
    name: &str,
    from: CanonicalType,
) -> Result<String, String> {
    let verdict = verify::document_output(path, target);
    if !verdict.is_pass() {
        return Err(format!(
            "structural validator: {}",
            verdict.reason().unwrap_or("invalid")
        ));
    }
    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    match target {
        TargetFormat::Pdf => {
            let data = std::fs::read(path).map_err(|e| e.to_string())?;
            let pages = verify::count_pdf_pages(&data);
            if pages == 0 {
                return Err("no page objects in the PDF".into());
            }
            Ok(format!("{pages} page(s)"))
        }
        TargetFormat::Md => {
            let text = read_text(path);
            if text.trim().is_empty() {
                return Err("empty markdown output".into());
            }
            match source_marker(from) {
                Some(marker) if !text.contains(marker) => {
                    return Err(format!("source marker {marker} missing from the markdown"))
                }
                _ => {}
            }
            // The minimal RTF fixture is a run of text with no heading, so the
            // heading assertion belongs only to sources that have one.
            let heading_expected = matches!(
                from,
                CanonicalType::Docx | CanonicalType::Doc | CanonicalType::Html
            );
            if heading_expected && !text.lines().any(|l| l.trim_start().starts_with('#')) {
                return Err("no heading in the markdown".into());
            }
            Ok(format!(
                "{} B of markdown ({})",
                text.len(),
                if heading_expected {
                    "heading present"
                } else {
                    "text extracted"
                }
            ))
        }
        TargetFormat::Html => {
            let text = read_text(path).to_ascii_lowercase();
            if !text.contains("<html") && !text.contains("<!doctype html") {
                return Err("no html element in the output".into());
            }
            Ok(format!("{} B of html", text.len()))
        }
        TargetFormat::Docx => {
            let data = std::fs::read(path).map_err(|e| e.to_string())?;
            let needle = b"word/document.xml";
            if !data.windows(needle.len()).any(|w| w == needle) {
                return Err("docx container has no word/document.xml".into());
            }
            Ok("docx container with word/document.xml".into())
        }
        TargetFormat::Xlsx => {
            let data = std::fs::read(path).map_err(|e| e.to_string())?;
            let needle = b"xl/workbook.xml";
            if !data.windows(needle.len()).any(|w| w == needle) {
                return Err("xlsx container has no xl/workbook.xml".into());
            }
            Ok("xlsx container with xl/workbook.xml".into())
        }
        TargetFormat::Pptx => {
            let data = std::fs::read(path).map_err(|e| e.to_string())?;
            let needle = b"ppt/presentation.xml";
            if !data.windows(needle.len()).any(|w| w == needle) {
                return Err("pptx container has no ppt/presentation.xml".into());
            }
            Ok("pptx container with ppt/presentation.xml".into())
        }
        TargetFormat::Csv => {
            let text = read_text(path);
            if !text.contains("Widget A") {
                return Err(format!(
                    "source cell Widget A missing (got {:?})",
                    text.chars().take(80).collect::<String>()
                ));
            }
            Ok(format!("{bytes} B of csv as {name}, source cell present"))
        }
        _ => {
            let text = read_text(path);
            if text.trim().is_empty() {
                return Err("empty text output".into());
            }
            Ok(format!("{} B of text", text.len()))
        }
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// The fixture that carries each input type. `SPEC.md` §8.3: every advertised
/// direction has a fixture — a type with no fixture is a suite FAIL, not a skip.
fn fixture_for(ty: CanonicalType) -> Option<&'static str> {
    Some(match ty {
        CanonicalType::Jpeg => "sample.jpg",
        CanonicalType::Png => "sample.png",
        CanonicalType::WebP => "sample.webp",
        CanonicalType::Tiff => "sample.tif",
        CanonicalType::Heic => "sample.heic",
        CanonicalType::Avif => "sample.avif",
        CanonicalType::Gif => "sample.gif",
        CanonicalType::Bmp => "sample.bmp",
        CanonicalType::Svg => "sample.svg",
        CanonicalType::Pdf => "sample.pdf",
        CanonicalType::Mp3 => "sample.mp3",
        CanonicalType::Wav => "sample.wav",
        CanonicalType::Flac => "sample.flac",
        CanonicalType::M4a => "sample.m4a",
        CanonicalType::Aac => "sample.aac",
        CanonicalType::Ogg => "sample.ogg",
        CanonicalType::Opus => "sample.opus",
        CanonicalType::Mp4 => "sample.mp4",
        CanonicalType::Mkv => "sample.mkv",
        CanonicalType::Mov => "sample.mov",
        CanonicalType::WebM => "sample.webm",
        CanonicalType::Docx => "sample.docx",
        CanonicalType::Xlsx => "sample.xlsx",
        CanonicalType::Pptx => "sample.pptx",
        CanonicalType::Csv => "sample.csv",
        CanonicalType::Html => "sample.html",
        CanonicalType::Doc => "sample.doc",
        CanonicalType::Xls => "sample.xls",
        CanonicalType::Ppt => "sample.ppt",
        CanonicalType::Rtf => "sample.rtf",
        CanonicalType::Markdown => "sample.md",
        CanonicalType::Txt => "sample.txt",
        CanonicalType::Unknown => return None,
    })
}

fn family_of(ty: CanonicalType) -> &'static str {
    match ty {
        CanonicalType::Jpeg
        | CanonicalType::Png
        | CanonicalType::Heic
        | CanonicalType::Avif
        | CanonicalType::WebP
        | CanonicalType::Gif
        | CanonicalType::Tiff
        | CanonicalType::Bmp
        | CanonicalType::Svg
        | CanonicalType::Pdf => "image",
        CanonicalType::Mp3
        | CanonicalType::Wav
        | CanonicalType::Flac
        | CanonicalType::M4a
        | CanonicalType::Aac
        | CanonicalType::Ogg
        | CanonicalType::Opus => "audio",
        CanonicalType::Mp4 | CanonicalType::Mkv | CanonicalType::Mov | CanonicalType::WebM => {
            "video"
        }
        _ => "document",
    }
}

/// Every advertised media direction, read out of the product's own matrix.
/// Document inputs are excluded here: their directions come from the
/// LibreOffice pipeline's own table (see [`advertised_docs`]) and are
/// cross-checked against the matrix rows in `preflight`.
fn advertised_media() -> Vec<(CanonicalType, TargetFormat, Engine)> {
    let mut out = Vec::new();
    for ty in ALL_CANONICAL_TYPES {
        if crate::engine::soffice::is_document(*ty) {
            continue;
        }
        for route in matrix::routes_for(*ty) {
            if route.status.is_proven() {
                out.push((*ty, route.target, route.engine));
            }
        }
    }
    out
}

/// The document directions the LibreOffice pipeline advertises.
fn advertised_docs() -> Vec<(CanonicalType, TargetFormat)> {
    let mut out: Vec<(CanonicalType, TargetFormat)> =
        crate::engine::soffice::proven_routes().to_vec();
    out.sort_by_key(|(from, to)| format!("{from:?}{to:?}"));
    out.dedup();
    out
}

/// The matrix's document rows with `Proven` status, for the cross-check against
/// the pipeline's own table.
fn matrix_proven_docs() -> Vec<(CanonicalType, TargetFormat)> {
    let mut out: Vec<(CanonicalType, TargetFormat)> = Vec::new();
    for ty in ALL_CANONICAL_TYPES {
        if !crate::engine::soffice::is_document(*ty) {
            continue;
        }
        for route in matrix::routes_for(*ty) {
            if route.status.is_proven() {
                out.push((*ty, route.target));
            }
        }
    }
    out.sort_by_key(|(from, to)| format!("{from:?}{to:?}"));
    out.dedup();
    out
}

// ---------------------------------------------------------------------------
// Preflight: the suite must be unable to run a stale or inconsistent claim
// ---------------------------------------------------------------------------

impl Suite {
    fn preflight(&mut self) {
        // The policy this suite asserts is "keep by default".
        if Options::default().strip_metadata {
            self.bad("policy", "precondition".into(), "Options::default() strips metadata".into(), 0);
        } else {
            self.ok(
                "policy",
                "precondition".into(),
                "Options::default().strip_metadata == false (SPEC §5 keep-by-default)".into(),
                "keep=default".into(),
                0,
                0,
            );
        }

        // 1. No duplicate directions.
        let media = advertised_media();
        let mut duplicates = 0;
        let mut seen: Vec<(CanonicalType, TargetFormat)> = Vec::new();
        for (from, to, _) in &media {
            if seen.contains(&(*from, *to)) {
                duplicates += 1;
                self.bad(
                    "preflight",
                    format!("{from}->{}", to.as_str()),
                    "the matrix lists this direction twice".into(),
                    0,
                );
            }
            seen.push((*from, *to));
        }
        if duplicates == 0 {
            self.ok(
                "preflight",
                "no duplicate directions".into(),
                format!("{} advertised media directions are unique", media.len()),
                "".into(),
                0,
                0,
            );
        }

        // 2. Every advertised direction has a fixture on disk — for documents as
        //    well, since `SPEC.md` §8.3 makes a fixture part of the claim.
        let mut missing: Vec<String> = Vec::new();
        let mut inputs: Vec<CanonicalType> = media.iter().map(|(from, _, _)| *from).collect();
        inputs.extend(advertised_docs().iter().map(|(from, _)| *from));
        inputs.sort_by_key(|ty| format!("{ty:?}"));
        inputs.dedup();
        for ty in &inputs {
            match fixture_for(*ty) {
                Some(name) if self.fixture(name).is_file() => {}
                Some(name) => missing.push(format!("{ty}: fixtures/{name} is absent")),
                None => missing.push(format!("{ty}: no fixture is mapped at all")),
            }
        }
        for gap in &missing {
            self.bad("preflight", gap.clone(), "advertised direction has no fixture".into(), 0);
        }
        if missing.is_empty() {
            self.ok(
                "preflight",
                "fixture per input type".into(),
                format!("{} input types resolve to a fixture file", inputs.len()),
                "".into(),
                0,
                0,
            );
        }

        // 3. Every advertised media direction is actually routed by the planner,
        //    with the engine the matrix names. This is the check that would have
        //    caught the planner's old `SameFormat` refusal of twelve advertised
        //    identity targets before a user did.
        let mut unrouted = 0;
        for (from, to, engine) in &media {
            match plan::engine_for(*from, *to) {
                Some(found) if found == *engine => {}
                Some(found) => {
                    unrouted += 1;
                    self.bad(
                        "preflight",
                        format!("{from}->{}", to.as_str()),
                        format!("matrix says {engine:?}, the planner says {found:?}"),
                        0,
                    );
                }
                None => {
                    unrouted += 1;
                    self.bad(
                        "preflight",
                        format!("{from}->{}", to.as_str()),
                        "advertised but the planner routes no engine to it".into(),
                        0,
                    );
                }
            }
        }
        if unrouted == 0 {
            self.ok(
                "preflight",
                "every advertised direction is routed".into(),
                format!("{unrouted} unrouted of {} advertised", media.len()),
                "".into(),
                0,
                0,
            );
        }

        // 4. The document sets must agree: the wheel's matrix and the pipeline's
        //    own proven table are two independent spellings of the same claim.
        let docs = advertised_docs();
        let matrix_docs = matrix_proven_docs();
        let only_in_pipeline: Vec<_> = docs.iter().filter(|d| !matrix_docs.contains(d)).collect();
        let only_in_matrix: Vec<_> = matrix_docs.iter().filter(|d| !docs.contains(d)).collect();
        if only_in_pipeline.is_empty() && only_in_matrix.is_empty() {
            self.ok(
                "preflight",
                "document sets agree".into(),
                format!(
                    "{} directions in both matrix and engine::soffice::proven_routes",
                    docs.len()
                ),
                "".into(),
                0,
                0,
            );
        } else {
            self.bad(
                "preflight",
                "document sets agree".into(),
                format!(
                    "pipeline-only {only_in_pipeline:?}; matrix-only {only_in_matrix:?}"
                ),
                0,
            );
        }

        // 5. Every advertised route cites evidence that still exists and still
        //    contains the marker literal. A route pointing at a dead transcript
        //    is a claim with no evidence.
        let mut dead = 0;
        for ty in ALL_CANONICAL_TYPES {
            for route in matrix::routes_for(*ty) {
                if let RouteStatus::Proven { evidence } = route.status {
                    let source = self.repo_root().join(evidence.source);
                    let text = read_text(&source);
                    if text.is_empty() || !text.contains(evidence.marker) {
                        dead += 1;
                        self.bad(
                            "preflight",
                            format!("{ty}->{}", route.target.as_str()),
                            format!(
                                "evidence {} ({}) is missing or no longer contains {:?}",
                                evidence.label, evidence.source, evidence.marker
                            ),
                            0,
                        );
                    }
                }
            }
        }
        if dead == 0 {
            self.ok(
                "preflight",
                "evidence still says so".into(),
                "every advertised route's evidence file contains its marker literal".into(),
                "".into(),
                0,
                0,
            );
        }
    }

    /// `fixtures/` and `evidence/` live at the repo root; the work dir does not.
    fn repo_root(&self) -> PathBuf {
        self.fix
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

// ---------------------------------------------------------------------------
// Directions
// ---------------------------------------------------------------------------

impl Suite {
    fn still_direction(&mut self, from: CanonicalType, to: TargetFormat) {
        let family = "image";
        let direction = format!("{from}->{}", to.as_str());
        let Some(fixture) = fixture_for(from) else {
            self.bad(family, direction, "no fixture for this input type".into(), 0);
            return;
        };
        let input = self.fixture(fixture);
        let facts = self.still_facts(&input);
        let expect = facts.map(|f| f.expected(from));
        let output = self.out(&format!("{}-to-{}.{}", from.as_str(), to.as_str(), to.extension()));
        let temp = self.tmp(&format!("{}-to-{}", from.as_str(), to.as_str()));
        let started = Instant::now();
        let job = Job {
            from,
            to,
            options: Options::default(),
        };
        let bytes;
        let mut detail;
        match convert::convert(&self.paths, &job, &input, &output, &temp) {
            Ok(report) => {
                bytes = report.output_bytes;
                self.maybe_inject(&output);
                let observed = Observed {
                    header: self.header(&output).unwrap_or_default(),
                    mean: self.mean(&output).ok(),
                };
                match expect {
                    Some(expect) => match check_still(&output, to, expect, &observed) {
                        Ok(d) => detail = d,
                        Err(why) => {
                            self.bad(family, direction, why, started.elapsed().as_millis());
                            return;
                        }
                    },
                    // The source cannot be opened on this dist: the *content*
                    // check cannot run either, so this is not a direction result.
                    None => {
                        let why = self
                            .header(&input)
                            .err()
                            .unwrap_or_else(|| "the source could not be measured".into());
                        self.skipped(
                            family,
                            direction,
                            format!("source not decodable on this dist: {why}"),
                            started.elapsed().as_millis(),
                        );
                        return;
                    }
                }
                let ms = started.elapsed().as_millis();
                let meta = self.still_metadata(from, to);
                match meta {
                    Ok(note) => self.ok(family, direction, detail, note, bytes, ms),
                    Err(why) => {
                        detail.push(' ');
                        self.bad(family, direction, format!("{detail}; metadata policy: {why}"), ms)
                    }
                }
            }
            Err(e) => {
                let text = e.to_string();
                let ms = started.elapsed().as_millis();
                match heif_engine_gap(&text, from, to) {
                    Some(why) => self.skipped(family, direction, why, ms),
                    None => self.bad(family, direction, format!("{text}"), ms),
                }
            }
        }
    }

    /// `SPEC.md` §5 metadata policy for one still direction, measured on a
    /// metadata-bearing source of the same input type (so the assertion is real
    /// even when the plain fixture carries no EXIF/ICC).
    fn still_metadata(&mut self, from: CanonicalType, to: TargetFormat) -> Result<String, String> {
        let Some(source) = self
            .meta_sources
            .iter()
            .find(|(ty, _)| *ty == from)
            .map(|(_, p)| p.clone())
        else {
            return Ok(format!(
                "n/a: no {} fixture carrying EXIF/ICC can be produced on this dist",
                from.as_str()
            ));
        };
        let src_facts = self
            .still_facts(&source)
            .ok_or_else(|| format!("metadata source {} unreadable", source.display()))?;

        // keep (default): the profile survives where the container has a field.
        let kept = self.out(&format!(
            "meta-{}-to-{}-keep.{}",
            from.as_str(),
            to.as_str(),
            to.extension()
        ));
        let keep = convert::convert(
            &self.paths,
            &Job {
                from,
                to,
                options: Options::default(),
            },
            &source,
            &kept,
            &self.tmp(&format!("meta-keep-{}-{}", from.as_str(), to.as_str())),
        )
        .map_err(|e| format!("keep run failed: {e}"))?;
        let _ = keep;
        let kept_header = self.header(&kept).map_err(|e| format!("keep output unreadable: {e}"))?;
        check_metadata(
            "keep",
            src_facts.icc && carries_icc(to),
            src_facts.orientation,
            kept_header.contains("icc-profile-data"),
            orientation_of(&kept_header),
        )?;

        // strip: nothing survives, in any container.
        let stripped = self.out(&format!(
            "meta-{}-to-{}-strip.{}",
            from.as_str(),
            to.as_str(),
            to.extension()
        ));
        convert::convert(
            &self.paths,
            &Job {
                from,
                to,
                options: Options {
                    strip_metadata: true,
                    ..Options::default()
                },
            },
            &source,
            &stripped,
            &self.tmp(&format!("meta-strip-{}-{}", from.as_str(), to.as_str())),
        )
        .map_err(|e| format!("strip run failed: {e}"))?;
        let strip_header = self
            .header(&stripped)
            .map_err(|e| format!("strip output unreadable: {e}"))?;
        check_metadata(
            "strip",
            false,
            0,
            strip_header.contains("icc-profile-data"),
            orientation_of(&strip_header),
        )?;
        Ok(format!("icc={} strip=clean", if carries_icc(to) { "kept" } else { "n/a" }))
    }

    fn audio_direction(&mut self, from: CanonicalType, to: TargetFormat) {
        let family = "audio";
        let direction = format!("{from}->{}", to.as_str());
        let Some(fixture) = fixture_for(from) else {
            self.bad(family, direction, "no fixture for this input type".into(), 0);
            return;
        };
        let input = self.fixture(fixture);
        let source = match self.probe(&input, false) {
            Ok(facts) => facts,
            Err(e) => {
                self.bad(family, direction, format!("source probe failed: {e}"), 0);
                return;
            }
        };
        let output = self.out(&format!("{}-to-{}.{}", from.as_str(), to.as_str(), to.extension()));
        let started = Instant::now();
        let job = Job {
            from,
            to,
            options: Options::default(),
        };
        match convert::convert(&self.paths, &job, &input, &output, &self.tmp(&format!("{}-to-{}", from.as_str(), to.as_str()))) {
            Ok(report) => {
                self.maybe_inject(&output);
                let facts = match self.probe(&output, false) {
                    Ok(f) => f,
                    Err(e) => {
                        let ms = started.elapsed().as_millis();
                        self.bad(family, direction, format!("output probe failed: {e}"), ms);
                        return;
                    }
                };
                let expect = AudioExpect {
                    to,
                    duration_ms: source.duration_ms.unwrap_or(2000),
                };
                let detail = match check_audio(&facts, &expect) {
                    Ok(d) => d,
                    Err(why) => {
                        let ms = started.elapsed().as_millis();
                        self.bad(family, direction, why, ms);
                        return;
                    }
                };
                let ms = started.elapsed().as_millis();
                let meta = self.audio_metadata(to);
                match meta {
                    Ok(note) => self.ok(family, direction, detail, note, report.output_bytes, ms),
                    Err(why) => self.bad(
                        family,
                        direction,
                        format!("{detail}; metadata policy: {why}"),
                        ms,
                    ),
                }
            }
            Err(e) => {
                let ms = started.elapsed().as_millis();
                self.bad(family, direction, format!("{e}"), ms);
            }
        }
    }

    /// `SPEC.md` §5 for audio: tags ride along by default and are gone with the
    /// strip toggle. Both halves are read back with `ffprobe`, not trusted from
    /// the library that wrote them.
    fn audio_metadata(&mut self, to: TargetFormat) -> Result<String, String> {
        let tagged = self.fixture("tagged.mp3");
        if !tagged.is_file() {
            return Ok("n/a: no tagged fixture".into());
        }
        let kept = self.out(&format!("tags-to-{}-keep.{}", to.as_str(), to.extension()));
        convert::convert(
            &self.paths,
            &Job {
                from: CanonicalType::Mp3,
                to,
                options: Options::default(),
            },
            &tagged,
            &kept,
            &self.tmp(&format!("tags-keep-{}", to.as_str())),
        )
        .map_err(|e| format!("tagged keep run failed: {e}"))?;
        let kept_tags = self.tags(&kept);
        if !kept_tags.to_lowercase().contains("title=") {
            return Err(format!(
                "keep mode: the tag write did not survive the container readback ({:?})",
                kept_tags.lines().take(4).collect::<Vec<_>>()
            ));
        }
        let stripped = self.out(&format!("tags-to-{}-strip.{}", to.as_str(), to.extension()));
        convert::convert(
            &self.paths,
            &Job {
                from: CanonicalType::Mp3,
                to,
                options: Options {
                    strip_metadata: true,
                    ..Options::default()
                },
            },
            &tagged,
            &stripped,
            &self.tmp(&format!("tags-strip-{}", to.as_str())),
        )
        .map_err(|e| format!("tagged strip run failed: {e}"))?;
        let strip_tags = self.tags(&stripped);
        if strip_tags.to_lowercase().contains("title=") {
            return Err("strip mode: the title tag is still in the output".into());
        }
        Ok("tags=carried strip=clean".into())
    }

    fn video_direction(&mut self, from: CanonicalType, to: TargetFormat) {
        let family = "video";
        let direction = format!("{from}->{}", to.as_str());
        let Some(fixture) = fixture_for(from) else {
            self.bad(family, direction, "no fixture for this input type".into(), 0);
            return;
        };
        let input = self.fixture(fixture);
        let source = match self.probe(&input, true) {
            Ok(facts) => facts,
            Err(e) => {
                self.bad(family, direction, format!("source probe failed: {e}"), 0);
                return;
            }
        };
        let output = self.out(&format!("{}-to-{}.{}", from.as_str(), to.as_str(), to.extension()));
        let started = Instant::now();
        let job = Job {
            from,
            to,
            options: Options::default(),
        };
        match convert::convert(&self.paths, &job, &input, &output, &self.tmp(&format!("{}-to-{}", from.as_str(), to.as_str()))) {
            Ok(report) => {
                self.maybe_inject(&output);
                let facts = match self.probe(&output, true) {
                    Ok(f) => f,
                    Err(e) => {
                        let ms = started.elapsed().as_millis();
                        self.bad(family, direction, format!("output probe failed: {e}"), ms);
                        return;
                    }
                };
                let expect = VideoExpect {
                    to,
                    width: source.width.unwrap_or(0),
                    height: source.height.unwrap_or(0),
                    duration_ms: source.duration_ms.unwrap_or(2000),
                };
                let ms = started.elapsed().as_millis();
                match check_video(&facts, &expect) {
                    Ok(detail) => self.ok(
                        family,
                        direction,
                        detail,
                        "n/a: SPEC §5 names no video metadata field".into(),
                        report.output_bytes,
                        ms,
                    ),
                    Err(why) => self.bad(family, direction, why, ms),
                }
            }
            Err(e) => {
                let ms = started.elapsed().as_millis();
                self.bad(family, direction, format!("{e}"), ms);
            }
        }
    }

    fn doc_direction(&mut self, from: CanonicalType, to: TargetFormat) {
        let family = "document";
        let direction = format!("{from}->{}", to.as_str());
        let Some(fixture) = fixture_for(from) else {
            self.bad(family, direction, "no fixture for this input type".into(), 0);
            return;
        };
        let input = self.fixture(fixture);
        let ext = verify::document_extension(to).unwrap_or("bin");
        let name = format!("{}-to-{}.{}", from.as_str(), to.as_str(), ext);
        let output = self.out(&name);
        let started = Instant::now();
        match doc::convert(
            &self.paths,
            &DocJob { from, to },
            &input,
            &output,
            &self.tmp(&format!("doc-{}-to-{}", from.as_str(), to.as_str())),
        ) {
            Ok(_) => {
                self.maybe_inject(&output);
                let ms = started.elapsed().as_millis();
                let bytes = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
                // The pptx/ppt fixtures are the only two-page documents here.
                if to == TargetFormat::Pdf && matches!(from, CanonicalType::Pptx | CanonicalType::Ppt) {
                    let data = std::fs::read(&output).unwrap_or_default();
                    let pages = verify::count_pdf_pages(&data);
                    if pages != 2 {
                        self.bad(family, direction, format!("expected 2 pages, found {pages}"), ms);
                        return;
                    }
                }
                match check_document(&output, to, &name, from) {
                    Ok(detail) => self.ok(
                        family,
                        direction,
                        detail,
                        "n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags".into(),
                        bytes,
                        ms,
                    ),
                    Err(why) => self.bad(family, direction, format!("content check: {why}"), ms),
                }
            }
            Err(e) => {
                let ms = started.elapsed().as_millis();
                self.bad(family, direction, format!("{e}"), ms);
            }
        }
    }
}

/// The Linux test dist ships vips without a HEIF module, and this box has no
/// AV1 encoder: those directions are engine-availability gaps, not pipeline
/// failures. The engine's own words go in the SKIP line.
fn heif_engine_gap(text: &str, from: CanonicalType, to: TargetFormat) -> Option<String> {
    let heif = matches!(from, CanonicalType::Heic | CanonicalType::Avif)
        || to == TargetFormat::Avif
        || to == TargetFormat::Heic;
    if !heif {
        return None;
    }
    let words = [
        "not a known file format",
        "heifsave",
        "heif",
        "Unsupported compression",
        "no decoding plugin",
    ];
    if words.iter().any(|w| text.contains(w)) {
        Some(format!(
            "engine cannot do this on this dist (heif/av1 unavailable): {text}"
        ))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// The controls: proof that every check can fail
// ---------------------------------------------------------------------------

impl Suite {
    /// Each control plants a fault and requires the *real* check to reject it.
    fn controls(&mut self) {
        println!("\n--- harness controls (a green suite must also prove it can go red) ---");
        let dir = self.work.join("controls");
        let _ = std::fs::create_dir_all(&dir);

        // 1. missing output
        let absent = dir.join("absent.png");
        let _ = std::fs::remove_file(&absent);
        self.expect_caught("missing output", &absent, TargetFormat::Png, (64, 48), "existence");

        // 2. zero-byte output
        let empty = dir.join("empty.png");
        std::fs::write(&empty, b"").unwrap();
        self.expect_caught("zero-byte output", &empty, TargetFormat::Png, (64, 48), "existence");

        // 3. wrong container: JPEG bytes under a .png name
        let wrong = dir.join("wrong-family.png");
        std::fs::copy(self.fixture("sample.jpg"), &wrong).unwrap();
        self.expect_caught("wrong container", &wrong, TargetFormat::Png, (64, 48), "magic");

        // 4. wrong geometry: a real PNG, but the source says 200x400
        let small = dir.join("wrong-dims.png");
        std::fs::copy(self.fixture("sample.png"), &small).unwrap();
        self.expect_caught("wrong geometry", &small, TargetFormat::Png, (200, 400), "geometry");

        // 5. a text file where a document belongs
        let text = dir.join("fake.docx");
        std::fs::write(&text, b"this is not a zip and never was\n").unwrap();
        self.expect_doc_caught("text file named .docx", &text, TargetFormat::Docx, "document part");

        // 6. a PDF header with no page objects
        let fake_pdf = dir.join("no-pages.pdf");
        std::fs::write(&fake_pdf, b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\ntrailer<<>>\n%%EOF\n").unwrap();
        self.expect_doc_caught("pdf with no pages", &fake_pdf, TargetFormat::Pdf, "pdf pages");

        // 7. metadata policy: a stripped copy offered to a keep expectation
        let src = self.fixture("exif_icc.jpg");
        let has_icc = self
            .header(&src)
            .map(|h| h.contains("icc-profile-data"))
            .unwrap_or(false);
        if has_icc {
            let stripped = dir.join("stripped.png");
            let _ = convert::convert(
                &self.paths,
                &Job {
                    from: CanonicalType::Jpeg,
                    to: TargetFormat::Png,
                    options: Options {
                        strip_metadata: true,
                        ..Options::default()
                    },
                },
                &src,
                &stripped,
                &dir.join("tmp"),
            );
            let header = self.header(&stripped).unwrap_or_default();
            let outcome = check_metadata(
                "keep",
                true,
                6,
                header.contains("icc-profile-data"),
                orientation_of(&header),
            );
            self.check_control("metadata dropped by a keep run", outcome, "metadata keep");

            // and the reverse: a kept copy offered to a strip expectation
            let kept = dir.join("kept.png");
            let _ = convert::convert(
                &self.paths,
                &Job {
                    from: CanonicalType::Jpeg,
                    to: TargetFormat::Png,
                    options: Options::default(),
                },
                &src,
                &kept,
                &dir.join("tmp2"),
            );
            let header = self.header(&kept).unwrap_or_default();
            let outcome = check_metadata(
                "strip",
                false,
                0,
                header.contains("icc-profile-data"),
                orientation_of(&header),
            );
            self.check_control("metadata surviving a strip run", outcome, "metadata strip");
        } else {
            self.control_not_run("metadata controls", "no ICC-bearing fixture to plant");
        }

        // 8. audio: a WAV offered to a codec expectation of mp3
        let wav = self.fixture("sample.wav");
        let mut wav_facts = self.probe(&wav, false).unwrap_or_default();
        let outcome = check_audio(
            &wav_facts,
            &AudioExpect {
                to: TargetFormat::Mp3,
                duration_ms: wav_facts.duration_ms.unwrap_or(2000),
            },
        );
        self.check_control("wrong audio codec", outcome, "audio codec");

        // 9. audio: a 2 s source described as 10 s
        wav_facts.duration_ms = Some(wav_facts.duration_ms.unwrap_or(2000));
        let outcome = check_audio(
            &wav_facts,
            &AudioExpect {
                to: TargetFormat::Wav,
                duration_ms: wav_facts.duration_ms.unwrap_or(0) + 8000,
            },
        );
        self.check_control("duration drift", outcome, "audio duration");

        // 10. video: an audio file offered to the video check
        let audio_facts = self.probe(&wav, true).unwrap_or_default();
        let outcome = check_video(
            &audio_facts,
            &VideoExpect {
                to: TargetFormat::Mp4,
                width: 64,
                height: 48,
                duration_ms: 2000,
            },
        );
        self.check_control("no video stream", outcome, "video stream");

        // 11. the advertised/fixture gap itself: Unknown has no fixture
        let gap = fixture_for(CanonicalType::Unknown).is_none();
        self.check_control(
            "advertised direction with no fixture",
            if gap { Err("no fixture".to_string()) } else { Ok(String::new()) },
            "fixture resolution",
        );

        // 12. the document set comparison must notice a one-sided claim. The
        //     planted pair has to be one the pipeline really does not advertise,
        //     so it is picked from the cuts rather than hard-coded.
        let a = advertised_docs();
        let candidates = [
            (CanonicalType::Markdown, TargetFormat::Pptx),
            (CanonicalType::Txt, TargetFormat::Xlsx),
            (CanonicalType::Csv, TargetFormat::Pptx),
            (CanonicalType::Html, TargetFormat::Pptx),
        ];
        match candidates.iter().find(|pair| !a.contains(pair)) {
            Some(pair) => {
                let mut b = a.clone();
                b.push(*pair);
                let differ = diff_docs(&a, &b);
                self.check_control(
                    "one-sided document advertisement",
                    if differ { Err("sets differ".to_string()) } else { Ok(String::new()) },
                    "document set equality",
                );
            }
            None => self.control_not_run(
                "one-sided document advertisement",
                "every candidate pair is already advertised",
            ),
        }
    }

    fn expect_caught(
        &mut self,
        name: &str,
        path: &Path,
        target: TargetFormat,
        expect: (u32, u32),
        what: &str,
    ) {
        let observed = Observed {
            header: self.header(path).unwrap_or_default(),
            mean: self.mean(path).ok(),
        };
        let outcome = check_still(path, target, expect, &observed);
        self.check_control(name, outcome, what);
    }

    fn expect_doc_caught(&mut self, name: &str, path: &Path, target: TargetFormat, what: &str) {
        let outcome = check_document(path, target, "control", CanonicalType::Docx);
        self.check_control(name, outcome, what);
    }

    /// A control passes when the planted fault was **reported**. A control that
    /// returns `Ok` means the suite would have called a broken output good.
    fn check_control(&mut self, name: &str, outcome: Result<String, String>, what: &str) {
        match outcome {
            Err(why) => {
                self.controls_pass += 1;
                println!("PASS(control)  {name:<34} caught by {what}: {why}");
            }
            Ok(detail) => {
                self.controls_fail += 1;
                println!(
                    "FAIL(control)  {name:<34} NOT caught by {what}: the check accepted it ({detail})"
                );
            }
        }
    }

    fn control_not_run(&mut self, name: &str, why: &str) {
        self.controls_fail += 1;
        println!("FAIL(control)  {name:<34} could not be planted: {why}");
    }
}

fn diff_docs(a: &[(CanonicalType, TargetFormat)], b: &[(CanonicalType, TargetFormat)]) -> bool {
    a.iter().any(|d| !b.contains(d)) || b.iter().any(|d| !a.contains(d))
}

// ---------------------------------------------------------------------------
// Metadata-bearing sources (synthesised once, with the real pipeline)
// ---------------------------------------------------------------------------

impl Suite {
    fn prepare_meta_sources(&mut self) {
        let base = self.fixture("exif_icc.jpg");
        if !base.is_file() {
            return;
        }
        self.meta_sources.push((CanonicalType::Jpeg, base.clone()));
        for to in [TargetFormat::Png, TargetFormat::Webp, TargetFormat::Tiff] {
            let target = match to {
                TargetFormat::Png => CanonicalType::Png,
                TargetFormat::Webp => CanonicalType::WebP,
                TargetFormat::Tiff => CanonicalType::Tiff,
                _ => continue,
            };
            let path = self.work.join(format!("meta-src.{}", to.extension()));
            match convert::convert(
                &self.paths,
                &Job {
                    from: CanonicalType::Jpeg,
                    to,
                    options: Options::default(),
                },
                &base,
                &path,
                &self.tmp(&format!("meta-src-{}", to.as_str())),
            ) {
                Ok(_) if self.header(&path).map(|h| h.contains("icc-profile-data")).unwrap_or(false) => {
                    self.meta_sources.push((target, path));
                }
                _ => println!(
                    "      note: no metadata-bearing {} source could be produced on this dist (policy rows will say n/a)",
                    to.as_str()
                ),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

impl Suite {
    fn report(&self) {
        let dir = std::env::var("EZC_E2E_REPORT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| self.work.join("report"));
        if std::fs::create_dir_all(&dir).is_err() {
            println!("(could not write the report to {})", dir.display());
            return;
        }

        let mut tsv = String::from("family\tdirection\tverdict\tmetadata\tbytes\tms\tdetail\n");
        for row in &self.rows {
            tsv.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                row.family,
                row.direction,
                row.kind.label(),
                row.metadata,
                row.bytes,
                row.ms,
                row.detail.replace('\t', " ")
            ));
        }
        let _ = std::fs::write(dir.join("report.tsv"), &tsv);

        let mut md = String::from(
            "| direction | family | verdict | metadata policy | detail |\n|---|---|---|---|---|\n",
        );
        for row in &self.rows {
            md.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                row.direction,
                row.family,
                row.kind.label(),
                if row.metadata.is_empty() { "—".into() } else { row.metadata.clone() },
                row.detail.replace('|', "\\|")
            ));
        }
        let _ = std::fs::write(dir.join("matrix.md"), &md);

        let json = format!(
            "{{\n  \"advertised\": {},\n  \"pass\": {},\n  \"fail\": {},\n  \"skip\": {},\n  \"controls_pass\": {},\n  \"controls_fail\": {},\n  \"green\": {},\n  \"strict\": {},\n  \"rows\": [\n{}\n  ]\n}}\n",
            self.pass + self.fail + self.skip,
            self.pass,
            self.fail,
            self.skip,
            self.controls_pass,
            self.controls_fail,
            self.fail == 0 && self.controls_fail == 0,
            self.strict,
            self.rows
                .iter()
                .map(|r| format!(
                    "    {{\"direction\": \"{}\", \"family\": \"{}\", \"verdict\": \"{}\", \"metadata\": \"{}\", \"bytes\": {}, \"ms\": {}, \"detail\": \"{}\"}}",
                    esc(&r.direction),
                    r.family,
                    r.kind.label(),
                    esc(&r.metadata),
                    r.bytes,
                    r.ms,
                    esc(&r.detail)
                ))
                .collect::<Vec<_>>()
                .join(",\n")
        );
        let _ = std::fs::write(dir.join("report.json"), json);
        println!("\nreport: {}/report.tsv, report.json, matrix.md", dir.display());
    }

    fn summary(&self) -> i32 {
        println!("\n--- per-family summary ---");
        println!("{:<10} {:>10} {:>6} {:>6} {:>6}", "family", "advertised", "pass", "fail", "skip");
        for family in ["image", "audio", "video", "document"] {
            let rows: Vec<&Row> = self.rows.iter().filter(|r| r.family == family).collect();
            if rows.is_empty() {
                continue;
            }
            let pass = rows.iter().filter(|r| r.kind == Kind::Pass).count();
            let fail = rows.iter().filter(|r| r.kind == Kind::Fail).count();
            let skip = rows.iter().filter(|r| r.kind == Kind::Skip).count();
            println!("{:<10} {:>10} {:>6} {:>6} {:>6}", family, rows.len(), pass, fail, skip);
        }
        let controls = self.controls_pass + self.controls_fail;
        println!(
            "\nadvertised={} pass={} fail={} skip={} controls={}/{} caught",
            self.pass + self.fail + self.skip,
            self.pass,
            self.fail,
            self.skip,
            self.controls_pass,
            controls
        );
        if self.fail > 0 || self.controls_fail > 0 {
            println!(
                "SUMMARY RED  ({} direction failure(s), {} control(s) not caught) — see report.tsv",
                self.fail, self.controls_fail
            );
            return 1;
        }
        if self.skip == 0 {
            println!("SUMMARY GREEN  every advertised direction passed on this dist");
            0
        } else if self.strict {
            println!(
                "SUMMARY RED  (--strict) {} advertised direction(s) could not run on this dist; see the SKIP lines",
                self.skip
            );
            1
        } else {
            println!(
                "SUMMARY GREEN for the {} directions this dist can run; {} SKIP (engine availability, listed above)",
                self.pass, self.skip
            );
            0
        }
    }
}

fn esc(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', " ")
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    let mut positional: Vec<String> = Vec::new();
    let mut media_only = false;
    let mut docs_only = false;
    let mut strict = false;
    let mut inject: Option<String> = None;
    for arg in args.iter().skip(1) {
        if arg == "--media-only" {
            media_only = true;
        } else if arg == "--docs-only" {
            docs_only = true;
        } else if arg == "--strict" {
            strict = true;
        } else if let Some(value) = arg.strip_prefix("--inject=") {
            inject = Some(value.to_string());
        } else {
            positional.push(arg.clone());
        }
    }
    if positional.len() < 2 {
        eprintln!(
            "usage: ezc-q3-e2e <fixtures-dir> <work-dir> [--media-only|--docs-only] [--strict] [--inject=drop-output]"
        );
        std::process::exit(2);
    }
    let fix = PathBuf::from(&positional[0]);
    let work = PathBuf::from(&positional[1]);
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).expect("work dir");

    let paths = EnginePaths::discover(Path::new("/nonexistent-q3-root"));
    let mut suite = Suite::new(paths, fix.clone(), work.clone(), strict, inject);

    println!("=== EZ Convert Q3 end-to-end suite ===");
    println!("fixtures: {}", fix.display());
    println!("work:     {}", work.display());
    println!(
        "engines:  vips={} ffmpeg={} soffice={}",
        short(&suite.paths.vips),
        short(&suite.paths.ffmpeg),
        short(&suite.paths.soffice)
    );
    if let Some(dir) = &suite.paths.plugin_dir {
        println!("plugins:  {}", dir.display());
    }
    for (program, program_args) in [
        (Program::Vips, vec![Arg::Word("--version")]),
        (Program::Ffmpeg, vec![Arg::Word("-hide_banner"), Arg::Word("-version")]),
    ] {
        let plan = Plan {
            steps: vec![Step {
                engine: Engine::Vips,
                program,
                args: program_args,
                reads: Source::Input,
                writes: crate::engine::plan::Output::None,
                purpose: "q3 version probe",
            }],
            expect: Expect::Image {
                target: TargetFormat::Png,
            },
            tag_merge: None,
            notes: vec![],
            temp_exts: vec![],
        };
        let inv = plan.resolve(&fix, &work.join("version.txt"), &work);
        if let Ok(invocations) = inv {
            if let Ok(outcome) = proc::run(&suite.paths, &invocations[0]) {
                let line = format!("{}{}", outcome.stdout, outcome.stderr);
                let first = line.lines().next().unwrap_or("").trim().to_string();
                println!("          {} {}", program.name(), first);
            }
        }
    }

    suite.preflight();
    suite.prepare_meta_sources();

    if !docs_only {
        println!("\n--- still images ---");
        for (from, to, _) in advertised_media() {
            if family_of(from) == "image" {
                suite.still_direction(from, to);
            }
        }
        println!("\n--- audio ---");
        for (from, to, _) in advertised_media() {
            if family_of(from) == "audio" {
                suite.audio_direction(from, to);
            }
        }
        println!("\n--- video ---");
        for (from, to, _) in advertised_media() {
            if family_of(from) == "video" {
                suite.video_direction(from, to);
            }
        }
    }
    if !media_only {
        println!("\n--- documents (LibreOffice) ---");
        let docs = advertised_docs();
        match crate::engine::soffice::warm_up_at_startup(&suite.paths, &work.join("cache")) {
            crate::engine::soffice::WarmUp::Warmed { bootstrapped, .. } => {
                println!("      soffice warm-up: bootstrapped={bootstrapped}")
            }
            other => println!("      soffice warm-up: {other}"),
        }
        for (from, to) in docs {
            suite.doc_direction(from, to);
        }
    }

    if !docs_only {
        suite.controls();
    }
    suite.report();
    let code = suite.summary();
    if code != 0 {
        std::process::exit(code);
    }
}

fn short(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
