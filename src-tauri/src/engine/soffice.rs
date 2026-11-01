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
use std::time::Instant;

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
    let writer = |convert_to: &'static str,
                  infilter: Option<&'static str>,
                  evidence: &'static str|
     -> DocRoute {
        DocRoute {
            convert_to,
            infilter,
            evidence,
        }
    };
    Some(match (from, to) {
        // ---- D1-D3: docx, the S3 transcript -----------------------------
        (C::Docx, T::Pdf) => writer("pdf:writer_pdf_Export", None, "D1"),
        (C::Docx, T::Md) => writer("md:Markdown", None, "D2"),
        (C::Docx, T::Html) => writer("html:HTML (StarWriter)", None, "D3"),
        // ---- D4/D5: spreadsheets ----------------------------------------
        (C::Csv, T::Xlsx) => writer(
            "xlsx:Calc MS Excel 2007 XML",
            Some("csv:44,34,76,1,,0,false,true,true"),
            "D4",
        ),
        (C::Xlsx, T::Csv) => writer(
            "csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true,false,false,1",
            None,
            "D5",
        ),
        // ---- D6/D7: html in, impress out --------------------------------
        (C::Html, T::Docx) => writer(
            "docx:MS Word 2007 XML",
            Some("HTML (StarWriter)"),
            "D6",
        ),
        (C::Pptx, T::Pdf) => writer("pdf:impress_pdf_Export", None, "D7"),

        // ---- legacy .doc (C3, measured 2026-11-01 on both legs) ---------
        (C::Doc, T::Pdf) => writer("pdf:writer_pdf_Export", None, "C3-DOC-PDF"),
        (C::Doc, T::Md) => writer("md:Markdown", None, "C3-DOC-MD"),
        (C::Doc, T::Html) => writer("html:HTML (StarWriter)", None, "C3-DOC-HTML"),
        (C::Doc, T::Docx) => writer("docx:MS Word 2007 XML", None, "C3-DOC-DOCX"),
        (C::Doc, T::Txt) => writer("txt:Text (encoded):UTF8", None, "C3-DOC-TXT"),
        // ---- legacy .xls ------------------------------------------------
        (C::Xls, T::Pdf) => writer("pdf:calc_pdf_Export", None, "C3-XLS-PDF"),
        (C::Xls, T::Csv) => writer(
            "csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true,false,false,1",
            None,
            "C3-XLS-CSV",
        ),
        (C::Xls, T::Xlsx) => writer("xlsx:Calc MS Excel 2007 XML", None, "C3-XLS-XLSX"),
        (C::Xls, T::Html) => writer("html:HTML (StarCalc)", None, "C3-XLS-HTML"),
        // ---- legacy .ppt ------------------------------------------------
        (C::Ppt, T::Pdf) => writer("pdf:impress_pdf_Export", None, "C3-PPT-PDF"),
        (C::Ppt, T::Pptx) => writer(
            "pptx:Impress MS PowerPoint 2007 XML",
            None,
            "C3-PPT-PPTX",
        ),
        // ---- rtf --------------------------------------------------------
        (C::Rtf, T::Pdf) => writer("pdf:writer_pdf_Export", None, "C3-RTF-PDF"),
        (C::Rtf, T::Docx) => writer("docx:MS Word 2007 XML", None, "C3-RTF-DOCX"),
        (C::Rtf, T::Html) => writer("html:HTML (StarWriter)", None, "C3-RTF-HTML"),
        (C::Rtf, T::Txt) => writer("txt:Text (encoded):UTF8", None, "C3-RTF-TXT"),

        // ---- C3b: the remaining document cells, measured 2026-11-01 on both
        // legs (evidence/c3-document.md §10). The family rule is visible in the
        // filters: `text` output from a spreadsheet is the Calc text filter with
        // a TAB separator, `html` out of Impress is `impress_html_Export`, and
        // every html *input* carries the html import filter.
        (C::Docx, T::Txt) => writer("txt:Text (encoded):UTF8", None, "C3B-DOCX-TXT"),
        (C::Docx, T::Rtf) => writer("rtf:Rich Text Format", None, "C3B-DOCX-RTF"),
        (C::Doc, T::Rtf) => writer("rtf:Rich Text Format", None, "C3B-DOC-RTF"),
        (C::Rtf, T::Md) => writer("md:Markdown", None, "C3B-RTF-MD"),
        (C::Markdown, T::Pdf) => writer("pdf:writer_pdf_Export", None, "C3B-MD-PDF"),
        (C::Markdown, T::Docx) => writer("docx:MS Word 2007 XML", None, "C3B-MD-DOCX"),
        (C::Markdown, T::Rtf) => writer("rtf:Rich Text Format", None, "C3B-MD-RTF"),
        (C::Markdown, T::Txt) => writer("txt:Text (encoded):UTF8", None, "C3B-MD-TXT"),
        (C::Markdown, T::Html) => writer("html:HTML (StarWriter)", None, "C3B-MD-HTML"),
        // html in: one import filter for every target, measured per target.
        (C::Html, T::Pdf) => writer(
            "pdf:writer_pdf_Export",
            Some("HTML (StarWriter)"),
            "C3B-HTML-PDF",
        ),
        (C::Html, T::Md) => writer("md:Markdown", Some("HTML (StarWriter)"), "C3B-HTML-MD"),
        (C::Html, T::Rtf) => writer(
            "rtf:Rich Text Format",
            Some("HTML (StarWriter)"),
            "C3B-HTML-RTF",
        ),
        // Calc: same StarCalc text filter as `xls -> csv`, TAB-separated.
        (C::Xlsx, T::Pdf) => writer("pdf:calc_pdf_Export", None, "C3B-XLSX-PDF"),
        (C::Xlsx, T::Html) => writer("html:HTML (StarCalc)", None, "C3B-XLSX-HTML"),
        (C::Xlsx, T::Txt) => writer(CALC_TEXT_FILTER, None, "C3B-XLSX-TXT"),
        (C::Xls, T::Txt) => writer(CALC_TEXT_FILTER, None, "C3B-XLS-TXT"),
        (C::Csv, T::Pdf) => writer("pdf:calc_pdf_Export", None, "C3B-CSV-PDF"),
        (C::Csv, T::Txt) => writer(CALC_TEXT_FILTER, None, "C3B-CSV-TXT"),
        // Impress: the html export is `impress_html_Export`, not the writer name.
        (C::Pptx, T::Html) => writer("html:impress_html_Export", None, "C3B-PPTX-HTML"),
        (C::Ppt, T::Html) => writer("html:impress_html_Export", None, "C3B-PPT-HTML"),
        _ => return None,
    })
}

