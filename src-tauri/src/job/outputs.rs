//! The output-rules engine: plan → convert → publish → journal (`SPEC.md` §5).
//!
//! This module is the one place the §5 rules meet real files. It is deliberately
//! split so the rules stay testable without an engine:
//!
//! - [`plan`] turns (inputs, target, settings, per-item strip flags) into a
//!   [`JobPlan`] — pure, no disk writes, no child processes. It is the same
//!   planning the window's preview uses (`model::naming::plan_batch`), so the
//!   name shown is the name written.
//! - [`run`] executes a plan: create the directories, write the `pending`
//!   journal, call the converter for each file, publish each result without
//!   overwriting, then rewrite the journal as `committed`. The converter is a
//!   closure, so the whole flow can be proven with a fake engine, and the real
//!   engine path (ffmpeg / vips / soffice) is the same code.
//! - [`undo`] applies the journal's verified delete.
//!
//! What "metadata keep/strip" means here: the toggle is *resolved* per file
//! (`settings.metadata.strip` as the default, a per-row override when the window
//! sets one) and handed to the converter as a plain `bool`. Keeping metadata is
//! the default (`SPEC.md` §5); the engine layer decides how to honour it
//! (`engine::plan::Options::strip_metadata`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::hash;
use super::journal::{
    now_unix_ms, DraftJob, FileRecord, Journal, JournalError, JournalStore, OutputRecord,
    UndoError, UndoReport,
};
use super::publish::{self, PublishError, Published, TempArea};
use crate::model::naming::{self, PlannedName};
use crate::model::settings::Settings;

/// One file to convert, as the job sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobItem {
    pub input: PathBuf,
    /// Per-file strip override. `None` means "use `settings.metadata.strip`".
    pub strip: Option<bool>,
}

impl JobItem {
    pub fn new(input: impl Into<PathBuf>) -> Self {
        Self {
            input: input.into(),
            strip: None,
        }
    }
}

/// What `run` will do, before it does anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobPlan {
    pub target: String,
    pub items: Vec<PlannedItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedItem {
    pub input: PathBuf,
    pub out_dir: PathBuf,
    pub out_path: PathBuf,
    pub renamed: bool,
    /// Resolved `SPEC.md` §5 strip toggle for this file.
    pub strip: bool,
}

/// Resolve the §5 rules for a batch. Pure: reads settings and (for collisions)
/// the destination directory listing, writes nothing.
pub fn plan(items: &[JobItem], target: &str, settings: &Settings) -> JobPlan {
    let inputs: Vec<PathBuf> = items.iter().map(|i| i.input.clone()).collect();
    let planned: Vec<PlannedName> = naming::plan_batch(&inputs, target, settings, items.len());

    let items = planned
        .into_iter()
        .zip(items)
        .map(|(name, item)| PlannedItem {
            strip: item.strip.unwrap_or(settings.metadata.strip),
            input: name.input,
            out_dir: name.out_dir,
            out_path: name.out_path,
            renamed: name.renamed,
        })
        .collect();

    JobPlan {
        target: target.trim_start_matches('.').to_string(),
        items,
    }
}

/// One file that could not be converted or published. The batch continues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemFailure {
    pub input: PathBuf,
    pub reason: String,
}

/// What a finished run produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobOutcome {
    pub job_id: String,
    /// Files now in the user's folder, in queue order.
    pub published: Vec<Published>,
    pub failures: Vec<ItemFailure>,
    /// `None` when nothing was published (no journal is kept for a job with
    /// nothing to undo).
    pub journal: Option<PathBuf>,
    pub created_dirs: Vec<PathBuf>,
    /// Unix ms after which Undo is refused.
    pub undo_deadline_unix_ms: u64,
}

impl JobOutcome {
    pub fn all_failed(&self) -> bool {
        self.published.is_empty() && !self.failures.is_empty()
    }
}

/// A run could not even begin (the journal could not be written). Per-file
/// problems are [`JobOutcome::failures`], not this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputError {
    Journal(JournalError),
    Io(String),
}

impl std::fmt::Display for OutputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputError::Journal(e) => write!(f, "{e}"),
            OutputError::Io(reason) => write!(f, "{reason}"),
        }
    }
}

