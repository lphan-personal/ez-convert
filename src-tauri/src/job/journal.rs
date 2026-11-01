//! The undo journal on disk (`ARCHITECTURE.md` §13, `SPEC.md` §5).
//!
//! The guarantee this module owns is narrow and load-bearing: **Undo deletes the
//! outputs this job created, and nothing else.** A path is not proof of
//! authorship — the user may have replaced the file, dropped another file at the
//! same name, or re-pointed a junction/symlink at the folder. So every delete is
//! gated on four checks, and if *any* output fails *any* check, **nothing** is
//! deleted:
//!
//! 1. the recorded path is absolute and is not one of the job's inputs;
//! 2. the file exists and is not itself a symlink;
//! 3. the file's parent directory canonicalises to the same directory recorded
//!    at publish time (a re-pointed folder fails here);
//! 4. the file's SHA-256 and size still match what this job published.
//!
//! The journal is written **before** the first publication (`pending`), rewritten
//! after it (`committed`), and removed after undo or expiry. A crash mid-flight
//! leaves a `pending` file, which the startup sweep marks `abandoned` and removes
//! — it never deletes a user's file, because a `pending` journal holds no
//! published hashes.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::hash;

/// On-disk journal schema. Bumped only when the shape changes incompatibly.
pub const JOURNAL_SCHEMA: u32 = 1;

/// `SPEC.md` §5: the Undo window.
pub const DEFAULT_WINDOW_SECONDS: u64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalState {
    /// Written before the first file was published. A crash here leaves no
    /// published output to undo.
    Pending,
    /// Outputs are published and hash-recorded; undo is armed until the window
    /// closes.
    Committed,
    /// Undo ran and the outputs were deleted.
    Undone,
    /// The window closed; the outputs are kept.
    Expired,
    /// A `pending` journal found at startup (the process died mid-conversion).
    Abandoned,
}

impl JournalState {
    pub fn as_str(&self) -> &'static str {
        match self {
            JournalState::Pending => "pending",
            JournalState::Committed => "committed",
            JournalState::Undone => "undone",
            JournalState::Expired => "expired",
            JournalState::Abandoned => "abandoned",
        }
    }
}

/// One file the job read, recorded so undo can refuse to delete it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRecord {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

/// One file the job published.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputRecord {
    pub path: String,
    /// Canonical form of the *parent directory* at publish time. Compared again
    /// at undo, so a re-pointed symlink/junction cannot redirect the delete.
    pub dir_canon: String,
    pub sha256: String,
    pub size: u64,
    /// Informational: a changed mtime is not by itself a reason to refuse (a
    /// backup tool touching the file must not block undo). The hash is the gate.
    pub mtime_unix_ms: u64,
}

/// One journal file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    pub schema: u32,
    pub job_id: String,
    pub created_at_unix_ms: u64,
    pub state: JournalState,
    pub inputs: Vec<FileRecord>,
    /// Paths the job intended to write. Recorded at `pending` so a crash leaves
    /// a diagnosable trail; never used as a delete list.
    #[serde(default)]
    pub intended: Vec<String>,
    pub outputs: Vec<OutputRecord>,
    /// Directories this job created (e.g. `converted/`), removed by undo only
    /// if empty afterwards.
    #[serde(default)]
    pub created_dirs: Vec<String>,
    pub engine: String,
    pub route: String,
    /// Per-file failures in a partially failed batch. Outputs that did publish
    /// are still undoable; this is what tells the UI the batch was partial.
    #[serde(default)]
    pub failures: u32,
}

impl Journal {
    /// When Undo stops being available.
    pub fn deadline_unix_ms(&self, window_seconds: u64) -> u64 {
        self.created_at_unix_ms + window_seconds.saturating_mul(1000)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalError {
    Io(String),
    Corrupt(String),
    NotFound(String),
}

impl std::fmt::Display for JournalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JournalError::Io(reason) => write!(f, "{reason}"),
            JournalError::Corrupt(what) => write!(f, "unreadable undo journal: {what}"),
            JournalError::NotFound(what) => write!(f, "no undo journal for {what}"),
        }
    }
}

