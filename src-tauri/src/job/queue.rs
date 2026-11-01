//! Queue admission, folder expansion, per-file state and batch progress.
//!
//! `ARCHITECTURE.md` §11: the window and the wheel share *this* admission path,
//! so naming, location and metadata rules cannot diverge between surfaces.
//!
//! `SPEC.md` §5 rules owned here:
//!
//! - multi-file drops are supported (each path is admitted independently);
//! - a dropped folder is enumerated **1 level deep** unless
//!   `folder_drop.include_subfolders` is on, and what is skipped is *counted and
//!   reported*, never silently dropped;
//! - batch jobs are recognised when the queue reaches `output.batch_threshold`.
//!
//! Detection (content sniffing) belongs to `C1`; a new item starts in
//! `DetectedState::Pending` and the window says so rather than guessing from the
//! extension.

use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::model::naming::{self, PathProblem};
use crate::model::settings::Settings;
use crate::model::types::{human_size, is_known_target, InputFile, ItemState};

/// Maximum directory recursion depth. A safety limit, not a promise: symlink
/// loops and pathological trees are cut off rather than walked forever.
pub const MAX_FOLDER_DEPTH: usize = 32;

/// A file the user dropped, as the queue holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: u64,
    pub input: InputFile,
    pub target: Option<String>,
    pub strip: bool,
    pub state: ItemState,
    pub detected: DetectedState,
    pub added_unix_ms: u128,
}

/// What the detection step knows about a file (`C1` fills this in).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectedState {
    /// Detection has not run yet.
    Pending,
    /// Detection ran and could not identify the content.
    Unknown,
    /// Detection ran and the content is of this canonical type.
    Known(String),
}

impl DetectedState {
    pub fn label(&self) -> &str {
        match self {
            DetectedState::Pending => "checking type…",
            DetectedState::Unknown => "unrecognised",
            DetectedState::Known(kind) => kind,
        }
    }
}

/// One refused path with the reason a user can act on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub path: String,
    pub reason: String,
}

/// Outcome of one admission (a drop or an "Add files").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdmissionReport {
    pub added: usize,
    pub folders_expanded: usize,
    pub skipped: Vec<Skipped>,
}

impl AdmissionReport {
    /// One-line summary for the window's status strip.
    pub fn summary(&self) -> String {
        let mut text = format!("{} file(s) added", self.added);
        if self.folders_expanded > 0 {
            text.push_str(&format!(" from {} folder(s)", self.folders_expanded));
        }
        if !self.skipped.is_empty() {
            text.push_str(&format!(", {} skipped", self.skipped.len()));
        }
        text
    }
}

/// Batch progress, derived from item states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub total: usize,
    pub queued: usize,
    pub running: usize,
    pub done: usize,
    pub failed: usize,
    pub skipped: usize,
    /// Terminal items / total, 0-100. `0` when the queue is empty.
    pub percent: u32,
    pub label: String,
}

impl Progress {
    pub fn of(items: &[Item]) -> Self {
        let mut queued = 0;
        let mut running = 0;
        let mut done = 0;
        let mut failed = 0;
        let mut skipped = 0;
        for item in items {
            match &item.state {
                ItemState::Queued => queued += 1,
                ItemState::Preparing | ItemState::Running | ItemState::Verifying => running += 1,
                ItemState::Done => done += 1,
                ItemState::Failed(_) => failed += 1,
                ItemState::Skipped(_) => skipped += 1,
                ItemState::Cancelled => failed += 1,
            }
        }
        let total = items.len();
        let terminal = done + failed + skipped;
        let percent = if total == 0 {
            0
        } else {
            ((terminal as f64 / total as f64) * 100.0).round() as u32
        };
        let label = if total == 0 {
            "Nothing queued".to_string()
        } else if terminal == total {
            if failed == 0 && skipped == 0 {
                format!("{done} of {total} converted")
            } else {
                format!("{done} of {total} converted · {} didn't", failed + skipped)
            }
        } else {
            format!("{terminal} of {total} finished")
        };
        Self {
            total,
            queued,
            running,
            done,
            failed,
            skipped,
            percent,
            label,
        }
    }
}

/// One row of the queue table as the window renders it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub dir: String,
    pub size_bytes: u64,
    pub size_text: String,
    pub target: Option<String>,
    pub strip: bool,
    pub state: String,
    pub state_detail: Option<String>,
    pub detected: String,
    /// Where the result will land, from the shared naming rules.
    pub preview_out: Option<String>,
    pub preview_dir: Option<String>,
    /// `true` when a `-1`, `-2` suffix will be added to avoid an overwrite.
    pub renamed: bool,
}

