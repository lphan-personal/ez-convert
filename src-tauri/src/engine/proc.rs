//! `engine/proc.rs` — the ONE place the app constructs a `Command`.
//!
//! Implements `ARCHITECTURE.md` §6.0.1 (argv rules A1-A7), §6.0.2 (constructed
//! environment), §6.0.3 (timeouts, bounded capture, reaping) and is the carrier
//! for §6.0.4's stderr rule. Nothing else in the app may spawn a process; a test
//! at the bottom of this file greps the tree for stray `Command::new` calls.
//!
//! The argv builders themselves are pure (`vips.rs`, `ffmpeg.rs`); this module
//! only executes a plan that they produced, after re-checking every path.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::stderr_rules;

/// Which sidecar binary a step runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Program {
    Vips,
    VipsHeader,
    Ffmpeg,
    Ffprobe,
    /// LibreOffice's console launcher: `soffice` on POSIX, `soffice.com` on
    /// Windows (`ARCHITECTURE.md` §6.2.1 — never `soffice.bin`, which exits 81
    /// on a profile that has never been bootstrapped).
    Soffice,
}

impl Program {
    pub fn name(self) -> &'static str {
        match self {
            Program::Vips => "vips",
            Program::VipsHeader => "vipsheader",
            Program::Ffmpeg => "ffmpeg",
            Program::Ffprobe => "ffprobe",
            Program::Soffice => "soffice",
        }
    }

    /// Wall-clock safety limit (`ARCHITECTURE.md` §6.0.3). These are limits, not
    /// performance claims: no large-input measurement backs them.
    pub fn timeout(self) -> Duration {
        match self {
            // A Windows first run of a *freshly unpacked* pack has to pay for
            // Defender scanning the whole vips tree: measured 96.5 s once the
            // scanner had seen it, 216.9 s on the genuine first touch (v1.0.1
            // acceptance, 2026-11-02), which the older 120 s cap killed mid-
            // conversion. A warm conversion is 0.13 s, so this cap is still a
            // hang guard with ~2.8x headroom over the measured cold start — it
            // clears the scanner instead of racing it.
            Program::Vips | Program::VipsHeader => Duration::from_secs(600),
            Program::Ffprobe => Duration::from_secs(30),
            // A long video legitimately runs for minutes; ffmpeg is governed by
            // the stall detector instead of a fixed cap (§6.0.3).
            Program::Ffmpeg => Duration::from_secs(1800),
            // Measured envelope for a 7-file document batch on this box is
            // ~0.2 s/file warm; 120 s is a hang guard, not a target.
            Program::Soffice => Duration::from_secs(120),
        }
    }

    /// Does this program have to be quiet when nothing happens for a while?
    pub fn stall_limit(self) -> Option<Duration> {
        match self {
            Program::Ffmpeg => Some(Duration::from_secs(120)),
            _ => None,
        }
    }
}

/// The engine set as installed on this machine.
///
/// Discovery is `§6.7`: paths come from the app's own install root, never from
/// `PATH`. Tests point at a scratch install with the environment variables below,
/// which is also how `scripts/run-media-e2e.sh` exercises a real engine without
/// packaging it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnginePaths {
    pub vips: PathBuf,
    pub vipsheader: PathBuf,
    pub vips_dir: PathBuf,
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub ffmpeg_dir: PathBuf,
    /// LibreOffice's console launcher. `EnginePaths::from_root` assumes the
    /// bundled layout `engines/soffice/program/`; a system or vendor install is
    /// reached through `EZC_SOFFICE_DIR` (see `discover`).
    pub soffice: PathBuf,
    pub soffice_dir: PathBuf,
    /// Linux dev/test only: a directory the dynamic loader must search for the
    /// engine's own shared libraries when the engine is not installed into a
    /// system prefix. Set from `EZC_ENGINE_LIB_DIR` (absolute) and exported as
    /// `LD_LIBRARY_PATH` for the child only. Production installs never need it:
    /// on Windows the DLLs sit beside `vips.exe`, and on macOS the bundle's
    /// `@rpath` covers it. Documented in `ARCHITECTURE.md` §6.0.2.
    pub lib_dir: Option<PathBuf>,
    /// Linux dev/test only, and the narrower sibling of `lib_dir`: the
    /// directory that holds vips's loadable modules (`vips-modules-*`) and
    /// `libheif/plugins`. An unpacked distribution (a `.deb` tree, a CI
    /// tarball) puts both next to the shared libraries instead of the compiled-in
    /// `/usr/lib/<multiarch>` path, and `env_clear` means the child would never
    /// see `LIBHEIF_PLUGIN_PATH` otherwise — so HEIC/AVIF decode fails with
    /// *"no decoding plugin installed"* even though the engine can do it.
    ///
    /// Set from `EZC_VIPS_PLUGIN_DIR` (absolute). Production installs never need
    /// it: the engine pack ships its plugins beside the sidecar and the OS loader
    /// finds them. Documented in `ARCHITECTURE.md` §6.0.2.
    pub plugin_dir: Option<PathBuf>,
}

