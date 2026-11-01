//! `engine/soffice.rs` — LibreOffice as a sidecar: the argv, the launcher rule,
//! the profile URL, and the warm-up call.
//!
//! Implements `ARCHITECTURE.md` §6.2. Everything here is either pure (argv as
//! data, URL construction, route table) or a thin call through
//! [`super::proc::run`] — this module never constructs a `Command`.
//!
//! Three measured facts shape it, none of them guessable:
//!
//! 1. **Which executable.** Windows must spawn `soffice.com`; POSIX must spawn
//!    `soffice`. `soffice.bin` exits **81** with empty stdout/stderr on a
//!    profile that has never been bootstrapped — which is exactly the first run
//!    of a freshly installed app (§6.2.1, control N1).
//! 2. **The profile is a URL.** `-env:UserInstallation` wants
//!    `file:///C:/Users/…/profile`. The spelling `file://` + `C:\…` is accepted
//!    **without an error** and quietly bootstraps a *different* profile, so the
//!    cold/warm contract would be re-paid on every job (§6.2.2).
//! 3. **`rc=0` carries no information.** A text file renamed `.docx` converts
//!    to a valid-looking 12,388 B PDF at exit 0 with empty stderr (§6.2 N3),
//!    which is why detection runs first and every output is validated.
//!
//! The seven convert targets below are literals from `spikes/s3/EVIDENCE.md`;
//! a filter is never assembled from user input.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::proc::{self, EnginePaths, Invocation, Outcome, Program};
use crate::model::types::{CanonicalType, TargetFormat};

/// The version `SPEC.md` §6 pins. `soffice --version` is the identity check
/// (§6.7); the worker reports a mismatch instead of converting.
pub const VERSION_PIN: &str = "LibreOffice 26.8.2";

/// The app-owned profile directory name, under the app cache dir.
pub const PROFILE_DIR_NAME: &str = "soffice-profile";

// ---------------------------------------------------------------------------
// Route table (§6.2.3) — the seven measured directions, nothing invented
// ---------------------------------------------------------------------------

/// One document conversion: the literal `--convert-to` value, the optional
/// `--infilter`, and the evidence line that proves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocRoute {
    /// Literal passed after `--convert-to`. Never built from user text.
    pub convert_to: &'static str,
    /// Literal passed as `--infilter=<…>` when the input needs a filter hint.
    pub infilter: Option<&'static str>,
    /// Evidence id in `spikes/s3/EVIDENCE.md` / `evidence/c3-document.md`.
    pub evidence: &'static str,
}

/// The route for `from → to`, or `None` when this build has no measured proof.
///
/// Returning `None` is the contract: `Planned` and `Cut` cells in
/// `model::matrix` are not routable here, so a caller cannot reach LibreOffice
/// with an argv nobody measured.
pub fn route(from: CanonicalType, to: TargetFormat) -> Option<DocRoute> {
    use CanonicalType as C;
    use TargetFormat as T;
    Some(match (from, to) {
        // D1 — measured 26,460 B (Linux) / 39,946 B (Windows), 1 page 612x792 pt.
        (C::Docx, T::Pdf) => DocRoute {
            convert_to: "pdf:writer_pdf_Export",
            infilter: None,
            evidence: "D1",
        },
        // D2 — 389 B Linux / 404 B Windows, real Markdown (heading, table).
        (C::Docx, T::Md) => DocRoute {
            convert_to: "md:Markdown",
            infilter: None,
            evidence: "D2",
        },
        // D3 — 4,064 B, `<h1>`, `<table>`, 13 `<p>`, `café` intact.
        (C::Docx, T::Html) => DocRoute {
            convert_to: "html:HTML (StarWriter)",
            infilter: None,
            evidence: "D3",
        },
        // D4 — 5,802 B xlsx, numeric cells, UTF-8 via the filter options. The
        // infilter token is the measured CSV import contract (comma, quote,
        // charset 76 = UTF-8, first row not a header).
        (C::Csv, T::Xlsx) => DocRoute {
            convert_to: "xlsx:Calc MS Excel 2007 XML",
            infilter: Some("csv:44,34,76,1,,0,false,true,true"),
            evidence: "D4",
        },
        // D5 — 147 B csv, and the name is `<stem>-<sheet>.csv`, not `<stem>.csv`.
        (C::Xlsx, T::Csv) => DocRoute {
            convert_to: "csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true,false,false,1",
            infilter: None,
            evidence: "D5",
        },
        // D6 — 6,863 B docx, a real 2x2 Word table.
        (C::Html, T::Docx) => DocRoute {
            convert_to: "docx:MS Word 2007 XML",
            infilter: Some("HTML (StarWriter)"),
            evidence: "D6",
        },
        // D7 — 33,957 B pdf, 2 pages 720x540 pt.
        (C::Pptx, T::Pdf) => DocRoute {
            convert_to: "pdf:impress_pdf_Export",
            infilter: None,
            evidence: "D7",
        },
        _ => return None,
    })
}

