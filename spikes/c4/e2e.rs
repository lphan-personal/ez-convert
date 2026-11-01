// C4 end-to-end: the output rules against a REAL engine (ffmpeg), real files and
// a real journal.
//
// The unit tests prove the rules with an injected fake converter; this proves the
// same code path with the engine the product ships, because the parts that can
// only fail for real are exactly the ones a fake cannot show:
//
//   E2E-1  same folder by default, and the output is a decodable WAV (the shared
//          `engine::verify` validator ran, not just "the file exists");
//   E2E-2  the metadata keep/strip toggle reaches ffmpeg+lofty: keeping merges
//          the source's ID3 tags, stripping skips the merge and the bytes differ;
//   E2E-3  a pre-existing file is suffixed, never replaced, and Undo deletes the
//          file this job wrote while leaving the user's file and the input alone;
//   E2E-4  a 3-file batch creates `converted/`, and Undo removes the outputs and
//          the folder it created;
//   E2E-5  the 10-second window: after it, Undo refuses and the files are kept;
//   E2E-6  a second Undo on the same job has nothing to delete.
//
// Usage: `spikes/c4/run-e2e.sh` (builds the harness and runs this file).
// Requires ffmpeg + ffprobe on disk and `EZC_FFMPEG_DIR` set to their directory.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

use ezc_c4_e2e::engine::convert::{self, Job as EngineJob};
use ezc_c4_e2e::engine::plan::Options;
use ezc_c4_e2e::engine::proc::EnginePaths;
use ezc_c4_e2e::job::journal::{now_unix_ms, JournalState, JournalStore};
use ezc_c4_e2e::job::outputs::{self, JobItem};
use ezc_c4_e2e::job::publish::TempArea;
use ezc_c4_e2e::model::settings::Settings;
use ezc_c4_e2e::model::types::{CanonicalType, TargetFormat};

struct Runner {
    work: PathBuf,
    fixtures: PathBuf,
    paths: EnginePaths,
    pass: usize,
    fail: usize,
    notes: Vec<String>,
}

impl Runner {
    fn new(work: PathBuf, fixtures: PathBuf) -> Self {
        let paths = EnginePaths::discover(Path::new("/nonexistent-bundle-root"));
        Self {
            work,
            fixtures,
            paths,
            pass: 0,
            fail: 0,
            notes: Vec::new(),
        }
    }

    fn check(&mut self, id: &str, what: &str, ok: bool, detail: String) {
        if ok {
            self.pass += 1;
            println!("PASS {id} {what} — {detail}");
        } else {
            self.fail += 1;
            println!("FAIL {id} {what} — {detail}");
        }
        self.notes.push(format!("{id}: {detail}"));
    }

    fn fixture(&self, name: &str) -> PathBuf {
        let path = self.fixtures.join(name);
        assert!(path.is_file(), "missing fixture {}", path.display());
        path
    }

    /// A fresh directory with a copy of `fixture` under `dir/in/<name>`.
    fn case(&self, tag: &str) -> PathBuf {
        let dir = self.work.join(tag);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("in")).unwrap();
        dir
    }

    fn stage(&self, dir: &Path, fixture: &str, name: &str) -> PathBuf {
        let dest = dir.join("in").join(name);
        fs::copy(self.fixture(fixture), &dest).unwrap();
        dest
    }

    fn journals(&self, dir: &Path) -> JournalStore {
        JournalStore::new(dir.join("journal"), 10)
    }

    fn temp(&self, dir: &Path) -> TempArea {
        TempArea::new(dir.join("jobs"))
    }
}

/// A real ffmpeg conversion, with the tag outcome reported back.
///
/// This is deliberately the *same* call the job worker makes
/// (`engine::convert::convert`), not a shortcut: it probes the source, plans,
/// spawns ffmpeg, verifies the output and merges tags.
fn real_convert(
    paths: &EnginePaths,
    input: &Path,
    output: &Path,
    temp_dir: &Path,
    strip: bool,
) -> Result<Option<usize>, String> {
    let job = EngineJob {
        from: CanonicalType::Mp3,
        to: TargetFormat::Wav,
        options: Options {
            strip_metadata: strip,
            ..Options::default()
        },
    };
    let report = convert::convert(paths, &job, input, output, temp_dir)
        .map_err(|e| e.to_string())?;
    if let Some(err) = report.tag_error.as_ref() {
        return Err(format!("tag merge failed: {err}"));
    }
    Ok(report.tags.map(|t| t.items))
}