impl EnginePaths {
    /// `root` is the bundle resource directory. The layout is the one the engine
    /// pack ships: `engines/vips/{vips,vipsheader}`, `engines/ffmpeg/` and
    /// `engines/soffice/program/`.
    pub fn from_root(root: &Path) -> Self {
        let vips_dir = root.join("engines").join("vips");
        let ffmpeg_dir = root.join("engines").join("ffmpeg");
        let soffice_dir = root.join("engines").join("soffice").join("program");
        let exe = |dir: &Path, stem: &str| -> PathBuf {
            let name = if cfg!(windows) {
                format!("{stem}.exe")
            } else {
                stem.to_string()
            };
            dir.join(name)
        };
        let mut paths = Self {
            vips: exe(&vips_dir, "vips"),
            vipsheader: exe(&vips_dir, "vipsheader"),
            vips_dir: vips_dir.clone(),
            ffmpeg: exe(&ffmpeg_dir, "ffmpeg"),
            ffprobe: exe(&ffmpeg_dir, "ffprobe"),
            ffmpeg_dir,
            // §6.2.1: the *console* launcher, not `soffice.bin`. On Windows the
            // launcher is `soffice.com` — the extension is already part of the
            // name, so it must not go through `exe()` (which appends `.exe`).
            soffice: soffice_dir.join(if cfg!(windows) { "soffice.com" } else { "soffice" }),
            soffice_dir,
            lib_dir: absolute_env_dir("EZC_ENGINE_LIB_DIR"),
            plugin_dir: absolute_env_dir("EZC_VIPS_PLUGIN_DIR"),
        };
        // On macOS the pack keeps LibreOffice as the `.app` TDF ships: its
        // bootstrap resolves data relative to the executable *inside* the bundle
        // (`../Resources`, `../Frameworks`), and a flattened copy aborts with a
        // UNO DeploymentException on the first conversion (CI run 41384142116).
        // `engines/soffice/program` stays a compatibility symlink, so the path
        // above still resolves on macOS — but when the real bundle path exists,
        // spawn that one.
        #[cfg(target_os = "macos")]
        {
            let bundled = root
                .join("engines")
                .join("soffice")
                .join("LibreOffice.app")
                .join("Contents")
                .join("MacOS")
                .join("soffice");
            if bundled.is_file() {
                if let Some(dir) = bundled.parent() {
                    paths.soffice_dir = dir.to_path_buf();
                }
                paths.soffice = bundled;
            }
        }
        // ffmpeg/ffprobe ship as Tauri `externalBin` sidecars (ARCHITECTURE §18):
        // the bundler strips the `-<triple>` suffix and drops them next to the
        // app executable, not under `engines/`. Accept both layouts — the
        // bundled `engines/ffmpeg/` tree wins when present (it is what a staged
        // pack provides), otherwise the siblings that the installer actually
        // wrote are used.
        if !paths.ffmpeg.is_file() && paths.ffmpeg_dir != root {
            let sibling = root.join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" });
            if sibling.is_file() {
                paths.ffmpeg = sibling;
                paths.ffprobe = root.join(if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" });
                paths.ffmpeg_dir = root.to_path_buf();
            }
        }
        paths
    }

    /// Test/dev override: `EZC_VIPS_DIR`, `EZC_FFMPEG_DIR` and `EZC_SOFFICE_DIR`
    /// replace the engine directories wholesale. Only absolute directories are
    /// honoured, so an attacker-controlled environment cannot reach a relative
    /// `PATH` lookup.
    pub fn discover(root: &Path) -> Self {
        let mut paths = Self::from_root(root);
        if let Some(dir) = absolute_env_dir("EZC_VIPS_DIR") {
            let exe = |stem: &str| {
                dir.join(if cfg!(windows) {
                    format!("{stem}.exe")
                } else {
                    stem.to_string()
                })
            };
            paths.vips = exe("vips");
            paths.vipsheader = exe("vipsheader");
            paths.vips_dir = dir;
        }
        if let Some(dir) = absolute_env_dir("EZC_FFMPEG_DIR") {
            let exe = |stem: &str| {
                dir.join(if cfg!(windows) {
                    format!("{stem}.exe")
                } else {
                    stem.to_string()
                })
            };
            paths.ffmpeg = exe("ffmpeg");
            paths.ffprobe = exe("ffprobe");
            paths.ffmpeg_dir = dir;
        }
        if let Some(dir) = absolute_env_dir("EZC_SOFFICE_DIR") {
            paths.set_soffice_dir(&dir);
        }
        if let Some(pack) = Self::pack_root_from_env() {
            paths.adopt_pack(&pack);
        }
        paths
    }

    /// Point the document engine at `dir` (`program/` of a LibreOffice tree).
    ///
    /// The launcher name is platform-decided, never caller-decided: Windows
    /// must spawn `soffice.com` and POSIX `soffice` (`ARCHITECTURE.md` §6.2.1).
    pub fn set_soffice_dir(&mut self, dir: &Path) {
        self.soffice = dir.join(if cfg!(windows) { "soffice.com" } else { "soffice" });
        self.soffice_dir = dir.to_path_buf();
    }

    /// Adopt a separate engine pack — the macOS layout (docs/MACOS-PACKAGING.md).
    ///
    /// A pack root is the same shape as a bundle's resource dir: it holds
    /// `engines/{vips,ffmpeg,soffice/program}`. Every engine the app bundle did
    /// not provide is taken from the pack; an engine the bundle *did* provide is
    /// never overridden. `pack_root` is the directory that contains `engines/`
    /// (on macOS that is `~/Library/Application Support/app.ezconvert.desktop`),
    /// and a path that already ends in `engines` is accepted too.
    pub fn adopt_pack(&mut self, pack_root: &Path) {
        let root = if pack_root.join("engines").is_dir() {
            pack_root.to_path_buf()
        } else {
            pack_root.parent().map(Path::to_path_buf).unwrap_or_default()
        };
        let pack = Self::from_root(&root);
        if !self.vips.is_file() && pack.vips.is_file() {
            self.vips = pack.vips;
            self.vipsheader = pack.vipsheader;
            self.vips_dir = pack.vips_dir;
        }
        if !self.ffmpeg.is_file() && pack.ffmpeg.is_file() {
            self.ffmpeg = pack.ffmpeg;
            self.ffprobe = pack.ffprobe;
            self.ffmpeg_dir = pack.ffmpeg_dir;
        }
        if !self.soffice.is_file() && pack.soffice.is_file() {
            self.set_soffice_dir(&pack.soffice_dir);
        }
    }

    /// `EZC_ENGINE_PACK`: an absolute directory holding an engine pack, for a
    /// test that must not touch the user's application data. Only absolute
    /// paths are honoured, like every other override in this module.
    pub fn pack_root_from_env() -> Option<PathBuf> {
        absolute_env_dir("EZC_ENGINE_PACK")
    }

    pub fn program(&self, program: Program) -> (&Path, &Path) {
        match program {
            Program::Vips => (&self.vips, &self.vips_dir),
            Program::VipsHeader => (&self.vipsheader, &self.vips_dir),
            Program::Ffmpeg => (&self.ffmpeg, &self.ffmpeg_dir),
            Program::Ffprobe => (&self.ffprobe, &self.ffmpeg_dir),
            Program::Soffice => (&self.soffice, &self.soffice_dir),
        }
    }

    /// §6.7: a capability is what runs, not what is listed. Cheap existence
    /// probe only — the real check is a 1x1 conversion, done by the startup
    /// probe once engines are packaged.
    pub fn installed(&self, program: Program) -> bool {
        self.program(program).0.is_file()
    }
}

fn absolute_env_dir(var: &str) -> Option<PathBuf> {
    let raw = std::env::var_os(var)?;
    let p = PathBuf::from(raw);
    if p.is_absolute() && p.is_dir() {
        Some(p)
    } else {
        None
    }
}

/// `vips-modules-8.16` → `[8, 16]`, so ABI directories can be ordered by version
/// instead of by text. An unparseable name sorts lowest.
fn module_abi(path: &Path) -> Vec<u32> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("vips-modules-"))
        .map(|version| {
            version
                .split('.')
                .filter_map(|part| part.parse::<u32>().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// The extra environment a vips child needs when its plugins were unpacked
/// beside the shared libraries instead of into the compiled-in multiarch
/// directory: `VIPS_PLUGINDIR` for vips's own modules (`vips-modules-<abi>`),
/// `LIBHEIF_PLUGIN_PATH` for libheif's codec plugins (`libheif/plugins`).
///
/// Pure and existence-checked, so the suite can assert the mapping without
/// spawning anything, and a tree that has neither directory yields nothing
/// (no empty variables are exported).
fn plugin_env(dir: &Path) -> Vec<(OsString, OsString)> {
    let mut out: Vec<(OsString, OsString)> = Vec::new();
    let mut modules: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_dir()
                        && path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .map(|name| name.starts_with("vips-modules-"))
                            .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    modules.sort_by_key(|path| module_abi(path));
    // Newest ABI wins when a tree holds more than one module directory
    // (`vips-modules-8.16` beats `vips-modules-8.9`, which sorts backwards as text).
    if let Some(newest) = modules.pop() {
        out.push((OsString::from("VIPS_PLUGINDIR"), newest.into_os_string()));
    }
    let heif = dir.join("libheif").join("plugins");
    if heif.is_dir() {
        out.push((OsString::from("LIBHEIF_PLUGIN_PATH"), heif.into_os_string()));
    }
    out
}

// ---------------------------------------------------------------------------
// Path admission (§6.0.1 A3/A4/A5)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    NotAbsolute(PathBuf),
    LeadingDash(PathBuf),
    BracketName(PathBuf),
    NetworkOrDevice(PathBuf),
    Traversal(PathBuf),
    Missing(PathBuf),
    Empty,
}

impl std::fmt::Display for PathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathError::NotAbsolute(p) => write!(f, "path is not absolute: {}", p.display()),
            PathError::LeadingDash(p) => {
                write!(f, "path starts with '-': {}", p.display())
            }
            PathError::BracketName(p) => {
                write!(
                    f,
                    "file name contains '[' or ']', which libvips reads as engine options: {}",
                    p.display()
                )
            }
            PathError::NetworkOrDevice(p) => {
                write!(f, "network or device path refused: {}", p.display())
            }
            PathError::Traversal(p) => write!(f, "path contains '..': {}", p.display()),
            PathError::Missing(p) => write!(f, "path does not exist: {}", p.display()),
            PathError::Empty => write!(f, "path is empty"),
        }
    }
}