/// Every measured document route, for the matrix cross-check and for tests.
pub fn proven_routes() -> &'static [(CanonicalType, TargetFormat)] {
    &[
        (CanonicalType::Docx, TargetFormat::Pdf),
        (CanonicalType::Docx, TargetFormat::Md),
        (CanonicalType::Docx, TargetFormat::Html),
        (CanonicalType::Csv, TargetFormat::Xlsx),
        (CanonicalType::Xlsx, TargetFormat::Csv),
        (CanonicalType::Html, TargetFormat::Docx),
        (CanonicalType::Pptx, TargetFormat::Pdf),
    ]
}

/// Is this canonical type handled by the document engine at all?
pub fn is_document(ty: CanonicalType) -> bool {
    matches!(
        ty,
        CanonicalType::Docx
            | CanonicalType::Xlsx
            | CanonicalType::Pptx
            | CanonicalType::Doc
            | CanonicalType::Xls
            | CanonicalType::Ppt
            | CanonicalType::Rtf
            | CanonicalType::Csv
            | CanonicalType::Markdown
            | CanonicalType::Html
            | CanonicalType::Txt
    )
}

// ---------------------------------------------------------------------------
// Launcher + profile URL (§6.2.1, §6.2.2)
// ---------------------------------------------------------------------------

/// The platform whose spelling a rule produces. Explicit so the Windows rules
/// are unit-testable from a Linux CI box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Posix,
}

/// This build's platform.
pub fn native_platform() -> Platform {
    if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Posix
    }
}

/// `soffice.com` on Windows, `soffice` on POSIX. Never `.exe`, never `.bin`.
pub fn launcher_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Windows => "soffice.com",
        Platform::Posix => "soffice",
    }
}

/// The console launcher inside a LibreOffice `program/` directory.
pub fn launcher(dir: &Path, platform: Platform) -> PathBuf {
    dir.join(launcher_name(platform))
}

/// Strip a Windows verbatim prefix (`\\?\`) that `std::fs::canonicalize`
/// produces, so paths handed to LibreOffice look like ordinary paths.
pub fn plain_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let stripped = text
        .strip_prefix(r"\\?\")
        .or_else(|| text.strip_prefix("//?/"))
        .unwrap_or(&text);
    PathBuf::from(stripped)
}

/// `-env:UserInstallation` value: a `file:` **URL**, not a path.
///
/// POSIX: `file://` + `/abs/path` → `file:///abs/path`.
/// Windows: `file://` + `/C:/abs/path` → `file:///C:/abs/path`, forward
/// slashes, drive letter. A bare `C:\…` makes LibreOffice treat the value as a
/// relative URL and bootstrap a different profile without an error.
pub fn file_url(path: &Path, platform: Platform) -> String {
    let mut text = plain_path(path).to_string_lossy().replace('\\', "/");
    if platform == Platform::Windows && !text.starts_with('/') {
        text.insert(0, '/');
    }
    if platform == Platform::Posix && !text.starts_with('/') {
        text.insert(0, '/');
    }
    format!("file://{text}")
}

/// The app-owned LibreOffice profile (`ARCHITECTURE.md` §6.2 operational
/// contract: one warm profile per worker, serialised, under the app cache).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub dir: PathBuf,
}

impl Profile {
    pub fn under(cache_dir: &Path) -> Self {
        Self {
            dir: cache_dir.join(PROFILE_DIR_NAME),
        }
    }

    pub fn url(&self, platform: Platform) -> String {
        file_url(&self.dir, platform)
    }

    /// LibreOffice writes `user/` into a profile the first time it starts.
    /// Presence of that directory is the cheap "this profile is warm" signal.
    pub fn bootstrapped(&self) -> bool {
        self.dir.join("user").is_dir()
    }

    /// Create the profile directory, or recreate it when a previous run left it
    /// in a state the engine refuses (a stale lock is the measured cause).
    pub fn reset(&self) -> std::io::Result<()> {
        if self.dir.exists() {
            std::fs::remove_dir_all(&self.dir)?;
        }
        std::fs::create_dir_all(&self.dir)
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)
    }
}

// ---------------------------------------------------------------------------
// argv
// ---------------------------------------------------------------------------