/// Plain-text export for a spreadsheet: the **Calc** text filter with a TAB
/// field separator (ASCII 9). `txt:Text (encoded):UTF8` is a Writer filter and
/// answers `rc=1 … Class:Write Code:16` on a Calc document; the filter name and
/// the separator are both part of the measurement.
pub const CALC_TEXT_FILTER: &str =
    "txt:Text - txt - csv (StarCalc):9,34,76,1,,0,false,true,true,false,false,1";

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
        (CanonicalType::Doc, TargetFormat::Pdf),
        (CanonicalType::Doc, TargetFormat::Md),
        (CanonicalType::Doc, TargetFormat::Html),
        (CanonicalType::Doc, TargetFormat::Docx),
        (CanonicalType::Doc, TargetFormat::Txt),
        (CanonicalType::Xls, TargetFormat::Pdf),
        (CanonicalType::Xls, TargetFormat::Csv),
        (CanonicalType::Xls, TargetFormat::Xlsx),
        (CanonicalType::Xls, TargetFormat::Html),
        (CanonicalType::Ppt, TargetFormat::Pdf),
        (CanonicalType::Ppt, TargetFormat::Pptx),
        (CanonicalType::Rtf, TargetFormat::Pdf),
        (CanonicalType::Rtf, TargetFormat::Docx),
        (CanonicalType::Rtf, TargetFormat::Html),
        (CanonicalType::Rtf, TargetFormat::Txt),
        // -- C3b (evidence/c3-document.md §10) --------------------------------
        (CanonicalType::Docx, TargetFormat::Txt),
        (CanonicalType::Docx, TargetFormat::Rtf),
        (CanonicalType::Doc, TargetFormat::Rtf),
        (CanonicalType::Rtf, TargetFormat::Md),
        (CanonicalType::Markdown, TargetFormat::Pdf),
        (CanonicalType::Markdown, TargetFormat::Docx),
        (CanonicalType::Markdown, TargetFormat::Rtf),
        (CanonicalType::Markdown, TargetFormat::Txt),
        (CanonicalType::Markdown, TargetFormat::Html),
        (CanonicalType::Html, TargetFormat::Pdf),
        (CanonicalType::Html, TargetFormat::Md),
        (CanonicalType::Html, TargetFormat::Rtf),
        (CanonicalType::Xlsx, TargetFormat::Pdf),
        (CanonicalType::Xlsx, TargetFormat::Html),
        (CanonicalType::Xlsx, TargetFormat::Txt),
        (CanonicalType::Xls, TargetFormat::Txt),
        (CanonicalType::Csv, TargetFormat::Pdf),
        (CanonicalType::Csv, TargetFormat::Txt),
        (CanonicalType::Pptx, TargetFormat::Html),
        (CanonicalType::Ppt, TargetFormat::Html),
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
// Hebrew fidelity: the font-replacement seed (card t_76ca844f)
// ---------------------------------------------------------------------------