/// True when the *text* of the path starts with a UNC or device prefix.
///
/// The check runs on the original string because Windows would otherwise
/// normalise `\\?\C:\x` into a plain path before it is visible.
pub fn is_network_or_device(raw: &OsStr) -> bool {
    let s = raw.to_string_lossy();
    s.starts_with("\\\\")
        || s.starts_with("//")
        || s.starts_with("\\\\.\\")
        || s.starts_with("\\\\?\\")
        || s.starts_with("//./")
        || s.starts_with("//?/")
}

/// §6.0.1 A3-A5: everything we hand to an engine passes through here first.
///
/// `must_exist` separates inputs (must be there) from outputs (must be a
/// canonical path inside an existing directory, file not yet written).
pub fn admit(raw: &Path, must_exist: bool) -> Result<PathBuf, PathError> {
    if raw.as_os_str().is_empty() {
        return Err(PathError::Empty);
    }
    if is_network_or_device(raw.as_os_str()) {
        return Err(PathError::NetworkOrDevice(raw.to_path_buf()));
    }
    if !raw.is_absolute() {
        return Err(PathError::NotAbsolute(raw.to_path_buf()));
    }
    if raw.components().any(|c| c == Component::ParentDir) {
        return Err(PathError::Traversal(raw.to_path_buf()));
    }
    if let Some(name) = raw.file_name() {
        if name.to_string_lossy().starts_with('-') {
            return Err(PathError::LeadingDash(raw.to_path_buf()));
        }
    }
    if must_exist {
        return std::fs::canonicalize(raw).map_err(|_| PathError::Missing(raw.to_path_buf()));
    }
    // Canonicalise the parent, keep our own file name: the file does not exist
    // yet, so canonicalize() would fail.
    let parent = raw.parent().ok_or(PathError::Empty)?;
    let parent = std::fs::canonicalize(parent)
        .map_err(|_| PathError::Missing(parent.to_path_buf()))?;
    let name = raw.file_name().ok_or(PathError::Empty)?;
    if name.to_string_lossy().contains(['/', '\\']) {
        // A file name can never contain a separator after Path parsing; this is
        // belt-and-braces against OsStr tricks.
        return Err(PathError::Traversal(raw.to_path_buf()));
    }
    Ok(parent.join(name))
}

