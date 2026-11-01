//! Output path derivation, collision suffixing and path admission.
//!
//! `ARCHITECTURE.md` §9 assigns this module to `C4`; `U2` implements the parts
//! the window needs to *show* the rules before a conversion runs — where the
//! result will land, what it will be called, and whether a collision will
//! rename it. `C4` owns publication (temp dir, hash, rename under a directory
//! lock) and the undo journal; the pure rules here are shared so the two
//! surfaces cannot diverge (§11).
//!
//! `SPEC.md` §5 rules encoded here:
//!
//! - same folder by default, or a chosen folder;
//! - batch jobs (N ≥ `batch_threshold`) go to a subfolder, unless turned off;
//! - collisions never overwrite: `stem-1.ext`, `stem-2.ext`, …

use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::model::settings::{is_reserved_windows_name, sanitize_subfolder, Settings};

/// Longest path accepted at admission on Windows (`ARCHITECTURE.md` §9.5).
pub const MAX_PATH_LEN: usize = 260;

/// Why a dropped path was refused before any work started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathProblem {
    ReservedName(String),
    TrailingDotOrSpace(String),
    TooLong(usize),
    NotAbsolute,
}

impl std::fmt::Display for PathProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathProblem::ReservedName(name) => write!(
                f,
                "{name:?} is a reserved device name on Windows and can't be written"
            ),
            PathProblem::TrailingDotOrSpace(name) => {
                write!(
                    f,
                    "{name:?} ends with a dot or a space, which Windows trims"
                )
            }
            PathProblem::TooLong(len) => {
                write!(
                    f,
                    "the path is {len} characters; the limit is {MAX_PATH_LEN}"
                )
            }
            PathProblem::NotAbsolute => write!(f, "the path is not absolute"),
        }
    }
}

/// Admission rule for a path the user supplied (`ARCHITECTURE.md` §9.5).
///
/// Applied on every platform: the app ships on Windows, and a name that cannot
/// be created there is refused up front rather than after a conversion.
pub fn validate_input_path(path: &Path) -> Result<(), PathProblem> {
    if !path.is_absolute() {
        return Err(PathProblem::NotAbsolute);
    }
    let raw = path.to_string_lossy();
    if raw.chars().count() > MAX_PATH_LEN {
        return Err(PathProblem::TooLong(raw.chars().count()));
    }
    if let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_string()) {
        if is_reserved_windows_name(&name) {
            return Err(PathProblem::ReservedName(name));
        }
        if name.ends_with('.') || name.ends_with(' ') {
            return Err(PathProblem::TrailingDotOrSpace(name));
        }
    }
    Ok(())
}

/// Directory a result for `input` is written to (`SPEC.md` §5, `§9.1`).
///
/// `batch_len` is the number of files in the batch this input belongs to.
pub fn resolve_out_dir(input: &Path, settings: &Settings, batch_len: usize) -> PathBuf {
    let base = input
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let root = match settings.output.location.as_str() {
        "chosen_folder" => settings
            .output
            .chosen_folder
            .as_deref()
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or(base),
        _ => base,
    };

    let batch_rule = settings.output.use_batch_subfolder
        && batch_len as u32 >= settings.output.batch_threshold
        && settings.output.location != "chosen_folder";

    if batch_rule {
        root.join(sanitize_subfolder(&settings.output.batch_subfolder))
    } else {
        root
    }
}

/// Stem used for the output file (`ARCHITECTURE.md` §9.2).
pub fn output_stem(input: &Path) -> String {
    input
        .file_stem()
        .map(|s| s.to_string_lossy().trim().to_string())
        .filter(|s| !s.is_empty() && !s.starts_with('.'))
        .unwrap_or_else(|| "converted".to_string())
}

