//! `diag` — the installed-build self-test behind `--diagnostics`.
//!
//! Ships inside the app binary on every platform. It resolves the engines the
//! way the app does, then runs one conversion per engine family through the
//! same entry points the job worker uses (`engine::convert::convert`,
//! `engine::doc::convert`). Its purpose is the packaging acceptance test: a
//! build that came out of an installer can prove itself on a machine with no
//! developer tools, and it runs on a headless runner because `main` dispatches
//! here *before* the window is created.
//!
//! Deliberately Tauri-free and `Command`-free: discovery is
//! [`EnginePaths::discover`] + [`EnginePaths::adopt_pack`], and every engine
//! process goes through `engine::proc` (ARCHITECTURE §2.2 — `engine/` keeps
//! exclusive ownership of process spawning).
//!
//! Usage:
//!   ez-convert --diagnostics [--fixtures <dir>] [--work <dir>]
//!
//! Prints one `engine ...` line per resolved program and one
//! `FAMILY <name> <PASS|FAIL> <bytes> <ms> <detail>` line per family, then
//! `SUMMARY families=<n> pass=<n> fail=<n>`. Exit code 0 only when every
//! family converted a real file that exists and is non-empty.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::engine::convert::{self, Job};
use crate::engine::doc::{self, DocJob};
use crate::engine::plan::Options;
use crate::engine::proc::EnginePaths;
use crate::model::types::{CanonicalType, TargetFormat};

/// True when the binary was invoked as the diagnostics CLI.
pub fn is_cli() -> bool {
    std::env::args().any(|arg| arg == "--diagnostics")
}

/// Entry point for `--diagnostics`. Returns the process exit code.
pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().collect();
    let fixtures = arg_value(&args, "--fixtures").unwrap_or_else(|| PathBuf::from("fixtures"));
    let work = arg_value(&args, "--work")
        .unwrap_or_else(|| std::env::temp_dir().join("ezc-diag"));

    let root = bundle_root();
    let mut paths = EnginePaths::discover(&root);
    if let Some(pack) = pack_root() {
        // macOS ships every engine in the separate engine pack (docs/MACOS-PACKAGING.md).
        paths.adopt_pack(&pack);
    }

    println!("bundle_root {}", root.display());
    for (name, path) in [
        ("vips", &paths.vips),
        ("vipsheader", &paths.vipsheader),
        ("ffmpeg", &paths.ffmpeg),
        ("ffprobe", &paths.ffprobe),
        ("soffice", &paths.soffice),
    ] {
        println!(
            "engine {name} {} {}",
            if path.is_file() { "found" } else { "MISSING" },
            path.display()
        );
    }
    println!("fixtures {}", fixtures.display());

    let _ = std::fs::remove_dir_all(&work);
    if let Err(e) = std::fs::create_dir_all(&work) {
        println!("SUMMARY families=0 pass=0 fail=1 work dir: {e}");
        return 1;
    }

    let cases: [(&str, CanonicalType, TargetFormat); 4] = [
        ("image", CanonicalType::Png, TargetFormat::Jpg),
        ("audio", CanonicalType::Wav, TargetFormat::Mp3),
        ("video", CanonicalType::Mp4, TargetFormat::Webm),
        ("document", CanonicalType::Docx, TargetFormat::Pdf),
    ];

    let mut pass = 0usize;
    let mut fail = 0usize;
    for (family, from, to) in cases {
        let started = Instant::now();
        let input = fixtures.join(format!("sample.{}", from.as_str()));
        let output = work.join(format!("diag-{family}.{}", to.as_str()));
        let temp = work.join(format!("tmp-{family}"));
        let ms = |t: Instant| t.elapsed().as_millis();

        if !input.is_file() {
            println!(
                "FAMILY {family} FAIL 0 {} fixture missing: {}",
                ms(started),
                input.display()
            );
            fail += 1;
            continue;
        }

        let result = match family {
            "document" => doc::convert(
                &paths,
                &DocJob { from, to },
                &input,
                &output,
                &temp,
            )
            .map(|report| report.produced.len())
            .map_err(|e| e.user_message()),
            _ => convert::convert(
                &paths,
                &Job {
                    from,
                    to,
                    options: Options::default(),
                },
                &input,
                &output,
                &temp,
            )
            .map(|report| report.output_bytes as usize)
            .map_err(|e| e.to_string()),
        };

        match result {
            Ok(bytes) => {
                let on_disk = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
                // "exit 0 is not success" (ARCHITECTURE §6.0.5): a conversion
                // that reports success but leaves no file is a FAIL here.
                if on_disk == 0 {
                    println!(
                        "FAMILY {family} FAIL 0 {} reported {bytes} but produced no file at {}",
                        ms(started),
                        output.display()
                    );
                    fail += 1;
                } else {
                    println!(
                        "FAMILY {family} PASS {on_disk} {} {}",
                        ms(started),
                        output.display()
                    );
                    pass += 1;
                }
            }
            Err(why) => {
                println!("FAMILY {family} FAIL 0 {} {why}", ms(started));
                fail += 1;
            }
        }
    }

    let total = pass + fail;
    println!("SUMMARY families={total} pass={pass} fail={fail}");
    if fail == 0 {
        0
    } else {
        1
    }
}

fn arg_value(args: &[String], flag: &str) -> Option<PathBuf> {
    let index = args.iter().position(|a| a == flag)?;
    args.get(index + 1).map(PathBuf::from)
}

/// The bundle's resource directory, derived from the running executable:
/// `<App>.app/Contents/MacOS/ez-convert` → `<App>.app/Contents/Resources`.
fn bundle_root() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let resources = dir.join("../Resources");
            if resources.is_dir() {
                return resources.canonicalize().unwrap_or(resources);
            }
            return dir.to_path_buf();
        }
    }
    std::env::current_dir().unwrap_or_default()
}

/// The per-user engine-pack root (`EZC_ENGINE_PACK`, else the platform's
/// application-data dir for this app). `adopt_pack` takes the same value the
/// Tauri shell passes from `app_data_dir()`, so the CLI and the window agree.
fn pack_root() -> Option<PathBuf> {
    let pack = EnginePaths::pack_root_from_env();
    if pack.is_some() {
        return pack;
    }
    if let Some(home) = std::env::var_os("HOME") {
        let base = PathBuf::from(home);
        let candidates = [
            base.join("Library/Application Support/app.ezconvert.desktop"),
            base.join(".local/share/app.ezconvert.desktop"),
        ];
        for candidate in candidates {
            if candidate.join("engines").is_dir() {
                return Some(candidate);
            }
        }
    }
    None
}

#[allow(dead_code)]
fn exists(path: &Path) -> bool {
    path.is_file()
}