/// Everything the window needs for one render.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueSnapshot {
    pub items: Vec<ItemView>,
    pub progress: Progress,
    /// `SPEC.md` §5: is the batch-folder rule active for the current queue?
    pub batch_folder_active: bool,
    pub batch_threshold: u32,
    pub batch_subfolder: String,
    /// Number of targets currently assigned, for the batch bar.
    pub with_target: usize,
}

/// The queue. One queue per window session; ids are monotonic.
#[derive(Debug, Default, Clone)]
pub struct Queue {
    items: Vec<Item>,
    next_id: u64,
    folders_expanded: usize,
}

impl Queue {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            next_id: 1,
            folders_expanded: 0,
        }
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.items.len();
        self.items.retain(|item| item.id != id);
        self.items.len() != before
    }

    /// Admits dropped paths. Folders are expanded per `SPEC.md` §5; duplicates
    /// (by canonical path), hidden files, oversized files and invalid names are
    /// refused with a reason and reported.
    pub fn add_paths(&mut self, paths: &[PathBuf], settings: &Settings) -> AdmissionReport {
        let mut report = AdmissionReport::default();
        self.folders_expanded = 0;
        let mut seen: BTreeSet<PathBuf> = self
            .items
            .iter()
            .map(|item| canonical_or_self(&item.input.path))
            .collect();

        let mut candidates: Vec<PathBuf> = Vec::new();
        for path in paths {
            match fs::metadata(path) {
                Ok(meta) if meta.is_dir() => {
                    report.folders_expanded += 1;
                    let expansion = expand_folder(path, settings.folder_drop.include_subfolders);
                    report.skipped.extend(expansion.skipped);
                    candidates.extend(expansion.files);
                }
                Ok(_) => candidates.push(path.clone()),
                Err(err) => report.skipped.push(Skipped {
                    path: path.to_string_lossy().to_string(),
                    reason: format!("can't be read ({})", io_reason(&err)),
                }),
            }
        }

        candidates.sort();
        for path in candidates {
            if let Err(problem) = naming::validate_input_path(&path) {
                report.skipped.push(describe_problem(&path, &problem));
                continue;
            }

            let canonical = canonical_or_self(&path);
            if !seen.insert(canonical) {
                report.skipped.push(Skipped {
                    path: path.to_string_lossy().to_string(),
                    reason: "already in the queue".into(),
                });
                continue;
            }

            let meta = match fs::metadata(&path) {
                Ok(meta) => meta,
                Err(err) => {
                    seen.remove(&canonical_or_self(&path));
                    report.skipped.push(Skipped {
                        path: path.to_string_lossy().to_string(),
                        reason: format!("can't be read ({})", io_reason(&err)),
                    });
                    continue;
                }
            };

            if !meta.is_file() {
                report.skipped.push(Skipped {
                    path: path.to_string_lossy().to_string(),
                    reason: "not a file".into(),
                });
                continue;
            }

            if is_hidden(&path) {
                report.skipped.push(Skipped {
                    path: path.to_string_lossy().to_string(),
                    reason: "hidden file".into(),
                });
                continue;
            }

            if meta.len() > settings.limits.max_input_bytes {
                report.skipped.push(Skipped {
                    path: path.to_string_lossy().to_string(),
                    reason: format!(
                        "larger than the {} limit",
                        human_size(settings.limits.max_input_bytes)
                    ),
                });
                continue;
            }

            let id = self.next_id;
            self.next_id += 1;
            self.items.push(Item {
                id,
                input: InputFile {
                    path,
                    size: meta.len(),
                },
                target: None,
                strip: settings.metadata.strip,
                state: ItemState::Queued,
                detected: DetectedState::Pending,
                added_unix_ms: now_ms(),
            });
            report.added += 1;
        }

        report
    }

    /// Sets the target on every item that has none, or on all items.
    pub fn set_target_all(&mut self, target: &str) {
        let target = target.trim().to_ascii_lowercase();
        if !is_known_target(&target) {
            return;
        }
        for item in &mut self.items {
            item.target = Some(target.clone());
        }
    }

    pub fn set_target(&mut self, id: u64, target: Option<&str>) -> bool {
        let normalised = target.map(|t| t.trim().to_ascii_lowercase());
        if let Some(t) = &normalised {
            if !is_known_target(t) {
                return false;
            }
        }
        match self.items.iter_mut().find(|item| item.id == id) {
            Some(item) => {
                item.target = normalised;
                true
            }
            None => false,
        }
    }

    pub fn set_strip_all(&mut self, strip: bool) {
        for item in &mut self.items {
            item.strip = strip;
        }
    }

    pub fn set_strip(&mut self, id: u64, strip: bool) -> bool {
        match self.items.iter_mut().find(|item| item.id == id) {
            Some(item) => {
                item.strip = strip;
                true
            }
            None => false,
        }
    }

    /// Drives the per-file state machine. The worker (`C4`) calls this; tests
    /// use it to prove progress rendering.
    pub fn set_state(&mut self, id: u64, state: ItemState) -> bool {
        match self.items.iter_mut().find(|item| item.id == id) {
            Some(item) => {
                item.state = state;
                true
            }
            None => false,
        }
    }

    pub fn progress(&self) -> Progress {
        Progress::of(&self.items)
    }

    /// Builds the render model, including the output preview from the shared
    /// naming rules (`model::naming`), so the window shows the same name the
    /// writer will produce.
    pub fn snapshot(&self, settings: &Settings) -> QueueSnapshot {
        let batch_len = self.items.len();
        let batch_folder_active = settings.output.use_batch_subfolder
            && batch_len as u32 >= settings.output.batch_threshold
            && settings.output.location == "same_folder";

        let items = self
            .items
            .iter()
            .map(|item| {
                let preview = item.target.as_deref().map(|target| {
                    naming::preview_batch(
                        std::slice::from_ref(&item.input.path),
                        target,
                        settings,
                        batch_len,
                    )
                    .remove(0)
                });
                ItemView {
                    id: item.id,
                    name: item
                        .input
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| item.input.path.to_string_lossy().to_string()),
                    path: item.input.path.to_string_lossy().to_string(),
                    dir: item
                        .input
                        .path
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    size_bytes: item.input.size,
                    size_text: human_size(item.input.size),
                    target: item.target.clone(),
                    strip: item.strip,
                    state: item.state.code().to_string(),
                    state_detail: item.state.detail().map(str::to_string),
                    detected: item.detected.label().to_string(),
                    preview_out: preview.as_ref().map(|p| p.out_path.clone()),
                    preview_dir: preview.as_ref().map(|p| p.out_dir.clone()),
                    renamed: preview.as_ref().map(|p| p.renamed).unwrap_or(false),
                }
            })
            .collect::<Vec<_>>();

        QueueSnapshot {
            with_target: items.iter().filter(|i| i.target.is_some()).count(),
            items,
            progress: self.progress(),
            batch_folder_active,
            batch_threshold: settings.output.batch_threshold,
            batch_subfolder: settings.output.batch_subfolder.clone(),
        }
    }
}