/// First free `<stem>[-n].<ext>` in `dir`, never overwriting.
///
/// `reserved` holds names already claimed by other queued items, so two files
/// with the same stem in one batch get `-1` / `-2` rather than racing for the
/// same name (`ARCHITECTURE.md` §9.3).
pub fn next_free_name(dir: &Path, stem: &str, ext: &str, reserved: &BTreeSet<PathBuf>) -> PathBuf {
    let ext = ext.trim_start_matches('.');
    let mut candidate = dir.join(format!("{stem}.{ext}"));
    let mut n = 0u32;
    while candidate.exists() || reserved.contains(&candidate) {
        n += 1;
        candidate = dir.join(format!("{stem}-{n}.{ext}"));
    }
    candidate
}

/// One row of the window's output preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewEntry {
    pub input: String,
    pub out_dir: String,
    pub out_path: String,
    /// `true` when the plain `<stem>.<ext>` name was taken and a suffix was added.
    pub renamed: bool,
    pub out_dir_exists: bool,
}

/// What every file in the batch will become, in queue order.
///
/// Uses real disk state plus intra-batch reservations, so the preview shows the
/// same `-1` / `-2` the writer will produce.
pub fn preview(inputs: &[PathBuf], target: &str, settings: &Settings) -> Vec<PreviewEntry> {
    preview_batch(inputs, target, settings, inputs.len())
}