impl From<std::io::Error> for JournalError {
    fn from(e: std::io::Error) -> Self {
        JournalError::Io(e.to_string())
    }
}

/// Why an undo was refused. Every arm means **nothing was deleted**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoError {
    NotFound(String),
    /// `SPEC.md` §5: a 10-second undo. After that the files are the user's.
    Expired {
        job_id: String,
        deadline_unix_ms: u64,
    },
    /// A `pending`/`abandoned` journal has no published hashes to verify.
    NotCommitted {
        job_id: String,
        state: JournalState,
    },
    /// Outputs exist but no longer hash to what this job wrote.
    Changed {
        job_id: String,
        files: Vec<String>,
    },
    /// An output is one of the job's own inputs, or is a symlink, or its folder
    /// is no longer the folder it was published into.
    Protected {
        job_id: String,
        files: Vec<String>,
    },
    Io(String),
}

impl std::fmt::Display for UndoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UndoError::NotFound(id) => write!(f, "nothing to undo for job {id}"),
            UndoError::Expired { job_id, .. } => write!(
                f,
                "the undo window for job {job_id} has passed; the files were kept"
            ),
            UndoError::NotCommitted { job_id, state } => write!(
                f,
                "job {job_id} never finished publishing ({}); there is nothing to delete",
                state.as_str()
            ),
            UndoError::Changed { files, .. } => write!(
                f,
                "some files changed since conversion — not touching them ({})",
                files.join(", ")
            ),
            UndoError::Protected { files, .. } => write!(
                f,
                "refusing to delete {} — it is an input or no longer the file this job wrote",
                files.join(", ")
            ),
            UndoError::Io(reason) => write!(f, "{reason}"),
        }
    }
}

/// What an undo actually did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoReport {
    pub job_id: String,
    pub deleted: Vec<PathBuf>,
    pub dirs_removed: Vec<PathBuf>,
}

/// What the startup sweep did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SweepReport {
    pub abandoned: Vec<String>,
    pub expired: Vec<String>,
    pub corrupt: Vec<PathBuf>,
    pub kept: usize,
}

/// A journal created by [`JournalStore::begin`], before anything is published.
#[derive(Debug, Clone)]
pub struct PendingJournal {
    pub path: PathBuf,
    pub journal: Journal,
}

/// Everything the caller knows before a conversion starts.
#[derive(Debug, Clone)]
pub struct DraftJob {
    pub job_id: String,
    pub engine: String,
    pub route: String,
    pub inputs: Vec<FileRecord>,
    pub intended: Vec<String>,
}

/// The journal directory plus the undo window.
#[derive(Debug, Clone)]
pub struct JournalStore {
    dir: PathBuf,
    window_seconds: u64,
}

impl JournalStore {
    pub fn new(dir: impl Into<PathBuf>, window_seconds: u64) -> Self {
        Self {
            dir: dir.into(),
            window_seconds: window_seconds.clamp(1, 3600),
        }
    }