impl From<JournalError> for OutputError {
    fn from(e: JournalError) -> Self {
        OutputError::Journal(e)
    }
}

/// Execute a plan.
///
/// `convert(input, temp_output, strip) -> Result<(), String>` is the engine
/// call. It must write a complete file at `temp_output`; this function never
/// lets the user's folder see a partial file (the engine writes into the job
/// temp area and the result is published atomically).
///
/// `engine` and `route` are recorded in the journal for the support log.
#[allow(clippy::too_many_arguments)]
pub fn run<F>(
    job_id: &str,
    plan: &JobPlan,
    journals: &JournalStore,
    temp_area: &TempArea,
    engine: &str,
    route: &str,
    mut convert: F,
) -> Result<JobOutcome, OutputError>
where
    F: FnMut(&Path, &Path, bool) -> Result<(), String>,
{
    let window_seconds = journals.window_seconds();
    let job_temp = temp_area
        .prepare(job_id)
        .map_err(|e: PublishError| OutputError::Io(e.to_string()))?;

    // Inputs are hashed up front: undo uses them to refuse deleting a file the
    // job read (`ARCHITECTURE.md` §13.3).
    let mut inputs = Vec::with_capacity(plan.items.len());
    for item in &plan.items {
        let record = match fs_stat(&item.input) {
            Ok(record) => record,
            Err(reason) => {
                // A missing input is reported as a per-file failure below; the
                // journal just cannot use its hash as a delete guard.
                eprintln!("ez-convert: cannot hash an input for the undo journal: {reason}");
                FileRecord {
                    path: item.input.to_string_lossy().to_string(),
                    sha256: String::new(),
                    size: 0,
                }
            }
        };
        inputs.push(record);
    }

    // Directories first: the journal records exactly what was created so undo
    // can remove an empty `converted/` again.
    let mut created_dirs: Vec<PathBuf> = Vec::new();
    let mut intended: Vec<String> = Vec::new();
    for item in &plan.items {
        intended.push(item.out_path.to_string_lossy().to_string());
        match publish::ensure_dir(&item.out_dir) {
            Ok(created) => {
                for dir in created {
                    if !created_dirs.contains(&dir) {
                        created_dirs.push(dir);
                    }
                }
            }
            Err(e) => {
                let _ = std::fs::remove_dir_all(&job_temp);
                return Err(OutputError::Io(format!(
                    "could not create {}: {e}",
                    item.out_dir.display()
                )));
            }
        }
    }

    let pending = journals.begin(DraftJob {
        job_id: job_id.to_string(),
        engine: engine.to_string(),
        route: route.to_string(),
        inputs,
        intended,
    })?;

    let mut reserved: BTreeSet<PathBuf> = BTreeSet::new();
    let mut published: Vec<Published> = Vec::new();
    let mut failures: Vec<ItemFailure> = Vec::new();

    for (index, item) in plan.items.iter().enumerate() {
        if !item.input.is_file() {
            failures.push(ItemFailure {
                input: item.input.clone(),
                reason: "the file is gone or unreadable".into(),
            });
            continue;
        }
        let temp_output = job_temp.join(format!(
            "{index}-{}.{}",
            publish::sanitise_component(&naming::output_stem(&item.input)),
            plan.target
        ));
        match convert(&item.input, &temp_output, item.strip) {
            Ok(()) => match publish::publish_new(
                &temp_output,
                &item.out_dir,
                &naming::output_stem(&item.input),
                &plan.target,
                &mut reserved,
            ) {
                Ok(file) => published.push(file),
                Err(e) => {
                    let _ = std::fs::remove_file(&temp_output);
                    failures.push(ItemFailure {
                        input: item.input.clone(),
                        reason: e.to_string(),
                    });
                }
            },
            Err(reason) => {
                let _ = std::fs::remove_file(&temp_output);
                failures.push(ItemFailure {
                    input: item.input.clone(),
                    reason,
                });
            }
        }
    }

    temp_area.cleanup(job_id);

    if published.is_empty() {
        // Nothing to undo: retire the journal rather than arming a delete that
        // would have no verified output.
        journals.abandon(&pending)?;
        return Ok(JobOutcome {
            job_id: job_id.to_string(),
            published,
            failures,
            journal: None,
            created_dirs,
            undo_deadline_unix_ms: 0,
        });
    }

    let outputs: Vec<OutputRecord> = published.iter().map(record_of).collect();
    let journal = journals.commit(
        &pending,
        outputs,
        created_dirs
            .iter()
            .map(|d| d.to_string_lossy().to_string())
            .collect(),
        failures.len() as u32,
    )?;

    Ok(JobOutcome {
        job_id: job_id.to_string(),
        published,
        failures,
        journal: Some(pending.path.clone()),
        created_dirs,
        undo_deadline_unix_ms: journal.deadline_unix_ms(window_seconds),
    })
}

