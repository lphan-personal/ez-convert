//! Publication: move a finished file out of the job temp area into the user's
//! folder without ever clobbering anything (`ARCHITECTURE.md` §9.3, §9.4).
//!
//! Three rules, each enforced here rather than by caller discipline:
//!
//! 1. **Never overwrite.** The final path is claimed and the file is placed in
//!    one step: `hard_link` onto the destination (which fails if it exists on
//!    both Windows and Unix) with a `create_new` copy as the cross-volume
//!    fallback (which is `O_EXCL`, so it is atomic too). `std::fs::rename` is
//!    deliberately *not* used on the final hop — on Unix it silently replaces
//!    an existing file, which is exactly the data loss `SPEC.md` §5 forbids.
//! 2. **One claim per directory at a time.** Probing for a free name and
//!    placing the file happen under a per-directory lock, so two jobs writing
//!    `photo.jpg` into the same folder cannot both think `photo.jpg` is free.
//!    The lock is in-process (this app is one process); a name that appears
//!    underneath us from another program is still caught, because the placement
//!    itself refuses to clobber and the caller re-probes.
//! 3. **Atomic publication.** The engine wrote into the job temp dir; the user's
//!    folder only ever gains a complete file or nothing at all. A crash mid-run
//!    leaves temp files behind and the user's folder untouched.
//!
//! `never_overwrite` is a rule in `SPEC.md` §5, not a preference, so there is no
//! "overwrite" branch here at all.

use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use super::hash;
use crate::model::naming;

/// How many suffixes to try before giving up. A directory with 10k files that
/// all share a stem is pathological; refusing beats spinning forever.
pub const MAX_SUFFIX_ATTEMPTS: u32 = 10_000;

/// A file that is now in the user's folder, with the facts undo needs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Published {
    pub path: PathBuf,
    pub sha256: String,
    pub size: u64,
    /// `0` when the platform did not report a modification time.
    pub mtime_unix_ms: u64,
    /// `true` when the plain `<stem>.<ext>` name was taken and a suffix was used.
    pub renamed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishError {
    /// The engine produced nothing (or a zero-byte file). Never published: a
    /// silent empty output is a bug, not a success (`ARCHITECTURE.md` §6.0.4).
    EmptyOutput(PathBuf),
    /// No free name within [`MAX_SUFFIX_ATTEMPTS`].
    NoFreeName {
        dir: PathBuf,
        stem: String,
    },
    Io(String),
}

impl std::fmt::Display for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PublishError::EmptyOutput(path) => {
                write!(f, "the engine wrote no data to {}", path.display())
            }
            PublishError::NoFreeName { dir, stem } => write!(
                f,
                "no free file name for {stem:?} in {} (10000 suffixes tried)",
                dir.display()
            ),
            PublishError::Io(reason) => write!(f, "{reason}"),
        }
    }
}