/// True when a file *name* carries libvips' bracket-option syntax.
///
/// libvips reads a trailing `[...]` on a filename as **engine options**
/// (`in[vips-option=value]`), and our own suffix (`[fail-on=error]`, `[Q=90]`)
/// is built from the same syntax. A user name that ends in a bracket group is
/// therefore re-parsed against our appended group, and which file comes out
/// depends on what else sits in the directory: measured with libvips 8.16.1,
/// `zz3[shrink=2][fail-on=error]` opens the literal `zz3[shrink=2]` when a bare
/// `zz3` neighbour is present and fails with "does not exist" when it is not
/// (Q1 finding F1). Anything a vips step *reads* must be free of `[`/`]`:
/// `engine::convert` stages such an input under an app-chosen name, and
/// [`plan::Plan::resolve`] refuses it if the staging is ever skipped.
pub fn has_bracket_syntax(name: &OsStr) -> bool {
    let s = name.to_string_lossy();
    s.contains('[') || s.contains(']')
}

// ---------------------------------------------------------------------------
// Running a plan step
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Invocation {
    pub program: Program,
    /// `argv[1..]`. The program itself is resolved from [`EnginePaths`], never
    /// taken from here.
    pub args: Vec<OsString>,
    /// Working directory (§6.0.2): the engine's own directory for vips.
    pub cwd: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub program: Program,
    pub argv: Vec<OsString>,
    pub status: Option<i32>,
    pub killed: Option<Kill>,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kill {
    Timeout,
    Stalled,
    Cancelled,
}

impl Outcome {
    /// `§6.0.4`: exit 0 alone is never success.
    pub fn is_fatal(&self) -> Option<FatalReason> {
        if let Some(kill) = self.killed {
            return Some(FatalReason::Killed(kill));
        }
        if let Some(code) = self.status {
            if code != 0 {
                return Some(FatalReason::Exit(code));
            }
        }
        if let Some(hit) = stderr_rules::first_fatal(&self.stderr) {
            return Some(FatalReason::Stderr(hit));
        }
        None
    }

    pub fn ok(&self) -> bool {
        self.is_fatal().is_none()
    }

    pub fn summary(&self) -> String {
        let mut out = format!("{} {:?}", self.program.name(), self.argv);
        if let Some(kill) = self.killed {
            out.push_str(&format!(" -> killed({kill:?})"));
        } else if let Some(code) = self.status {
            out.push_str(&format!(" -> rc={code}"));
        }
        if !self.stderr.trim().is_empty() {
            out.push_str(&format!(" stderr={}", self.stderr.trim()));
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FatalReason {
    Exit(i32),
    Killed(Kill),
    Stderr(&'static str),
}

impl std::fmt::Display for FatalReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FatalReason::Exit(c) => write!(f, "engine exited {c}"),
            FatalReason::Killed(k) => write!(f, "engine killed ({k:?})"),
            FatalReason::Stderr(p) => write!(f, "engine stderr matched fatal pattern {p:?}"),
        }
    }
}

/// Bounded capture (§6.0.3): 1 MiB per stream, head and tail kept.
const CAPTURE_CAP: usize = 1024 * 1024;

struct Buffer {
    head: Vec<u8>,
    tail: Vec<u8>,
    total: u64,
}

impl Buffer {
    fn new() -> Self {
        Self {
            head: Vec::new(),
            tail: Vec::new(),
            total: 0,
        }
    }

    fn push(&mut self, chunk: &[u8]) {
        self.total += chunk.len() as u64;
        if self.head.len() < CAPTURE_CAP {
            let room = CAPTURE_CAP - self.head.len();
            let take = room.min(chunk.len());
            self.head.extend_from_slice(&chunk[..take]);
        }
        self.tail.extend_from_slice(chunk);
        if self.tail.len() > CAPTURE_CAP {
            let drop = self.tail.len() - CAPTURE_CAP;
            self.tail.drain(..drop);
        }
    }

    fn finish(self) -> String {
        String::from_utf8_lossy(&self.head).into_owned()
    }
}

fn drain(mut stream: impl Read + Send + 'static, activity: Arc<AtomicU64>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buf = Buffer::new();
        let mut chunk = [0u8; 8192];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    buf.push(&chunk[..n]);
                    activity.fetch_add(n as u64, Ordering::Relaxed);
                }
                Err(_) => break,
            }
        }
        buf.finish()
    })
}