fn is_wav(path: &Path) -> bool {
    match fs::read(path) {
        Ok(bytes) => bytes.len() > 1024 && bytes.starts_with(b"RIFF") && bytes[8..12] == *b"WAVE",
        Err(_) => false,
    }
}

fn run(runner: &mut Runner) {
    // ---------------------------------------------------------------- E2E-1
    let dir = runner.case("keep");
    let input = runner.stage(&dir, "tagged.mp3", "tone.mp3");
    let input_before = fs::read(&input).unwrap();
    let journals = runner.journals(&dir);
    let temp = runner.temp(&dir);
    let settings = Settings::default();

    let tag_counts: RefCell<Vec<Option<usize>>> = RefCell::new(Vec::new());
    let plan = outputs::plan(&[JobItem::new(&input)], "wav", &settings);
    let outcome = outputs::run(
        "c4_e2e_1",
        &plan,
        &journals,
        &temp,
        "ffmpeg+lofty",
        "mp3->wav",
        |source: &Path, target: &Path, strip: bool| {
            let tags = real_convert(&runner.paths, source, target, &dir.join("jobs").join("probe"), strip)?;
            tag_counts.borrow_mut().push(tags);
            Ok(())
        },
    )
    .expect("run should not fail at the journal level");

    let produced = dir.join("in/tone.wav");
    runner.check(
        "E2E-1a",
        "a single file lands next to its source",
        outcome.published.len() == 1 && produced.is_file() && outcome.failures.is_empty(),
        format!(
            "published={} path={} failures={:?}",
            outcome.published.len(),
            produced.display(),
            outcome.failures
        ),
    );
    runner.check(
        "E2E-1b",
        "the output is a validated WAV, and the input is untouched",
        is_wav(&produced) && fs::read(&input).unwrap() == input_before,
        format!(
            "size={} first4={:?} input_unchanged={}",
            fs::metadata(&produced).map(|m| m.len()).unwrap_or(0),
            fs::read(&produced).map(|b| b[..4].to_vec()).unwrap_or_default(),
            fs::read(&input).unwrap() == input_before
        ),
    );
    let kept_tags = tag_counts.borrow()[0];
    runner.check(
        "E2E-1c",
        "metadata is kept by default (tags merged by lofty)",
        matches!(kept_tags, Some(n) if n > 0),
        format!("tags_merged={kept_tags:?}"),
    );
    let kept_bytes = fs::read(&produced).unwrap();
    let journal = journals.read(&outcome.journal.clone().unwrap()).unwrap();
    runner.check(
        "E2E-1d",
        "the journal is committed with the published hash",
        journal.state == JournalState::Committed
            && journal.outputs.len() == 1
            && journal.outputs[0].sha256 == ezc_c4_e2e::job::hash::sha256_bytes(&kept_bytes),
        format!(
            "state={:?} outputs={} sha256={}",
            journal.state,
            journal.outputs.len(),
            journal.outputs[0].sha256
        ),
    );

    // ---------------------------------------------------------------- E2E-2
    let strip_dir = runner.case("strip");
    let strip_input = runner.stage(&strip_dir, "tagged.mp3", "tone.mp3");
    let strip_journals = runner.journals(&strip_dir);
    let strip_temp = runner.temp(&strip_dir);
    let mut strip_settings = Settings::default();
    strip_settings.metadata.strip = true;
    let strip_tags: RefCell<Vec<Option<usize>>> = RefCell::new(Vec::new());
    let strip_plan = outputs::plan(&[JobItem::new(&strip_input)], "wav", &strip_settings);
    let strip_outcome = outputs::run(
        "c4_e2e_2",
        &strip_plan,
        &strip_journals,
        &strip_temp,
        "ffmpeg+lofty",
        "mp3->wav",
        |source: &Path, target: &Path, strip: bool| {
            let tags =
                real_convert(&runner.paths, source, target, &strip_dir.join("jobs/probe"), strip)?;
            strip_tags.borrow_mut().push(tags);
            Ok(())
        },
    )
    .expect("strip run");
    let stripped = strip_dir.join("in/tone.wav");
    let stripped_bytes = fs::read(&stripped).unwrap();
    let stripped_tags = strip_tags.borrow()[0];
    runner.check(
        "E2E-2a",
        "settings.metadata.strip=true reaches the engine (no tag merge)",
        strip_outcome.published.len() == 1
            && strip_outcome.failures.is_empty()
            && stripped_tags.is_none(),
        format!(
            "published={} tags_merged={stripped_tags:?} failures={:?}",
            strip_outcome.published.len(),
            strip_outcome.failures
        ),
    );
    runner.check(
        "E2E-2b",
        "the stripped output is a different, still-valid WAV",
        is_wav(&stripped) && stripped_bytes != kept_bytes,
        format!(
            "strip_len={} keep_len={} identical={}",
            stripped_bytes.len(),
            kept_bytes.len(),
            stripped_bytes == kept_bytes
        ),
    );

    // ---------------------------------------------------------------- E2E-3
    let col_dir = runner.case("collide");
    let col_input = runner.stage(&col_dir, "tagged.mp3", "tone.mp3");
    let user_file = col_dir.join("in/tone.wav");
    fs::write(&user_file, b"THE USER'S OWN WAV").unwrap();
    let col_journals = runner.journals(&col_dir);
    let col_temp = runner.temp(&col_dir);
    let col_plan = outputs::plan(&[JobItem::new(&col_input)], "wav", &settings);
    let renamed_planned = col_plan.items[0].renamed;
    let col_outcome = outputs::run(
        "c4_e2e_3",
        &col_plan,
        &col_journals,
        &col_temp,
        "ffmpeg+lofty",
        "mp3->wav",
        |source: &Path, target: &Path, strip: bool| {
            real_convert(&runner.paths, source, target, &col_dir.join("jobs/probe"), strip)
                .map(|_| ())
        },
    )
    .expect("collision run");
    let produced = col_dir.join("in/tone-1.wav");
    runner.check(
        "E2E-3a",
        "an existing file is stepped over, never replaced",
        renamed_planned
            && col_outcome.published.len() == 1
            && col_outcome.published[0].path == produced
            && fs::read(&user_file).unwrap() == b"THE USER'S OWN WAV",
        format!(
            "planned_renamed={renamed_planned} published={} user_file_intact={}",
            col_outcome.published[0].path.display(),
            fs::read(&user_file).unwrap() == b"THE USER'S OWN WAV"
        ),
    );
    let undo = journals_undo(&col_journals, &col_outcome.journal.clone().unwrap());
    runner.check(
        "E2E-3b",
        "undo deletes the file this job wrote and nothing else",
        undo.as_ref().map(|r| r.deleted.clone()).unwrap_or_default() == vec![produced.clone()]
            && !produced.exists()
            && user_file.exists()
            && col_input.exists(),
        format!(
            "deleted={:?} user_file_kept={} input_kept={}",
            undo.map(|r| r.deleted),
            user_file.exists(),
            col_input.exists()
        ),
    );

    // ---------------------------------------------------------------- E2E-4
    let batch_dir = runner.case("batch");
    let batch_inputs: Vec<PathBuf> = ["a", "b", "c"]
        .iter()
        .map(|name| runner.stage(&batch_dir, "tagged.mp3", &format!("{name}.mp3")))
        .collect();
    let batch_journals = runner.journals(&batch_dir);
    let batch_temp = runner.temp(&batch_dir);
    let batch_plan = outputs::plan(
        &batch_inputs.iter().cloned().map(JobItem::new).collect::<Vec<_>>(),
        "wav",
        &settings,
    );
    let batch_outcome = outputs::run(
        "c4_e2e_4",
        &batch_plan,
        &batch_journals,
        &batch_temp,
        "ffmpeg+lofty",
        "mp3->wav",
        |source: &Path, target: &Path, strip: bool| {
            real_convert(&runner.paths, source, target, &batch_dir.join("jobs/probe"), strip)
                .map(|_| ())
        },
    )
    .expect("batch run");
    let converted = batch_dir.join("in/converted");
    let batch_count = fs::read_dir(&converted)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("wav"))
                .count()
        })
        .unwrap_or(0);
    runner.check(
        "E2E-4a",
        "a 3-file batch goes into converted/",
        batch_outcome.published.len() == 3
            && batch_count == 3
            && converted.is_dir()
            && batch_outcome.created_dirs.contains(&converted),
        format!(
            "published={} wavs_in_converted={} created_dirs={:?}",
            batch_outcome.published.len(),
            batch_count,
            batch_outcome.created_dirs
        ),
    );
    let undo = journals_undo(&batch_journals, &batch_outcome.journal.clone().unwrap());
    runner.check(
        "E2E-4b",
        "undo removes the outputs and the empty converted/ folder",
        undo.as_ref().map(|r| r.deleted.len()).unwrap_or(0) == 3
            && !converted.exists()
            && batch_inputs.iter().all(|p| p.is_file()),
        format!(
            "deleted={} converted_gone={} inputs_kept={}",
            undo.as_ref().map(|r| r.deleted.len()).unwrap_or(0),
            !converted.exists(),
            batch_inputs.iter().all(|p| p.is_file())
        ),
    );

    // ---------------------------------------------------------------- E2E-5/6
    // The keep job from E2E-1 is still armed. Undo it for real, then check the
    // second undo and the closed window on the batch job's journal.
    let undo = journals_undo(&journals, &outcome.journal.clone().unwrap());
    runner.check(
        "E2E-5a",
        "undo inside the window deletes the output and keeps the input",
        undo.as_ref().map(|r| r.deleted.clone()).unwrap_or_default() == vec![produced_path(&dir)]
            && !produced_path(&dir).exists()
            && input.exists(),
        format!(
            "deleted={:?} input_kept={}",
            undo.map(|r| r.deleted),
            input.exists()
        ),
    );
    let second = journals.undo(&outcome.journal.clone().unwrap(), now_unix_ms());
    runner.check(
        "E2E-6",
        "a second undo has nothing to delete",
        second.is_err(),
        format!("second_undo={second:?}"),
    );

    // A fresh job, then a simulated 10 s wait: the window closes, files stay.
    let win_dir = runner.case("window");
    let win_input = runner.stage(&win_dir, "tagged.mp3", "tone.mp3");
    let win_journals = runner.journals(&win_dir);
    let win_temp = runner.temp(&win_dir);
    let win_plan = outputs::plan(&[JobItem::new(&win_input)], "wav", &settings);
    let win_outcome = outputs::run(
        "c4_e2e_5",
        &win_plan,
        &win_journals,
        &win_temp,
        "ffmpeg+lofty",
        "mp3->wav",
        |source: &Path, target: &Path, strip: bool| {
            real_convert(&runner.paths, source, target, &win_dir.join("jobs/probe"), strip)
                .map(|_| ())
        },
    )
    .expect("window run");
    let win_out = win_dir.join("in/tone.wav");
    let late = win_outcome.undo_deadline_unix_ms + 1;
    let expired = win_journals.undo(&win_outcome.journal.clone().unwrap(), late);
    runner.check(
        "E2E-5b",
        "after the 10-second window undo refuses and the file is kept",
        expired.is_err() && win_out.exists(),
        format!(
            "result={:?} kept={} deadline={} now={}",
            expired,
            win_out.exists(),
            win_outcome.undo_deadline_unix_ms,
            late
        ),
    );
}