/// Same as [`preview`] but declares the size of the whole batch explicitly, so
/// a single queued row can be previewed against the batch rule.
pub fn preview_batch(
    inputs: &[PathBuf],
    target: &str,
    settings: &Settings,
    batch_len: usize,
) -> Vec<PreviewEntry> {
    let mut reserved: BTreeSet<PathBuf> = BTreeSet::new();
    let mut out = Vec::with_capacity(inputs.len());
    let batch_len = batch_len.max(inputs.len());

    for input in inputs {
        let dir = resolve_out_dir(input, settings, batch_len);
        let stem = output_stem(input);
        let chosen = next_free_name(&dir, &stem, target, &reserved);
        let plain = dir.join(format!("{stem}.{}", target.trim_start_matches('.')));
        reserved.insert(chosen.clone());
        out.push(PreviewEntry {
            input: input.to_string_lossy().to_string(),
            out_dir_exists: dir.is_dir(),
            out_dir: dir.to_string_lossy().to_string(),
            out_path: chosen.to_string_lossy().to_string(),
            renamed: chosen != plain,
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// An absolute path, written the way this platform spells one.
    ///
    /// `Path::new("/out")` is *rooted but not absolute* on Windows — no drive or
    /// UNC prefix, so `is_absolute()` is `false` there. Test data that hard-codes
    /// POSIX paths therefore exercised the "not absolute" fallback instead of the
    /// rule under test. Both arms are type-checked on every platform because
    /// `cfg!` is a runtime constant, so neither can rot.
    fn abs(p: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(format!("C:\\{}", p.replace('/', "\\")))
        } else {
            PathBuf::from(format!("/{p}"))
        }
    }

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir =
                std::env::temp_dir().join(format!("ezc-name-{tag}-{}-{nanos}", std::process::id()));
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

    #[test]
    fn same_folder_is_the_default() {
        let s = Settings::default();
        let out = resolve_out_dir(Path::new("/photos/holiday/img.heic"), &s, 1);
        assert_eq!(out, PathBuf::from("/photos/holiday"));
    }

    #[test]
    fn a_batch_of_three_goes_to_the_batch_folder() {
        let s = Settings::default();
        let root = Path::new("/photos/holiday/a.png");
        assert_eq!(
            resolve_out_dir(root, &s, 2),
            PathBuf::from("/photos/holiday")
        );
        assert_eq!(
            resolve_out_dir(root, &s, 3),
            PathBuf::from("/photos/holiday/converted")
        );
        assert_eq!(
            resolve_out_dir(root, &s, 9),
            PathBuf::from("/photos/holiday/converted")
        );
    }

    #[test]
    fn batch_rule_can_be_turned_off_or_renamed() {
        let mut s = Settings::default();
        s.output.use_batch_subfolder = false;
        assert_eq!(
            resolve_out_dir(Path::new("/x/a.png"), &s, 5),
            PathBuf::from("/x")
        );

        let mut s = Settings::default();
        s.output.batch_subfolder = "out".into();
        assert_eq!(
            resolve_out_dir(Path::new("/x/a.png"), &s, 5),
            PathBuf::from("/x/out")
        );
    }

    #[test]
    fn batch_threshold_is_respected() {
        let mut s = Settings::default();
        s.output.batch_threshold = 10;
        assert_eq!(
            resolve_out_dir(Path::new("/x/a.png"), &s, 9),
            PathBuf::from("/x")
        );
        assert_eq!(
            resolve_out_dir(Path::new("/x/a.png"), &s, 10),
            PathBuf::from("/x/converted")
        );
    }

    #[test]
    fn chosen_folder_wins_and_skips_the_batch_folder() {
        let mut s = Settings::default();
        s.output.location = "chosen_folder".into();
        let chosen = abs("out");
        s.output.chosen_folder = Some(chosen.to_string_lossy().into_owned());
        assert_eq!(
            resolve_out_dir(&abs("x").join("a.png"), &s, 5),
            chosen,
            "a batch of 5 still goes to the chosen folder, not the batch subfolder"
        );
    }

    #[test]
    fn collisions_append_a_suffix_and_never_overwrite() {
        let dir = TempDir::new("collide");
        fs::write(dir.path().join("photo.png"), b"a").unwrap();
        fs::write(dir.path().join("photo-1.png"), b"b").unwrap();

        let reserved = BTreeSet::new();
        let name = next_free_name(dir.path(), "photo", "png", &reserved);
        assert_eq!(name, dir.path().join("photo-2.png"));
        assert!(
            dir.path().join("photo.png").exists(),
            "original must survive"
        );
    }

    #[test]
    fn two_inputs_with_the_same_stem_get_different_names() {
        let dir = TempDir::new("batch");
        fs::write(dir.path().join("a.png"), b"x").unwrap();
        fs::write(dir.path().join("a.jpg"), b"y").unwrap();
        let mut s = Settings::default();
        s.output.use_batch_subfolder = false;

        let previews = preview(
            &[dir.path().join("a.png"), dir.path().join("a.jpg")],
            "webp",
            &s,
        );
        assert_eq!(previews.len(), 2);
        assert_ne!(previews[0].out_path, previews[1].out_path);
        assert!(!previews[0].renamed);
        assert!(previews[1].renamed);
        assert!(previews[1].out_path.ends_with("a-1.webp"));
    }

    #[test]
    fn preview_reports_the_directory_it_expects_to_create() {
        let dir = TempDir::new("mkdir");
        fs::write(dir.path().join("a.png"), b"x").unwrap();
        let mut s = Settings::default();
        s.output.use_batch_subfolder = true;
        let previews = preview(&[dir.path().join("a.png")], "jpg", &s);
        assert_eq!(previews[0].out_dir, dir.path().to_string_lossy());
        assert!(previews[0].out_dir_exists);

        let mut s = Settings::default();
        s.output.batch_threshold = 1;
        let previews = preview(&[dir.path().join("a.png")], "jpg", &s);
        assert!(!previews[0].out_dir_exists);
        assert!(previews[0].out_dir.ends_with("converted"));
    }

    #[test]
    fn reserved_names_and_trailing_dots_are_refused() {
        assert_eq!(
            validate_input_path(&abs("tmp").join("CON")),
            Err(PathProblem::ReservedName("CON".into()))
        );
        assert_eq!(
            validate_input_path(&abs("tmp").join("report.")),
            Err(PathProblem::TrailingDotOrSpace("report.".into()))
        );
        assert_eq!(
            validate_input_path(Path::new("relative/file.png")),
            Err(PathProblem::NotAbsolute)
        );
        let long = abs("tmp").join("a".repeat(300));
        assert!(matches!(
            validate_input_path(&long),
            Err(PathProblem::TooLong(_))
        ));
        assert_eq!(validate_input_path(&abs("tmp").join("fine.png")), Ok(()));
    }

    #[test]
    fn output_stem_falls_back_when_there_is_no_name() {
        assert_eq!(output_stem(Path::new("/tmp/report.docx")), "report");
        assert_eq!(output_stem(Path::new("/tmp/.hidden")), "converted");
    }
}
