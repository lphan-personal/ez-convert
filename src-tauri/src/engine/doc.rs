//! `engine/doc.rs` — the document pipeline: plan → one batched spawn → attribute
//! → verify.
//!
//! The document engine is not like the media engines: LibreOffice writes into a
//! directory with names **it** chooses (`<stem>-<sheet>.csv` for a spreadsheet,
//! measured §6.2 D5), reports success for inputs it did not really convert
//! (§6.2 N3), and carries no useful exit code. So the pipeline is built around
//! three rules:
//!
//! 1. **One spawn per batch.** Every file in the batch shares the route and the
//!    outdir; measured 0.20 s/file batched vs 0.52 s/file spawned.
//! 2. **Attribution by listing, not by assumption.** After the run the outdir is
//!    listed and each produced file is matched back to its input stem. A file
//!    that produced nothing is a failure, never a silent skip.
//! 3. **`rc=0` is not success.** The output must exist, be non-empty and pass a
//!    structural validator (`verify::document_output`) before it is published.
//!
//! The user-facing text of every failure is the §14 message table, verbatim.

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::proc::{self, EnginePaths, Outcome, Program};
use super::soffice::{self, BatchSpec, DocRoute};
use super::verify::{self, Verdict};
use crate::model::types::{CanonicalType, TargetFormat};

/// One document conversion request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocJob {
    pub from: CanonicalType,
    pub to: TargetFormat,
}

/// What one input turned into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Produced {
    pub input: PathBuf,
    /// Usually one file; more than one means LibreOffice split the output
    /// (per-sheet exports). The current CSV filter exports a single sheet, so a
    /// multi-output result is reported rather than guessed at.
    pub outputs: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocError {
    /// The pair has no measured route (matrix `Planned`/`Cut`).
    NotRouted { from: CanonicalType, to: TargetFormat },
    /// The engine ran and said no, was killed, or could not be spawned.
    Engine(String),
    /// `rc=0` and no expected file appeared — the measured silent no-op.
    NoOutput {
        outdir: PathBuf,
        expected: &'static str,
        listing: Vec<String>,
    },
    /// A file appeared and does not hold up structurally.
    Verify(String),
    /// More than one file for one input; naming is not ours to invent here.
    MultiOutput(Vec<PathBuf>),
    MissingEngine(&'static str),
    /// A batch may only contain one route.
    MixedBatch(&'static str),
    Io(String),
}

impl DocError {
    /// The §14 message, verbatim. This is what the window shows; raw engine
    /// stderr stays in the diagnostics pane.
    pub fn user_message(&self) -> String {
        match self {
            DocError::NotRouted { from, to } => format!(
                "There's no path from {:?} to {} yet.",
                from,
                to.as_str()
            ),
            DocError::Engine(_) | DocError::Verify(_) => {
                "The result didn't pass a check, so it wasn't saved. The original is unchanged."
                    .to_string()
            }
            DocError::NoOutput { .. } => "LibreOffice reported success but wrote no file — \
                 nothing was saved. This is a bug; the details are in the log."
                .to_string(),
            DocError::MultiOutput(paths) => format!(
                "This file converted into {} parts. Nothing was saved; the details are in the log.",
                paths.len()
            ),
            DocError::MissingEngine(engine) => format!(
                "{engine} isn't available, so document conversions are off. \
                 Reinstall EZ Convert to restore it."
            ),
            DocError::MixedBatch(_) | DocError::Io(_) => {
                "The result didn't pass a check, so it wasn't saved. \
                 The original is unchanged."
                    .to_string()
            }
        }
    }
}

impl std::fmt::Display for DocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocError::NotRouted { from, to } => {
                write!(f, "{from:?} -> {:?} has no measured LibreOffice filter", to)
            }
            DocError::Engine(s) => write!(f, "soffice failed: {s}"),
            DocError::NoOutput {
                outdir,
                expected,
                listing,
            } => write!(
                f,
                "soffice exited 0 but wrote no .{expected} file in {} (outdir: {})",
                outdir.display(),
                if listing.is_empty() {
                    "empty".to_string()
                } else {
                    listing.join(", ")
                }
            ),
            DocError::Verify(s) => write!(f, "output failed verification: {s}"),
            DocError::MultiOutput(paths) => write!(
                f,
                "one input produced {} outputs: {:?}",
                paths.len(),
                paths
                    .iter()
                    .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .collect::<Vec<_>>()
            ),
            DocError::MissingEngine(name) => write!(f, "{name} is not installed"),
            DocError::MixedBatch(what) => write!(f, "mixed batch: {what}"),
            DocError::Io(s) => write!(f, "{s}"),
        }
    }
}