fn produced_path(dir: &Path) -> PathBuf {
    dir.join("in/tone.wav")
}

fn journals_undo(
    journals: &JournalStore,
    path: &Path,
) -> Result<ezc_c4_e2e::job::journal::UndoReport, String> {
    journals.undo(path, now_unix_ms()).map_err(|e| e.to_string())
}

#[test]
fn c4_output_rules_end_to_end() {
    let fixtures = PathBuf::from(
        std::env::var("EZC_FIXTURES").unwrap_or_else(|_| "fixtures".to_string()),
    );
    let work = PathBuf::from(
        std::env::var("EZC_WORK").unwrap_or_else(|_| "target/c4-e2e".to_string()),
    );
    let ffmpeg_dir = std::env::var("EZC_FFMPEG_DIR").unwrap_or_else(|_| "/usr/local/bin".into());
    std::env::set_var("EZC_FFMPEG_DIR", &ffmpeg_dir);

    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).unwrap();

    let mut runner = Runner::new(work.clone(), fixtures);
    if !runner.paths.ffmpeg.is_file() || !runner.paths.ffprobe.is_file() {
        panic!(
            "ffmpeg/ffprobe not found at {} / {} — set EZC_FFMPEG_DIR",
            runner.paths.ffmpeg.display(),
            runner.paths.ffprobe.display()
        );
    }
    println!("engine: ffmpeg at {}", runner.paths.ffmpeg.display());
    run(&mut runner);
    println!(
        "\n{} passed, {} failed ({} checks)",
        runner.pass,
        runner.fail,
        runner.pass + runner.fail
    );
    assert_eq!(runner.fail, 0, "some end-to-end checks failed; see PASS/FAIL above");
}