impl From<io::Error> for PublishError {
    fn from(e: io::Error) -> Self {
        PublishError::Io(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Per-directory lock
// ---------------------------------------------------------------------------

/// One mutex per directory, keyed by the path as given (normalised so
/// `C:\out` and `C:\out\` are the same lock).
fn directory_locks() -> &'static Mutex<HashMap<PathBuf, Arc<Mutex<()>>>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_key(dir: &Path) -> PathBuf {
    // `canonicalize` would fail on a directory we are about to create, so
    // normalise textually: trailing separators and `.` components removed.
    let mut key = PathBuf::new();
    for part in dir.components() {
        match part {
            std::path::Component::CurDir => {}
            other => key.push(other.as_os_str()),
        }
    }
    key
}

/// The lock guarding claims inside `dir`.
pub fn directory_lock(dir: &Path) -> Arc<Mutex<()>> {
    let mut map = directory_locks()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    map.entry(lock_key(dir))
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

// ---------------------------------------------------------------------------
// Temp area
// ---------------------------------------------------------------------------

/// The per-job temp area engines write into (`ARCHITECTURE.md` §9.4).
#[derive(Debug, Clone)]
pub struct TempArea {
    root: PathBuf,
}

impl TempArea {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The production location: `<cache>/jobs`.
    pub fn in_cache() -> Self {
        Self::new(super::paths::jobs_dir())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Directory for one job. `job_id` is sanitised so a crafted id cannot
    /// escape the temp root (`ARCHITECTURE.md` §16 S2).
    pub fn job_dir(&self, job_id: &str) -> PathBuf {
        self.root.join(sanitise_component(job_id))
    }

    /// Create a fresh, empty temp dir for `job_id`.
    pub fn prepare(&self, job_id: &str) -> Result<PathBuf, PublishError> {
        let dir = self.job_dir(job_id);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Remove a job's temp dir. Best effort: leftovers are disposable.
    pub fn cleanup(&self, job_id: &str) {
        let dir = self.job_dir(job_id);
        if dir.exists() {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

/// Keep only characters that are safe in a Windows path component, so a job id
/// can never name a path outside the temp root.
///
/// `.` is not in the allowed set on purpose: `..` is the one path component that
/// means "go up", and this is the only place a caller-supplied string becomes a
/// file name.
pub fn sanitise_component(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "job".to_string()
    } else {
        trimmed
    }
}

// ---------------------------------------------------------------------------
// Directory creation that undo can reverse
// ---------------------------------------------------------------------------

/// Create `dir` (and any missing parents), returning exactly the directories
/// this call created, parents first.
///
/// Undo removes these again if — and only if — they are empty afterwards, so a
/// batch that created `converted/` does not leave an empty folder behind. A
/// directory the user already had is never reported, therefore never removed.
pub fn ensure_dir(dir: &Path) -> Result<Vec<PathBuf>, PublishError> {
    if dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut missing: Vec<PathBuf> = Vec::new();
    let mut cursor = dir;
    loop {
        if cursor.is_dir() {
            break;
        }
        missing.push(cursor.to_path_buf());
        match cursor.parent() {
            Some(parent) if parent != cursor => cursor = parent,
            _ => break,
        }
    }
    let mut created = Vec::with_capacity(missing.len());
    for path in missing.iter().rev() {
        fs::create_dir(path)?;
        created.push(path.clone());
    }
    Ok(created)
}

/// Remove the directories a job created, deepest first, only while empty.
///
/// Returns the ones actually removed. A folder the user has since put
/// something in is left alone — undo removes this job's files, not the user's.
pub fn remove_created_dirs(created: &[String]) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    let mut ordered: Vec<PathBuf> = created.iter().map(PathBuf::from).collect();
    ordered.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for dir in ordered {
        // `remove_dir` refuses a non-empty directory, which is the check we need.
        if fs::remove_dir(&dir).is_ok() {
            removed.push(dir);
        }
    }
    removed
}

// ---------------------------------------------------------------------------
// Publication
// ---------------------------------------------------------------------------

/// Publish `temp` into `dir` under the first free `<stem>[-n].<ext>` name.
///
/// `reserved` carries names already claimed in this run (a batch converting
/// `a.png` and `a.jpg` to `a.webp`); entries are added as names are taken.
pub fn publish_new(
    temp: &Path,
    dir: &Path,
    stem: &str,
    ext: &str,
    reserved: &mut BTreeSet<PathBuf>,
) -> Result<Published, PublishError> {
    let size = fs::metadata(temp)
        .map_err(|e| PublishError::Io(format!("{}: {e}", temp.display())))?
        .len();
    if size == 0 {
        return Err(PublishError::EmptyOutput(temp.to_path_buf()));
    }

    let ext = ext.trim_start_matches('.');
    let plain = dir.join(format!("{stem}.{ext}"));
    let lock = directory_lock(dir);
    let _guard = lock.lock().unwrap_or_else(|poison| poison.into_inner());

    ensure_dir(dir)?;

    let mut attempts = 0u32;
    loop {
        let candidate = naming::next_free_name(dir, stem, ext, reserved);
        match place_without_clobber(temp, &candidate) {
            Ok(()) => {
                reserved.insert(candidate.clone());
                return published(candidate.clone(), candidate != plain);
            }
            Err(Place::Collision) => {
                // Someone outside this process took the name between the probe
                // and the placement. Probe again; the loop is bounded.
                reserved.insert(candidate);
                attempts += 1;
                if attempts > MAX_SUFFIX_ATTEMPTS {
                    return Err(PublishError::NoFreeName {
                        dir: dir.to_path_buf(),
                        stem: stem.to_string(),
                    });
                }
            }
            Err(Place::Io(reason)) => return Err(PublishError::Io(reason)),
        }
    }
}

/// Publish `temp` at exactly `dest` (engine-chosen names, e.g. one CSV per
/// sheet). Refuses to replace anything: an existing `dest` is
/// [`PublishError::EmptyOutput`]-free but still an error — the caller decides
/// whether to re-name or report.
pub fn publish_exact(temp: &Path, dest: &Path) -> Result<Published, PublishError> {
    let size = fs::metadata(temp)
        .map_err(|e| PublishError::Io(format!("{}: {e}", temp.display())))?
        .len();
    if size == 0 {
        return Err(PublishError::EmptyOutput(temp.to_path_buf()));
    }
    let dir = dest.parent().unwrap_or_else(|| Path::new("."));
    let lock = directory_lock(dir);
    let _guard = lock.lock().unwrap_or_else(|poison| poison.into_inner());
    ensure_dir(dir)?;
    if dest.exists() {
        return Err(PublishError::Io(format!(
            "{} already exists and this build never overwrites",
            dest.display()
        )));
    }
    match place_without_clobber(temp, dest) {
        Ok(()) => published(dest.to_path_buf(), true),
        Err(Place::Collision) => Err(PublishError::Io(format!(
            "{} appeared while publishing",
            dest.display()
        ))),
        Err(Place::Io(reason)) => Err(PublishError::Io(reason)),
    }
}

fn published(path: PathBuf, renamed: bool) -> Result<Published, PublishError> {
    let meta = fs::metadata(&path)?;
    let sha256 = hash::sha256_file(&path)?;
    let mtime_unix_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Ok(Published {
        path,
        sha256,
        size: meta.len(),
        mtime_unix_ms,
        renamed,
    })
}

enum Place {
    Collision,
    Io(String),
}

/// Move `temp` to `dest` without ever replacing an existing file.
///
/// `hard_link` is the fast path and it *is* the no-clobber check: it fails with
/// `AlreadyExists` on both Windows and Unix. The `create_new` copy is the
/// cross-volume / filesystem-without-links fallback, and it is equally atomic
/// about not clobbering.
fn place_without_clobber(temp: &Path, dest: &Path) -> Result<(), Place> {
    if dest.exists() {
        return Err(Place::Collision);
    }
    match fs::hard_link(temp, dest) {
        Ok(()) => {
            let _ = fs::remove_file(temp);
            sync_dir(dest);
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(Place::Collision),
        Err(_) => copy_without_clobber(temp, dest),
    }
}

fn copy_without_clobber(temp: &Path, dest: &Path) -> Result<(), Place> {
    let mut source = File::open(temp).map_err(|e| Place::Io(format!("{}: {e}", temp.display())))?;
    let mut sink = match OpenOptions::new().write(true).create_new(true).open(dest) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Err(Place::Collision),
        Err(e) => return Err(Place::Io(format!("{}: {e}", dest.display()))),
    };
    let copied = io::copy(&mut source, &mut sink).and_then(|_| sink.flush());
    if let Err(e) = copied.and_then(|_| sink.sync_all()) {
        drop(sink);
        let _ = fs::remove_file(dest);
        return Err(Place::Io(format!("writing {}: {e}", dest.display())));
    }
    drop(sink);
    let _ = fs::remove_file(temp);
    sync_dir(dest);
    Ok(())
}

/// Best-effort directory fsync so a rename survives a power cut on Unix.
/// Windows has no equivalent through `std`; `FlushFileBuffers` on the file is
/// what durability there relies on, which `sync_all` already did.
fn sync_dir(dest: &Path) {
    #[cfg(unix)]
    {
        if let Some(dir) = dest.parent() {
            if let Ok(handle) = File::open(dir) {
                let _ = handle.sync_all();
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = dest;
    }
}

/// Read a small file's bytes — used by tests and diagnostics, not by the job
/// path (which streams).
#[allow(dead_code)]
pub fn read_all(path: &Path) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir()
                .join(format!("ezc-publish-{tag}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn temp_file(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join("temp").join(name);
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

    #[test]
    fn first_publication_uses_the_plain_name_and_moves_the_bytes() {
        let dir = TempDir::new("plain");
        let out = dir.path().join("out");
        let temp = dir.temp_file("a.png", b"image-bytes");
        let mut reserved = BTreeSet::new();
        let published = publish_new(&temp, &out, "photo", "png", &mut reserved).unwrap();
        assert_eq!(published.path, out.join("photo.png"));
        assert!(!published.renamed);
        assert_eq!(published.size, 11);
        assert_eq!(published.sha256, hash::sha256_bytes(b"image-bytes"));
        assert_eq!(fs::read(&published.path).unwrap(), b"image-bytes");
        assert!(!temp.exists(), "the temp copy is consumed, not duplicated");
    }

    #[test]
    fn an_existing_file_is_never_replaced() {
        let dir = TempDir::new("clobber");
        let out = dir.path().join("out");
        fs::create_dir_all(&out).unwrap();
        fs::write(out.join("photo.png"), b"THE USER'S FILE").unwrap();
        let temp = dir.temp_file("a.png", b"new bytes");

        let published = publish_new(&temp, &out, "photo", "png", &mut BTreeSet::new()).unwrap();
        assert_eq!(published.path, out.join("photo-1.png"));
        assert!(published.renamed);
        assert_eq!(
            fs::read(out.join("photo.png")).unwrap(),
            b"THE USER'S FILE",
            "the original must survive byte for byte"
        );
    }

    #[test]
    fn a_zero_byte_output_is_refused_not_published() {
        let dir = TempDir::new("empty");
        let out = dir.path().join("out");
        let temp = dir.temp_file("a.png", b"");
        let err = publish_new(&temp, &out, "photo", "png", &mut BTreeSet::new()).unwrap_err();
        assert!(matches!(err, PublishError::EmptyOutput(_)));
        assert!(!out.join("photo.png").exists());
    }

    #[test]
    fn entries_claimed_earlier_in_the_batch_are_skipped() {
        let dir = TempDir::new("reserved");
        let out = dir.path().join("out");
        let mut reserved = BTreeSet::new();
        let first = publish_new(
            &dir.temp_file("1", b"one"),
            &out,
            "a",
            "webp",
            &mut reserved,
        )
        .unwrap();
        let second = publish_new(
            &dir.temp_file("2", b"two"),
            &out,
            "a",
            "webp",
            &mut reserved,
        )
        .unwrap();
        assert_eq!(first.path, out.join("a.webp"));
        assert_eq!(second.path, out.join("a-1.webp"));
    }

    #[test]
    fn concurrent_publishers_of_the_same_name_all_succeed_and_none_clobbers() {
        // The per-directory lock is the only thing standing between two jobs
        // and the same file name; 8 threads is enough to make a missing lock
        // fail loudly and repeatedly.
        let dir = TempDir::new("race");
        let out = dir.path().join("out");
        fs::create_dir_all(&out).unwrap();
        let mut handles = Vec::new();
        for i in 0..8u8 {
            let temp = dir.temp_file(&format!("t{i}"), format!("payload-{i}").as_bytes());
            let out = out.clone();
            handles.push(std::thread::spawn(move || {
                let mut reserved = BTreeSet::new();
                publish_new(&temp, &out, "photo", "jpg", &mut reserved).unwrap()
            }));
        }
        let mut paths: Vec<PathBuf> = handles
            .into_iter()
            .map(|h| h.join().unwrap().path)
            .collect();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), 8, "every publication must get its own name");

        // Every payload is still exactly one file with exactly its own bytes.
        let mut payloads: Vec<String> = fs::read_dir(&out)
            .unwrap()
            .map(|e| fs::read_to_string(e.unwrap().path()).unwrap())
            .collect();
        payloads.sort();
        let expected: Vec<String> = (0..8).map(|i| format!("payload-{i}")).collect();
        assert_eq!(payloads, expected);
    }

    #[test]
    fn ensure_dir_reports_exactly_what_it_created() {
        let dir = TempDir::new("mkdir");
        let nested = dir.path().join("a").join("b");
        let created = ensure_dir(&nested).unwrap();
        assert_eq!(
            created.len(),
            2,
            "both levels are newly created: {created:?}"
        );
        assert!(nested.is_dir());
        // Second call creates nothing, so undo has nothing to remove.
        assert!(ensure_dir(&nested).unwrap().is_empty());
    }

    #[test]
    fn undo_dirs_are_removed_only_while_empty() {
        let dir = TempDir::new("rmdir");
        let nested = dir.path().join("converted");
        fs::create_dir_all(&nested).unwrap();
        let created = vec![nested.to_string_lossy().to_string()];

        fs::write(nested.join("user-file.txt"), b"mine").unwrap();
        assert!(
            remove_created_dirs(&created).is_empty(),
            "a folder the user put something in is not ours to delete"
        );
        assert!(nested.is_dir());

        fs::remove_file(nested.join("user-file.txt")).unwrap();
        assert_eq!(remove_created_dirs(&created), vec![nested.clone()]);
        assert!(!nested.exists());
    }

    #[test]
    fn publish_exact_refuses_an_existing_destination() {
        let dir = TempDir::new("exact");
        let out = dir.path().join("out");
        fs::create_dir_all(&out).unwrap();
        let dest = out.join("report-Data.csv");
        fs::write(&dest, b"previous").unwrap();
        let temp = dir.temp_file("csv", b"new");
        let err = publish_exact(&temp, &dest).unwrap_err();
        assert!(format!("{err}").contains("never overwrites"));
        assert_eq!(fs::read(&dest).unwrap(), b"previous");
    }

    #[test]
    fn a_job_id_cannot_escape_the_temp_root() {
        let area = TempArea::new("/tmp/ezc-temp-root");
        assert_eq!(
            area.job_dir("j_000001"),
            PathBuf::from("/tmp/ezc-temp-root/j_000001")
        );
        assert_eq!(area.job_dir(""), PathBuf::from("/tmp/ezc-temp-root/job"));
        // A traversal attempt becomes a single, harmless name inside the root.
        let hostile = area.job_dir("../../etc");
        assert_eq!(hostile.parent(), Some(Path::new("/tmp/ezc-temp-root")));
        let name = hostile.file_name().unwrap().to_string_lossy().to_string();
        assert!(!name.contains("..") && !name.contains('/') && !name.contains('\\'));
        assert!(!name.is_empty());
    }
}