/// The five Culmus families the licence strip removed (card t_20fae747, F1),
/// each mapped onto an OFL-1.1 family that still ships.
///
/// Chosen by measurement, not taste: the advance width of the fixture's Hebrew
/// line in each candidate, against the stripped original
/// (`evidence/hebrew-fidelity.md` §2). The three named twins are the Libre
/// Hebrew faces the pack already carries — David Libre +8.0 %, Frank Ruhl
/// Hofshi +4.4 %, Miriam Libre +4.3 %, each closer to the original than Noto
/// Serif Hebrew (+11.1 % / +13.5 % / +6.9 %) and, more to the point, the same
/// designs. `Miriam Mono CLM` has no Hebrew + monospace twin: Liberation Mono is
/// monospaced, covers all 27 Hebrew letters and matches the original's width to
/// 0.02 %. `Nachlieli CLM` is a Hebrew sans (Arial lineage), so it goes to Alef,
/// the pack's OFL Hebrew sans (−4.0 %, the same band as the other twins).
pub const FONT_SUBSTITUTIONS: [(&str, &str); 5] = [
    ("David CLM", "David Libre"),
    ("Frank Ruehl CLM", "Frank Ruhl Hofshi"),
    ("Miriam CLM", "Miriam Libre"),
    ("Miriam Mono CLM", "Liberation Mono"),
    ("Nachlieli CLM", "Alef"),
];

/// The config file LibreOffice reads a user profile from. The font-replacement
/// table (`Tools ▸ Options ▸ LibreOffice ▸ Fonts`) is stored here as
/// `/org.openoffice.Office.Common/Font/Substitution`.
pub const REGISTRY_FILE: &str = "registrymodifications.xcu";

const FONT_SUBSTITUTION_PATH: &str = "/org.openoffice.Office.Common/Font/Substitution";
const XCU_HEADER: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
    "<oor:items xmlns:oor=\"http://openoffice.org/2001/registry\"",
    " xmlns:xs=\"http://www.w3.org/2001/XMLSchema\"",
    " xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n"
);
const XCU_FOOTER: &str = "</oor:items>\n";

fn replacement_item() -> String {
    format!(
        " <item oor:path=\"{FONT_SUBSTITUTION_PATH}\">\
         <prop oor:name=\"Replacement\" oor:op=\"fuse\"><value>true</value></prop></item>\n"
    )
}

fn pair_item(replace: &str, substitute: &str) -> String {
    // `Always=true` is not decoration. LibreOffice 26.8 applies a pair with
    // `Always=false` on the screen only: the same document exported to PDF with
    // `Always=false` came out with LO's fallbacks (Noto Serif / Linux Libertine
    // G / Noto Sans) byte for byte as if the table did not exist, and only
    // `Always=true` put the twins in the PDF (measured, §1 of the evidence).
    format!(
        " <item oor:path=\"{FONT_SUBSTITUTION_PATH}/FontPairs\">\
         <node oor:name=\"{replace}\" oor:op=\"replace\">\
         <prop oor:name=\"ReplaceFont\" oor:op=\"fuse\"><value>{replace}</value></prop>\
         <prop oor:name=\"SubstituteFont\" oor:op=\"fuse\"><value>{substitute}</value></prop>\
         <prop oor:name=\"OnScreenOnly\" oor:op=\"fuse\"><value>false</value></prop>\
         <prop oor:name=\"Always\" oor:op=\"fuse\"><value>true</value></prop>\
         </node></item>\n"
    )
}

