// C2 end-to-end media matrix: the real engines, the real planner, real files.
//
// Every case below runs `engine::convert::convert` — the same function the job
// worker calls — against an installed `vips` + `ffmpeg`, then re-opens the
// output with the engine that should be able to read it and asserts a property
// that a broken conversion cannot fake (dimensions, frame count, sample rate,
// EXIF orientation, ICC presence, tag values).
//
// It prints one line per direction (`PASS`/`FAIL`/`SKIP` with the reason) so the
// transcript can be pasted into evidence without editing. Exit code is 1 if any
// case fails.
//
// Build + run: `spikes/c2/run-e2e.sh` (wraps this file in a throwaway cargo
// crate so `lofty` is available; the repo itself cannot be built here because
// Tauri needs GTK).
//
// Engines: `EZC_VIPS_DIR`, `EZC_FFMPEG_DIR`, optional `EZC_ENGINE_LIB_DIR`
// (Linux prefix installs). Fixtures: `<repo>/fixtures`.

use std::path::{Path, PathBuf};

use crate::engine::plan::{Arg, PathRef, Plan, Step, Source, Output, Expect};
use crate::engine::proc::{self, EnginePaths, Program};
use crate::engine::{convert, verify};
use crate::model::types::{CanonicalType, Engine, TargetFormat};

struct Harness {
    paths: EnginePaths,
    fix: PathBuf,
    work: PathBuf,
    pass: usize,
    fail: usize,
    skip: usize,
    failures: Vec<String>,
}

impl Harness {
    fn new(fix: PathBuf, work: PathBuf) -> Self {
        let paths = EnginePaths::discover(Path::new("/nonexistent-bundle-root"));
        Self {
            paths,
            fix,
            work,
            pass: 0,
            fail: 0,
            skip: 0,
            failures: Vec::new(),
        }
    }

    fn ok(&mut self, label: &str, detail: String) {
        self.pass += 1;
        println!("PASS  {label:<28} {detail}");
    }

    fn bad(&mut self, label: &str, detail: String) {
        self.fail += 1;
        println!("FAIL  {label:<28} {detail}");
        self.failures.push(format!("{label}: {detail}"));
    }

    fn skipped(&mut self, label: &str, why: String) {
        self.skip += 1;
        println!("SKIP  {label:<28} {why}");
    }

    /// One conversion, then a property assertion on the result.
    fn case(
        &mut self,
        from: CanonicalType,
        to: TargetFormat,
        input: &Path,
        options: crate::engine::plan::Options,
        check: impl Fn(&Harness, &Path) -> Result<String, String>,
    ) {
        let label = format!("{:?}->{:?}", from, to.extension());
        let out_dir = self.work.join("out");
        let _ = std::fs::create_dir_all(&out_dir);
        let stem = format!(
            "{}-{}-{}",
            input.file_stem().unwrap_or_default().to_string_lossy(),
            to.extension(),
            self.pass + self.fail + self.skip
        );
        let output = out_dir.join(format!("{stem}.{}", to.extension()));
        let temp = self.work.join(format!("tmp-{stem}"));

        let job = convert::Job {
            from,
            to,
            options,
        };
        match convert::convert(&self.paths, &job, input, &output, &temp) {
            Ok(report) => match check(self, &output) {
                Ok(detail) => {
                    let tag_note = match (&report.tags, &report.tag_error) {
                        (Some(t), _) => format!(" tags={}pics={}", t.items, t.pictures),
                        (None, Some(e)) => format!(" tag_err={e}"),
                        _ => String::new(),
                    };
                    self.ok(
                        &format!("{label} {}", input.file_name().unwrap_or_default().to_string_lossy()),
                        format!("{detail}{tag_note} {}B", report.output_bytes),
                    );
                }
                Err(why) => self.bad(
                    &format!("{label} {}", input.file_name().unwrap_or_default().to_string_lossy()),
                    why,
                ),
            },
            Err(e) => {
                let text = e.to_string();
                // The Linux test build of vips loads HEIF/AVIF through a dynamic
                // module that this packaging does not register. That is an
                // engine-availability fact, not a pipeline failure: report it as
                // a SKIP with the engine's own words.
                if text.contains("not a known file format") || text.contains("heif") {
                    self.skipped(
                        &format!("{label} {}", input.file_name().unwrap_or_default().to_string_lossy()),
                        format!("engine cannot do this on this dist: {text}"),
                    );
                } else {
                    self.bad(
                        &format!("{label} {}", input.file_name().unwrap_or_default().to_string_lossy()),
                        text,
                    );
                }
            }
        }
    }