    /// The production store: `<cache>/journal`, window from settings.
    pub fn from_settings(settings: &crate::model::settings::Settings) -> Self {
        Self::new(
            super::paths::journal_dir(),
            settings.undo.window_seconds as u64,
        )
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn window_seconds(&self) -> u64 {
        self.window_seconds
    }

    fn path_for(&self, job_id: &str, created_at_unix_ms: u64) -> PathBuf {
        self.dir.join(format!(
            "{}-{created_at_unix_ms}.json",
            super::publish::sanitise_component(job_id)
        ))
    }

    /// Write the `pending` journal. Called **before** the first publication.
    pub fn begin(&self, draft: DraftJob) -> Result<PendingJournal, JournalError> {
        fs::create_dir_all(&self.dir)?;
        let created_at_unix_ms = now_unix_ms();
        let journal = Journal {
            schema: JOURNAL_SCHEMA,
            job_id: draft.job_id,
            created_at_unix_ms,
            state: JournalState::Pending,
            inputs: draft.inputs,
            intended: draft.intended,
            outputs: Vec::new(),
            created_dirs: Vec::new(),
            engine: draft.engine,
            route: draft.route,
            failures: 0,
        };
        let path = self.path_for(&journal.job_id, created_at_unix_ms);
        write_atomic(&path, &journal)?;
        Ok(PendingJournal { path, journal })
    }

    /// Rewrite the journal as `committed` with the files that really exist.
    pub fn commit(
        &self,
        pending: &PendingJournal,
        outputs: Vec<OutputRecord>,
        created_dirs: Vec<String>,
        failures: u32,
    ) -> Result<Journal, JournalError> {
        let mut journal = pending.journal.clone();
        journal.state = JournalState::Committed;
        journal.outputs = outputs;
        journal.created_dirs = created_dirs;
        journal.failures = failures;
        write_atomic(&pending.path, &journal)?;
        Ok(journal)
    }

    /// Mark a job abandoned and forget its journal (no outputs to delete).
    pub fn abandon(&self, pending: &PendingJournal) -> Result<(), JournalError> {
        let mut journal = pending.journal.clone();
        journal.state = JournalState::Abandoned;
        write_atomic(&pending.path, &journal)?;
        fs::remove_file(&pending.path).or_else(ignore_missing)?;
        Ok(())
    }

    /// Read one journal file.
    pub fn read(&self, path: &Path) -> Result<Journal, JournalError> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(JournalError::NotFound(path.display().to_string()))
            }
            Err(e) => return Err(JournalError::Io(e.to_string())),
        };
        serde_json::from_str(&text)
            .map_err(|e| JournalError::Corrupt(format!("{}: {e}", path.display())))
    }

    /// Every journal file currently on disk, newest first.
    pub fn list(&self) -> Result<Vec<(PathBuf, Journal)>, JournalError> {
        if !self.dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut found = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            // An unreadable journal is the sweep's business, not this listing's.
            if let Ok(journal) = self.read(&path) {
                found.push((path, journal));
            }
        }
        found.sort_by_key(|(_, j)| std::cmp::Reverse(j.created_at_unix_ms));
        Ok(found)
    }

    /// The newest committed journal whose window is still open.
    pub fn latest_undoable(
        &self,
        now_unix_ms: u64,
    ) -> Result<Option<(PathBuf, Journal)>, JournalError> {
        Ok(self.list()?.into_iter().find(|(_, j)| {
            j.state == JournalState::Committed
                && now_unix_ms <= j.deadline_unix_ms(self.window_seconds)
        }))
    }

    /// The newest committed journal, whatever its window. Used by the Undo
    /// button so an answer can distinguish "the window has passed" from
    /// "there is nothing to undo".
    pub fn latest_committed(&self) -> Result<Option<(PathBuf, Journal)>, JournalError> {
        Ok(self
            .list()?
            .into_iter()
            .find(|(_, j)| j.state == JournalState::Committed))
    }

    /// Find a job's journal by id (tests and diagnostics).
    pub fn find(&self, job_id: &str) -> Result<Option<(PathBuf, Journal)>, JournalError> {
        Ok(self.list()?.into_iter().find(|(_, j)| j.job_id == job_id))
    }

    /// Undo one journal: verify every output, then delete all of them.
    ///
    /// `now_unix_ms` is a parameter so expiry is testable without sleeping.
    pub fn undo(&self, path: &Path, now_unix_ms: u64) -> Result<UndoReport, UndoError> {
        let journal = self.read(path).map_err(|e| match e {
            JournalError::NotFound(_) => UndoError::NotFound(path.display().to_string()),
            other => UndoError::Io(other.to_string()),
        })?;

        if journal.state != JournalState::Committed {
            return Err(UndoError::NotCommitted {
                job_id: journal.job_id.clone(),
                state: journal.state,
            });
        }
        if now_unix_ms > journal.deadline_unix_ms(self.window_seconds) {
            // The window is closed: retire the journal, keep the files.
            let mut expired = journal.clone();
            expired.state = JournalState::Expired;
            let _ = write_atomic(path, &expired);
            let _ = fs::remove_file(path);
            return Err(UndoError::Expired {
                job_id: journal.job_id.clone(),
                deadline_unix_ms: journal.deadline_unix_ms(self.window_seconds),
            });
        }

        self.verify_then_delete(&journal, path)
    }

    fn verify_then_delete(&self, journal: &Journal, path: &Path) -> Result<UndoReport, UndoError> {
        if journal.outputs.is_empty() {
            // Nothing was published; retire the journal so it stops showing up.
            let mut done = journal.clone();
            done.state = JournalState::Undone;
            let _ = write_atomic(path, &done);
            let _ = fs::remove_file(path);
            return Ok(UndoReport {
                job_id: journal.job_id.clone(),
                deleted: Vec::new(),
                dirs_removed: Vec::new(),
            });
        }

        let mut changed = Vec::new();
        let mut protected = Vec::new();

        for output in &journal.outputs {
            let recorded = PathBuf::from(&output.path);
            if !is_absolute(&recorded) {
                protected.push(output.path.clone());
                continue;
            }
            if journal
                .inputs
                .iter()
                .any(|input| same_file(&PathBuf::from(&input.path), &recorded))
            {
                protected.push(output.path.clone());
                continue;
            }
            let meta = match fs::symlink_metadata(&recorded) {
                Ok(meta) => meta,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    changed.push(output.path.clone());
                    continue;
                }
                Err(e) => return Err(UndoError::Io(format!("{}: {e}", recorded.display()))),
            };
            if meta.file_type().is_symlink() {
                protected.push(output.path.clone());
                continue;
            }
            // A folder re-pointed since publication is not the folder we wrote to.
            let parent = recorded.parent().unwrap_or_else(|| Path::new("."));
            let parent_canon = fs::canonicalize(parent)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            if !output.dir_canon.is_empty() && parent_canon != output.dir_canon {
                protected.push(output.path.clone());
                continue;
            }
            let size = meta.len();
            let digest = hash::sha256_file(&recorded)
                .map_err(|e| UndoError::Io(format!("{}: {e}", recorded.display())))?;
            if size != output.size || digest != output.sha256 {
                changed.push(output.path.clone());
            }
        }

        if !protected.is_empty() {
            return Err(UndoError::Protected {
                job_id: journal.job_id.clone(),
                files: protected,
            });
        }
        if !changed.is_empty() {
            return Err(UndoError::Changed {
                job_id: journal.job_id.clone(),
                files: changed,
            });
        }

        let mut deleted = Vec::with_capacity(journal.outputs.len());
        for output in &journal.outputs {
            let path = PathBuf::from(&output.path);
            match fs::remove_file(&path) {
                Ok(()) => deleted.push(path),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => deleted.push(path),
                Err(e) => return Err(UndoError::Io(format!("{}: {e}", path.display()))),
            }
        }
        let dirs_removed = super::publish::remove_created_dirs(&journal.created_dirs);

        let mut done = journal.clone();
        done.state = JournalState::Undone;
        let _ = write_atomic(path, &done);
        let _ = fs::remove_file(path);

        Ok(UndoReport {
            job_id: journal.job_id.clone(),
            deleted,
            dirs_removed,
        })
    }

    /// Startup sweep. **Never deletes an output** — it only retires journals.
    pub fn sweep(&self, now_unix_ms: u64) -> Result<SweepReport, JournalError> {
        let mut report = SweepReport::default();
        if !self.dir.is_dir() {
            return Ok(report);
        }
        for entry in fs::read_dir(&self.dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let journal = match self.read(&path) {
                Ok(journal) => journal,
                Err(_) => {
                    // Unparseable (crash mid-write). Keep the evidence under a
                    // name that can no longer be acted on.
                    let corrupt = path.with_extension("corrupt");
                    let _ = fs::rename(&path, &corrupt);
                    report.corrupt.push(corrupt);
                    continue;
                }
            };
            match journal.state {
                JournalState::Pending => {
                    report.abandoned.push(journal.job_id.clone());
                    let mut abandoned = journal;
                    abandoned.state = JournalState::Abandoned;
                    let _ = write_atomic(&path, &abandoned);
                    let _ = fs::remove_file(&path);
                }
                JournalState::Committed => {
                    if now_unix_ms > journal.deadline_unix_ms(self.window_seconds) {
                        report.expired.push(journal.job_id.clone());
                        let mut expired = journal;
                        expired.state = JournalState::Expired;
                        let _ = write_atomic(&path, &expired);
                        let _ = fs::remove_file(&path);
                    } else {
                        report.kept += 1;
                    }
                }
                // Undone / expired / abandoned files are leftovers from a crash
                // between "write" and "remove".
                _ => {
                    let _ = fs::remove_file(&path);
                }
            }
        }
        Ok(report)
    }
}