/// The `<item>` block this module seeds, without the document envelope.
pub fn font_substitution_items() -> String {
    let mut out = replacement_item();
    for (replace, substitute) in FONT_SUBSTITUTIONS {
        out.push_str(&pair_item(replace, substitute));
    }
    out
}

impl Profile {
    /// A profile at an explicit directory — the per-conversion temp profile
    /// `engine/doc.rs` hands to `-env:UserInstallation`.
    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Write the font-replacement table into `<profile>/user/registrymodifications.xcu`
    /// **before** LibreOffice starts on this profile.
    ///
    /// Why a profile file and not the engine pack: the same five
    /// `FontSubstitutions` nodes added to the pack's `share/registry/main.xcd`
    /// were measured to change nothing on the 26.8.2 export path (a shipped
    /// substitution, `Agency FB → Agdasima`, is equally inert), and the pack is
    /// MPL-2.0 content we do not modify. The profile is app-owned and identical
    /// on Windows and POSIX, so this is the mechanism that travels.
    ///
    /// Idempotent and non-destructive: an already-seeded profile (LibreOffice
    /// rewrites this file on exit and keeps items it did not originate) is left
    /// alone, and a profile that already holds other settings keeps them — the
    /// missing items are inserted before `</oor:items>`.
    pub fn seed_font_substitutions(&self) -> std::io::Result<()> {
        let user = self.dir.join("user");
        std::fs::create_dir_all(&user)?;
        let path = user.join(REGISTRY_FILE);
        match std::fs::read_to_string(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => std::fs::write(
                &path,
                format!("{XCU_HEADER}{}{XCU_FOOTER}", font_substitution_items()),
            ),
            Err(error) => Err(error),
            Ok(existing) => {
                let mut missing = String::new();
                if !existing.contains("oor:name=\"Replacement\"") {
                    missing.push_str(&replacement_item());
                }
                for (replace, substitute) in FONT_SUBSTITUTIONS {
                    if !existing.contains(&format!("<node oor:name=\"{replace}\"")) {
                        missing.push_str(&pair_item(replace, substitute));
                    }
                }
                if missing.is_empty() {
                    return Ok(());
                }
                match existing.rfind(XCU_FOOTER.trim_end()) {
                    Some(at) => std::fs::write(
                        &path,
                        format!("{}{missing}{}", &existing[..at], &existing[at..]),
                    ),
                    // Not a config file we recognise (no closing element): the
                    // profile is disposable, so replace it with a valid one.
                    None => std::fs::write(&path, format!("{XCU_HEADER}{missing}{XCU_FOOTER}")),
                }
            }
        }
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

/// How the startup warm-up went. Recorded for the diagnostics pane; a warm-up
/// is a performance call, so nothing here is fatal to a job (§6.2, §6.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WarmUp {
    /// No document engine on disk: nothing to warm and no document targets.
    NotInstalled,
    /// An earlier launch already bootstrapped the profile: nothing to pay.
    AlreadyWarm,
    /// Bootstrapped in this launch. `millis` is the spawn, `bootstrapped` the
    /// measured effect — a plausible exit code is not the claim.
    Warmed { millis: u128, bootstrapped: bool },
    /// The launcher exists and the warm-up failed. The first job still runs.
    Failed(String),
}

impl std::fmt::Display for WarmUp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WarmUp::NotInstalled => write!(f, "no document engine installed"),
            WarmUp::AlreadyWarm => write!(f, "profile already warm"),
            WarmUp::Warmed { millis, .. } => write!(f, "profile bootstrapped in {millis} ms"),
            WarmUp::Failed(reason) => write!(f, "warm-up failed: {reason}"),
        }
    }
}