/// Result of enumerating a folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expansion {
    pub files: Vec<PathBuf>,
    /// Files that were found but refused (hidden, unreadable).
    pub skipped: Vec<Skipped>,
    pub subfolders: usize,
}

/// Enumerates `root` one level deep, or recursively when `include_subfolders`
/// is on (`SPEC.md` §5). Symlinked directories are not followed.
pub fn expand_folder(root: &Path, include_subfolders: bool) -> Expansion {
    let mut out = Expansion::default();
    walk(root, include_subfolders, 0, &mut out, &mut BTreeSet::new());
    out.files.sort();
    out
}

fn walk(
    dir: &Path,
    recurse: bool,
    depth: usize,
    out: &mut Expansion,
    visited: &mut BTreeSet<PathBuf>,
) {
    let canonical = canonical_or_self(dir);
    if !visited.insert(canonical) {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            out.skipped.push(Skipped {
                path: dir.to_string_lossy().to_string(),
                reason: format!("folder can't be read ({})", io_reason(&err)),
            });
            return;
        }
    };

    let mut batch: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    batch.sort();

    for path in batch {
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(err) => {
                out.skipped.push(Skipped {
                    path: path.to_string_lossy().to_string(),
                    reason: format!("can't be read ({})", io_reason(&err)),
                });
                continue;
            }
        };
        if meta.is_dir() {
            out.subfolders += 1;
            if recurse && depth + 1 < MAX_FOLDER_DEPTH && !meta.file_type().is_symlink() {
                walk(&path, recurse, depth + 1, out, visited);
            } else if !recurse {
                out.skipped.push(Skipped {
                    path: path.to_string_lossy().to_string(),
                    reason: "subfolder — turn on \"Include subfolders\" to include it".into(),
                });
            }
            continue;
        }
        if meta.file_type().is_symlink() {
            // A symlinked file is admitted only if it resolves to a file; the
            // queue stores the link and the canonical form dedupes it.
            match fs::metadata(&path) {
                Ok(target) if target.is_file() => out.files.push(path),
                _ => out.skipped.push(Skipped {
                    path: path.to_string_lossy().to_string(),
                    reason: "broken link".into(),
                }),
            }
            continue;
        }
        if is_hidden(&path) {
            out.skipped.push(Skipped {
                path: path.to_string_lossy().to_string(),
                reason: "hidden file".into(),
            });
            continue;
        }
        out.files.push(path);
    }
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .map(|n| n.to_string_lossy().starts_with('.'))
        .unwrap_or(false)
}

