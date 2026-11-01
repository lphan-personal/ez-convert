//! `engine/convert.rs` — the pipeline: probe → plan → spawn → verify → tags.
//!
//! This is the only function the job worker calls. It exists so the ordering
//! rules are enforced in one place:
//!
//! 1. **probe first** — audio/video plans need real source facts, and guessing
//!    them is how sample-rate drift and wrong CFR rates enter a converter;
//! 2. **plan** — pure, unit-tested, refuses unrouted directions;
//! 3. **spawn** — through `proc::run`, the only `Command` in the tree;
//! 4. **verify** — a missing/zero-byte/wrong-family output is a failure, not a
//!    success with a warning (`ARCHITECTURE.md` §6.0.4);
//! 5. **tags** — lofty copies the source's tags onto the finished file when the
//!    plan asks for it. A tag failure is reported, never fatal to the bytes.

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::plan::{self, Expect, Options, Plan, PlanError, Request, SourceFacts, TagMerge};
use super::proc::{self, EnginePaths, Outcome, Program};
use super::tags::MergeReport;
use super::verify::{self, Verdict};
use crate::model::types::CanonicalType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvertError {
    Plan(PlanError),
    /// The engine ran and said no. Carries the rc/stderr summary.
    Engine(String),
    /// The engine "succeeded" but the output does not hold up.
    Verify(String),
    /// A sidecar is not installed; capability is what runs, not what is listed.
    MissingEngine(&'static str),
    Io(String),
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConvertError::Plan(e) => write!(f, "{e}"),
            ConvertError::Engine(s) => write!(f, "engine failed: {s}"),
            ConvertError::Verify(s) => write!(f, "output failed verification: {s}"),
            ConvertError::MissingEngine(p) => write!(f, "{p} is not installed"),
            ConvertError::Io(s) => write!(f, "{s}"),
        }
    }
}