/// Convert one published file into the journal's record.
fn record_of(file: &Published) -> OutputRecord {
    let parent = file.path.parent().unwrap_or_else(|| Path::new("."));
    OutputRecord {
        path: file.path.to_string_lossy().to_string(),
        dir_canon: std::fs::canonicalize(parent)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        sha256: file.sha256.clone(),
        size: file.size,
        mtime_unix_ms: file.mtime_unix_ms,
    }
}

fn fs_stat(path: &Path) -> Result<FileRecord, String> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .len();
    let sha256 = hash::sha256_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(FileRecord {
        path: path.to_string_lossy().to_string(),
        sha256,
        size,
    })
}

/// Undo the newest committed job.
///
/// Deliberately *not* filtered to open windows first: if the user clicks Undo a
/// moment too late, the honest answer is "the window has passed, the files were
/// kept" ([`UndoError::Expired`]), not "nothing to undo".
pub fn undo_latest(journals: &JournalStore, now_unix_ms: u64) -> Result<UndoReport, UndoError> {
    match journals
        .latest_committed()
        .map_err(|e| UndoError::Io(e.to_string()))?
    {
        Some((path, _)) => journals.undo(&path, now_unix_ms),
        None => Err(UndoError::NotFound("the last conversion".into())),
    }
}

/// Undo a named job.
pub fn undo_job(
    journals: &JournalStore,
    job_id: &str,
    now_unix_ms: u64,
) -> Result<UndoReport, UndoError> {
    match journals
        .find(job_id)
        .map_err(|e| UndoError::Io(e.to_string()))?
    {
        Some((path, _)) => journals.undo(&path, now_unix_ms),
        None => Err(UndoError::NotFound(job_id.to_string())),
    }
}

/// Startup housekeeping (`ARCHITECTURE.md` §13.5): retire stale journals, never
/// delete an output.
pub fn startup_sweep(journals: &JournalStore) -> Result<super::journal::SweepReport, OutputError> {
    Ok(journals.sweep(now_unix_ms())?)
}

