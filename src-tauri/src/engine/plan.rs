//! `engine/plan.rs` — the media planner: `(source type, target, options)` → argv.
//!
//! Pure. No I/O, no `Command`, no clock. Every argument is either a literal from
//! a closed enum, a number we computed, or a path slot that `proc::admit`
//! re-validates before spawn (`ARCHITECTURE.md` §6.0.1 A2/A3/A7). That is what
//! lets the safety properties be unit tests instead of a code review.
//!
//! Two rules shape the whole module:
//!
//! 1. **Still images never reach ffmpeg as a decoder or encoder.** The single
//!    exception is GIF *output*, where `SPEC.md` §9.3 D2 routes a
//!    vips-normalised PNG into ffmpeg's palette encoder. The plan models that as
//!    a `Temp` input, so "ffmpeg reads the user's still image" is impossible by
//!    construction and asserted by a test.
//! 2. **Every audio encode names its sample rate and sample format.** Measured:
//!    libmp3lame rejects 96 kHz (rc=234) and libopus rejects anything but
//!    48/24 kHz, so the rate is clamped to the target's measured set rather than
//!    left to ffmpeg's default (`evidence/c2-media.md` §4).

use std::path::{Path, PathBuf};

use super::proc::{self, EnginePaths, Invocation, PathError, Program};
use super::vips;
use crate::model::types::{CanonicalType, Engine, TargetFormat};

// ---------------------------------------------------------------------------
// argv as data
// ---------------------------------------------------------------------------

/// Which path a slot resolves to. The planner never sees a path string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathRef {
    /// The user's input file.
    Input,
    /// The conversion's final output.
    Output,
    /// An intermediate the app owns (job temp dir), addressed by step index.
    Temp(usize),
}

/// Bracket options attached to a filename (`in.jpg[fail-on=error]`,
/// `out.jpg[Q=90,strip]`). Keys are static; values are numbers or static
/// keywords, so no user text can reach an option.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PathOpts {
    /// `fail-on=error` — the loader must fail loudly on a decode error.
    pub fail_on: bool,
    /// `Q=n` on lossy savers.
    pub quality: Option<u8>,
    /// `compression=<keyword>` or `compression=<n>` for PNG.
    pub compression: Option<Compression>,
    /// `strip` — remove EXIF/ICC. Measured on jpg/png/webp/tiff.
    pub strip: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    Deflate,
    Png(u8),
}

impl PathOpts {
    fn render(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.fail_on {
            parts.push("fail-on=error".into());
        }
        if let Some(q) = self.quality {
            parts.push(format!("Q={}", q.clamp(1, 100)));
        }
        match self.compression {
            Some(Compression::Deflate) => parts.push("compression=deflate".into()),
            Some(Compression::Png(n)) => {
                parts.push(format!("compression={}", n.min(9)));
            }
            None => {}
        }
        if self.strip {
            parts.push("strip".into());
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("[{}]", parts.join(","))
        }
    }
}

/// One argv element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arg {
    /// A bare word from a closed set (`copy`, `-vn`, `libmp3lame`, `yuv420p`).
    Word(&'static str),
    /// A path with optional bracket options.
    Path(PathRef, PathOpts),
    /// A number we computed (bitrate, quality, dimension, rate).
    Number(u32),
    /// A bitrate in kbit/s, rendered with the `k` suffix. Measured: `-b:a 192`
    /// is read as **192 bits per second** (ffmpeg prints "maybe you mean 192k"
    /// and then crawls), so the suffix is not cosmetic.
    Kbps(u32),
    /// A frame rate as a rational (`-r 30000/1001`).
    Fps { num: u32, den: u32 },
    /// A scale factor as a decimal (`vips resize … 0.5000`).
    Percent(u8),
    /// Milliseconds rendered as seconds with three decimals (`-ss 0.500`).
    Millis(u32),
    /// A filter graph built from numbers only, never from user text.
    Filter(super::ffmpeg::Filter),
}

impl Arg {
    pub fn is_option(&self) -> bool {
        match self {
            Arg::Word(w) => w.starts_with('-'),
            _ => false,
        }
    }
}

/// What a step consumes and produces — this is what makes the
/// "ffmpeg never decodes a still image" rule structural.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The user's file.
    Input,
    /// An intermediate produced by an earlier step.
    Temp(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    Final,
    Temp(usize),
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub engine: Engine,
    pub program: Program,
    pub args: Vec<Arg>,
    pub reads: Source,
    pub writes: Output,
    /// Why this step exists, for the job log (`"vips normalise"`, …).
    pub purpose: &'static str,
}

/// What the finished file must prove before we call it done (`§6.0.5`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    Image { target: TargetFormat },
    Audio { target: TargetFormat },
    Video { target: TargetFormat },
}

/// How an audio file's tags survive: ffmpeg is told to drop them
/// (`-map_metadata -1`) and this in-process step re-applies the source's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagMerge {
    pub from: PathRef,
    pub to: PathRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub steps: Vec<Step>,
    pub expect: Expect,
    pub tag_merge: Option<TagMerge>,
    pub notes: Vec<&'static str>,
    /// Extension of each intermediate, indexed like `PathRef::Temp(index)`.
    /// vips chooses its saver by file name, so a temp without the right suffix
    /// fails with "not a known file format" (measured).
    pub temp_exts: Vec<&'static str>,
}