/// The startup hook the window calls once, on a worker thread (§6.2 operational
/// contract): the measured install-cold *first conversion* is 5,111 ms on
/// Windows, and that cost belongs to app start, not to the user's first job.
///
/// Two rules make this safe to call unconditionally:
///
/// 1. **Bootstrap once.** `Profile::bootstrapped()` (the `user/` directory the
///    engine writes) is the cheap "nothing to pay" signal, so a relaunch does
///    not spawn a process at all.
/// 2. **`rc=0` is not the claim.** The result is read back from the profile,
///    so a launcher that exits 0 and writes nothing is `Failed`, not `Warmed`.
pub fn warm_up_at_startup(paths: &EnginePaths, cache_dir: &Path) -> WarmUp {
    if !paths.installed(Program::Soffice) {
        return WarmUp::NotInstalled;
    }
    let profile = Profile::under(cache_dir);
    if profile.bootstrapped() {
        return WarmUp::AlreadyWarm;
    }
    let work = cache_dir.join("warm-up");
    if let Err(e) = std::fs::create_dir_all(&work) {
        return WarmUp::Failed(format!("cannot create {}: {e}", work.display()));
    }
    let started = Instant::now();
    match warm_up(paths, &profile, &work) {
        Ok(outcome) => {
            let millis = started.elapsed().as_millis();
            if profile.bootstrapped() {
                WarmUp::Warmed {
                    millis,
                    bootstrapped: true,
                }
            } else {
                WarmUp::Failed(format!(
                    "no user/ in {} after {} ms (status {:?}): {}",
                    profile.dir.display(),
                    millis,
                    outcome.status,
                    outcome.stderr.trim()
                ))
            }
        }
        Err(e) => WarmUp::Failed(e.to_string()),
    }
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
        assert_eq!(
            proven_routes().len(),
            42,
            "every routed direction must be listed in proven_routes()"
        );
    }

    /// The C3b literals, character for character. Two of them are the measured
    /// trap: the Writer text filter fails on a Calc document, and Impress's html
    /// export is `impress_html_Export` (the writer name answers Parameter:26).
    #[test]
    fn the_c3b_filters_are_the_measured_literals() {
        let cases = [
            (
                CanonicalType::Docx,
                TargetFormat::Txt,
                "txt:Text (encoded):UTF8",
                None,
            ),
            (
                CanonicalType::Docx,
                TargetFormat::Rtf,
                "rtf:Rich Text Format",
                None,
            ),
            (
                CanonicalType::Xls,
                TargetFormat::Txt,
                CALC_TEXT_FILTER,
                None,
            ),
            (
                CanonicalType::Xlsx,
                TargetFormat::Txt,
                CALC_TEXT_FILTER,
                None,
            ),
            (
                CanonicalType::Csv,
                TargetFormat::Txt,
                CALC_TEXT_FILTER,
                None,
            ),
            (
                CanonicalType::Ppt,
                TargetFormat::Html,
                "html:impress_html_Export",
                None,
            ),
            (
                CanonicalType::Pptx,
                TargetFormat::Html,
                "html:impress_html_Export",
                None,
            ),
        ];
        for (from, to, convert_to, infilter) in cases {
            let r = route(from, to).unwrap_or_else(|| panic!("{from:?} -> {to:?} not routed"));
            assert_eq!(r.convert_to, convert_to, "{from:?} -> {to:?}");
            assert_eq!(r.infilter, infilter, "{from:?} -> {to:?}");
        }
        // The Calc text filter really is the Calc filter with a TAB separator.
        assert!(CALC_TEXT_FILTER.starts_with("txt:Text - txt - csv (StarCalc):9,"));
        assert!(CALC_TEXT_FILTER.contains(":9,"));
        // And the Writer text filter is still the writer one for writer documents.
        assert_eq!(
            route(CanonicalType::Markdown, TargetFormat::Txt)
                .unwrap()
                .convert_to,
            "txt:Text (encoded):UTF8"
        );
    }

    /// Every html input carries the html import filter, for every target.
    #[test]
    fn every_html_route_carries_the_html_import_filter() {
        let targets = [
            TargetFormat::Pdf,
            TargetFormat::Md,
            TargetFormat::Docx,
            TargetFormat::Rtf,
        ];
        for to in targets {
            let r = route(CanonicalType::Html, to)
                .unwrap_or_else(|| panic!("Html -> {to:?} not routed"));
            assert_eq!(
                r.infilter,
                Some("HTML (StarWriter)"),
                "Html -> {to:?} must import with the html filter"
            );
        }
    }

    #[test]
    fn unmeasured_directions_have_no_route() {
        // A row is routed only when both legs measured a validated output. These
        // are the cells C3b leaves `Cut`: either LibreOffice has no filter for
        // the pair (measured `rc=1 … Class:Write Code:16`, or `rc=134` for the
        // Writer->Calc attempt), or the pair converts but is not offered for a
        // product reason stated in the matrix.
        for (from, to) in [
            (CanonicalType::Docx, TargetFormat::Xlsx),
            (CanonicalType::Pptx, TargetFormat::Md),
            (CanonicalType::Pptx, TargetFormat::Txt),
            (CanonicalType::Xls, TargetFormat::Md),
            (CanonicalType::Ppt, TargetFormat::Md),
            (CanonicalType::Ppt, TargetFormat::Txt),
            (CanonicalType::Html, TargetFormat::Txt),
            // txt stays input-only in v1: see `CUT_DOC_TXT_SOURCE`.
            (CanonicalType::Txt, TargetFormat::Pdf),
            (CanonicalType::Txt, TargetFormat::Docx),
            (CanonicalType::Txt, TargetFormat::Rtf),
            (CanonicalType::Txt, TargetFormat::Md),
            (CanonicalType::Pdf, TargetFormat::Txt),
        ] {
            assert!(route(from, to).is_none(), "{from:?} -> {to:?} must not be routed");
        }
    }

    #[test]
    fn legacy_routes_use_the_family_filter_not_the_writer_filter() {
        // The measured trap: a Calc document exported with `writer_pdf_Export`
        // fails or produces nothing; the family filter is not interchangeable.
        assert_eq!(
            route(CanonicalType::Xls, TargetFormat::Pdf).unwrap().convert_to,
            "pdf:calc_pdf_Export"
        );
        assert_eq!(
            route(CanonicalType::Ppt, TargetFormat::Pdf).unwrap().convert_to,
            "pdf:impress_pdf_Export"
        );
        assert_eq!(
            route(CanonicalType::Doc, TargetFormat::Pdf).unwrap().convert_to,
            "pdf:writer_pdf_Export"
        );
        // Plain text: the Writer filter for a Writer document, the Calc filter
        // (TAB-separated) for a spreadsheet. Measured both ways in C3b §10.
        assert_eq!(
            route(CanonicalType::Xls, TargetFormat::Txt).unwrap().convert_to,
            CALC_TEXT_FILTER
        );
        assert_eq!(
            route(CanonicalType::Rtf, TargetFormat::Txt).unwrap().convert_to,
            "txt:Text (encoded):UTF8"
        );
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

    // -----------------------------------------------------------------------
    // Startup warm-up (§6.2 operational contract)
    // -----------------------------------------------------------------------

    /// A scratch cache dir with an `engines/soffice/program/` root beside it.
    #[cfg(unix)]
    fn warm_work(name: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("ezc-warm-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("root");
        let cache = base.join("cache");
        std::fs::create_dir_all(root.join("engines/soffice/program")).unwrap();
        std::fs::create_dir_all(&cache).unwrap();
        (root, cache)
    }

    /// Write an executable fake launcher at the path `from_root` expects.
    #[cfg(unix)]
    fn fake_launcher(root: &Path, body: &str) -> EnginePaths {
        use std::os::unix::fs::PermissionsExt;
        let mut paths = EnginePaths::from_root(root);
        std::fs::write(&paths.soffice, format!("#!/bin/sh\n{body}")).unwrap();
        let mut perms = std::fs::metadata(&paths.soffice).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&paths.soffice, perms).unwrap();
        paths.soffice_dir = paths.soffice.parent().unwrap().to_path_buf();
        paths
    }

    #[test]
    fn a_missing_engine_is_not_a_warm_up_attempt() {
        let (root, cache) = {
            let base = std::env::temp_dir().join(format!("ezc-warm-none-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&base);
            (base.join("root"), base.join("cache"))
        };
        let paths = EnginePaths::from_root(&root);
        assert_eq!(
            warm_up_at_startup(&paths, &cache),
            WarmUp::NotInstalled,
            "no launcher on disk means nothing to warm, and no spawn"
        );
    }

    /// The engine is installed and the profile is already bootstrapped: the
    /// relaunch path spawns nothing at all.
    #[cfg(unix)]
    #[test]
    fn a_bootstrapped_profile_is_left_alone() {
        let (root, cache) = warm_work("already");
        let paths = fake_launcher(&root, "echo SHOULD-NOT-RUN >> \"$PWD/spawned.log\"\n");
        let profile = Profile::under(&cache);
        std::fs::create_dir_all(profile.dir.join("user")).unwrap();
        assert_eq!(
            warm_up_at_startup(&paths, &cache),
            WarmUp::AlreadyWarm,
            "a relaunch must not spawn soffice again"
        );
        assert!(
            !cache.join("warm-up").exists(),
            "the warm-up work dir must not even be created when the profile is warm"
        );
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }

    /// The measured failure shape: the launcher runs, says nothing, exits 0 and
    /// writes no profile. That is `Failed`, never `Warmed` (§6.0.4).
    #[cfg(unix)]
    #[test]
    fn exit_zero_without_a_bootstrapped_profile_is_a_failure() {
        let (root, cache) = warm_work("noop");
        let paths = fake_launcher(&root, "exit 0\n");
        match warm_up_at_startup(&paths, &cache) {
            WarmUp::Failed(reason) => assert!(reason.contains("no user/"), "{reason}"),
            other => panic!("expected Failed, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }

    /// A launcher that does what LibreOffice does — writes `user/` under the
    /// profile it is handed — is `Warmed`, and the reported effect is read back
    /// from disk rather than inferred from the exit code.
    #[cfg(unix)]
    #[test]
    fn a_bootstrapped_profile_after_the_spawn_is_warmed() {
        let (root, cache) = warm_work("real");
        // `-env:UserInstallation=file:///…` -> create <dir>/user, like soffice.
        let body = "profile=\"\"\nfor a in \"$@\"; do\n  case \"$a\" in\n    -env:UserInstallation=*) profile=\"${a#-env:UserInstallation=}\" ;;\n  esac\ndone\nprofile=\"${profile#file://}\"\nmkdir -p \"$profile/user\"\nexit 0\n";
        let paths = fake_launcher(&root, body);
        match warm_up_at_startup(&paths, &cache) {
            WarmUp::Warmed { bootstrapped, .. } => assert!(bootstrapped),
            other => panic!("expected Warmed, got {other:?}"),
        }
        // Second call sees the bootstrapped profile and spawns nothing.
        assert_eq!(warm_up_at_startup(&paths, &cache), WarmUp::AlreadyWarm);
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }

    // ---------------------------------------------------------------------
    // Hebrew fidelity seed (card t_76ca844f)
    // ---------------------------------------------------------------------

    fn scratch_profile(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ezc-profile-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The table is the licence decision's other half: every stripped family
    /// must name a family the pack actually ships, and `Always=true` is the
    /// measured flag (see `pair_item`).
    #[test]
    fn every_stripped_family_maps_to_a_shipped_ofl_twin() {
        let expected = [
            ("David CLM", "David Libre"),
            ("Frank Ruehl CLM", "Frank Ruhl Hofshi"),
            ("Miriam CLM", "Miriam Libre"),
            ("Miriam Mono CLM", "Liberation Mono"),
            ("Nachlieli CLM", "Alef"),
        ];
        assert_eq!(FONT_SUBSTITUTIONS, expected);
        let items = font_substitution_items();
        assert!(items.contains("<value>true</value></prop></item>"), "Replacement=true");
        for (replace, substitute) in expected {
            let node = format!(
                "<node oor:name=\"{replace}\" oor:op=\"replace\">\
                 <prop oor:name=\"ReplaceFont\" oor:op=\"fuse\"><value>{replace}</value></prop>\
                 <prop oor:name=\"SubstituteFont\" oor:op=\"fuse\"><value>{substitute}</value></prop>\
                 <prop oor:name=\"OnScreenOnly\" oor:op=\"fuse\"><value>false</value></prop>\
                 <prop oor:name=\"Always\" oor:op=\"fuse\"><value>true</value></prop>"
            );
            assert!(items.contains(&node), "missing pair {replace} -> {substitute}");
        }
        assert_eq!(items.matches("/FontPairs").count(), 5);
    }

    /// The seed has to land exactly where LibreOffice looks for a profile's
    /// config — `<profile>/user/registrymodifications.xcu` — or nothing happens.
    #[test]
    fn the_seed_is_written_where_libreoffice_reads_it() {
        let dir = scratch_profile("where");
        let profile = Profile::at(dir.clone());
        profile.seed_font_substitutions().unwrap();

        let path = dir.join("user").join(REGISTRY_FILE);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(text.trim_end().ends_with("</oor:items>"));
        assert!(text.contains("/org.openoffice.Office.Common/Font/Substitution"));
        for (replace, _) in FONT_SUBSTITUTIONS {
            assert!(text.contains(replace), "missing {replace}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The profile outlives a batch, so the seed runs on every conversion: it
    /// must not append a second copy of the table.
    #[test]
    fn seeding_twice_does_not_duplicate_the_table() {
        let dir = scratch_profile("twice");
        let profile = Profile::at(dir.clone());
        profile.seed_font_substitutions().unwrap();
        let once = std::fs::read_to_string(dir.join("user").join(REGISTRY_FILE)).unwrap();
        profile.seed_font_substitutions().unwrap();
        profile.seed_font_substitutions().unwrap();
        let thrice = std::fs::read_to_string(dir.join("user").join(REGISTRY_FILE)).unwrap();
        assert_eq!(once, thrice);
        for (replace, _) in FONT_SUBSTITUTIONS {
            assert_eq!(thrice.matches(replace).count(), 2, "{replace} appears twice per pair");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A profile LibreOffice has already written must keep its own items — the
    /// seed adds what is missing and touches nothing else.
    #[test]
    fn seeding_merges_into_a_profile_libreoffice_already_wrote() {
        let dir = scratch_profile("merge");
        let user = dir.join("user");
        std::fs::create_dir_all(&user).unwrap();
        let path = user.join(REGISTRY_FILE);
        let existing = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<oor:items \
             xmlns:oor=\"http://openoffice.org/2001/registry\" \
             xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" \
             xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n\
             <item oor:path=\"/org.openoffice.Office.Common/Misc\"><prop oor:name=\"FirstRun\" \
             oor:op=\"fuse\"><value>false</value></prop></item>\n</oor:items>\n";
        std::fs::write(&path, existing).unwrap();

        Profile::at(dir.clone()).seed_font_substitutions().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("/org.openoffice.Office.Common/Misc"), "foreign item lost");
        assert!(text.contains("FirstRun"));
        assert!(text.contains("SubstituteFont"), "table not added");
        assert!(text.trim_end().ends_with("</oor:items>"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LibreOffice rewrites the file on exit (it reorders properties within an
    /// item). The re-read after such a rewrite must not re-add the table.
    #[test]
    fn a_rewritten_config_is_recognised_as_already_seeded() {
        let dir = scratch_profile("rewritten");
        let user = dir.join("user");
        std::fs::create_dir_all(&user).unwrap();
        let path = user.join(REGISTRY_FILE);
        // Shape taken from a profile LibreOffice wrote after reading our seed.
        let rewritten = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<oor:items \
             xmlns:oor=\"http://openoffice.org/2001/registry\" \
             xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" \
             xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n\
             <item oor:path=\"/org.openoffice.Office.Common/Font/Substitution\">\
             <prop oor:name=\"Replacement\" oor:op=\"fuse\"><value>true</value></prop></item>\n\
             <item oor:path=\"/org.openoffice.Office.Common/Font/Substitution/FontPairs\">\
             <node oor:name=\"David CLM\" oor:op=\"replace\">\
             <prop oor:name=\"Always\" oor:op=\"fuse\"><value>true</value></prop>\
             <prop oor:name=\"ReplaceFont\" oor:op=\"fuse\"><value>David CLM</value></prop>\
             <prop oor:name=\"SubstituteFont\" oor:op=\"fuse\"><value>David Libre</value></prop>\
             </node></item>\n</oor:items>\n";
        std::fs::write(&path, rewritten).unwrap();

        Profile::at(dir.clone()).seed_font_substitutions().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("<node oor:name=\"David CLM\"").count(), 1, "duplicate pair");
        // The four other families were never in this file and must be added.
        assert_eq!(text.matches("/FontPairs").count(), 5);
        assert_eq!(text.matches("oor:name=\"Replacement\"").count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