fn ignore_missing(err: std::io::Error) -> Result<(), std::io::Error> {
    if err.kind() == std::io::ErrorKind::NotFound {
        Ok(())
    } else {
        Err(err)
    }
}

/// Absolute on the platform that will run the delete (a rooted `/x` is *not*
/// absolute on Windows, so the check goes through `Path::is_absolute`).
fn is_absolute(path: &Path) -> bool {
    path.is_absolute()
}

/// Do two paths name the same file? Canonical when both exist (catches
/// `a/../b`, case differences on Windows and links), textual otherwise.
fn same_file(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Atomic write: temp file in the same directory, `sync_all`, then rename.
fn write_atomic(path: &Path, journal: &Journal) -> Result<(), JournalError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_vec_pretty(journal)
        .map_err(|e| JournalError::Io(format!("serialising journal: {e}")))?;
    let temp = path.with_extension(format!("json.tmp{}", std::process::id()));
    {
        let mut file = fs::File::create(&temp)?;
        file.write_all(&text)?;
        file.sync_all()?;
    }
    fs::rename(&temp, path).map_err(|e| {
        let _ = fs::remove_file(&temp);
        JournalError::Io(format!("{}: {e}", path.display()))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir()
                .join(format!("ezc-journal-{tag}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A published-file record for `path`, hashed from disk.
    fn record(path: &Path) -> OutputRecord {
        let meta = fs::metadata(path).unwrap();
        OutputRecord {
            path: path.to_string_lossy().to_string(),
            dir_canon: fs::canonicalize(path.parent().unwrap())
                .unwrap()
                .to_string_lossy()
                .to_string(),
            sha256: hash::sha256_file(path).unwrap(),
            size: meta.len(),
            mtime_unix_ms: 0,
        }
    }

    fn input_record(path: &Path) -> FileRecord {
        FileRecord {
            path: path.to_string_lossy().to_string(),
            sha256: hash::sha256_file(path).unwrap(),
            size: fs::metadata(path).unwrap().len(),
        }
    }

    fn draft(job_id: &str, out: &Path) -> DraftJob {
        DraftJob {
            job_id: job_id.to_string(),
            engine: "test".into(),
            route: "test".into(),
            inputs: Vec::new(),
            intended: vec![out.to_string_lossy().to_string()],
        }
    }

    #[test]
    fn a_committed_journal_round_trips_through_disk() {
        let dir = TempDir::new("roundtrip");
        let store = JournalStore::new(dir.path().join("journal"), 10);
        let out = dir.path().join("a.pdf");
        fs::write(&out, b"pdf-bytes").unwrap();

        let pending = store.begin(draft("j_1", &out)).unwrap();
        assert_eq!(pending.journal.state, JournalState::Pending);
        let committed = store
            .commit(&pending, vec![record(&out)], Vec::new(), 0)
            .unwrap();
        assert_eq!(committed.state, JournalState::Committed);

        let (path, loaded) = store.find("j_1").unwrap().expect("journal on disk");
        assert_eq!(loaded.state, JournalState::Committed);
        assert_eq!(loaded.outputs.len(), 1);
        assert_eq!(loaded.outputs[0].sha256, hash::sha256_bytes(b"pdf-bytes"));
        assert_eq!(store.read(&path).unwrap(), loaded);
        // The temp file used for the atomic write is gone.
        let leftovers: Vec<_> = fs::read_dir(store.dir())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains(".tmp")
            })
            .collect();
        assert!(leftovers.is_empty(), "atomic write left a temp file behind");
    }

    #[test]
    fn undo_deletes_the_outputs_and_retires_the_journal() {
        let dir = TempDir::new("undo");
        let store = JournalStore::new(dir.path().join("journal"), 10);
        let out = dir.path().join("a.pdf");
        fs::write(&out, b"one").unwrap();
        let pending = store.begin(draft("j_2", &out)).unwrap();
        store
            .commit(&pending, vec![record(&out)], Vec::new(), 0)
            .unwrap();

        let report = store
            .undo(&pending.path, now_unix_ms())
            .expect("undo should succeed");
        assert_eq!(report.deleted, vec![out.clone()]);
        assert!(!out.exists(), "the output is gone");
        assert!(store.find("j_2").unwrap().is_none(), "journal retired");
    }

    #[test]
    fn undo_deletes_nothing_when_one_output_changed() {
        let dir = TempDir::new("changed");
        let store = JournalStore::new(dir.path().join("journal"), 10);
        let a = dir.path().join("a.pdf");
        let b = dir.path().join("b.pdf");
        fs::write(&a, b"one").unwrap();
        fs::write(&b, b"two").unwrap();
        let pending = store.begin(draft("j_3", &a)).unwrap();
        store
            .commit(&pending, vec![record(&a), record(&b)], Vec::new(), 0)
            .unwrap();

        // The user edits one of them: the job did not write these bytes.
        fs::write(&b, b"USER EDIT").unwrap();
        let err = store.undo(&pending.path, now_unix_ms()).unwrap_err();
        match &err {
            UndoError::Changed { files, .. } => {
                assert_eq!(files.len(), 1);
                assert!(files[0].ends_with("b.pdf"));
            }
            other => panic!("expected Changed, got {other:?}"),
        }
        assert!(
            a.exists(),
            "all-or-nothing: the untouched output survives too"
        );
        assert!(b.exists(), "the user's edit survives");
        assert!(
            store.find("j_3").unwrap().is_some(),
            "a refused undo leaves the journal for a later attempt"
        );
    }

    #[test]
    fn undo_refuses_a_file_the_job_treats_as_an_input() {
        let dir = TempDir::new("input");
        let store = JournalStore::new(dir.path().join("journal"), 10);
        let source = dir.path().join("photo.png");
        fs::write(&source, b"source").unwrap();

        // A journal that names an input as an output can never delete it, even
        // if the hash matches — this is the guard against a corrupted or
        // hand-edited journal.
        let pending = store
            .begin(DraftJob {
                job_id: "j_4".into(),
                engine: "test".into(),
                route: "test".into(),
                inputs: vec![input_record(&source)],
                intended: vec![source.to_string_lossy().to_string()],
            })
            .unwrap();
        let mut forged = record(&source);
        forged.path = source.to_string_lossy().to_string();
        store.commit(&pending, vec![forged], Vec::new(), 0).unwrap();

        let err = store.undo(&pending.path, now_unix_ms()).unwrap_err();
        assert!(matches!(err, UndoError::Protected { .. }), "got {err:?}");
        assert!(source.exists(), "an input is never deleted by undo");
    }

    #[test]
    fn undo_is_refused_once_the_window_closed() {
        let dir = TempDir::new("expired");
        let store = JournalStore::new(dir.path().join("journal"), 10);
        let out = dir.path().join("a.pdf");
        fs::write(&out, b"one").unwrap();
        let pending = store.begin(draft("j_5", &out)).unwrap();
        store
            .commit(&pending, vec![record(&out)], Vec::new(), 0)
            .unwrap();

        let later = now_unix_ms() + 10_001;
        let err = store.undo(&pending.path, later).unwrap_err();
        assert!(
            matches!(err, UndoError::Expired { .. }),
            "a 10 s window must close; got {err:?}"
        );
        assert!(out.exists(), "expiry means the files are kept");
        assert!(store.find("j_5").unwrap().is_none(), "journal retired");
    }

    #[test]
    fn undo_inside_the_window_still_works_at_ten_seconds() {
        let dir = TempDir::new("edge");
        let store = JournalStore::new(dir.path().join("journal"), 10);
        let out = dir.path().join("a.pdf");
        fs::write(&out, b"one").unwrap();
        let pending = store.begin(draft("j_6", &out)).unwrap();
        let committed = store
            .commit(&pending, vec![record(&out)], Vec::new(), 0)
            .unwrap();
        let deadline = committed.deadline_unix_ms(10);
        assert!(store.undo(&pending.path, deadline).is_ok());
    }

    #[test]
    fn undo_removes_the_folder_the_job_created_but_not_one_with_user_files() {
        let dir = TempDir::new("dirs");
        let store = JournalStore::new(dir.path().join("journal"), 10);
        let converted = dir.path().join("converted");
        fs::create_dir_all(&converted).unwrap();
        let out = converted.join("a.pdf");
        fs::write(&out, b"one").unwrap();

        let pending = store.begin(draft("j_7", &out)).unwrap();
        store
            .commit(
                &pending,
                vec![record(&out)],
                vec![converted.to_string_lossy().to_string()],
                0,
            )
            .unwrap();
        let report = store.undo(&pending.path, now_unix_ms()).unwrap();
        assert_eq!(report.dirs_removed, vec![converted.clone()]);
        assert!(!converted.exists());

        // Now with something of the user's inside: the folder must survive.
        let converted = dir.path().join("converted2");
        fs::create_dir_all(&converted).unwrap();
        let out = converted.join("a.pdf");
        fs::write(&out, b"one").unwrap();
        fs::write(converted.join("notes.txt"), b"mine").unwrap();
        let pending = store.begin(draft("j_8", &out)).unwrap();
        store
            .commit(
                &pending,
                vec![record(&out)],
                vec![converted.to_string_lossy().to_string()],
                0,
            )
            .unwrap();
        let report = store.undo(&pending.path, now_unix_ms()).unwrap();
        assert!(report.dirs_removed.is_empty());
        assert!(converted.is_dir());
        assert!(converted.join("notes.txt").exists());
    }

    #[test]
    fn a_pending_journal_is_abandoned_at_startup_and_deletes_nothing() {
        let dir = TempDir::new("sweep-pending");
        let store = JournalStore::new(dir.path().join("journal"), 10);
        // A file the pending journal *intended* to write already exists — the
        // user's own file, in this scenario. The sweep must not touch it.
        let out = dir.path().join("a.pdf");
        fs::write(&out, b"user file").unwrap();
        store.begin(draft("j_9", &out)).unwrap();

        let report = store.sweep(now_unix_ms()).unwrap();
        assert_eq!(report.abandoned, vec!["j_9".to_string()]);
        assert_eq!(fs::read(&out).unwrap(), b"user file");
        assert!(
            store.list().unwrap().is_empty(),
            "abandoned journal removed"
        );
    }

    #[test]
    fn the_sweep_expires_old_committed_journals_and_keeps_their_outputs() {
        let dir = TempDir::new("sweep-committed");
        let store = JournalStore::new(dir.path().join("journal"), 10);
        let out = dir.path().join("a.pdf");
        fs::write(&out, b"one").unwrap();
        let pending = store.begin(draft("j_10", &out)).unwrap();
        store
            .commit(&pending, vec![record(&out)], Vec::new(), 0)
            .unwrap();

        let fresh = store.sweep(now_unix_ms()).unwrap();
        assert_eq!(fresh.kept, 1, "a live undo window must survive a restart");

        let later = store.sweep(now_unix_ms() + 60_000).unwrap();
        assert_eq!(later.expired, vec!["j_10".to_string()]);
        assert!(out.exists(), "expiry never deletes an output");
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn latest_undoable_prefers_the_newest_open_window() {
        let dir = TempDir::new("latest");
        let store = JournalStore::new(dir.path().join("journal"), 10);
        let first = dir.path().join("first.pdf");
        fs::write(&first, b"1").unwrap();
        let p1 = store.begin(draft("j_11", &first)).unwrap();
        store
            .commit(&p1, vec![record(&first)], Vec::new(), 0)
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second = dir.path().join("second.pdf");
        fs::write(&second, b"2").unwrap();
        let p2 = store.begin(draft("j_12", &second)).unwrap();
        store
            .commit(&p2, vec![record(&second)], Vec::new(), 0)
            .unwrap();

        let (path, journal) = store.latest_undoable(now_unix_ms()).unwrap().unwrap();
        assert_eq!(journal.job_id, "j_12");
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            name.starts_with("j_12-") && name.ends_with(".json"),
            "journal file is named after its job, got {name}"
        );
        // Far in the future nothing is undoable.
        assert!(store
            .latest_undoable(now_unix_ms() + 3_600_000)
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_corrupt_journal_is_set_aside_and_never_acted_on() {
        let dir = TempDir::new("corrupt");
        let store = JournalStore::new(dir.path().join("journal"), 10);
        fs::create_dir_all(store.dir()).unwrap();
        let bad = store.dir().join("j_13-1.json");
        fs::write(&bad, b"{ not json").unwrap();

        let report = store.sweep(now_unix_ms()).unwrap();
        assert_eq!(report.corrupt.len(), 1);
        assert!(!bad.exists());
        assert!(
            report.corrupt[0].exists(),
            "the evidence is kept as .corrupt"
        );
    }

    #[cfg(unix)]
    #[test]
    fn undo_refuses_a_symlinked_output() {
        let dir = TempDir::new("symlink");
        let store = JournalStore::new(dir.path().join("journal"), 10);
        let real = dir.path().join("elsewhere.pdf");
        fs::write(&real, b"one").unwrap();
        let linked = dir.path().join("a.pdf");
        std::os::unix::fs::symlink(&real, &linked).unwrap();

        let pending = store.begin(draft("j_14", &real)).unwrap();
        let mut rec = record(&real);
        rec.path = linked.to_string_lossy().to_string();
        rec.dir_canon = fs::canonicalize(dir.path())
            .unwrap()
            .to_string_lossy()
            .to_string();
        store.commit(&pending, vec![rec], Vec::new(), 0).unwrap();

        let err = store.undo(&pending.path, now_unix_ms()).unwrap_err();
        assert!(matches!(err, UndoError::Protected { .. }), "got {err:?}");
        assert!(real.exists());
        assert!(linked.exists());
    }

    #[cfg(unix)]
    #[test]
    fn undo_refuses_when_the_output_folder_was_re_pointed() {
        let dir = TempDir::new("repoint");
        let store = JournalStore::new(dir.path().join("journal"), 10);
        let outdir = dir.path().join("out");
        fs::create_dir_all(&outdir).unwrap();
        let out = outdir.join("a.pdf");
        fs::write(&out, b"one").unwrap();
        let pending = store.begin(draft("j_15", &out)).unwrap();
        store
            .commit(&pending, vec![record(&out)], Vec::new(), 0)
            .unwrap();

        // Move the real folder away and put a symlink in its place, with a
        // same-named, same-content file. A delete must not follow it.
        let moved = dir.path().join("moved");
        fs::rename(&outdir, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &outdir).unwrap();

        let err = store.undo(&pending.path, now_unix_ms()).unwrap_err();
        assert!(matches!(err, UndoError::Protected { .. }), "got {err:?}");
        assert!(moved.join("a.pdf").exists(), "the real file is untouched");
    }
}