/// Result of one batch: what was produced, and the engine outcome that produced
/// it (kept for the job log and for evidence transcripts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchReport {
    pub produced: Vec<Produced>,
    pub steps: Vec<Outcome>,
    pub duration_ms: u128,
    pub route: DocRoute,
}

impl BatchReport {
    pub fn total_outputs(&self) -> usize {
        self.produced.iter().map(|p| p.outputs.len()).sum()
    }
}

fn extension_for(target: TargetFormat) -> Result<&'static str, DocError> {
    verify::document_extension(target).ok_or(DocError::Io(format!(
        "no document extension for {}",
        target.as_str()
    )))
}

/// The route, or the honest refusal.
pub fn route_or_error(job: &DocJob) -> Result<DocRoute, DocError> {
    soffice::route(job.from, job.to).ok_or(DocError::NotRouted {
        from: job.from,
        to: job.to,
    })
}

/// Create (or recreate) the per-job outdir. LibreOffice writes into whatever is
/// there, so a stale outdir would make attribution lie.
fn prepare_outdir(outdir: &Path) -> Result<PathBuf, DocError> {
    if outdir.exists() {
        std::fs::remove_dir_all(outdir).map_err(|e| DocError::Io(e.to_string()))?;
    }
    std::fs::create_dir_all(outdir).map_err(|e| DocError::Io(e.to_string()))?;
    std::fs::canonicalize(outdir).map_err(|e| DocError::Io(e.to_string()))
}