fn canonical_or_self(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn io_reason(err: &std::io::Error) -> &'static str {
    match err.kind() {
        std::io::ErrorKind::PermissionDenied => "permission denied",
        std::io::ErrorKind::NotFound => "no longer there",
        _ => "read error",
    }
}

fn describe_problem(path: &Path, problem: &PathProblem) -> Skipped {
    Skipped {
        path: path.to_string_lossy().to_string(),
        reason: match problem {
            PathProblem::ReservedName(name) => format!("{name:?} is reserved on Windows"),
            PathProblem::TrailingDotOrSpace(name) => {
                format!("{name:?} ends with a dot or a space")
            }
            PathProblem::TooLong(_) => "path is too long".into(),
            PathProblem::NotAbsolute => "path is not absolute".into(),
        },
    }
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir()
                .join(format!("ezc-queue-{tag}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, rel: &str, bytes: usize) -> PathBuf {
            let path = self.0.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, vec![b'x'; bytes]).unwrap();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn multi_file_drop_admits_every_file() {
        let dir = TempDir::new("multi");
        let a = dir.write("a.png", 10);
        let b = dir.write("b.jpg", 20);
        let c = dir.write("c.heic", 30);

        let mut queue = Queue::new();
        let report = queue.add_paths(&[a, b, c], &Settings::default());
        assert_eq!(report.added, 3);
        assert!(report.skipped.is_empty());
        assert_eq!(queue.len(), 3);
        assert_eq!(queue.progress().total, 3);
        assert_eq!(report.summary(), "3 file(s) added");
    }

    #[test]
    fn folder_drop_is_one_level_deep_by_default_and_reports_what_it_skipped() {
        let dir = TempDir::new("folder");
        let folder = dir.path().join("drop");
        fs::create_dir_all(folder.join("nested")).unwrap();
        fs::write(folder.join("top.png"), b"x").unwrap();
        fs::write(folder.join("nested/deep.png"), b"x").unwrap();

        let mut queue = Queue::new();
        let report = queue.add_paths(&[folder.clone()], &Settings::default());
        assert_eq!(report.added, 1, "only the top-level file");
        assert_eq!(report.folders_expanded, 1);
        assert!(
            report
                .skipped
                .iter()
                .any(|s| s.reason.contains("Include subfolders")),
            "skipped subfolder must be reported: {:?}",
            report.skipped
        );
    }

    #[test]
    fn include_subfolders_changes_what_is_admitted() {
        let dir = TempDir::new("recursive");
        let folder = dir.path().join("drop");
        fs::create_dir_all(folder.join("nested/deeper")).unwrap();
        fs::write(folder.join("top.png"), b"x").unwrap();
        fs::write(folder.join("nested/deep.png"), b"x").unwrap();
        fs::write(folder.join("nested/deeper/deepest.png"), b"x").unwrap();

        let mut settings = Settings::default();
        settings.folder_drop.include_subfolders = true;

        let mut queue = Queue::new();
        let report = queue.add_paths(&[folder], &settings);
        assert_eq!(report.added, 3, "skipped: {:?}", report.skipped);
    }

    #[test]
    fn hidden_files_are_skipped_with_a_reason() {
        let dir = TempDir::new("hidden");
        let visible = dir.write("visible.png", 4);
        let hidden = dir.write(".secret.png", 4);
        let mut queue = Queue::new();
        let report = queue.add_paths(&[visible, hidden], &Settings::default());
        assert_eq!(report.added, 1);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].reason, "hidden file");
    }

    #[test]
    fn the_same_file_twice_is_admitted_once() {
        let dir = TempDir::new("dupe");
        let a = dir.write("a.png", 4);
        let mut queue = Queue::new();
        let report = queue.add_paths(&[a.clone(), a.clone()], &Settings::default());
        assert_eq!(report.added, 1);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].reason, "already in the queue");
    }

    #[test]
    fn oversized_files_are_refused_at_admission() {
        let dir = TempDir::new("big");
        let big = dir.write("big.png", 4096);
        let mut settings = Settings::default();
        settings.limits.max_input_bytes = 1024;
        let mut queue = Queue::new();
        let report = queue.add_paths(&[big], &settings);
        assert_eq!(report.added, 0);
        assert!(report.skipped[0].reason.contains("limit"));
    }

    #[test]
    fn missing_paths_are_reported_not_silently_dropped() {
        let mut queue = Queue::new();
        let report = queue.add_paths(
            &[PathBuf::from("/definitely/not/here.png")],
            &Settings::default(),
        );
        assert_eq!(report.added, 0);
        assert_eq!(report.skipped.len(), 1);
    }

    #[test]
    fn targets_can_be_set_per_row_and_in_bulk() {
        let dir = TempDir::new("targets");
        let a = dir.write("a.png", 4);
        let b = dir.write("b.png", 4);
        let mut queue = Queue::new();
        queue.add_paths(&[a, b], &Settings::default());

        queue.set_target_all("PNG");
        assert_eq!(queue.items()[0].target.as_deref(), Some("png"));
        assert!(!queue.set_target(queue.items()[0].id, Some("klingon")));
        assert!(queue.set_target(queue.items()[0].id, Some("pdf")));
        assert_eq!(queue.items()[0].target.as_deref(), Some("pdf"));
        assert_eq!(queue.items()[1].target.as_deref(), Some("png"));
    }

    #[test]
    fn strip_starts_from_the_setting_and_is_per_row() {
        let dir = TempDir::new("strip");
        let a = dir.write("a.png", 4);
        let b = dir.write("b.png", 4);
        let mut settings = Settings::default();
        settings.metadata.strip = true;
        let mut queue = Queue::new();
        queue.add_paths(&[a, b], &settings);
        assert!(queue.items()[0].strip);
        assert!(queue.set_strip(queue.items()[0].id, false));
        assert!(!queue.items()[0].strip);
        assert!(queue.items()[1].strip);
        queue.set_strip_all(false);
        assert!(queue.items().iter().all(|i| !i.strip));
    }

    #[test]
    fn progress_counts_terminal_items_and_reports_failures() {
        let dir = TempDir::new("progress");
        let paths: Vec<_> = (0..4).map(|i| dir.write(&format!("f{i}.png"), 4)).collect();
        let mut queue = Queue::new();
        queue.add_paths(&paths, &Settings::default());
        let ids: Vec<u64> = queue.items().iter().map(|i| i.id).collect();

        assert_eq!(queue.progress().percent, 0);
        queue.set_state(ids[0], ItemState::Done);
        queue.set_state(ids[1], ItemState::Running);
        queue.set_state(ids[2], ItemState::Failed("decode".into()));
        let p = queue.progress();
        assert_eq!(
            (p.total, p.done, p.failed, p.running, p.queued),
            (4, 1, 1, 1, 1)
        );
        assert_eq!(p.percent, 50);
        assert_eq!(p.label, "2 of 4 finished");

        queue.set_state(ids[3], ItemState::Skipped("dupe".into()));
        let p = queue.progress();
        assert_eq!(p.percent, 75);

        queue.set_state(ids[1], ItemState::Done);
        let p = queue.progress();
        assert_eq!(p.percent, 100);
        assert_eq!(p.label, "2 of 4 converted · 2 didn't");
    }

    #[test]
    fn snapshot_shows_the_batch_folder_rule_and_previews() {
        let dir = TempDir::new("snapshot");
        let paths: Vec<_> = (0..3)
            .map(|i| dir.write(&format!("pic{i}.png"), 4))
            .collect();
        let mut queue = Queue::new();
        queue.add_paths(&paths, &Settings::default());
        queue.set_target_all("jpg");

        let snapshot = queue.snapshot(&Settings::default());
        assert!(
            snapshot.batch_folder_active,
            "3 files must trigger the batch rule"
        );
        assert_eq!(snapshot.items.len(), 3);
        assert!(snapshot.items.iter().all(|i| i
            .preview_out
            .as_deref()
            .unwrap_or("")
            .contains("converted")));
        assert_eq!(snapshot.with_target, 3);
        assert_eq!(snapshot.items[0].detected, "checking type…");
    }

    #[test]
    fn removing_and_clearing_reset_the_queue() {
        let dir = TempDir::new("remove");
        let a = dir.write("a.png", 4);
        let mut queue = Queue::new();
        queue.add_paths(&[a], &Settings::default());
        let id = queue.items()[0].id;
        assert!(queue.remove(id));
        assert!(queue.is_empty());
        assert!(!queue.remove(id));
        queue.add_paths(&[dir.write("b.png", 4)], &Settings::default());
        queue.clear();
        assert_eq!(queue.progress().total, 0);
    }
}