// ---------------------------------------------------------------------------
// Request shape
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// 1..=100, used by lossy savers.
    pub quality: u8,
    /// `SPEC.md` §5 strip toggle. Measured to remove EXIF/ICC on every saver
    /// this planner uses; for audio it means "skip the tag merge".
    pub strip_metadata: bool,
    pub resize: Option<Resize>,
    pub trim: Option<Trim>,
    /// GIF output rate (frames per second). Ignored for still images.
    pub gif_fps: u8,
    /// GIF long edge in pixels; keeps a 4K clip from producing a 40 MB GIF.
    pub gif_max_width: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            quality: 90,
            strip_metadata: false,
            resize: None,
            trim: None,
            gif_fps: 12,
            gif_max_width: 480,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resize {
    /// Still images: scale factor in percent (vips `resize`).
    Percent(u8),
    /// Video: target width in pixels, height derived (`scale=W:-2`).
    Width(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trim {
    /// Seconds from the start.
    pub start_ms: u32,
    /// Length in milliseconds.
    pub length_ms: u32,
}

/// Facts `ffprobe` measured about the source. The planner is pure, so these are
/// inputs, not lookups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFacts {
    Stills,
    Audio {
        sample_rate: u32,
        channels: u16,
        /// `bits_per_raw_sample` from ffprobe; 0 when the probe did not report.
        bits: u16,
        tags: bool,
    },
    Video {
        fps_num: u32,
        fps_den: u32,
        width: u32,
        height: u32,
        has_audio: bool,
        audio_rate: u32,
    },
}

impl SourceFacts {
    pub fn audio() -> Self {
        SourceFacts::Audio {
            sample_rate: 44100,
            channels: 2,
            bits: 16,
            tags: true,
        }
    }

    pub fn video() -> Self {
        SourceFacts::Video {
            fps_num: 25,
            fps_den: 1,
            width: 1920,
            height: 1080,
            has_audio: true,
            audio_rate: 48000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request<'a> {
    pub from: CanonicalType,
    pub to: TargetFormat,
    pub options: Options,
    pub facts: SourceFacts,
    /// Paths are only used to *count* intermediates; they are resolved later.
    pub temp_dir: &'a Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// The matrix says this direction is not routed (Planned or Cut).
    NotRouted { from: CanonicalType, to: TargetFormat },
    /// The option does not apply to this media family.
    BadOption(&'static str),
    /// Facts missing: the caller must probe before planning.
    FactsMismatch(&'static str),
    /// Path admission failed (`§6.0.1`).
    Path(PathError),
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::NotRouted { from, to } => {
                write!(f, "{:?} -> {:?} has no routed engine", from, to.extension())
            }
            PlanError::BadOption(what) => write!(f, "option does not apply: {what}"),
            PlanError::FactsMismatch(what) => write!(f, "missing source facts: {what}"),
            PlanError::Path(e) => write!(f, "{e}"),
        }
    }
}

impl From<PathError> for PlanError {
    fn from(e: PathError) -> Self {
        PlanError::Path(e)
    }
}

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

/// Build the plan for one conversion. Pure.
pub fn plan(req: &Request<'_>) -> Result<Plan, PlanError> {
    // Identity (`from` and `to` are the same format) is a **real direction**
    // here, not a skip: the target matrix only models it where the re-encode
    // does work the user can ask for — jpg autorot/compress (`V6`), an audio
    // bitrate change, a video trim/resize. Refusing it made twelve wheel-visible
    // targets unconvertible (found by the Q3 suite, card `t_9a4c622d`), so the
    // planner now treats `from == to` like any other target and lets the plan
    // and the output validators decide. See `ARCHITECTURE.md` §17.15.
    match media_family(req.from) {
        Family::Still => plan_still(req),
        Family::Audio => plan_audio(req),
        Family::Video => plan_video(req),
        Family::None => Err(PlanError::NotRouted {
            from: req.from,
            to: req.to,
        }),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Still,
    Audio,
    Video,
    None,
}

pub fn media_family(t: CanonicalType) -> Family {
    match t {
        CanonicalType::Jpeg
        | CanonicalType::Png
        | CanonicalType::Heic
        | CanonicalType::Avif
        | CanonicalType::WebP
        | CanonicalType::Gif
        | CanonicalType::Tiff
        | CanonicalType::Bmp
        | CanonicalType::Svg => Family::Still,
        CanonicalType::Mp3
        | CanonicalType::Wav
        | CanonicalType::Flac
        | CanonicalType::M4a
        | CanonicalType::Aac
        | CanonicalType::Ogg
        | CanonicalType::Opus => Family::Audio,
        CanonicalType::Mp4 | CanonicalType::Mkv | CanonicalType::Mov | CanonicalType::WebM => {
            Family::Video
        }
        _ => Family::None,
    }
}

fn plan_still(req: &Request<'_>) -> Result<Plan, PlanError> {
    // SVG input is NOT routed through vips even though a dev build can read it:
    // measured on the Debian libvips build `vips copy sample.svg out.png` works
    // because librsvg is linked, but the **shipping** dist has no librsvg DLL
    // (S2 dist listing), so the shipped app would offer a target that fails.
    // SVG stays on the Resvg route (`ARCHITECTURE.md` §6.5, Planned) until that
    // card proves it.
    if req.from == CanonicalType::Svg {
        return Err(PlanError::NotRouted {
            from: req.from,
            to: req.to,
        });
    }
    if req.options.trim.is_some() {
        return Err(PlanError::BadOption("trim applies to video only"));
    }
    if matches!(req.options.resize, Some(Resize::Width(_))) {
        return Err(PlanError::BadOption(
            "video resize takes a width; stills take a percent",
        ));
    }
    match req.to {
        TargetFormat::Gif => plan_still_to_gif(req),
        TargetFormat::Jpg | TargetFormat::Png | TargetFormat::Webp | TargetFormat::Tiff => {
            Ok(Plan {
                steps: vec![vips_step(req)?],
                expect: Expect::Image { target: req.to },
                tag_merge: None,
                notes: vips::notes_for(req.from),
                temp_exts: vec![],
            })
        }
        TargetFormat::Avif => Ok(Plan {
            steps: vec![vips_step(req)?],
            expect: Expect::Image { target: req.to },
            tag_merge: None,
            notes: vec![
                "avif output keeps EXIF orientation as metadata rather than baking it \
                 (libheif applies irot itself on decode)",
            ],
                temp_exts: vec![],
        }),
        other => Err(PlanError::NotRouted {
            from: req.from,
            to: other,
        }),
    }
}

fn vips_step(req: &Request<'_>) -> Result<Step, PlanError> {
    let opts = PathOpts {
        fail_on: true,
        quality: None,
        compression: None,
        strip: req.options.strip_metadata,
    };
    let saver = PathOpts {
        fail_on: false,
        quality: vips::quality_for(req.to, req.options.quality),
        compression: vips::compression_for(req.to),
        strip: req.options.strip_metadata,
    };
    let (op, args) = vips::build_still(
        req.from,
        req.to,
        PathRef::Output,
        opts,
        saver,
        match req.options.resize {
            Some(Resize::Percent(p)) if p > 0 && p != 100 => Some(p),
            Some(Resize::Percent(0)) => return Err(PlanError::BadOption("resize percent 0")),
            _ => None,
        },
    );
    Ok(Step {
        engine: Engine::Vips,
        program: Program::Vips,
        args,
        reads: Source::Input,
        writes: Output::Final,
        purpose: op,
    })
}

fn plan_still_to_gif(req: &Request<'_>) -> Result<Plan, PlanError> {
    // Two steps: vips decodes/rotates/normalises into a PNG we own, ffmpeg only
    // palette-encodes that PNG (SPEC §9.3 D2). ffmpeg never sees the user file.
    let normalise_in = PathOpts {
        fail_on: true,
        ..PathOpts::default()
    };
    let normalise_out = PathOpts {
        fail_on: false,
        quality: None,
        compression: Some(Compression::Png(9)),
        // GIF cannot carry EXIF/ICC, so the intermediate does not need them.
        strip: true,
    };
    let (purpose, vips_args) = vips::build_still(
        req.from,
        TargetFormat::Png,
        PathRef::Temp(0),
        normalise_in,
        normalise_out,
        match req.options.resize {
            Some(Resize::Percent(p)) if p > 0 && p != 100 => Some(p),
            Some(Resize::Percent(0)) => return Err(PlanError::BadOption("resize percent 0")),
            _ => None,
        },
    );
    let mut steps = vec![Step {
        engine: Engine::Vips,
        program: Program::Vips,
        args: vips_args,
        reads: Source::Input,
        writes: Output::Temp(0),
        purpose,
    }];
    steps.push(Step {
        engine: Engine::Ffmpeg,
        program: Program::Ffmpeg,
        args: super::ffmpeg::build_gif_from_png(super::ffmpeg::GifSource::Still),
        reads: Source::Temp(0),
        writes: Output::Final,
        purpose: "ffmpeg palette-encode",
    });
    Ok(Plan {
        steps,
        expect: Expect::Image {
            target: TargetFormat::Gif,
        },
        tag_merge: None,
        notes: vec![
            "gif output: vips normalises (decode, autorot, strip), ffmpeg palette-encodes",
        ],
        temp_exts: vec!["png"],
    })
}

// ---------------------------------------------------------------------------
// Audio
// ---------------------------------------------------------------------------

fn plan_audio(req: &Request<'_>) -> Result<Plan, PlanError> {
    let (sample_rate, _channels, bits, tags) = match req.facts {
        SourceFacts::Audio {
            sample_rate,
            channels,
            bits,
            tags,
        } => (sample_rate, channels, bits, tags),
        _ => return Err(PlanError::FactsMismatch("audio source facts")),
    };
    if req.options.trim.is_some() {
        return Err(PlanError::BadOption("trim applies to video only"));
    }
    if req.options.resize.is_some() {
        return Err(PlanError::BadOption("resize applies to images and video"));
    }
    let encode =
        super::ffmpeg::AudioEncode::for_target(req.to, sample_rate, bits, req.options.quality)
            .ok_or(PlanError::NotRouted {
                from: req.from,
                to: req.to,
            })?;
    let mut steps = vec![Step {
        engine: Engine::Ffmpeg,
        program: Program::Ffmpeg,
        args: super::ffmpeg::build_audio(&encode, req.options.strip_metadata),
        reads: Source::Input,
        writes: Output::Final,
        purpose: "ffmpeg audio encode",
    }];
    let _ = &mut steps;
    let tag_merge = if req.options.strip_metadata || !tags {
        None
    } else {
        Some(TagMerge {
            from: PathRef::Input,
            to: PathRef::Output,
        })
    };
    Ok(Plan {
        steps,
        expect: Expect::Audio { target: req.to },
        tag_merge,
        notes: vec![
            "audio tags are merged in-process by lofty after the encode; ffmpeg's tag mapping is not trusted",
        ],
        temp_exts: vec![],
    })
}

// ---------------------------------------------------------------------------
// Video
// ---------------------------------------------------------------------------

fn plan_video(req: &Request<'_>) -> Result<Plan, PlanError> {
    let facts = match req.facts {
        SourceFacts::Video {
            fps_num,
            fps_den,
            width,
            height,
            has_audio,
            audio_rate,
        } => {
            if fps_num == 0 || fps_den == 0 {
                return Err(PlanError::FactsMismatch("frame rate"));
            }
            (fps_num, fps_den, width, height, has_audio, audio_rate)
        }
        _ => return Err(PlanError::FactsMismatch("video source facts")),
    };
    let (fps_num, fps_den, _w, _h, has_audio, audio_rate) = facts;
    let resize_width = match req.options.resize {
        Some(Resize::Width(w)) if w >= 16 => Some(w),
        Some(Resize::Width(_)) => return Err(PlanError::BadOption("resize width below 16 px")),
        Some(Resize::Percent(_)) => {
            return Err(PlanError::BadOption(
                "video resize takes a width; stills take a percent",
            ))
        }
        None => None,
    };
    if req.to == TargetFormat::Gif {
        // Trim goes in front of the input for a fast seek, and the duration in
        // front of the output for an exact length (measured, §6.3 T3/T4).
        let args = super::ffmpeg::build_gif_from_video(
            req.options.gif_fps,
            req.options.gif_max_width,
            req.options.trim,
        );
        return Ok(Plan {
            steps: vec![Step {
                engine: Engine::Ffmpeg,
                program: Program::Ffmpeg,
                args,
                reads: Source::Input,
                writes: Output::Final,
                purpose: "ffmpeg palette-encode (video)",
            }],
            expect: Expect::Image {
                target: TargetFormat::Gif,
            },
            tag_merge: None,
            notes: vec!["video -> gif: fps + long-edge cap, palettegen/paletteuse"],
            temp_exts: vec![],
        });
    }
    let encode = super::ffmpeg::VideoEncode::for_target(req.to, audio_rate).ok_or(
        PlanError::NotRouted {
            from: req.from,
            to: req.to,
        },
    )?;
    let args = super::ffmpeg::build_video(
        &encode,
        fps_num,
        fps_den,
        has_audio,
        resize_width,
        req.options.trim,
        req.options.quality,
    );
    Ok(Plan {
        steps: vec![Step {
            engine: Engine::Ffmpeg,
            program: Program::Ffmpeg,
            args,
            reads: Source::Input,
            writes: Output::Final,
            purpose: "ffmpeg video transcode (CFR)",
        }],
        expect: Expect::Video { target: req.to },
        tag_merge: None,
        notes: vec![
            "video is forced to constant frame rate: -fps_mode cfr + -r <source rate>",
            "display-matrix rotation is applied by ffmpeg on re-encode (measured)",
        ],
        temp_exts: vec![],
    })
}

// ---------------------------------------------------------------------------
// Resolution: argv data → concrete argv
// ---------------------------------------------------------------------------

impl Plan {
    /// Temp file for a step, inside the job's own temp directory.
    pub fn temp_path(temp_dir: &Path, index: usize, extension: &str) -> PathBuf {
        temp_dir.join(format!("step{index}.{extension}"))
    }

    /// Turn the plan into spawn-ready invocations, re-validating every path
    /// (`§6.0.1` A3/A5). `output`'s parent must already exist.
    pub fn resolve(
        &self,
        input: &Path,
        output: &Path,
        temp_dir: &Path,
    ) -> Result<Vec<Invocation>, PathError> {
        let input = proc::admit(input, true)?;
        let output = proc::admit(output, false)?;
        let mut invocations = Vec::with_capacity(self.steps.len());
        for step in &self.steps {
            let mut argv: Vec<std::ffi::OsString> = Vec::with_capacity(step.args.len());
            for arg in &step.args {
                match arg {
                    Arg::Word(w) => argv.push(std::ffi::OsString::from(w)),
                    Arg::Number(n) => argv.push(std::ffi::OsString::from(n.to_string())),
                    Arg::Kbps(n) => argv.push(std::ffi::OsString::from(format!("{n}k"))),
                    Arg::Fps { num, den } => {
                        argv.push(std::ffi::OsString::from(format!("{num}/{den}")))
                    }
                    Arg::Percent(p) => {
                        argv.push(std::ffi::OsString::from(format!("{:.4}", *p as f64 / 100.0)))
                    }
                    Arg::Millis(ms) => {
                        argv.push(std::ffi::OsString::from(format!("{:.3}", *ms as f64 / 1000.0)))
                    }
                    Arg::Filter(f) => argv.push(std::ffi::OsString::from(f.render())),
                    Arg::Path(reference, opts) => {
                        let base = match reference {
                            PathRef::Input => input.clone(),
                            PathRef::Output => output.clone(),
                            PathRef::Temp(i) => {
                                // Temp paths are ours: built from the job temp
                                // dir and never from user text, and they carry
                                // the extension the saver needs.
                                let dir = proc::admit(temp_dir, false)?;
                                let ext = self
                                    .temp_exts
                                    .get(*i)
                                    .copied()
                                    .unwrap_or("tmp");
                                dir.join(format!("step{i}.{ext}"))
                            }
                        };
                        let rendered = opts.render();
                        // F1 safety net: a vips step that *reads* a file whose
                        // name carries `[`/`]` would have its own bracket group
                        // re-parsed by the loader (see `proc::has_bracket_syntax`).
                        // `engine::convert` stages such inputs under an app-chosen
                        // name first, so reaching this with brackets means the
                        // staging was skipped — refuse loudly instead of opening
                        // the wrong file.
                        if !rendered.is_empty()
                            && *reference == PathRef::Input
                            && matches!(step.program, Program::Vips | Program::VipsHeader)
                        {
                            if base
                                .file_name()
                                .is_some_and(|name| proc::has_bracket_syntax(name))
                            {
                                return Err(PathError::BracketName(base));
                            }
                        }
                        let mut os = base.into_os_string();
                        if !rendered.is_empty() {
                            os.push(rendered);
                        }
                        argv.push(os);
                    }
                }
            }
            invocations.push(Invocation {
                program: step.program,
                args: argv,
                cwd: temp_dir.to_path_buf(),
            });
        }
        Ok(invocations)
    }
}

/// Convenience for tests and callers that only want the shape.
impl Step {
    /// `argv` as display strings, with slots rendered symbolically.
    pub fn describe(&self) -> String {
        let mut out = String::new();
        for arg in &self.args {
            if !out.is_empty() {
                out.push(' ');
            }
            match arg {
                Arg::Word(w) => out.push_str(w),
                Arg::Number(n) => out.push_str(&n.to_string()),
                Arg::Kbps(n) => out.push_str(&format!("{n}k")),
                Arg::Fps { num, den } => out.push_str(&format!("{num}/{den}")),
                Arg::Percent(p) => out.push_str(&format!("{:.4}", *p as f64 / 100.0)),
                Arg::Millis(ms) => out.push_str(&format!("{:.3}", *ms as f64 / 1000.0)),
                Arg::Filter(f) => out.push_str(&f.render()),
                Arg::Path(PathRef::Input, o) => {
                    out.push_str("<in>");
                    out.push_str(&o.render());
                }
                Arg::Path(PathRef::Output, o) => {
                    out.push_str("<out>");
                    out.push_str(&o.render());
                }
                Arg::Path(PathRef::Temp(i), o) => {
                    out.push_str(&format!("<tmp{i}>"));
                    out.push_str(&o.render());
                }
            }
        }
        format!("{} {}", self.program.name(), out)
    }
}

/// Which engine the planner chose, for the matrix and for tests.
pub fn engine_for(from: CanonicalType, to: TargetFormat) -> Option<Engine> {
    let req = Request {
        from,
        to,
        options: Options::default(),
        facts: match media_family(from) {
            Family::Audio => SourceFacts::audio(),
            Family::Video => SourceFacts::video(),
            _ => SourceFacts::Stills,
        },
        temp_dir: Path::new("."),
    };
    plan(&req).ok().and_then(|p| p.steps.last().map(|s| s.engine))
}

/// Ensure the engine set is complete enough to run this plan.
pub fn missing_engine(paths: &EnginePaths, plan: &Plan) -> Option<Program> {
    plan.steps
        .iter()
        .map(|s| s.program)
        .find(|p| !paths.installed(*p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::types::ALL_TARGETS;

    fn still(from: CanonicalType, to: TargetFormat) -> Result<Plan, PlanError> {
        plan(&Request {
            from,
            to,
            options: Options::default(),
            facts: SourceFacts::Stills,
            temp_dir: Path::new("/tmp"),
        })
    }

    fn audio(from: CanonicalType, to: TargetFormat) -> Plan {
        plan(&Request {
            from,
            to,
            options: Options::default(),
            facts: SourceFacts::audio(),
            temp_dir: Path::new("/tmp"),
        })
        .expect("audio plan")
    }

    fn video(from: CanonicalType, to: TargetFormat) -> Plan {
        plan(&Request {
            from,
            to,
            options: Options::default(),
            facts: SourceFacts::video(),
            temp_dir: Path::new("/tmp"),
        })
        .expect("video plan")
    }

    fn number_after(args: &[Arg], flag: &str) -> Option<u32> {
        let pos = args
            .iter()
            .position(|a| matches!(a, Arg::Word(w) if *w == flag))?;
        match args.get(pos + 1)? {
            Arg::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// The acceptance rule, as a test: no still-image conversion hands the
    /// user's file to ffmpeg, and no still-image output is encoded by ffmpeg.
    #[test]
    fn ffmpeg_never_reads_or_writes_a_still_image() {
        let stills = [
            CanonicalType::Jpeg,
            CanonicalType::Png,
            CanonicalType::WebP,
            CanonicalType::Tiff,
            CanonicalType::Heic,
            CanonicalType::Avif,
            CanonicalType::Gif,
            CanonicalType::Svg,
        ];
        for from in stills {
            for to in ALL_TARGETS.iter().copied() {
                let Ok(p) = still(from, to) else { continue };
                for step in &p.steps {
                    if step.engine != Engine::Ffmpeg {
                        continue;
                    }
                    assert!(
                        !matches!(step.reads, Source::Input),
                        "{from:?} -> {to:?}: ffmpeg reads the original still image"
                    );
                    // The only tolerated ffmpeg use is GIF palette encoding,
                    // and even then it must not be the encoder of a raster
                    // target other than gif.
                    assert_eq!(
                        to,
                        TargetFormat::Gif,
                        "{from:?} -> {to:?}: ffmpeg used for a non-gif still target"
                    );
                }
            }
        }
    }

    #[test]
    fn the_only_ffmpeg_step_in_a_still_plan_is_the_gif_palette_encoder() {
        let gif = still(CanonicalType::Jpeg, TargetFormat::Gif).expect("jpg -> gif");
        assert_eq!(gif.steps.len(), 2);
        assert_eq!(gif.steps[0].engine, Engine::Vips);
        assert_eq!(gif.steps[0].writes, Output::Temp(0));
        assert_eq!(gif.steps[1].engine, Engine::Ffmpeg);
        assert_eq!(gif.steps[1].reads, Source::Temp(0));
        assert!(gif.steps[1]
            .args
            .contains(&Arg::Filter(super::super::ffmpeg::Filter::PaletteFromStill)));
    }

    #[test]
    fn image_targets_are_encoded_by_vips() {
        for from in [
            CanonicalType::Jpeg,
            CanonicalType::Png,
            CanonicalType::WebP,
            CanonicalType::Tiff,
        ] {
            for to in [
                TargetFormat::Jpg,
                TargetFormat::Png,
                TargetFormat::Webp,
                TargetFormat::Tiff,
            ] {
                let p = still(from, to).unwrap_or_else(|e| panic!("{from:?}->{to:?}: {e}"));
                assert_eq!(p.steps.len(), 1);
                assert_eq!(p.steps[0].program, Program::Vips);
            }
        }
    }

    #[test]
    fn loader_guard_is_present_on_every_still_read() {
        for to in [TargetFormat::Jpg, TargetFormat::Png, TargetFormat::Webp] {
            let p = still(CanonicalType::Tiff, to).unwrap();
            assert!(
                p.steps[0]
                    .args
                    .iter()
                    .any(|a| matches!(a, Arg::Path(PathRef::Input, o) if o.fail_on)),
                "TIFF input without fail-on=error: {:?}",
                p.steps[0].args
            );
        }
    }

    #[test]
    fn metadata_is_kept_by_default_and_stripped_on_request() {
        let keep = still(CanonicalType::Jpeg, TargetFormat::Png).unwrap();
        assert!(!keep.options_strip_is_set());
        let strip = plan(&Request {
            from: CanonicalType::Jpeg,
            to: TargetFormat::Png,
            options: Options {
                strip_metadata: true,
                ..Options::default()
            },
            facts: SourceFacts::Stills,
            temp_dir: Path::new("/tmp"),
        })
        .unwrap();
        assert!(strip.options_strip_is_set());
    }

    impl Plan {
        fn options_strip_is_set(&self) -> bool {
            self.steps.iter().any(|s| {
                s.args.iter().any(|a| {
                    matches!(a, Arg::Path(_, o) if o.strip)
                })
            })
        }
    }

    #[test]
    fn audio_plan_always_names_rate_and_sample_format() {
        let sources = [
            CanonicalType::Mp3,
            CanonicalType::Wav,
            CanonicalType::Flac,
            CanonicalType::M4a,
            CanonicalType::Aac,
            CanonicalType::Ogg,
            CanonicalType::Opus,
        ];
        let targets = [
            TargetFormat::Mp3,
            TargetFormat::Wav,
            TargetFormat::Flac,
            TargetFormat::M4a,
            TargetFormat::Aac,
            TargetFormat::Ogg,
            TargetFormat::Opus,
        ];
        for from in sources {
            for to in targets {
                let p = audio(from, to);
                let args = &p.steps[0].args;
                let words: Vec<&'static str> = args
                    .iter()
                    .filter_map(|a| match a {
                        Arg::Word(w) => Some(*w),
                        _ => None,
                    })
                    .collect();
                assert!(words.contains(&"-ar"), "{from:?}->{to:?} has no -ar");
                assert!(
                    words.contains(&"-sample_fmt"),
                    "{from:?}->{to:?} has no -sample_fmt"
                );
                assert!(words.contains(&"-map_metadata"), "no -map_metadata");
                assert!(words.contains(&"-vn"), "{from:?}->{to:?} has no -vn");
                assert!(
                    args.iter().any(|a| matches!(a, Arg::Number(_))),
                    "{from:?}->{to:?} has no numeric rate"
                );
            }
        }
    }

    #[test]
    fn opus_output_is_always_48k() {
        let p = audio(CanonicalType::Wav, TargetFormat::Opus);
        assert_eq!(number_after(&p.steps[0].args, "-ar"), Some(48000));
    }

    #[test]
    fn a_96k_source_is_clamped_for_mp3_and_kept_for_flac() {
        let base = Request {
            from: CanonicalType::Wav,
            to: TargetFormat::Mp3,
            options: Options::default(),
            facts: SourceFacts::Audio {
                sample_rate: 96_000,
                channels: 2,
                bits: 24,
                tags: false,
            },
            temp_dir: Path::new("/tmp"),
        };
        let p = plan(&base).unwrap();
        assert_eq!(
            number_after(&p.steps[0].args, "-ar"),
            Some(48000),
            "mp3 must clamp 96 kHz to 48 kHz"
        );

        let flac = plan(&Request {
            to: TargetFormat::Flac,
            ..base
        })
        .unwrap();
        assert_eq!(
            number_after(&flac.steps[0].args, "-ar"),
            Some(96000),
            "flac keeps the source rate"
        );
    }

    #[test]
    fn audio_tags_merge_unless_stripping() {
        let keep = audio(CanonicalType::Mp3, TargetFormat::Ogg);
        assert!(keep.tag_merge.is_some());
        let strip = plan(&Request {
            from: CanonicalType::Mp3,
            to: TargetFormat::Ogg,
            options: Options {
                strip_metadata: true,
                ..Options::default()
            },
            facts: SourceFacts::audio(),
            temp_dir: Path::new("/tmp"),
        })
        .unwrap();
        assert!(strip.tag_merge.is_none());
    }

    #[test]
    fn video_plan_forces_cfr_with_the_source_rate() {
        let p = video(CanonicalType::Mov, TargetFormat::Mp4);
        let args = &p.steps[0].args;
        assert!(args.contains(&Arg::Word("-fps_mode")));
        let idx = args
            .iter()
            .position(|a| *a == Arg::Word("-fps_mode"))
            .unwrap();
        assert_eq!(args[idx + 1], Arg::Word("cfr"));
        assert!(args.contains(&Arg::Fps { num: 25, den: 1 }));
    }

    #[test]
    fn video_targets_pick_lgpl_encoders_only() {
        for to in [
            TargetFormat::Mp4,
            TargetFormat::Mkv,
            TargetFormat::Mov,
            TargetFormat::Webm,
        ] {
            if to == TargetFormat::Mkv {
                continue;
            }
            let p = video(CanonicalType::Mkv, to);
            for arg in &p.steps[0].args {
                if let Arg::Word(w) = arg {
                    assert_ne!(*w, "libx264", "{to:?} chose a GPL encoder");
                    assert_ne!(*w, "libx265", "{to:?} chose a GPL encoder");
                }
            }
        }
        let mp4 = video(CanonicalType::Mkv, TargetFormat::Mp4);
        assert!(mp4.steps[0].args.contains(&Arg::Word("libopenh264")));
        let webm = video(CanonicalType::Mp4, TargetFormat::Webm);
        assert!(webm.steps[0].args.contains(&Arg::Word("libvpx-vp9")));
        assert!(webm.steps[0].args.contains(&Arg::Word("libopus")));
    }

    #[test]
    fn video_trim_is_expressed_as_fast_seek_plus_exact_length() {
        let p = plan(&Request {
            from: CanonicalType::Mkv,
            to: TargetFormat::Mp4,
            options: Options {
                trim: Some(Trim {
                    start_ms: 500,
                    length_ms: 1000,
                }),
                ..Options::default()
            },
            facts: SourceFacts::video(),
            temp_dir: Path::new("/tmp"),
        })
        .unwrap();
        let args = &p.steps[0].args;
        let ss = args.iter().position(|a| *a == Arg::Word("-ss")).unwrap();
        // -ss comes before -i so the demuxer seeks instead of decoding.
        let i = args.iter().position(|a| *a == Arg::Word("-i")).unwrap();
        assert!(ss < i, "-ss must precede -i for a fast seek");
        assert!(args.contains(&Arg::Word("-t")));
        let t = args.iter().position(|a| *a == Arg::Word("-t")).unwrap();
        assert_eq!(args[t + 1], Arg::Millis(1000));
        assert_eq!(args[ss + 1], Arg::Millis(500));
    }

    /// Identity is advertised by the matrix (jpg autorot/compress, an audio
    /// bitrate change, a video trim/resize), so it must *plan*, not refuse:
    /// twelve wheel-visible targets were unconvertible while this returned
    /// `SameFormat` (found by the Q3 end-to-end suite).
    #[test]
    fn identity_is_planned_as_a_real_re_encode() {
        let jpg = still(CanonicalType::Jpeg, TargetFormat::Jpg).expect("jpg->jpg plans");
        assert_eq!(jpg.steps.len(), 1);
        assert_eq!(jpg.steps[0].program, Program::Vips);
        let mp3 = plan(&Request {
            from: CanonicalType::Mp3,
            to: TargetFormat::Mp3,
            options: Options::default(),
            facts: SourceFacts::audio(),
            temp_dir: Path::new("/tmp"),
        })
        .expect("mp3->mp3 plans");
        assert_eq!(mp3.steps[0].program, Program::Ffmpeg);
        let mp4 = plan(&Request {
            from: CanonicalType::Mp4,
            to: TargetFormat::Mp4,
            options: Options::default(),
            facts: SourceFacts::video(),
            temp_dir: Path::new("/tmp"),
        })
        .expect("mp4->mp4 plans");
        assert_eq!(mp4.steps[0].program, Program::Ffmpeg);
        assert_eq!(mp4.expect, Expect::Video { target: TargetFormat::Mp4 });
    }

    #[test]
    fn unrouted_directions_are_refused_not_guessed() {
        assert!(matches!(
            still(CanonicalType::Jpeg, TargetFormat::Bmp),
            Err(PlanError::NotRouted { .. })
        ));
        // SVG input stays Planned: the shipping vips dist has no librsvg.
        assert!(matches!(
            still(CanonicalType::Svg, TargetFormat::Jpg),
            Err(PlanError::NotRouted { .. })
        ));
        assert!(matches!(
            still(CanonicalType::Jpeg, TargetFormat::Pdf),
            Err(PlanError::NotRouted { .. })
        ));
        assert!(matches!(
            plan(&Request {
                from: CanonicalType::Mp3,
                to: TargetFormat::Mp4,
                options: Options::default(),
                facts: SourceFacts::audio(),
                temp_dir: Path::new("/tmp"),
            }),
            Err(PlanError::NotRouted { .. })
        ));
    }

    #[test]
    fn still_resize_uses_percent_and_video_resize_uses_width() {
        let p = plan(&Request {
            from: CanonicalType::Jpeg,
            to: TargetFormat::Png,
            options: Options {
                resize: Some(Resize::Percent(50)),
                ..Options::default()
            },
            facts: SourceFacts::Stills,
            temp_dir: Path::new("/tmp"),
        })
        .unwrap();
        assert!(p.steps[0].args.iter().any(|a| *a == Arg::Word("resize")));
        assert!(matches!(
            plan(&Request {
                from: CanonicalType::Jpeg,
                to: TargetFormat::Png,
                options: Options {
                    resize: Some(Resize::Width(800)),
                    ..Options::default()
                },
                facts: SourceFacts::Stills,
                temp_dir: Path::new("/tmp"),
            }),
            Err(PlanError::BadOption(_))
        ));
        let v = plan(&Request {
            from: CanonicalType::Mkv,
            to: TargetFormat::Mp4,
            options: Options {
                resize: Some(Resize::Width(640)),
                ..Options::default()
            },
            facts: SourceFacts::video(),
            temp_dir: Path::new("/tmp"),
        })
        .unwrap();
        assert!(v.steps[0]
            .args
            .iter()
            .any(|a| matches!(a, Arg::Filter(super::super::ffmpeg::Filter::ScaleWidth(640)))));
    }

    #[test]
    fn plan_rejects_missing_facts() {
        assert!(matches!(
            plan(&Request {
                from: CanonicalType::Mp3,
                to: TargetFormat::Wav,
                options: Options::default(),
                facts: SourceFacts::Stills,
                temp_dir: Path::new("/tmp"),
            }),
            Err(PlanError::FactsMismatch(_))
        ));
    }

    #[test]
    fn options_never_carry_user_text() {
        let opts = PathOpts {
            fail_on: true,
            quality: Some(90),
            compression: Some(Compression::Deflate),
            strip: true,
        };
        let rendered = opts.render();
        assert!(
            rendered
                .trim_matches(['[', ']'])
                .split(',')
                .all(|part| part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '=')),
            "option text is not a closed grammar: {rendered}"
        );
    }

    #[test]
    fn every_planned_step_uses_an_approved_engine() {
        for from in ALL_TARGETS.iter().copied() {
            let _ = from;
        }
        // Engines the media planner may name: vips, ffmpeg. Anything else means
        // a document or raster-upstream card leaked into this module.
        for from in [
            CanonicalType::Jpeg,
            CanonicalType::Png,
            CanonicalType::WebP,
            CanonicalType::Tiff,
            CanonicalType::Avif,
            CanonicalType::Mp3,
            CanonicalType::Wav,
            CanonicalType::Flac,
            CanonicalType::Mp4,
            CanonicalType::Mkv,
            CanonicalType::Mov,
            CanonicalType::WebM,
        ] {
            for to in ALL_TARGETS.iter().copied() {
                let req = Request {
                    from,
                    to,
                    options: Options::default(),
                    facts: match media_family(from) {
                        Family::Audio => SourceFacts::audio(),
                        Family::Video => SourceFacts::video(),
                        _ => SourceFacts::Stills,
                    },
                    temp_dir: Path::new("/tmp"),
                };
                if let Ok(p) = plan(&req) {
                    for step in p.steps {
                        assert!(
                            matches!(step.engine, Engine::Vips | Engine::Ffmpeg),
                            "{from:?} -> {to:?} used {:?}",
                            step.engine
                        );
                    }
                }
            }
        }
    }

    // -- F1: bracket filenames never reach a vips argv ----------------------

    /// The pinned rule from Q1 finding F1: a vips step appends `[fail-on=error]`
    /// to the input path, so a name that itself carries `[`/`]` is refused
    /// rather than handed over to be re-parsed. The staged form — what
    /// `engine::convert` produces for such a file — resolves normally, and the
    /// argv then carries exactly the one bracket group *we* wrote.
    #[test]
    fn a_vips_step_refuses_a_bracketed_input_name_and_accepts_the_staged_form() {
        let dir = std::env::temp_dir().join(format!("ezc-f1-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");

        let bracketed = dir.join("zz3[shrink=2]");
        std::fs::write(&bracketed, b"not a real png").expect("fixture");
        let out = dir.join("out.jpg");

        let p = still(CanonicalType::Png, TargetFormat::Jpg).expect("png -> jpg");
        let err = p
            .resolve(&bracketed, &out, &dir)
            .expect_err("a bracketed vips input must be refused");
        assert!(
            matches!(err, PathError::BracketName(_)),
            "expected BracketName, got {err:?}"
        );

        let staged = dir.join("input.png");
        std::fs::write(&staged, b"not a real png").expect("staged fixture");
        let invocations = p
            .resolve(&staged, &out, &dir)
            .expect("staged input resolves");
        let rendered: Vec<String> = invocations[0]
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let input_arg = rendered
            .iter()
            .find(|a| a.ends_with("input.png[fail-on=error]"))
            .unwrap_or_else(|| panic!("no rendered input slot in {rendered:?}"));
        assert_eq!(
            input_arg.matches('[').count(),
            1,
            "the user's brackets must not survive into argv: {input_arg}"
        );

        // A bracket-bearing *output* name is allowed: the suffix our saver
        // writes is the trailing group, so the name stays literal (the app
        // always ends output names with the target extension).
        let out_bracketed = dir.join("zz[1].jpg");
        p.resolve(&staged, &out_bracketed, &dir)
            .expect("an output name may carry brackets");
    }
}