/// Spawn one engine invocation. Everything the contract demands happens here:
/// absolute program path, constructed environment, scrubbed `VIPS_*`/`GIO_*`,
/// CWD control, no shell, bounded capture, timeout and stall kill.
pub fn run(paths: &EnginePaths, inv: &Invocation) -> std::io::Result<Outcome> {
    run_with(paths, inv, &[])
}

/// `extra_env` is the one controlled exception to `env_clear`: a caller may pass
/// `LD_LIBRARY_PATH` for an engine that is not installed into a system prefix
/// (`tests`, Linux dev). It is never inherited from the parent process.
pub fn run_with(
    paths: &EnginePaths,
    inv: &Invocation,
    extra_env: &[(OsString, OsString)],
) -> std::io::Result<Outcome> {
    let (program_path, engine_dir) = paths.program(inv.program);
    let started = Instant::now();

    let mut cmd = Command::new(program_path);
    cmd.args(&inv.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(&inv.cwd)
        // §6.0.2: a constructed environment, not the inherited one.
        .env_clear()
        .env("PATH", engine_dir)
        .env("TMPDIR", &inv.cwd)
        .env("TMP", &inv.cwd)
        .env("TEMP", &inv.cwd)
        .env("HOME", &inv.cwd)
        .env("USERPROFILE", &inv.cwd);
    if inv.program == Program::Soffice {
        // §6.2: the document launcher is a *shell wrapper* on POSIX (`soffice`
        // execs `dirname`, `basename`, `ls`, `sed` before it ever reaches
        // `soffice.bin`), so it needs the system directories on PATH. This is
        // still a constructed PATH — the inherited one is discarded.
        if !cfg!(windows) {
            let mut path = std::ffi::OsString::from(engine_dir);
            path.push(":/usr/bin:/bin");
            cmd.env("PATH", path);
            // svp is the X11-free Linux VCL plugin; DISPLAY is already gone
            // because the environment was cleared.
            cmd.env("SAL_USE_VCLPLUGIN", "svp");
        }
        // Windows has a native VCL backend and must never be given `svp`
        // (`ARCHITECTURE.md` §6.2.2 point 2).
        cmd.env("SAL_DISABLE_OPENCL", "1");
    }
    if let Some(lib) = &paths.lib_dir {
        cmd.env("LD_LIBRARY_PATH", lib);
    }
    // An unpacked vips finds its loadable modules (HEIF/AVIF, JXL, PDF) and
    // libheif's codec plugins only if it is told where the tree put them. The
    // parent environment was cleared above, so this cannot leak in by accident.
    if matches!(inv.program, Program::Vips | Program::VipsHeader) {
        if let Some(dir) = &paths.plugin_dir {
            for (key, value) in plugin_env(dir) {
                cmd.env(key, value);
            }
        }
    }
    for (k, v) in extra_env {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn()?;
    let activity = Arc::new(AtomicU64::new(0));
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let out_handle = drain(stdout, Arc::clone(&activity));
    let err_handle = drain(stderr, Arc::clone(&activity));

    let deadline = started + inv.program.timeout();
    let stall_limit = inv.program.stall_limit();
    let mut last_seen = 0u64;
    let mut last_change = Instant::now();
    let mut killed = None;

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => return Err(e),
        }
        let now = Instant::now();
        if now >= deadline {
            killed = Some(Kill::Timeout);
            let _ = child.kill();
            break child.wait()?;
        }
        if let Some(limit) = stall_limit {
            let seen = activity.load(Ordering::Relaxed);
            if seen != last_seen {
                last_seen = seen;
                last_change = now;
            } else if now.duration_since(last_change) >= limit {
                killed = Some(Kill::Stalled);
                let _ = child.kill();
                break child.wait()?;
            }
        }
        std::thread::sleep(Duration::from_millis(15));
    };

    let stdout = out_handle.join().unwrap_or_default();
    let stderr = err_handle.join().unwrap_or_default();

    Ok(Outcome {
        program: inv.program,
        argv: inv.args.clone(),
        status: status.code(),
        killed,
        stdout,
        stderr,
        duration: started.elapsed(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_are_refused() {
        assert_eq!(
            admit(Path::new("out.png"), false),
            Err(PathError::NotAbsolute(PathBuf::from("out.png")))
        );
    }

    #[test]
    fn leading_dash_file_names_are_refused() {
        let p = std::env::temp_dir().join("--vips-config");
        assert_eq!(
            admit(&p, false),
            Err(PathError::LeadingDash(p.clone()))
        );
    }

    #[test]
    fn network_and_device_paths_are_refused() {
        for raw in [
            r"\\server\share\x.png",
            r"\\?\C:\x.png",
            r"\\.\pipe\x",
            "//server/share/x.png",
        ] {
            let p = Path::new(raw);
            assert!(admit(p, false).is_err(), "{raw} was accepted");
        }
    }

    #[test]
    fn traversal_is_refused() {
        // Built from the host's own temp dir so the path is absolute on every
        // platform (a hard-coded `/tmp/...` is not absolute on Windows, which
        // would make this assert `NotAbsolute` and hide the rule under test).
        let p = std::env::temp_dir().join("..").join("etc").join("passwd");
        assert_eq!(admit(&p, true), Err(PathError::Traversal(p.clone())));
    }

    #[test]
    fn existing_input_is_canonicalised() {
        let dir = std::env::temp_dir();
        let f = dir.join(format!("ezc-admit-{}.bin", std::process::id()));
        std::fs::write(&f, b"x").unwrap();
        let got = admit(&f, true).expect("admitted");
        assert!(got.is_absolute() && got.ends_with(f.file_name().unwrap()));
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn output_path_is_canonicalised_through_its_parent() {
        let dir = std::env::temp_dir();
        let out = dir.join(format!("ezc-out-{}.png", std::process::id()));
        let got = admit(&out, false).expect("admitted");
        assert_eq!(got.file_name(), out.file_name());
        assert!(got.is_absolute());
    }

    #[test]
    fn missing_input_is_reported_not_guessed() {
        let p = std::env::temp_dir().join("ezc-definitely-absent-42.png");
        assert_eq!(admit(&p, true), Err(PathError::Missing(p.clone())));
    }

    #[test]
    fn capture_is_bounded_and_keeps_the_tail() {
        let mut b = Buffer::new();
        let chunk = vec![b'a'; 4096];
        for _ in 0..1024 {
            b.push(&chunk);
        }
        assert!(b.head.len() <= CAPTURE_CAP);
        assert!(b.tail.len() <= CAPTURE_CAP);
        assert_eq!(b.total, 4096 * 1024);
        let s = b.finish();
        assert_eq!(s.len(), CAPTURE_CAP);
    }

    #[test]
    fn exit_zero_with_fatal_stderr_is_fatal() {
        let o = Outcome {
            program: Program::Vips,
            argv: vec![],
            status: Some(0),
            killed: None,
            stdout: String::new(),
            stderr: "VipsJpeg: premature end of JPEG image".into(),
            duration: Duration::ZERO,
        };
        assert!(!o.ok());
        assert!(matches!(o.is_fatal(), Some(FatalReason::Stderr(_))));
    }

    #[test]
    fn every_program_has_a_timeout_and_ffmpeg_has_a_stall_limit() {
        assert!(Program::Vips.timeout().as_secs() > 0);
        assert!(Program::Ffprobe.timeout().as_secs() > 0);
        assert!(Program::Ffmpeg.stall_limit().is_some());
        assert!(Program::Vips.stall_limit().is_none());
        // The Windows first run of a freshly unpacked pack pays for the AV
        // scanner: 216.9 s measured on 2026-11-02 against a 120 s cap, i.e. a
        // killed conversion. The cap has to clear a cold scan with headroom.
        assert!(
            Program::Vips.timeout() >= Duration::from_secs(600),
            "vips cap must clear a cold first-run AV scan (measured 216.9 s)"
        );
    }

    /// An unpacked vips has to be told where its own modules and libheif's
    /// codec plugins landed; a tree with neither must export nothing at all
    /// (an empty `LIBHEIF_PLUGIN_PATH` disables the compiled-in default).
    #[test]
    fn plugin_env_maps_an_unpacked_tree_and_stays_empty_otherwise() {
        let root = std::env::temp_dir().join(format!("ezc-plugins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("vips-modules-8.9")).unwrap();
        std::fs::create_dir_all(root.join("vips-modules-8.16")).unwrap();
        std::fs::create_dir_all(root.join("libheif").join("plugins")).unwrap();
        let got = plugin_env(&root);
        let value = |key: &str| {
            got.iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(
            value("VIPS_PLUGINDIR"),
            Some(root.join("vips-modules-8.16").into_os_string()),
            "the newest module ABI wins when a tree holds more than one"
        );
        assert_eq!(
            value("LIBHEIF_PLUGIN_PATH"),
            Some(root.join("libheif").join("plugins").into_os_string())
        );
        let bare = root.join("bare");
        std::fs::create_dir_all(&bare).unwrap();
        assert!(plugin_env(&bare).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// §2.2/§6.0: `proc.rs` is the only file allowed to construct a Command.
    #[test]
    fn only_proc_constructs_commands() {
        let root = match option_env!("EZC_REPO_ROOT") {
            Some(r) => std::path::Path::new(r).join("src-tauri/src"),
            // `cargo test` runs with the crate root as the working directory.
            None => std::path::PathBuf::from("src"),
        };
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            let entries = std::fs::read_dir(&dir).expect("read src");
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().map(|e| e == "rs").unwrap_or(false) {
                    let text = std::fs::read_to_string(&p).expect("read file");
                    if p.ends_with("engine/proc.rs") {
                        continue;
                    }
                    for (i, line) in text.lines().enumerate() {
                        assert!(
                            !line.contains("Command::new"),
                            "{}:{} constructs a Command outside proc.rs: {line}",
                            p.display(),
                            i + 1
                        );
                    }
                }
            }
        }
    }
}