/// Build the journal record the UI and the support log can show.
pub fn describe(journal: &Journal) -> String {
    format!(
        "job {} [{}] {} output(s) via {} ({})",
        journal.job_id,
        journal.state.as_str(),
        journal.outputs.len(),
        journal.engine,
        journal.route
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir()
                .join(format!("ezc-outputs-{tag}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn file(&self, rel: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, bytes).unwrap();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A fake engine: writes the input's bytes backwards, so the output is
    /// recognisably the engine's and not the source.
    fn fake_engine(input: &Path, output: &Path, strip: bool) -> Result<(), String> {
        let mut bytes = fs::read(input).map_err(|e| e.to_string())?;
        if strip {
            bytes.extend_from_slice(b"|stripped");
        }
        bytes.reverse();
        fs::write(output, bytes).map_err(|e| e.to_string())
    }

    fn harness(dir: &TempDir) -> (JournalStore, TempArea, Settings) {
        (
            JournalStore::new(dir.path().join("journal"), 10),
            TempArea::new(dir.path().join("jobs")),
            Settings::default(),
        )
    }

    #[test]
    fn a_single_file_lands_next_to_its_source() {
        let dir = TempDir::new("single");
        let (journals, temp, settings) = harness(&dir);
        let input = dir.file("doc/report.docx", b"docx bytes");

        let plan = plan(&[JobItem::new(&input)], "pdf", &settings);
        assert_eq!(plan.items[0].out_dir, dir.path().join("doc"));
        assert_eq!(plan.items[0].out_path, dir.path().join("doc/report.pdf"));
        assert!(!plan.items[0].renamed);

        let outcome = run(
            "j_1",
            &plan,
            &journals,
            &temp,
            "fake",
            "docx2pdf",
            fake_engine,
        )
        .unwrap();
        assert_eq!(outcome.published.len(), 1);
        assert!(dir.path().join("doc/report.pdf").is_file());
        assert!(outcome.journal.is_some());
    }

    #[test]
    fn three_files_go_to_the_batch_folder_and_the_toggle_moves_them_back() {
        let dir = TempDir::new("batch");
        let (journals, temp, mut settings) = harness(&dir);
        let inputs: Vec<PathBuf> = (0..3)
            .map(|i| dir.file(&format!("photos/p{i}.png"), format!("img{i}").as_bytes()))
            .collect();
        let items: Vec<JobItem> = inputs.iter().map(JobItem::new).collect();

        // Default (`SPEC.md` §5): a batch of 3+ gets `converted/`.
        let plan_batch_on = plan(&items, "jpg", &settings);
        assert!(plan_batch_on
            .items
            .iter()
            .all(|p| p.out_dir == dir.path().join("photos/converted")));
        let outcome = run(
            "j_2",
            &plan_batch_on,
            &journals,
            &temp,
            "fake",
            "png2jpg",
            fake_engine,
        )
        .unwrap();
        assert_eq!(outcome.published.len(), 3);
        for item in &plan_batch_on.items {
            assert!(item.out_path.is_file());
        }

        // Toggle off: same folder.
        settings.output.use_batch_subfolder = false;
        let plan_off = plan(&items, "webp", &settings);
        assert!(plan_off
            .items
            .iter()
            .all(|p| p.out_dir == dir.path().join("photos")));
        let outcome = run(
            "j_3",
            &plan_off,
            &journals,
            &temp,
            "fake",
            "png2webp",
            fake_engine,
        )
        .unwrap();
        assert_eq!(outcome.published.len(), 3);
        assert!(dir.path().join("photos/p0.webp").is_file());
    }

    #[test]
    fn two_files_stay_in_the_same_folder_even_with_the_batch_rule_on() {
        let dir = TempDir::new("two");
        let (_, _, settings) = harness(&dir);
        let inputs = vec![
            JobItem::new(dir.file("a/one.png", b"1")),
            JobItem::new(dir.file("a/two.png", b"2")),
        ];
        let plan = plan(&inputs, "jpg", &settings);
        assert!(plan.items.iter().all(|p| p.out_dir == dir.path().join("a")));
    }

    #[test]
    fn the_chosen_folder_toggle_redirects_every_output() {
        let dir = TempDir::new("chosen");
        let (journals, temp, mut settings) = harness(&dir);
        let chosen = dir.path().join("elsewhere/out");
        settings.output.location = "chosen_folder".into();
        settings.output.chosen_folder = Some(chosen.to_string_lossy().into_owned());

        let inputs: Vec<JobItem> = (0..3)
            .map(|i| JobItem::new(dir.file(&format!("src/s{i}.png"), b"x")))
            .collect();
        let plan = plan(&inputs, "jpg", &settings);
        assert!(plan.items.iter().all(|p| p.out_dir == chosen));
        let outcome = run(
            "j_4",
            &plan,
            &journals,
            &temp,
            "fake",
            "png2jpg",
            fake_engine,
        )
        .unwrap();
        assert_eq!(outcome.published.len(), 3);
        assert!(chosen.join("s0.jpg").is_file());
        // Neither the chosen folder nor its missing parent existed, and both
        // were created by this job — so undo must take them away again.
        assert_eq!(outcome.created_dirs.last(), Some(&chosen));
        assert_eq!(outcome.created_dirs.len(), 2);
        journals
            .undo(&outcome.journal.clone().unwrap(), now_unix_ms())
            .unwrap();
        assert!(
            !chosen.exists(),
            "an empty folder this job created is removed"
        );
        assert!(!dir.path().join("elsewhere").exists());
    }

    #[test]
    fn an_existing_output_is_suffixed_and_the_original_is_untouched() {
        let dir = TempDir::new("suffix");
        let (journals, temp, mut settings) = harness(&dir);
        settings.output.use_batch_subfolder = false;
        let input = dir.file("docs/report.docx", b"source");
        let existing = dir.file("docs/report.pdf", b"THE USER'S PDF");

        let plan = plan(&[JobItem::new(&input)], "pdf", &settings);
        assert_eq!(plan.items[0].out_path, dir.path().join("docs/report-1.pdf"));
        assert!(plan.items[0].renamed);

        let outcome = run(
            "j_5",
            &plan,
            &journals,
            &temp,
            "fake",
            "docx2pdf",
            fake_engine,
        )
        .unwrap();
        assert_eq!(
            outcome.published[0].path,
            dir.path().join("docs/report-1.pdf")
        );
        assert_eq!(fs::read(&existing).unwrap(), b"THE USER'S PDF");
    }

    #[test]
    fn the_metadata_toggle_reaches_the_engine_for_every_file() {
        let dir = TempDir::new("strip");
        let (journals, temp, mut settings) = harness(&dir);
        settings.output.use_batch_subfolder = false;

        // Keep by default (`SPEC.md` §5).
        assert!(!settings.metadata.strip);
        let inputs = vec![
            JobItem::new(dir.file("m/a.mp3", b"aaa")),
            JobItem::new(dir.file("m/b.mp3", b"bbb")),
        ];
        let plan_keep = plan(&inputs, "wav", &settings);
        assert!(plan_keep.items.iter().all(|p| !p.strip));
        let outcome = run(
            "j_6",
            &plan_keep,
            &journals,
            &temp,
            "fake",
            "mp32wav",
            fake_engine,
        )
        .unwrap();
        assert_eq!(outcome.published.len(), 2);
        let kept = fs::read(dir.path().join("m/a.wav")).unwrap();
        assert!(
            !kept.contains(&b'|'),
            "the default must not strip: {kept:?}"
        );

        // Setting on: every file is stripped.
        settings.metadata.strip = true;
        let plan_strip = plan(&inputs, "wav", &settings);
        assert!(plan_strip.items.iter().all(|p| p.strip));
        let outcome = run(
            "j_7",
            &plan_strip,
            &journals,
            &temp,
            "fake",
            "mp32wav",
            fake_engine,
        )
        .unwrap();
        assert_eq!(outcome.published.len(), 2);
        let stripped = fs::read(dir.path().join("m/a-1.wav")).unwrap();
        assert!(
            stripped.contains(&b'|'),
            "the toggle must reach the engine: {stripped:?}"
        );

        // A per-row override beats the setting in both directions.
        let plan_override = plan(
            &[
                JobItem {
                    input: dir.path().join("m/a.mp3"),
                    strip: Some(false),
                },
                JobItem {
                    input: dir.path().join("m/b.mp3"),
                    strip: Some(true),
                },
            ],
            "flac",
            &settings,
        );
        assert!(!plan_override.items[0].strip);
        assert!(plan_override.items[1].strip);
    }

    #[test]
    fn a_failed_conversion_publishes_nothing_and_leaves_no_journal() {
        let dir = TempDir::new("failed");
        let (journals, temp, settings) = harness(&dir);
        let input = dir.file("f/a.png", b"x");
        let plan = plan(&[JobItem::new(&input)], "jpg", &settings);

        let outcome = run(
            "j_8",
            &plan,
            &journals,
            &temp,
            "fake",
            "png2jpg",
            |_i: &Path, _o: &Path, _s: bool| Err("engine said no".to_string()),
        )
        .unwrap();
        assert!(outcome.published.is_empty());
        assert_eq!(outcome.failures.len(), 1);
        assert_eq!(outcome.failures[0].reason, "engine said no");
        assert!(
            outcome.journal.is_none(),
            "nothing published, nothing to undo"
        );
        assert!(!dir.path().join("f/a.jpg").exists());
        assert_eq!(fs::read(&input).unwrap(), b"x", "the input is untouched");
        // The temp area is cleaned up and the journal directory is empty.
        assert!(!temp.job_dir("j_8").exists());
        assert!(journals.list().unwrap().is_empty());
    }

    #[test]
    fn an_engine_that_writes_nothing_is_a_failure_not_a_silent_success() {
        let dir = TempDir::new("silent");
        let (journals, temp, settings) = harness(&dir);
        let input = dir.file("s/a.png", b"x");
        let plan = plan(&[JobItem::new(&input)], "jpg", &settings);

        let outcome = run(
            "j_9",
            &plan,
            &journals,
            &temp,
            "fake",
            "png2jpg",
            |_i: &Path, output: &Path, _s: bool| {
                fs::write(output, b"").unwrap();
                Ok(())
            },
        )
        .unwrap();
        assert!(outcome.published.is_empty());
        assert_eq!(outcome.failures.len(), 1);
        assert!(
            outcome.failures[0].reason.contains("no data"),
            "expected an explicit empty-output failure, got {:?}",
            outcome.failures[0].reason
        );
        assert!(!dir.path().join("s/a.jpg").exists());
    }

    #[test]
    fn undo_removes_only_the_files_this_job_published() {
        let dir = TempDir::new("ownership");
        let (journals, temp, settings) = harness(&dir);
        let outdir = dir.path().join("work");
        let first = dir.file("work/photo.png", b"one");
        let decoy = dir.file("work/keep.txt", b"the user's file");

        let plan_a = plan(&[JobItem::new(&first)], "jpg", &settings);
        let job_a = run(
            "j_10",
            &plan_a,
            &journals,
            &temp,
            "fake",
            "png2jpg",
            fake_engine,
        )
        .unwrap();
        assert!(outdir.join("photo.jpg").is_file());

        // A second job into the same folder, and a file the user placed there
        // between the two runs.
        let other = dir.file("work/other.png", b"two");
        let user_file = dir.file("work/user-note.jpg", b"not ours");
        let plan_b = plan(&[JobItem::new(&other)], "jpg", &settings);
        let job_b = run(
            "j_11",
            &plan_b,
            &journals,
            &temp,
            "fake",
            "png2jpg",
            fake_engine,
        )
        .unwrap();

        let report = undo_job(&journals, "j_11", now_unix_ms()).unwrap();
        assert_eq!(report.deleted, vec![outdir.join("other.jpg")]);
        assert!(!outdir.join("other.jpg").exists());
        assert!(outdir.join("photo.jpg").exists(), "job A's output survives");
        assert_eq!(fs::read(&user_file).unwrap(), b"not ours");
        assert_eq!(fs::read(&decoy).unwrap(), b"the user's file");
        assert!(job_a.journal.is_some() && job_b.journal.is_some());
    }

    #[test]
    fn undo_refuses_after_the_window_and_keeps_the_files() {
        let dir = TempDir::new("window");
        let (journals, temp, settings) = harness(&dir);
        let input = dir.file("w/a.png", b"x");
        let plan_first = plan(&[JobItem::new(&input)], "jpg", &settings);
        let outcome = run(
            "j_12",
            &plan_first,
            &journals,
            &temp,
            "fake",
            "png2jpg",
            fake_engine,
        )
        .unwrap();

        let late = outcome.undo_deadline_unix_ms + 1;
        let err = undo_latest(&journals, late).unwrap_err();
        assert!(matches!(err, UndoError::Expired { .. }), "got {err:?}");
        assert!(
            dir.path().join("w/a.jpg").is_file(),
            "expiry keeps the output"
        );

        // And a new run is undoable again.
        let input = dir.file("w/b.png", b"y");
        let plan_next = plan(&[JobItem::new(&input)], "jpg", &settings);
        let outcome = run(
            "j_13",
            &plan_next,
            &journals,
            &temp,
            "fake",
            "png2jpg",
            fake_engine,
        )
        .unwrap();
        let report = undo_latest(&journals, outcome.undo_deadline_unix_ms - 1).unwrap();
        assert_eq!(report.deleted, vec![dir.path().join("w/b.jpg")]);
    }

    #[test]
    fn undo_refuses_when_the_user_edited_an_output() {
        let dir = TempDir::new("edited");
        let (journals, temp, settings) = harness(&dir);
        let a = dir.file("e/a.png", b"aaa");
        let b = dir.file("e/b.png", b"bbb");
        let plan = plan(&[JobItem::new(&a), JobItem::new(&b)], "jpg", &settings);
        let outcome = run(
            "j_14",
            &plan,
            &journals,
            &temp,
            "fake",
            "png2jpg",
            fake_engine,
        )
        .unwrap();
        assert_eq!(outcome.published.len(), 2);

        fs::write(dir.path().join("e/a.jpg"), b"edited by the user").unwrap();
        let err = journals
            .undo(&outcome.journal.unwrap(), now_unix_ms())
            .unwrap_err();
        assert!(matches!(err, UndoError::Changed { .. }), "got {err:?}");
        assert!(dir.path().join("e/a.jpg").is_file());
        assert!(dir.path().join("e/b.jpg").is_file(), "nothing is deleted");
    }

    #[test]
    fn a_partially_failed_batch_still_publishes_and_still_undoes() {
        let dir = TempDir::new("partial");
        let (journals, temp, settings) = harness(&dir);
        let good = dir.file("p/good.png", b"good");
        let missing = dir.path().join("p/missing.png");
        let plan = plan(
            &[JobItem::new(&good), JobItem::new(&missing)],
            "jpg",
            &settings,
        );

        let outcome = run(
            "j_15",
            &plan,
            &journals,
            &temp,
            "fake",
            "png2jpg",
            fake_engine,
        )
        .unwrap();
        assert_eq!(outcome.published.len(), 1);
        assert_eq!(outcome.failures.len(), 1);
        assert!(outcome.failures[0].input.ends_with("missing.png"));
        let journal = journals.read(&outcome.journal.clone().unwrap()).unwrap();
        assert_eq!(
            journal.failures, 1,
            "the journal records the partial failure"
        );

        let report = journals
            .undo(&outcome.journal.unwrap(), now_unix_ms())
            .unwrap();
        assert_eq!(report.deleted, vec![dir.path().join("p/good.jpg")]);
    }

    #[test]
    fn a_batch_undo_takes_the_converted_folder_with_it() {
        let dir = TempDir::new("batchundo");
        let (journals, temp, settings) = harness(&dir);
        let inputs: Vec<JobItem> = (0..3)
            .map(|i| JobItem::new(dir.file(&format!("b/{i}.png"), b"x")))
            .collect();
        let plan = plan(&inputs, "jpg", &settings);
        let outcome = run(
            "j_16",
            &plan,
            &journals,
            &temp,
            "fake",
            "png2jpg",
            fake_engine,
        )
        .unwrap();
        let converted = dir.path().join("b/converted");
        assert!(converted.is_dir());
        assert_eq!(outcome.published.len(), 3);

        let report = journals
            .undo(&outcome.journal.unwrap(), now_unix_ms())
            .unwrap();
        assert_eq!(report.deleted.len(), 3);
        assert!(report.dirs_removed.contains(&converted));
        assert!(!converted.exists(), "the empty batch folder is gone");
        assert!(dir.path().join("b/0.png").is_file(), "inputs are untouched");
    }

    #[test]
    fn a_dropped_folder_is_enumerated_one_level_deep_unless_subfolders_are_included() {
        // `SPEC.md` §5 also covers folder drops; the enumeration lives in
        // `job::queue` and this is the end-to-end statement of the rule: the
        // files a folder drop admits are exactly the ones the output rules see.
        let dir = TempDir::new("drop");
        dir.file("drop/a.png", b"a");
        dir.file("drop/b.png", b"b");
        dir.file("drop/nested/deep.png", b"deep");

        let shallow = crate::job::queue::expand_folder(&dir.path().join("drop"), false);
        let names: Vec<String> = shallow
            .files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["a.png".to_string(), "b.png".to_string()]);

        let recursive = crate::job::queue::expand_folder(&dir.path().join("drop"), true);
        assert_eq!(recursive.files.len(), 3);

        // And the output rules handle the collision the recursive drop creates:
        // two files called `deep.png` would be suffixed, not overwritten.
        let settings = Settings::default();
        let plan = plan(
            &[
                JobItem::new(dir.path().join("drop/nested/deep.png")),
                JobItem::new(dir.path().join("drop/nested/deep.png")),
            ],
            "jpg",
            &settings,
        );
        assert_ne!(plan.items[0].out_path, plan.items[1].out_path);
        assert!(plan.items[1].renamed);
    }
}