/// One LibreOffice batch: a single route, one outdir, N inputs.
///
/// Batching is measured (`ARCHITECTURE.md` §6.2 operational contract): 0.20 s
/// per file batched vs 0.52 s per file when each file spawns its own process.
/// A batch is only legal when every file shares the route and the outdir.
#[derive(Debug, Clone, Copy)]
pub struct BatchSpec<'a> {
    pub route: DocRoute,
    pub launcher: &'a Path,
    pub profile_url: &'a str,
    pub outdir: &'a Path,
    pub inputs: &'a [PathBuf],
}

/// The measured envelope, in the measured order (§6.2.1).
pub fn batch_argv(spec: &BatchSpec<'_>) -> Vec<OsString> {
    let mut argv: Vec<OsString> = Vec::with_capacity(8 + spec.inputs.len());
    argv.push(spec.launcher.as_os_str().to_os_string());
    argv.push("--headless".into());
    argv.push("--norestore".into());
    argv.push("--nolockcheck".into());
    argv.push("--nodefault".into());
    argv.push("--nologo".into());
    argv.push(format!("-env:UserInstallation={}", spec.profile_url).into());
    if let Some(filter) = spec.route.infilter {
        argv.push(format!("--infilter={filter}").into());
    }
    argv.push("--convert-to".into());
    argv.push(spec.route.convert_to.into());
    argv.push("--outdir".into());
    argv.push(spec.outdir.as_os_str().to_os_string());
    for input in spec.inputs {
        argv.push(plain_path(input).into_os_string());
    }
    argv
}

/// `--terminate_after_init`: bootstraps the profile and exits without opening a
/// document. This is the startup warm-up — the measured install-cold first
/// conversion is 5,111 ms on Windows, and that cost must not land on the user's
/// first job (§6.2 operational contract).
pub fn warm_up_argv(launcher: &Path, profile_url: &str) -> Vec<OsString> {
    vec![
        launcher.as_os_str().to_os_string(),
        "--headless".into(),
        "--norestore".into(),
        "--nolockcheck".into(),
        "--nodefault".into(),
        "--nologo".into(),
        format!("-env:UserInstallation={profile_url}").into(),
        "--terminate_after_init".into(),
    ]
}

impl BatchSpec<'_> {
    /// Spawn-ready command for [`super::proc::run`].
    pub fn invocation(&self, cwd: &Path) -> Invocation {
        Invocation {
            program: Program::Soffice,
            args: batch_argv(self),
            cwd: cwd.to_path_buf(),
        }
    }
}

/// Run the warm-up. The caller decides whether a failure is fatal: an app that
/// cannot warm its profile should still try the user's job (§6.7 — never a
/// silent fallback, but this is a performance call, not a capability one).
pub fn warm_up(
    paths: &EnginePaths,
    profile: &Profile,
    cwd: &Path,
) -> std::io::Result<Outcome> {
    profile.ensure()?;
    let invocation = Invocation {
        program: Program::Soffice,
        args: warm_up_argv(&paths.soffice, &profile.url(native_platform())),
        cwd: cwd.to_path_buf(),
    };
    proc::run(paths, &invocation)
}

/// `soffice --version` text, for the §6.7 identity check.
pub fn version_argv(launcher: &Path) -> Vec<OsString> {
    vec![
        launcher.as_os_str().to_os_string(),
        "--headless".into(),
        "--version".into(),
    ]
}

