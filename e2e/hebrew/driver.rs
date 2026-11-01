//! Card t_76ca844f — Hebrew fidelity through the shipped document pipeline.
//!
//! Converts `evidence/fixtures/hebrew-clm-ctl.docx` (five paragraphs, each
//! naming a Culmus family in `w:rFonts` for `ascii`, `hAnsi`, `eastAsia` and
//! `cs`) with `engine::doc::convert` — the code path a drop takes, including the
//! profile seed — and prints the fonts the produced PDF embeds.
//!
//! The point of going through the pipeline instead of calling `soffice` is that
//! the measurement then covers the seed: argv, profile and mechanism are the
//! shipped ones, so a regression in any of them shows up here.
//!
//! Driver for `scripts/hebrew-fidelity.sh`:
//!   hebrew-fidelity <fixture.docx> <work-dir> <pack-program-or-empty>

use crate::engine;
use crate::model::types::{CanonicalType, TargetFormat};
use std::path::{Path, PathBuf};

pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: hebrew-fidelity <fixture.docx> <work-dir>");
        return 2;
    }
    let fixture = PathBuf::from(&args[1]);
    let work = PathBuf::from(&args[2]);
    // The profile URL is built from this path, and `file_url` assumes an
    // absolute path — a relative work dir would hand LibreOffice a profile under
    // `/` and it exits 77 ("User installation could not be completed").
    std::fs::create_dir_all(&work).expect("work dir");
    let work = work.canonicalize().expect("absolute work dir");

    // `discover` honours EZC_SOFFICE_DIR / EZC_ENGINE_LIB_DIR: the pack under
    // test is picked by the environment, never by argv, exactly as in e2e.
    let root = Path::new("/nonexistent-bundle");
    let paths = engine::proc::EnginePaths::discover(root);
    if !paths.soffice.is_file() {
        eprintln!(
            "no soffice at {} — set EZC_SOFFICE_DIR to the pack's program/ directory",
            paths.soffice.display()
        );
        return 2;
    }

    let out = work.join("hebrew-clm.pdf");
    let temp = work.join("tmp");
    let _ = std::fs::remove_file(&out);
    let job = engine::doc::DocJob {
        from: CanonicalType::Docx,
        to: TargetFormat::Pdf,
    };
    match engine::doc::convert(&paths, &job, &fixture, &out, &temp) {
        Ok(report) => {
            for produced in &report.produced {
                for output in &produced.outputs {
                    println!("produced: {} ({} B)", output.display(), size(output));
                }
            }
            0
        }
        Err(error) => {
            eprintln!("convert failed: {error:?}");
            1
        }
    }
}

fn size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}