/// Directory entries with the target extension, sorted for a stable log.
fn list_with_extension(dir: &Path, ext: &str) -> Result<Vec<PathBuf>, DocError> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| DocError::Io(e.to_string()))? {
        let entry = entry.map_err(|e| DocError::Io(e.to_string()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let matches = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case(ext))
            .unwrap_or(false);
        if matches {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// Does `output_stem` belong to `input_stem`?
///
/// Exact match, or the measured LibreOffice renaming `<stem>-<sheet>` (§6.2 D5).
/// `sample2` must not match `sample`, hence the separator requirement.
pub fn stem_belongs(output_stem: &str, input_stem: &str) -> bool {
    output_stem.eq_ignore_ascii_case(input_stem)
        || output_stem
            .to_ascii_lowercase()
            .starts_with(&format!("{}-", input_stem.to_ascii_lowercase()))
}

/// Attribute produced files to inputs. Longest stem wins, so a batch containing
/// `a.docx` and `a-b.docx` cannot cross-attribute `a-b.pdf`.
pub fn attribute(inputs: &[PathBuf], produced: &[PathBuf]) -> Vec<Produced> {
    let stems: Vec<String> = inputs.iter().map(|p| stem_of(p)).collect();
    let mut out: Vec<Produced> = inputs
        .iter()
        .map(|p| Produced {
            input: p.clone(),
            outputs: Vec::new(),
        })
        .collect();
    for file in produced {
        let file_stem = stem_of(file);
        let mut best: Option<(usize, usize)> = None; // (stem length, index)
        for (index, stem) in stems.iter().enumerate() {
            if stem_belongs(&file_stem, stem) {
                let len = stem.len();
                if best.map(|(current, _)| len > current).unwrap_or(true) {
                    best = Some((len, index));
                }
            }
        }
        if let Some((_, index)) = best {
            out[index].outputs.push(file.clone());
        }
    }
    out
}

/// Run one homogeneous batch through a single `soffice` invocation.
///
/// The caller passes files that share `job` and `outdir`; anything else is
/// rejected by [`DocError::MixedBatch`] rather than silently mis-converted.
pub fn convert_batch(
    paths: &EnginePaths,
    job: &DocJob,
    inputs: &[PathBuf],
    outdir: &Path,
    temp_dir: &Path,
) -> Result<BatchReport, DocError> {
    let started = Instant::now();
    if inputs.is_empty() {
        return Err(DocError::MixedBatch("no inputs"));
    }
    let route = route_or_error(job)?;
    if !paths.installed(Program::Soffice) {
        return Err(DocError::MissingEngine("LibreOffice"));
    }

    let mut admitted = Vec::with_capacity(inputs.len());
    for input in inputs {
        let path = proc::admit(input, true).map_err(|e| DocError::Io(e.to_string()))?;
        admitted.push(soffice::plain_path(&path));
    }

    let outdir = prepare_outdir(outdir)?;
    std::fs::create_dir_all(temp_dir).map_err(|e| DocError::Io(e.to_string()))?;

    let platform = soffice::native_platform();
    let profile_url = soffice::file_url(&temp_dir.join("profile"), platform);
    let launcher = if paths.soffice.is_file() {
        paths.soffice.clone()
    } else {
        // The install layout may name the directory differently; the launcher
        // *name* stays platform-decided.
        soffice::launcher(&paths.soffice_dir, platform)
    };

    let spec = BatchSpec {
        route,
        launcher: &launcher,
        profile_url: &profile_url,
        outdir: &outdir,
        inputs: &admitted,
    };
    let outcome = proc::run(paths, &spec.invocation(temp_dir))
        .map_err(|e| DocError::Engine(format!("could not spawn soffice: {e}")))?;
    if let Some(reason) = outcome.is_fatal() {
        let stderr = outcome.stderr.trim();
        return Err(DocError::Engine(format!(
            "{reason}{}{}",
            if stderr.is_empty() { "" } else { "; stderr: " },
            stderr
        )));
    }

    let ext = extension_for(job.to)?;
    let files = list_with_extension(&outdir, ext)?;
    let produced = attribute(&admitted, &files);

    // Rule 2: an input with no output is a failure even at rc=0 (§6.2 N3/N5).
    if let Some(empty) = produced.iter().find(|p| p.outputs.is_empty()) {
        let listing = std::fs::read_dir(&outdir)
            .map(|entries| {
                let mut names: Vec<String> = entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect();
                names.sort();
                names
            })
            .unwrap_or_default();
        return Err(DocError::NoOutput {
            outdir: outdir.clone(),
            expected: ext,
            listing: {
                let mut names = listing;
                names.insert(
                    0,
                    format!(
                        "no output for {}",
                        empty
                            .input
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    ),
                );
                names
            },
        });
    }

    // Rule 3: every published file must pass its structural validator.
    for item in &produced {
        for file in &item.outputs {
            match verify::document_output(file, job.to) {
                Verdict::Pass => {}
                Verdict::Fail(reason) => {
                    // Never leave an invalid file behind for a later job to find.
                    let _ = std::fs::remove_file(file);
                    return Err(DocError::Verify(reason));
                }
            }
        }
    }

    Ok(BatchReport {
        produced,
        steps: vec![outcome],
        duration_ms: started.elapsed().as_millis(),
        route,
    })
}

/// Single-file conversion: run the batch of one and publish the result at
/// `output`.
///
/// The engine writes `<stem>.<ext>` or `<stem>-<sheet>.<ext>`; the job is the
/// one that decides the final name, so exactly one produced file is moved onto
/// `output`. More than one produced file is reported, never guessed at.
pub fn convert(
    paths: &EnginePaths,
    job: &DocJob,
    input: &Path,
    output: &Path,
    temp_dir: &Path,
) -> Result<BatchReport, DocError> {
    let job_dir = temp_dir.join("doc");
    let outdir = job_dir.join("out");
    let report = convert_batch(paths, job, &[input.to_path_buf()], &outdir, &job_dir)?;

    let mut files: Vec<PathBuf> = report
        .produced
        .iter()
        .flat_map(|p| p.outputs.iter().cloned())
        .collect();
    if files.is_empty() {
        return Err(DocError::NoOutput {
            outdir,
            expected: extension_for(job.to)?,
            listing: Vec::new(),
        });
    }
    if files.len() > 1 {
        return Err(DocError::MultiOutput(files));
    }
    let produced = files.pop().expect("checked len == 1");

    // Publish: rename inside the same volume, copy+remove across volumes.
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| DocError::Io(e.to_string()))?;
    }
    if output.exists() {
        std::fs::remove_file(output).map_err(|e| DocError::Io(e.to_string()))?;
    }
    if std::fs::rename(&produced, output).is_err() {
        std::fs::copy(&produced, output).map_err(|e| DocError::Io(e.to_string()))?;
        let _ = std::fs::remove_file(&produced);
    }

    let mut report = report;
    report.produced = vec![Produced {
        input: input.to_path_buf(),
        outputs: vec![output.to_path_buf()],
    }];
    Ok(report)
}

/// Scoped CSV/text legality check used by the job builder: a document job may
/// only be created for a canonical type the document engine handles.
pub fn accepts(job: &DocJob) -> bool {
    soffice::is_document(job.from) && soffice::route(job.from, job.to).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn attribution_prefers_the_longest_matching_stem() {
        let ins = inputs(&["/d/a.docx", "/d/a-b.docx"]);
        let outs = inputs(&["/d/a-b.pdf", "/d/a.pdf"]);
        let produced = attribute(&ins, &outs);
        assert_eq!(produced[0].outputs, vec![PathBuf::from("/d/a.pdf")]);
        assert_eq!(produced[1].outputs, vec![PathBuf::from("/d/a-b.pdf")]);
    }

    #[test]
    fn the_measured_sheet_suffix_is_attributed_to_its_input() {
        let ins = inputs(&["/d/sample.xlsx"]);
        let outs = inputs(&["/d/sample-Data.csv"]);
        let produced = attribute(&ins, &outs);
        assert_eq!(produced[0].outputs, vec![PathBuf::from("/d/sample-Data.csv")]);
    }

    #[test]
    fn a_longer_stem_is_not_attributed_to_a_shorter_one() {
        // `sample2.csv` is not `sample`'s output; the separator is required.
        let ins = inputs(&["/d/sample.docx"]);
        let outs = inputs(&["/d/sample2.pdf"]);
        let produced = attribute(&ins, &outs);
        assert!(produced[0].outputs.is_empty());
        assert!(stem_belongs("sample-Data", "sample"));
        assert!(!stem_belongs("sample2", "sample"));
        assert!(stem_belongs("SAMPLE-Data", "sample"), "Windows is case-insensitive");
    }

    #[test]
    fn an_unrouted_pair_is_refused_before_any_spawn() {
        let paths = EnginePaths::from_root(Path::new("/definitely/not/here"));
        let err = convert_batch(
            &paths,
            &DocJob {
                from: CanonicalType::Rtf,
                to: TargetFormat::Pdf,
            },
            &inputs(&["/d/x.rtf"]),
            Path::new("/tmp/ezc-doc-out"),
            Path::new("/tmp"),
        )
        .unwrap_err();
        assert!(matches!(err, DocError::NotRouted { .. }), "got {err:?}");
    }

    #[test]
    fn a_missing_engine_is_named_not_silently_skipped() {
        let paths = EnginePaths::from_root(Path::new("/definitely/not/here"));
        let err = convert_batch(
            &paths,
            &DocJob {
                from: CanonicalType::Docx,
                to: TargetFormat::Pdf,
            },
            &inputs(&["/d/x.docx"]),
            Path::new("/tmp/ezc-doc-out2"),
            Path::new("/tmp"),
        )
        .unwrap_err();
        // Input admission runs first and reports the missing file, which is the
        // honest answer; either way no engine is spawned.
        assert!(
            matches!(err, DocError::MissingEngine(_) | DocError::Io(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn document_jobs_only_exist_for_document_types() {
        assert!(accepts(&DocJob {
            from: CanonicalType::Docx,
            to: TargetFormat::Pdf
        }));
        assert!(!accepts(&DocJob {
            from: CanonicalType::Jpeg,
            to: TargetFormat::Png
        }));
        assert!(!accepts(&DocJob {
            from: CanonicalType::Docx,
            to: TargetFormat::Txt
        }));
    }

    #[test]
    fn every_failure_has_a_user_message_from_the_table() {
        let cases = [
            DocError::NotRouted {
                from: CanonicalType::Rtf,
                to: TargetFormat::Pdf,
            },
            DocError::NoOutput {
                outdir: PathBuf::from("/tmp/o"),
                expected: "pdf",
                listing: vec![],
            },
            DocError::Verify("not a pdf".into()),
            DocError::MissingEngine("LibreOffice"),
        ];
        for case in cases {
            let message = case.user_message();
            assert!(
                message.ends_with('.') || message.ends_with('!'),
                "{message}"
            );
            assert!(!message.contains("rc="), "no raw exit codes in user text: {message}");
        }
    }

    // -----------------------------------------------------------------
    // Engine stubs: the two acceptance criteria that cannot be proven with
    // the real engine, because the real engine *succeeds* in both traps.
    // -----------------------------------------------------------------

    #[cfg(unix)]
    fn stub_engine(dir: &Path, body: &str) -> EnginePaths {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join("soffice");
        let mut file = std::fs::File::create(&path).unwrap();
        write!(file, "#!/bin/sh\n{body}\n").unwrap();
        drop(file);
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();

        let mut paths = EnginePaths::from_root(Path::new("/nonexistent-bundle"));
        paths.set_soffice_dir(dir);
        paths
    }

    #[cfg(unix)]
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ezc-doc-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A corrupt docx must never reach the engine: detection says text, the
    /// matrix offers no document target, and the stub proves nothing was
    /// spawned (it writes a marker when invoked).
    #[cfg(unix)]
    #[test]
    fn a_text_file_renamed_docx_never_reaches_soffice() {
        use crate::model::{detect, matrix};
        let fix = crate::model::test_support::fixture("text_renamed.docx");
        let detected = detect::detect_path(&fix);
        assert_eq!(detected.ty, CanonicalType::Txt);

        // The matrix must not offer any document target for it.
        let offered = matrix::offered_targets(detected.ty);
        assert!(
            !offered.contains(&TargetFormat::Pdf),
            "a text file named .docx must not be offered pdf"
        );

        let work = scratch("trap");
        let marker = work.join("spawned.log");
        let paths = stub_engine(
            &work.join("engine"),
            &format!("echo spawned >> '{}'", marker.display()),
        );
        let err = convert(
            &paths,
            &DocJob {
                from: detected.ty,
                to: TargetFormat::Pdf,
            },
            &fix,
            &work.join("out.pdf"),
            &work.join("tmp"),
        )
        .unwrap_err();
        assert!(matches!(err, DocError::NotRouted { .. }), "got {err:?}");
        assert!(err.user_message().contains("no path from"), "{}", err.user_message());
        assert!(
            !marker.exists(),
            "soffice was spawned for an input detection refuses"
        );
        let _ = std::fs::remove_dir_all(&work);
    }

    /// The measured `rc=0 / no file` no-op (§6.2 N5) is a failure, not a skip.
    #[cfg(unix)]
    #[test]
    fn exit_zero_with_no_file_is_reported_as_no_output() {
        let work = scratch("noop");
        let input = work.join("sample.docx");
        std::fs::copy(crate::model::test_support::fixture("sample.docx"), &input).unwrap();
        let paths = stub_engine(&work.join("engine"), "exit 0");
        let err = convert(
            &paths,
            &DocJob {
                from: CanonicalType::Docx,
                to: TargetFormat::Pdf,
            },
            &input,
            &work.join("out.pdf"),
            &work.join("tmp"),
        )
        .unwrap_err();
        match &err {
            DocError::NoOutput { expected, .. } => assert_eq!(*expected, "pdf"),
            other => panic!("expected NoOutput, got {other:?}"),
        }
        assert!(!work.join("out.pdf").exists(), "nothing may be published");
        assert!(err.user_message().contains("wrote no file"), "{}", err.user_message());
        let _ = std::fs::remove_dir_all(&work);
    }

    /// `rc=0` plus a file that is not what it claims is `OutputInvalid`.
    #[cfg(unix)]
    #[test]
    fn exit_zero_with_a_garbage_file_is_output_invalid() {
        let work = scratch("garbage");
        let input = work.join("sample.docx");
        std::fs::copy(crate::model::test_support::fixture("sample.docx"), &input).unwrap();
        // The measured trap shape: a file that exists, is non-empty, and is not
        // a PDF at all.
        let paths = stub_engine(
            &work.join("engine"),
            "out=\"\"\nprev=\"\"\nfor a in \"$@\"; do\n  [ \"$prev\" = \"--outdir\" ] && out=\"$a\"\n  prev=\"$a\"\ndone\nprintf 'this is not a docx at all' > \"$out/sample.pdf\"\nexit 0",
        );
        let err = convert(
            &paths,
            &DocJob {
                from: CanonicalType::Docx,
                to: TargetFormat::Pdf,
            },
            &input,
            &work.join("out.pdf"),
            &work.join("tmp"),
        )
        .unwrap_err();
        match &err {
            DocError::Verify(reason) => assert!(reason.contains("%PDF-"), "{reason}"),
            other => panic!("expected Verify, got {other:?}"),
        }
        assert!(!work.join("out.pdf").exists(), "an invalid result may not be published");
        assert!(
            err.user_message().contains("didn't pass a check"),
            "{}",
            err.user_message()
        );
        let _ = std::fs::remove_dir_all(&work);
    }

    /// A batch of three is one spawn and three attributed outputs.
    #[cfg(unix)]
    #[test]
    fn a_batch_of_three_is_one_spawn_with_three_outputs() {
        let work = scratch("batch");
        let mut ins = Vec::new();
        for name in ["one.docx", "two.docx", "three.docx"] {
            let path = work.join(name);
            std::fs::copy(crate::model::test_support::fixture("sample.docx"), &path).unwrap();
            ins.push(path);
        }
        let counter = work.join("spawns.log");
        let paths = stub_engine(
            &work.join("engine"),
            &format!(
                "echo run >> '{}'\nout=\"\"\nprev=\"\"\nfor a in \"$@\"; do\n  [ \"$prev\" = \"--outdir\" ] && out=\"$a\"\n  prev=\"$a\"\ndone\nprintf '%%PDF-1.7\\n1 0 obj\\n<< /Type /Page >>\\nendobj\\n' > \"$out/one.pdf\"\nprintf '%%PDF-1.7\\n1 0 obj\\n<< /Type /Page >>\\nendobj\\n' > \"$out/two.pdf\"\nprintf '%%PDF-1.7\\n1 0 obj\\n<< /Type /Page >>\\nendobj\\n' > \"$out/three.pdf\"\nexit 0",
                counter.display()
            ),
        );
        let report = convert_batch(
            &paths,
            &DocJob {
                from: CanonicalType::Docx,
                to: TargetFormat::Pdf,
            },
            &ins,
            &work.join("out"),
            &work.join("tmp"),
        )
        .expect("batch should succeed");

        let spawns = std::fs::read_to_string(&counter).unwrap();
        assert_eq!(spawns.lines().count(), 1, "three files must be one spawn");
        assert_eq!(report.produced.len(), 3);
        assert_eq!(report.total_outputs(), 3);
        for item in &report.produced {
            assert_eq!(item.outputs.len(), 1, "{item:?}");
        }
        let _ = std::fs::remove_dir_all(&work);
    }
}