/// Does the version string satisfy the pin? `LibreOffice 26.8.2.2 9dea406ab`
/// passes for a `26.8.2` pin (the build hash is not part of the version);
/// `26.7.x` and `26.8.20` do not.
pub fn version_matches(reported: &str, pin: &str) -> bool {
    fn version_token(text: &str) -> Option<&str> {
        text.split_whitespace()
            .find(|token| token.starts_with(|c: char| c.is_ascii_digit()))
    }
    let (Some(want), Some(have)) = (version_token(pin), version_token(reported)) else {
        return false;
    };
    have == want || have.starts_with(&format!("{want}."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv_strings(argv: &[OsString]) -> Vec<String> {
        argv.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    /// The seven literals are copied from `spikes/s3/EVIDENCE.md`. A typo here
    /// is a wrong conversion, so each is asserted character for character.
    #[test]
    fn the_seven_measured_filters_are_literals_from_the_transcript() {
        let cases = [
            (CanonicalType::Docx, TargetFormat::Pdf, "pdf:writer_pdf_Export", None, "D1"),
            (CanonicalType::Docx, TargetFormat::Md, "md:Markdown", None, "D2"),
            (
                CanonicalType::Docx,
                TargetFormat::Html,
                "html:HTML (StarWriter)",
                None,
                "D3",
            ),
            (
                CanonicalType::Csv,
                TargetFormat::Xlsx,
                "xlsx:Calc MS Excel 2007 XML",
                Some("csv:44,34,76,1,,0,false,true,true"),
                "D4",
            ),
            (
                CanonicalType::Xlsx,
                TargetFormat::Csv,
                "csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true,false,false,1",
                None,
                "D5",
            ),
            (
                CanonicalType::Html,
                TargetFormat::Docx,
                "docx:MS Word 2007 XML",
                Some("HTML (StarWriter)"),
                "D6",
            ),
            (CanonicalType::Pptx, TargetFormat::Pdf, "pdf:impress_pdf_Export", None, "D7"),
        ];
        for (from, to, convert_to, infilter, evidence) in cases {
            let r = route(from, to).unwrap_or_else(|| panic!("{from:?} -> {to:?} not routed"));
            assert_eq!(r.convert_to, convert_to);
            assert_eq!(r.infilter, infilter);
            assert_eq!(r.evidence, evidence);
        }
        assert_eq!(proven_routes().len(), cases.len());
    }

    #[test]
    fn unmeasured_directions_have_no_route() {
        // Legacy Office, rtf, txt and the cross-family spreadsheet cases are
        // `Planned`/`Cut` in the matrix. A route here would let the wheel offer
        // an argv nobody measured.
        for (from, to) in [
            (CanonicalType::Doc, TargetFormat::Pdf),
            (CanonicalType::Xls, TargetFormat::Csv),
            (CanonicalType::Ppt, TargetFormat::Pdf),
            (CanonicalType::Rtf, TargetFormat::Pdf),
            (CanonicalType::Docx, TargetFormat::Txt),
            (CanonicalType::Xlsx, TargetFormat::Pdf),
            (CanonicalType::Pptx, TargetFormat::Md),
            (CanonicalType::Csv, TargetFormat::Txt),
            (CanonicalType::Txt, TargetFormat::Pdf),
            // The measured silent-garbage trap: text named .docx never gets a
            // document route, because detection says Txt (§6.2 N3).
            (CanonicalType::Txt, TargetFormat::Docx),
        ] {
            assert!(route(from, to).is_none(), "{from:?} -> {to:?} must not be routed");
        }
    }

    #[test]
    fn launcher_is_platform_decided_and_never_soffice_bin() {
        assert_eq!(launcher_name(Platform::Windows), "soffice.com");
        assert_eq!(launcher_name(Platform::Posix), "soffice");
        for platform in [Platform::Windows, Platform::Posix] {
            let name = launcher(Path::new("/lo/program"), platform);
            assert_ne!(name.file_name().unwrap(), "soffice.bin");
            assert_ne!(name.file_name().unwrap(), "soffice.exe");
            assert!(name.starts_with("/lo/program"));
        }
        let windows = launcher(Path::new(r"C:\Program Files\LibreOffice\program"), Platform::Windows);
        assert_eq!(windows.file_name().unwrap(), "soffice.com");
    }

    #[test]
    fn profile_url_is_a_file_url_on_both_platforms() {
        assert_eq!(
            file_url(Path::new("/home/ryo/.cache/ez/profile"), Platform::Posix),
            "file:///home/ryo/.cache/ez/profile"
        );
        assert_eq!(
            file_url(Path::new(r"C:\Users\ryo\AppData\Local\ez\profile"), Platform::Windows),
            "file:///C:/Users/ryo/AppData/Local/ez/profile"
        );
        // A verbatim path (what canonicalize() returns on Windows) must not leak
        // `\\?\` into the URL.
        assert_eq!(
            file_url(Path::new(r"\\?\C:\Users\ryo\p"), Platform::Windows),
            "file:///C:/Users/ryo/p"
        );
    }

    #[test]
    fn the_windows_url_is_not_a_naive_concatenation() {
        // The measured failure mode: `file://` + `C:\…` is accepted silently and
        // bootstraps a *different* profile (§6.2.2 point 1).
        let raw = Path::new(r"C:\p");
        let naive = format!("file://{}", raw.display());
        assert_eq!(naive, r"file://C:\p");
        assert_ne!(file_url(raw, Platform::Windows), naive);
        assert!(file_url(raw, Platform::Windows).starts_with("file:///"));
    }

    #[test]
    fn batch_argv_follows_the_measured_order() {
        let inputs = vec![
            PathBuf::from("/data/one.docx"),
            PathBuf::from("/data/two.docx"),
        ];
        let spec = BatchSpec {
            route: route(CanonicalType::Docx, TargetFormat::Pdf).unwrap(),
            launcher: Path::new("/lo/soffice"),
            profile_url: "file:///tmp/profile",
            outdir: Path::new("/tmp/out"),
            inputs: &inputs,
        };
        let argv = argv_strings(&batch_argv(&spec));
        assert_eq!(argv[0], "/lo/soffice");
        for flag in ["--headless", "--norestore", "--nolockcheck", "--nodefault", "--nologo"] {
            assert!(argv.contains(&flag.to_string()), "missing {flag}: {argv:?}");
        }
        assert!(argv.contains(&"-env:UserInstallation=file:///tmp/profile".to_string()));
        let convert = argv.iter().position(|a| a == "--convert-to").unwrap();
        assert_eq!(argv[convert + 1], "pdf:writer_pdf_Export");
        let outdir = argv.iter().position(|a| a == "--outdir").unwrap();
        assert_eq!(argv[outdir + 1], "/tmp/out");
        // Inputs are last, so a file name can never be parsed as a flag.
        assert_eq!(&argv[argv.len() - 2..], &["/data/one.docx", "/data/two.docx"]);
        // The measured envelope has no --infilter unless the route needs one.
        assert!(!argv.iter().any(|a| a.starts_with("--infilter=")));
    }

    #[test]
    fn infilter_appears_only_when_the_route_measured_one() {
        let inputs = vec![PathBuf::from("/data/one.csv")];
        let spec = BatchSpec {
            route: route(CanonicalType::Csv, TargetFormat::Xlsx).unwrap(),
            launcher: Path::new("/lo/soffice"),
            profile_url: "file:///tmp/p",
            outdir: Path::new("/tmp/out"),
            inputs: &inputs,
        };
        let argv = argv_strings(&batch_argv(&spec));
        let filter = "--infilter=csv:44,34,76,1,,0,false,true,true";
        assert!(argv.contains(&filter.to_string()));
        let infilter = argv.iter().position(|a| a == filter).unwrap();
        let convert = argv.iter().position(|a| a == "--convert-to").unwrap();
        assert!(infilter < convert, "infilter must precede --convert-to");
    }

    #[test]
    fn a_batch_is_one_spawn_with_every_input() {
        let inputs: Vec<PathBuf> = (0..5)
            .map(|i| PathBuf::from(format!("/data/f{i}.docx")))
            .collect();
        let spec = BatchSpec {
            route: route(CanonicalType::Docx, TargetFormat::Md).unwrap(),
            launcher: Path::new("/lo/soffice"),
            profile_url: "file:///tmp/p",
            outdir: Path::new("/tmp/out"),
            inputs: &inputs,
        };
        let argv = argv_strings(&batch_argv(&spec));
        let tail: Vec<&String> = argv.iter().skip(argv.len() - inputs.len()).collect();
        assert_eq!(tail.len(), 5, "five inputs must produce one argv, not five runs");
    }

    #[test]
    fn warm_up_argv_bootstraps_and_exits() {
        let argv = argv_strings(&warm_up_argv(Path::new("/lo/soffice"), "file:///tmp/p"));
        assert_eq!(argv[0], "/lo/soffice");
        assert!(argv.iter().any(|a| a == "--terminate_after_init"));
        assert!(argv.contains(&"-env:UserInstallation=file:///tmp/p".to_string()));
    }

    #[test]
    fn version_pin_accepts_the_patch_suffix_only() {
        assert!(version_matches("LibreOffice 26.8.2.2 9dea406ab", "LibreOffice 26.8.2"));
        assert!(version_matches("LibreOffice 26.8.2", "LibreOffice 26.8.2"));
        assert!(!version_matches("LibreOffice 26.7.9", "LibreOffice 26.8.2"));
        assert!(!version_matches("LibreOffice 26.8.20", "LibreOffice 26.8.2"));
    }

    #[test]
    fn every_route_is_in_the_matrix_as_proven() {
        // §0: a route exists here only when the matrix says `Proven`. This test
        // is the guard against the engine layer getting ahead of the evidence.
        for (from, to) in proven_routes() {
            let row = crate::model::matrix::routes_for(*from);
            let cell = row
                .iter()
                .find(|r| r.target == *to)
                .unwrap_or_else(|| panic!("{from:?} -> {to:?} missing from the matrix"));
            assert!(
                matches!(cell.status, crate::model::types::RouteStatus::Proven { .. }),
                "{from:?} -> {to:?} is routed but not Proven in the matrix"
            );
        }
    }
}