    fn fixture(&self, name: &str) -> PathBuf {
        self.fix.join(name)
    }

    /// `vipsheader -a <path>` through the spawn contract.
    fn header(&self, path: &Path) -> Result<String, String> {
        let args = vec![
            Arg::Word("-a"),
            Arg::Path(PathRef::Input, Default::default()),
        ];
        self.run_tool(Program::VipsHeader, args, path)
    }

    fn probe(&self, path: &Path, video: bool) -> Result<verify::ProbeFacts, String> {
        let args = if video {
            crate::engine::ffmpeg::build_probe_video()
        } else {
            crate::engine::ffmpeg::build_probe_audio()
        };
        let text = self.run_tool(Program::Ffprobe, args, path)?;
        Ok(verify::parse_probe(&text))
    }

    fn run_tool(&self, program: Program, args: Vec<Arg>, path: &Path) -> Result<String, String> {
        let plan = Plan {
            steps: vec![Step {
                engine: Engine::Vips,
                program,
                args,
                reads: Source::Input,
                writes: Output::None,
                purpose: "harness probe",
            }],
            expect: Expect::Image {
                target: TargetFormat::Png,
            },
            tag_merge: None,
            notes: vec![],
                temp_exts: vec![],
        };
        let invocations = plan
            .resolve(path, path, &self.work)
            .map_err(|e| e.to_string())?;
        let outcome = proc::run(&self.paths, &invocations[0]).map_err(|e| e.to_string())?;
        if let Some(reason) = outcome.is_fatal() {
            return Err(format!("{}: {reason}", program.name()));
        }
        Ok(outcome.stdout)
    }
}

// Which target is just "the same format" for a source type: the planner refuses
// those (SameFormat) and the queue skips them, so they are not directions.
fn same_format(from: CanonicalType, to: TargetFormat) -> bool {
    matches!(
        (from, to),
        (CanonicalType::Jpeg, TargetFormat::Jpg)
            | (CanonicalType::Png, TargetFormat::Png)
            | (CanonicalType::WebP, TargetFormat::Webp)
            | (CanonicalType::Tiff, TargetFormat::Tiff)
            | (CanonicalType::Avif, TargetFormat::Avif)
            | (CanonicalType::Gif, TargetFormat::Gif)
            | (CanonicalType::Mp3, TargetFormat::Mp3)
            | (CanonicalType::Wav, TargetFormat::Wav)
            | (CanonicalType::Flac, TargetFormat::Flac)
            | (CanonicalType::M4a, TargetFormat::M4a)
            | (CanonicalType::Aac, TargetFormat::Aac)
            | (CanonicalType::Ogg, TargetFormat::Ogg)
            | (CanonicalType::Opus, TargetFormat::Opus)
            | (CanonicalType::Mp4, TargetFormat::Mp4)
            | (CanonicalType::Mkv, TargetFormat::Mkv)
            | (CanonicalType::Mov, TargetFormat::Mov)
            | (CanonicalType::WebM, TargetFormat::Webm)
    )
}