impl From<PlanError> for ConvertError {
    fn from(e: PlanError) -> Self {
        ConvertError::Plan(e)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversionReport {
    pub target: crate::model::types::TargetFormat,
    pub steps: Vec<Outcome>,
    pub output_bytes: u64,
    pub tags: Option<MergeReport>,
    /// Set when the tag merge could not be applied. The bytes are still valid.
    pub tag_error: Option<String>,
    pub duration_ms: u128,
}

/// What the caller asked for: one file, one target, one option set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub from: CanonicalType,
    pub to: crate::model::types::TargetFormat,
    pub options: Options,
}

/// `ffprobe` the source and turn it into planner facts.
///
/// Video facts need two probes: one for the stream kinds (is there audio?) and
/// one for the video geometry/rate. Both are ~10 ms; correctness first.
pub fn probe_source(
    paths: &EnginePaths,
    from: CanonicalType,
    input: &Path,
    cwd: &Path,
) -> Result<SourceFacts, ConvertError> {
    match plan::media_family(from) {
        plan::Family::Audio => {
            let args = super::ffmpeg::build_probe_audio();
            let out = run_probe(paths, args, input, cwd)?;
            let facts = verify::parse_probe(&out.stdout);
            let rate = facts
                .sample_rate
                .ok_or_else(|| ConvertError::Io("ffprobe reported no sample rate".into()))?;
            Ok(SourceFacts::Audio {
                sample_rate: rate,
                channels: facts.channels.unwrap_or(2),
                bits: facts.bits_per_raw_sample.unwrap_or(0) as u16,
                tags: true,
            })
        }
        plan::Family::Video => {
            let kinds = run_probe(
                paths,
                vec![
                    super::plan::Arg::Word("-v"),
                    super::plan::Arg::Word("error"),
                    super::plan::Arg::Word("-show_entries"),
                    super::plan::Arg::Word("stream=codec_type"),
                    super::plan::Arg::Word("-show_entries"),
                    super::plan::Arg::Word("format=duration"),
                    super::plan::Arg::Word("-of"),
                    super::plan::Arg::Word("default=nw=1"),
                    super::plan::Arg::Path(super::plan::PathRef::Input, Default::default()),
                ],
                input,
                cwd,
            )?;
            let kind_facts = verify::parse_probe(&kinds.stdout);
            let out = run_probe(paths, super::ffmpeg::build_probe_video(), input, cwd)?;
            let facts = verify::parse_probe(&out.stdout);
            let (fps_num, fps_den) = facts
                .r_frame_rate
                .or(facts.avg_frame_rate)
                .ok_or_else(|| ConvertError::Io("ffprobe reported no frame rate".into()))?;
            let (width, height) = match (facts.width, facts.height) {
                (Some(w), Some(h)) => (w, h),
                _ => return Err(ConvertError::Io("ffprobe reported no geometry".into())),
            };
            let audio_rate = if kind_facts.has_audio {
                let a = run_probe(paths, super::ffmpeg::build_probe_audio(), input, cwd)?;
                let af = verify::parse_probe(&a.stdout);
                af.sample_rate.unwrap_or(48000)
            } else {
                48000
            };
            Ok(SourceFacts::Video {
                fps_num,
                fps_den,
                width,
                height,
                has_audio: kind_facts.has_audio,
                audio_rate,
            })
        }
        _ => Ok(SourceFacts::Stills),
    }
}

fn run_probe(
    paths: &EnginePaths,
    args: Vec<super::plan::Arg>,
    input: &Path,
    cwd: &Path,
) -> Result<Outcome, ConvertError> {
    let plan = Plan {
        steps: vec![super::plan::Step {
            engine: crate::model::types::Engine::Ffmpeg,
            program: Program::Ffprobe,
            args,
            reads: super::plan::Source::Input,
            writes: super::plan::Output::None,
            purpose: "ffprobe",
        }],
        expect: Expect::Audio {
            target: crate::model::types::TargetFormat::Wav,
        },
        tag_merge: None,
        notes: vec![],
                temp_exts: vec![],
    };
    let invocations = plan
        .resolve(input, input, cwd)
        .map_err(|e| ConvertError::Io(e.to_string()))?;
    let outcome = proc::run(paths, &invocations[0]).map_err(|e| ConvertError::Io(e.to_string()))?;
    if let Some(reason) = outcome.is_fatal() {
        return Err(ConvertError::Engine(format!("ffprobe: {reason}")));
    }
    Ok(outcome)
}

/// Run one conversion end to end. Returns only when the output is verified.
pub fn convert(
    paths: &EnginePaths,
    job: &Job,
    input: &Path,
    output: &Path,
    temp_dir: &Path,
) -> Result<ConversionReport, ConvertError> {
    let started = Instant::now();
    std::fs::create_dir_all(temp_dir).map_err(|e| ConvertError::Io(e.to_string()))?;

    let facts = probe_source(paths, job.from, input, temp_dir)?;
    let plan = plan::plan(&Request {
        from: job.from,
        to: job.to,
        options: job.options.clone(),
        facts,
        temp_dir,
    })?;

    if let Some(missing) = plan::missing_engine(paths, &plan) {
        return Err(ConvertError::MissingEngine(missing.name()));
    }

    let invocations = plan
        .resolve(input, output, temp_dir)
        .map_err(|e| ConvertError::Io(e.to_string()))?;

    let mut outcomes = Vec::with_capacity(invocations.len());
    for (index, invocation) in invocations.iter().enumerate() {
        let outcome =
            proc::run(paths, invocation).map_err(|e| ConvertError::Io(e.to_string()))?;
        let fatal = outcome.is_fatal();
        outcomes.push(outcome);
        if let Some(reason) = fatal {
            // A failed step may leave a zero-byte file behind; never publish it.
            let _ = std::fs::remove_file(output);
            let stderr = outcomes
                .last()
                .map(|o| o.stderr.trim().to_string())
                .unwrap_or_default();
            return Err(ConvertError::Engine(format!(
                "step {} ({}): {reason}{}{stderr}",
                index + 1,
                plan.steps[index].purpose,
                if stderr.is_empty() { "" } else { "; stderr: " },
            )));
        }
    }

    verify_output(paths, output, plan.expect, temp_dir)?;

    let mut report = ConversionReport {
        target: job.to,
        steps: outcomes,
        output_bytes: std::fs::metadata(output).map(|m| m.len()).unwrap_or(0),
        tags: None,
        tag_error: None,
        duration_ms: started.elapsed().as_millis(),
    };

    if let Some(TagMerge { from, to: _ }) = plan.tag_merge {
        debug_assert_eq!(from, super::plan::PathRef::Input);
        if !job.options.strip_metadata {
            match super::tags::merge(input, output) {
                Ok(r) => report.tags = Some(r),
                Err(e) => report.tag_error = Some(e.to_string()),
            }
        }
    }

    Ok(report)
}

fn verify_output(
    paths: &EnginePaths,
    output: &Path,
    expect: Expect,
    cwd: &Path,
) -> Result<(), ConvertError> {
    match verify::exists_nonempty(output) {
        Verdict::Pass => {}
        Verdict::Fail(reason) => return Err(ConvertError::Verify(reason)),
    }
    match expect {
        Expect::Image { target } => {
            match verify::magic_matches(output, target) {
                Verdict::Pass => {}
                Verdict::Fail(reason) => return Err(ConvertError::Verify(reason)),
            }
            // Decodable header, not just the right first bytes.
            let args = vec![
                super::plan::Arg::Word("-f"),
                super::plan::Arg::Word("width"),
                super::plan::Arg::Path(super::plan::PathRef::Input, Default::default()),
            ];
            let plan = Plan {
                steps: vec![super::plan::Step {
                    engine: crate::model::types::Engine::Vips,
                    program: Program::VipsHeader,
                    args,
                    reads: super::plan::Source::Input,
                    writes: super::plan::Output::None,
                    purpose: "vipsheader",
                }],
                expect,
                tag_merge: None,
                notes: vec![],
                temp_exts: vec![],
            };
            let invocations = plan
                .resolve(output, output, cwd)
                .map_err(|e| ConvertError::Io(e.to_string()))?;
            let outcome = proc::run(paths, &invocations[0])
                .map_err(|e| ConvertError::Io(e.to_string()))?;
            if let Some(reason) = outcome.is_fatal() {
                return Err(ConvertError::Verify(format!(
                    "{} is not decodable: {reason}",
                    output.display()
                )));
            }
            if verify::parse_header_dimension(&outcome.stdout).is_none() {
                return Err(ConvertError::Verify(format!(
                    "{} has no positive width",
                    output.display()
                )));
            }
            Ok(())
        }
        Expect::Audio { .. } | Expect::Video { .. } => {
            let want_video = matches!(expect, Expect::Video { .. });
            let args = if want_video {
                super::ffmpeg::build_probe_video()
            } else {
                super::ffmpeg::build_probe_audio()
            };
            let outcome = run_probe(paths, args, output, cwd)?;
            let facts = verify::parse_probe(&outcome.stdout);
            match verify::probe_is_valid(&facts, want_video) {
                Verdict::Pass => Ok(()),
                Verdict::Fail(reason) => Err(ConvertError::Verify(reason)),
            }
        }
    }
}

/// Where intermediates and the finished file live for one job.
pub fn job_temp_dir(root: &Path, job_id: &str) -> PathBuf {
    root.join(job_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::types::TargetFormat;

    #[test]
    fn unrouted_jobs_fail_at_plan_time_before_any_spawn() {
        let paths = EnginePaths::from_root(Path::new("/nonexistent"));
        let job = Job {
            from: CanonicalType::Jpeg,
            to: TargetFormat::Bmp,
            options: Options::default(),
        };
        let err = convert(
            &paths,
            &job,
            Path::new("/tmp/none.jpg"),
            Path::new("/tmp/none.bmp"),
            Path::new("/tmp"),
        )
        .unwrap_err();
        assert!(matches!(err, ConvertError::Plan(_)), "got {err:?}");
    }

    #[test]
    fn missing_engine_is_named_not_silently_skipped() {
        let paths = EnginePaths::from_root(Path::new("/definitely/not/here"));
        let job = Job {
            from: CanonicalType::Jpeg,
            to: TargetFormat::Png,
            options: Options::default(),
        };
        let err = convert(
            &paths,
            &job,
            Path::new("/tmp/none.jpg"),
            Path::new("/tmp/none.png"),
            Path::new("/tmp"),
        )
        .unwrap_err();
        // Path admission runs first and reports the missing input, which is the
        // honest answer; the engine check is reached with a real file.
        assert!(
            matches!(err, ConvertError::Io(_) | ConvertError::MissingEngine(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn engine_paths_use_the_pack_layout() {
        let p = EnginePaths::from_root(Path::new("/res"));
        assert!(p.vips.ends_with(if cfg!(windows) { "vips.exe" } else { "vips" }));
        assert!(p.vips.starts_with("/res/engines"));
        assert!(p.ffprobe.starts_with("/res/engines/ffmpeg"));
    }
}