fn main() {
    let fix = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("fixtures"));
    let work = std::env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("ezc-c2-e2e"));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).expect("work dir");

    let mut h = Harness::new(fix.clone(), work.clone());
    println!("engines: vips={:?} ffmpeg={:?}", h.paths.vips, h.paths.ffmpeg);
    println!("fixtures: {}", fix.display());

    // ---------------------------------------------------------------- images
    println!("\n## still images (vips only)");
    let images = [
        ("sample.jpg", CanonicalType::Jpeg),
        ("sample.png", CanonicalType::Png),
        ("sample.webp", CanonicalType::WebP),
        ("sample.tif", CanonicalType::Tiff),
        ("sample.gif", CanonicalType::Gif),
        ("exif_icc.jpg", CanonicalType::Jpeg),
    ];
    let still_targets = [
        TargetFormat::Jpg,
        TargetFormat::Png,
        TargetFormat::Webp,
        TargetFormat::Tiff,
        TargetFormat::Avif,
    ];
    for (file, from) in images {
        let input = h.fixture(file);
        for to in still_targets {
            if same_format(from, to) {
                continue;
            }
            h.case(from, to, &input, Default::default(), |h, out| {
                let exists = verify::exists_nonempty(out);
                if !exists.is_pass() {
                    return Err(exists.reason().unwrap_or("missing").to_string());
                }
                let magic = verify::magic_matches(out, to);
                if !magic.is_pass() {
                    return Err(magic.reason().unwrap_or("wrong family").to_string());
                }
                Ok(format!("{}B {}", std::fs::metadata(out).unwrap().len(), to.as_str()))
            });
        }
        // GIF output for a still image: vips normalise -> ffmpeg palette.
        if !same_format(from, TargetFormat::Gif) {
            h.case(from, TargetFormat::Gif, &input, Default::default(), |h, out| {
                let facts = h.probe(out, true)?;
                if facts.codec_name.as_deref() != Some("gif") {
                    return Err(format!("codec={:?} not gif", facts.codec_name));
                }
                Ok(format!(
                    "{}x{} frames={:?}",
                    facts.width.unwrap_or(0),
                    facts.height.unwrap_or(0),
                    facts.avg_frame_rate
                ))
            });
        }
    }

    // -------------------------------------------------- EXIF / ICC / rotation
    println!("\n## metadata (kept by default, stripped on request)");
    let exif_in = h.fixture("exif_icc.jpg");
    for to in [TargetFormat::Png, TargetFormat::Webp, TargetFormat::Tiff] {
        let out = work.join(format!("meta-kept.{}", to.extension()));
        let job = convert::Job {
            from: CanonicalType::Jpeg,
            to,
            options: Default::default(),
        };
        let report = convert::convert(&h.paths, &job, &exif_in, &out, &work.join("tmp-meta"));
        match report {
            Ok(_) => {
                let header = h.header(&out).unwrap_or_default();
                let exif = header.contains("exif-ifd0-Orientation");
                let icc = header.contains("icc-profile-data");
                // TIFF keeps the ICC profile and bakes the orientation into the
                // pixels (autorot writes orientation=1); vips does not attach a
                // second EXIF blob inside a TIFF, so only ICC is asserted there.
                let exif_ok = to == TargetFormat::Tiff || exif;
                if exif_ok && icc {
                    h.ok(
                        &format!("EXIF+ICC kept -> {}", to.as_str()),
                        format!("exif={exif} icc={icc} (ICC always; EXIF blob where the container has one)"),
                    );
                } else {
                    h.bad(
                        &format!("EXIF+ICC kept -> {}", to.as_str()),
                        format!("exif={exif} icc={icc}"),
                    );
                }
            }
            Err(e) => h.bad(&format!("EXIF+ICC kept -> {}", to.as_str()), e.to_string()),
        }

        let stripped = work.join(format!("meta-strip.{}", to.extension()));
        let job = convert::Job {
            from: CanonicalType::Jpeg,
            to,
            options: crate::engine::plan::Options {
                strip_metadata: true,
                ..Default::default()
            },
        };
        match convert::convert(&h.paths, &job, &exif_in, &stripped, &work.join("tmp-strip")) {
            Ok(_) => {
                let header = h.header(&stripped).unwrap_or_default();
                let exif = header.contains("exif-ifd0-Orientation");
                let icc = header.contains("icc-profile-data");
                if !exif && !icc {
                    h.ok(
                        &format!("strip removes EXIF/ICC -> {}", to.as_str()),
                        "no exif/icc tags in the output".into(),
                    );
                } else {
                    h.bad(
                        &format!("strip removes EXIF/ICC -> {}", to.as_str()),
                        format!("exif={exif} icc={icc} still present"),
                    );
                }
            }
            Err(e) => h.bad(&format!("strip removes EXIF/ICC -> {}", to.as_str()), e.to_string()),
        }
    }

    // orientation: 400x200 with Orientation=6 must come out 200x400
    let rotated = work.join("autorot.png");
    let job = convert::Job {
        from: CanonicalType::Jpeg,
        to: TargetFormat::Png,
        options: Default::default(),
    };
    match convert::convert(&h.paths, &job, &exif_in, &rotated, &work.join("tmp-rot")) {
        Ok(_) => {
            let header = h.header(&rotated).unwrap_or_default();
            let dims: Vec<&str> = header
                .lines()
                .filter(|l| l.starts_with("width:") || l.starts_with("height:"))
                .collect();
            if dims == vec!["width: 200", "height: 400"] {
                h.ok("orientation baked (autorot)", "400x200 + Orientation=6 -> 200x400".into());
            } else {
                h.bad("orientation baked (autorot)", format!("{dims:?}"));
            }
        }
        Err(e) => h.bad("orientation baked (autorot)", e.to_string()),
    }

    // resize: 50 %
    let resized = work.join("resize.png");
    let job = convert::Job {
        from: CanonicalType::Jpeg,
        to: TargetFormat::Png,
        options: crate::engine::plan::Options {
            resize: Some(crate::engine::plan::Resize::Percent(50)),
            ..Default::default()
        },
    };
    let src_w = 400u32;
    match convert::convert(&h.paths, &job, &exif_in, &resized, &work.join("tmp-resize")) {
        Ok(_) => {
            let header = h.header(&resized).unwrap_or_default();
            if header.contains(&format!("width: {}", src_w / 2)) {
                h.ok("image resize 50%", format!("width {} -> {}", src_w, src_w / 2));
            } else {
                h.bad("image resize 50%", header.lines().take(3).collect::<Vec<_>>().join(" "));
            }
        }
        Err(e) => h.bad("image resize 50%", e.to_string()),
    }

    // ---------------------------------------------------------------- audio
    println!("\n## audio (LGPL ffmpeg, explicit -ar/-sample_fmt)");
    let audio: [(CanonicalType, &str); 7] = [
        (CanonicalType::Mp3, "sample.mp3"),
        (CanonicalType::Wav, "sample.wav"),
        (CanonicalType::Flac, "sample.flac"),
        (CanonicalType::Aac, "sample.aac"),
        (CanonicalType::M4a, "sample.m4a"),
        (CanonicalType::Ogg, "sample.ogg"),
        (CanonicalType::Opus, "sample.opus"),
    ];
    let audio_targets = [
        TargetFormat::Mp3,
        TargetFormat::Wav,
        TargetFormat::Flac,
        TargetFormat::M4a,
        TargetFormat::Aac,
        TargetFormat::Ogg,
        TargetFormat::Opus,
    ];
    for (from, file) in audio {
        let input = h.fixture(file);
        for to in audio_targets {
            if from == CanonicalType::Mp3 && to == TargetFormat::Mp3 {
                continue;
            }
            let skip_self = match (from, to) {
                (CanonicalType::Wav, TargetFormat::Wav)
                | (CanonicalType::Flac, TargetFormat::Flac)
                | (CanonicalType::Aac, TargetFormat::Aac)
                | (CanonicalType::M4a, TargetFormat::M4a)
                | (CanonicalType::Ogg, TargetFormat::Ogg)
                | (CanonicalType::Opus, TargetFormat::Opus) => true,
                _ => false,
            };
            if skip_self {
                continue;
            }
            h.case(from, to, &input, Default::default(), |h, out| {
                let facts = h.probe(out, false)?;
                if facts.sample_rate.is_none() {
                    return Err("no sample rate in the output".into());
                }
                if facts.duration_ms.unwrap_or(0) < 20 {
                    return Err(format!("duration {:?} ms", facts.duration_ms));
                }
                Ok(format!(
                    "{} {} Hz {} ch",
                    facts.codec_name.clone().unwrap_or_default(),
                    facts.sample_rate.unwrap_or(0),
                    facts.channels.unwrap_or(0)
                ))
            });
        }
    }

    // rate clamping and wide-bit handling
    println!("\n## audio rate discipline (measured encoder limits)");
    let hi = h.fixture("tone24_96k.wav");
    for (to, want_rate) in [
        (TargetFormat::Mp3, 48000u32),
        (TargetFormat::Flac, 96000),
        (TargetFormat::Opus, 48000),
        (TargetFormat::Aac, 96000),
    ] {
        let out = work.join(format!("rate.{}", to.extension()));
        let job = convert::Job {
            from: CanonicalType::Wav,
            to,
            options: Default::default(),
        };
        match convert::convert(&h.paths, &job, &hi, &out, &work.join("tmp-rate")) {
            Ok(_) => match h.probe(&out, false) {
                Ok(facts) => {
                    let got = facts.sample_rate.unwrap_or(0);
                    if got == want_rate {
                        h.ok(
                            &format!("96 kHz source -> {}", to.as_str()),
                            format!("sample_rate={got} Hz (clamped as measured)"),
                        );
                    } else {
                        h.bad(
                            &format!("96 kHz source -> {}", to.as_str()),
                            format!("sample_rate={got}, expected {want_rate}"),
                        );
                    }
                }
                Err(e) => h.bad(&format!("96 kHz source -> {}", to.as_str()), e),
            },
            Err(e) => h.bad(&format!("96 kHz source -> {}", to.as_str()), e.to_string()),
        }
    }

    // tag carry-over through lofty
    println!("\n## audio tags (lofty merge after the encode)");
    let tagged = h.fixture("tagged.mp3");
    for to in [
        TargetFormat::Wav,
        TargetFormat::Flac,
        TargetFormat::M4a,
        TargetFormat::Ogg,
        TargetFormat::Opus,
        TargetFormat::Aac,
    ] {
        let out = work.join(format!("tags.{}", to.extension()));
        let job = convert::Job {
            from: CanonicalType::Mp3,
            to,
            options: Default::default(),
        };
        match convert::convert(&h.paths, &job, &tagged, &out, &work.join("tmp-tags")) {
            Ok(report) => {
                let text = h
                    .run_tool(
                        Program::Ffprobe,
                        vec![
                            Arg::Word("-v"),
                            Arg::Word("error"),
                            Arg::Word("-show_entries"),
                            Arg::Word("format_tags"),
                            Arg::Word("-show_entries"),
                            // Ogg/Opus keep their comments on the stream, MP3 and
                            // FLAC on the format: ask for both or the readback
                            // lies about perfectly good tags.
                            Arg::Word("stream_tags"),
                            Arg::Word("-of"),
                            Arg::Word("default=nw=1"),
                            Arg::Path(PathRef::Input, Default::default()),
                        ],
                        &out,
                    )
                    .unwrap_or_default();
                let title = text.to_lowercase().contains("title=c2 tag probe");
                match (&report.tags, &report.tag_error) {
                    (Some(r), _) if title => h.ok(
                        &format!("tags survive mp3 -> {}", to.as_str()),
                        format!(
                            "{} items, {} pictures, readback has title",
                            r.items, r.pictures
                        ),
                    ),
                    (Some(r), _) => h.bad(
                        &format!("tags survive mp3 -> {}", to.as_str()),
                        format!(
                            "lofty wrote {} items but the target does not read back a title ({:?})",
                            r.items, text
                        ),
                    ),
                    (None, Some(e)) => h.bad(
                        &format!("tags survive mp3 -> {}", to.as_str()),
                        format!("tag merge failed and was reported: {e}"),
                    ),
                    (None, None) => h.bad(
                        &format!("tags survive mp3 -> {}", to.as_str()),
                        "no tag merge ran and no error was reported".into(),
                    ),
                }
            }
            Err(e) => h.bad(&format!("tags survive mp3 -> {}", to.as_str()), e.to_string()),
        }
    }

    // ----------------------------------------------------------------- video
    println!("\n## video (CFR forced)");
    let video: [(CanonicalType, &str); 4] = [
        (CanonicalType::Mp4, "sample.mp4"),
        (CanonicalType::Mkv, "sample.mkv"),
        (CanonicalType::Mov, "sample.mov"),
        (CanonicalType::WebM, "sample.webm"),
    ];
    let video_targets = [
        TargetFormat::Mp4,
        TargetFormat::Mkv,
        TargetFormat::Mov,
        TargetFormat::Webm,
    ];
    for (from, file) in video {
        let input = h.fixture(file);
        for to in video_targets {
            let same = matches!(
                (from, to),
                (CanonicalType::Mp4, TargetFormat::Mp4)
                    | (CanonicalType::Mkv, TargetFormat::Mkv)
                    | (CanonicalType::Mov, TargetFormat::Mov)
                    | (CanonicalType::WebM, TargetFormat::Webm)
            );
            if same {
                continue;
            }
            h.case(from, to, &input, Default::default(), |h, out| {
                let facts = h.probe(out, true)?;
                let v = verify::probe_is_valid(&facts, true);
                if !v.is_pass() {
                    return Err(v.reason().unwrap_or("invalid").to_string());
                }
                if verify::is_vfr(&facts) {
                    return Err(format!(
                        "output is variable frame rate: {:?} vs {:?}",
                        facts.r_frame_rate, facts.avg_frame_rate
                    ));
                }
                Ok(format!(
                    "{} {}x{} {}fps dur={:?}ms",
                    facts.codec_name.clone().unwrap_or_default(),
                    facts.width.unwrap_or(0),
                    facts.height.unwrap_or(0),
                    facts.r_frame_rate.map(|(n, d)| n as f64 / d as f64).unwrap_or(0.0),
                    facts.duration_ms
                ))
            });
        }
        // video -> gif
        h.case(from, TargetFormat::Gif, &input, Default::default(), |h, out| {
            let facts = h.probe(out, true)?;
            if facts.codec_name.as_deref() != Some("gif") {
                return Err("not a gif".into());
            }
            Ok(format!(
                "{}x{} frames={:?}",
                facts.width.unwrap_or(0),
                facts.height.unwrap_or(0),
                facts.avg_frame_rate
            ))
        });
    }

    // VFR input -> CFR output
    println!("\n## the VFR trap (measured control)");
    let vfr = h.fixture("sample_vfr.mkv");
    let vfr_in = h.probe(&vfr, true).unwrap_or_default();
    println!(
        "      source: r_frame_rate={:?} avg_frame_rate={:?} is_vfr={}",
        vfr_in.r_frame_rate,
        vfr_in.avg_frame_rate,
        verify::is_vfr(&vfr_in)
    );
    let out = work.join("vfr-cfr.mp4");
    let job = convert::Job {
        from: CanonicalType::Mkv,
        to: TargetFormat::Mp4,
        options: Default::default(),
    };
    match convert::convert(&h.paths, &job, &vfr, &out, &work.join("tmp-vfr")) {
        Ok(_) => match h.probe(&out, true) {
            Ok(facts) => {
                if !verify::is_vfr(&facts) && facts.r_frame_rate == facts.avg_frame_rate {
                    h.ok(
                        "VFR -> CFR",
                        format!("{:?} == {:?} (constant)", facts.r_frame_rate, facts.avg_frame_rate),
                    );
                } else {
                    h.bad(
                        "VFR -> CFR",
                        format!("still variable: {:?} vs {:?}", facts.r_frame_rate, facts.avg_frame_rate),
                    );
                }
            }
            Err(e) => h.bad("VFR -> CFR", e),
        },
        Err(e) => h.bad("VFR -> CFR", e.to_string()),
    }

    // trim: 0.5 s in, 1.0 s long
    let trimmed = work.join("trim.mp4");
    let job = convert::Job {
        from: CanonicalType::Mkv,
        to: TargetFormat::Mp4,
        options: crate::engine::plan::Options {
            trim: Some(crate::engine::plan::Trim {
                start_ms: 500,
                length_ms: 1000,
            }),
            ..Default::default()
        },
    };
    let clip = h.fixture("sample_vfr.mkv");
    match convert::convert(&h.paths, &job, &clip, &trimmed, &work.join("tmp-trim")) {
        Ok(_) => match h.probe(&trimmed, true) {
            Ok(facts) => {
                let ms = facts.duration_ms.unwrap_or(0);
                if (950..=1060).contains(&ms) {
                    h.ok("trim 0.5 s + 1.0 s", format!("duration={ms} ms, frames={:?}", facts.r_frame_rate));
                } else {
                    h.bad("trim 0.5 s + 1.0 s", format!("duration={ms} ms, expected ~1000"));
                }
            }
            Err(e) => h.bad("trim 0.5 s + 1.0 s", e),
        },
        Err(e) => h.bad("trim 0.5 s + 1.0 s", e.to_string()),
    }

    // video resize
    let small = work.join("resized.mp4");
    let job = convert::Job {
        from: CanonicalType::Mkv,
        to: TargetFormat::Mp4,
        options: crate::engine::plan::Options {
            resize: Some(crate::engine::plan::Resize::Width(160)),
            ..Default::default()
        },
    };
    match convert::convert(&h.paths, &job, &clip, &small, &work.join("tmp-vsmall")) {
        Ok(_) => match h.probe(&small, true) {
            Ok(facts) => {
                if facts.width == Some(160) {
                    h.ok("video resize to 160", format!("{}x{}", facts.width.unwrap_or(0), facts.height.unwrap_or(0)));
                } else {
                    h.bad("video resize to 160", format!("width={:?}", facts.width));
                }
            }
            Err(e) => h.bad("video resize to 160", e),
        },
        Err(e) => h.bad("video resize to 160", e.to_string()),
    }

    // --------------------------------------------------------------- summary
    println!(
        "\nSUMMARY pass={} fail={} skip={}",
        h.pass, h.fail, h.skip
    );
    if !h.failures.is_empty() {
        println!("\nfailures:");
        for f in &h.failures {
            println!("  - {f}");
        }
        std::process::exit(1);
    }
}
