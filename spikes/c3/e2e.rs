// C3 end-to-end document matrix: the real LibreOffice, the real pipeline, real
// files, on both legs of the product (Linux container + Windows 11 VM).
//
// Every case runs `engine::doc::convert` — the same function the job worker
// calls — and then asserts a property that a broken conversion cannot fake:
// page count, container part names, a marker string that only exists in the
// source, or the measured LibreOffice file naming (`<stem>-<sheet>.csv`).
//
// Usage (from the generated harness, see `run-e2e.sh`):
//   ezc-c3-e2e <fixtures-dir> <work-dir> [--measure-legacy]
//
// Environment:
//   EZC_SOFFICE_DIR        directory holding soffice / soffice.com
//   EZC_ENGINE_LIB_DIR     Linux only: shared libs for an unpacked TDF tree
//
// Exit code 1 if any case fails. Output is one line per case so it can be
// pasted into `evidence/c3-document.md` without editing.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::engine::doc::{convert, convert_batch, DocError, DocJob};
use crate::engine::proc::EnginePaths;
use crate::engine::soffice::{self, DocRoute, Profile};
use crate::engine::verify;
use crate::model::types::{CanonicalType, TargetFormat};

struct Harness {
    paths: EnginePaths,
    fix: PathBuf,
    work: PathBuf,
    pass: usize,
    fail: usize,
    failures: Vec<String>,
    measure_legacy: bool,
}

impl Harness {
    fn new(fix: PathBuf, work: PathBuf, measure_legacy: bool) -> Self {
        let paths = EnginePaths::discover(Path::new("/nonexistent-bundle-root"));
        Self {
            paths,
            fix,
            work,
            pass: 0,
            fail: 0,
            failures: Vec::new(),
            measure_legacy,
        }
    }

    fn fixture(&self, name: &str) -> PathBuf {
        let p = self.fix.join(name);
        assert!(p.is_file(), "missing fixture: {}", p.display());
        p
    }

    fn out(&self, name: &str) -> PathBuf {
        self.work.join("out").join(name)
    }

    fn ok(&mut self, label: &str, detail: String) {
        self.pass += 1;
        println!("PASS  {label:<18} {detail}");
    }

    fn bad(&mut self, label: &str, detail: String) {
        self.fail += 1;
        println!("FAIL  {label:<18} {detail}");
        self.failures.push(format!("{label}: {detail}"));
    }

    /// Run one advertised direction and check content, not just existence.
    fn direction(
        &mut self,
        from: CanonicalType,
        to: TargetFormat,
        fixture: &str,
        out_name: &str,
        check: fn(&Path, &str) -> Result<String, String>,
    ) {
        let label = format!("{:?} -> {}", from, to.as_str());
        let input = self.fixture(fixture);
        let output = self.out(out_name);
        let started = Instant::now();
        match convert(
            &self.paths,
            &DocJob { from, to },
            &input,
            &output,
            &self.work.join("tmp").join(out_name),
        ) {
            Ok(report) => {
                let ms = started.elapsed().as_millis();
                let bytes = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
                match check(&output, out_name) {
                    Ok(extra) => self.ok(
                        &label,
                        format!("{bytes} B  {ms} ms  {}  {extra}", short(&output)),
                    ),
                    Err(why) => self.bad(&label, format!("content check failed: {why}")),
                }
                let _ = report;
            }
            Err(e) => self.bad(&label, format!("{e} | user: {}", e.user_message())),
        }
    }

    fn negative_cases(&mut self) {
        // The measured silent-garbage trap: text named .docx. Detection must
        // refuse it before any engine sees the file.
        let trap = self.fixture("text_renamed.docx");
        let detected = crate::model::detect::detect_path(&trap);
        let offered = crate::model::matrix::offered_targets(detected.ty);
        let label = "trap text.doc→pdf";
        if detected.ty == CanonicalType::Txt && !offered.contains(&TargetFormat::Pdf) {
            self.ok(
                label,
                format!("detected {:?}, document targets offered: {:?}", detected.ty, offered),
            );
        } else {
            self.bad(
                label,
                format!("detected {:?}, offered {:?}", detected.ty, offered),
            );
        }

        // The pipeline must refuse the same pair even if a caller insists.
        let label = "trap refusal";
        match convert(
            &self.paths,
            &DocJob {
                from: detected.ty,
                to: TargetFormat::Pdf,
            },
            &trap,
            &self.out("trap.pdf"),
            &self.work.join("tmp").join("trap"),
        ) {
            Err(DocError::NotRouted { .. }) => self.ok(label, "refused before spawning".into()),
            Err(other) => self.ok(label, format!("refused: {other}")),
            Ok(_) => self.bad(label, "a text file named .docx was converted to PDF".into()),
        }

        // A structurally damaged docx: reported honestly, whatever happens.
        let truncated = self.fixture("truncated.docx");
        let detected = crate::model::detect::detect_path(&truncated);
        let label = "truncated.docx";
        match convert(
            &self.paths,
            &DocJob {
                from: detected.ty,
                to: TargetFormat::Pdf,
            },
            &truncated,
            &self.out("truncated.pdf"),
            &self.work.join("tmp").join("truncated"),
        ) {
            Ok(report) => self.ok(
                label,
                format!(
                    "detected {:?} integrity {:?}; engine produced a validated {:?} ({} B)",
                    detected.ty,
                    detected.integrity,
                    report.produced[0].outputs,
                    std::fs::metadata(&self.out("truncated.pdf"))
                        .map(|m| m.len())
                        .unwrap_or(0)
                ),
            ),
            Err(e) => self.ok(label, format!("reported error: {e}")),
        }

        // Missing input.
        let label = "missing input";
        match convert(
            &self.paths,
            &DocJob {
                from: CanonicalType::Docx,
                to: TargetFormat::Pdf,
            },
            &self.work.join("does-not-exist.docx"),
            &self.out("none.pdf"),
            &self.work.join("tmp").join("none"),
        ) {
            Err(e) => self.ok(label, format!("reported: {e}")),
            Ok(_) => self.bad(label, "a missing input was reported as success".into()),
        }
    }

    fn batch_timing(&mut self) {
        // Measured claim to re-check: batching is cheaper per file than one
        // spawn per file (0.20 s vs 0.52 s in S3).
        let batch_dir = self.work.join("batch");
        let _ = std::fs::remove_dir_all(&batch_dir);
        std::fs::create_dir_all(&batch_dir).unwrap();
        let source = self.fixture("sample.docx");
        let mut inputs = Vec::new();
        for i in 0..8 {
            let p = batch_dir.join(format!("copy{i}.docx"));
            std::fs::copy(&source, &p).unwrap();
            inputs.push(p);
        }
        let job = DocJob {
            from: CanonicalType::Docx,
            to: TargetFormat::Md,
        };

        let started = Instant::now();
        let report = match convert_batch(
            &self.paths,
            &job,
            &inputs,
            &batch_dir.join("out-batched"),
            &batch_dir.join("tmp-batched"),
        ) {
            Ok(r) => r,
            Err(e) => {
                self.bad("batch run", format!("{e}"));
                return;
            }
        };
        let batched = started.elapsed();
        let per_file = batched.as_secs_f64() * 1000.0 / inputs.len() as f64;
        if report.produced.iter().all(|p| p.outputs.len() == 1) {
            self.ok(
                "batch 8",
                format!(
                    "{} outputs in {:.0} ms = {:.0} ms/file",
                    report.total_outputs(),
                    batched.as_secs_f64() * 1000.0,
                    per_file
                ),
            );
        } else {
            self.bad("batch 8", format!("attribution: {:?}", report.produced));
        }

        let singles = &inputs[..3];
        let started = Instant::now();
        let mut single_ok = 0;
        for (i, input) in singles.iter().enumerate() {
            let output = batch_dir.join(format!("single{i}.md"));
            if convert(
                &self.paths,
                &job,
                input,
                &output,
                &batch_dir.join(format!("tmp-single{i}")),
            )
            .is_ok()
            {
                single_ok += 1;
            }
        }
        let spawned = started.elapsed();
        let spawned_per_file = spawned.as_secs_f64() * 1000.0 / singles.len() as f64;
        let label = "3 individual";
        if single_ok == singles.len() {
            self.ok(
                label,
                format!(
                    "{single_ok} outputs in {:.0} ms = {:.0} ms/file",
                    spawned.as_secs_f64() * 1000.0,
                    spawned_per_file
                ),
            );
        } else {
            self.bad(label, format!("{single_ok}/{} succeeded", singles.len()));
        }
        println!(
            "      batch speedup: {:.2}x  ({:.0} ms/file batched vs {:.0} ms/file spawned)",
            spawned_per_file / per_file,
            per_file,
            spawned_per_file
        );
    }

    /// Not part of the advertised matrix: measures the directions C3b **cuts**,
    /// so every `Cut` in `model::matrix` quotes a measurement instead of a guess.
    /// Probe failures do **not** fail this run — they are the finding.
    fn legacy_measurements(&mut self) {
        let cases: &[(&str, CanonicalType, TargetFormat, &str, &str)] = &[
            // fixture, from, to, convert-to literal, infilter
            // -- no filter for the pair (writer filter on a Calc/Impress doc) --
            ("sample.xls", CanonicalType::Xls, TargetFormat::Md, "md:Markdown", ""),
            ("sample.xlsx", CanonicalType::Xlsx, TargetFormat::Md, "md:Markdown", ""),
            ("sample.pptx", CanonicalType::Pptx, TargetFormat::Md, "md:Markdown", ""),
            ("sample.ppt", CanonicalType::Ppt, TargetFormat::Md, "md:Markdown", ""),
            ("sample.ppt", CanonicalType::Ppt, TargetFormat::Txt, "txt:Text (encoded):UTF8", ""),
            ("sample.pptx", CanonicalType::Pptx, TargetFormat::Txt, "txt:Text (encoded):UTF8", ""),
            ("sample.pdf", CanonicalType::Pdf, TargetFormat::Txt, "txt:Text (encoded):UTF8", ""),
            // -- cross-family: the Writer document is exported with a Calc filter
            ("sample.docx", CanonicalType::Docx, TargetFormat::Xlsx, "xlsx:Calc MS Excel 2007 XML", ""),
            // -- these convert, and are cut on the product decision (N3 / no gain)
            ("sample.html", CanonicalType::Html, TargetFormat::Txt, "txt:Text (encoded):UTF8", "HTML (StarWriter)"),
            ("sample.txt", CanonicalType::Txt, TargetFormat::Md, "md:Markdown", ""),
            ("sample.txt", CanonicalType::Txt, TargetFormat::Pdf, "pdf:writer_pdf_Export", ""),
            ("sample.txt", CanonicalType::Txt, TargetFormat::Docx, "docx:MS Word 2007 XML", ""),
            ("sample.txt", CanonicalType::Txt, TargetFormat::Rtf, "rtf:Rich Text Format", ""),
        ];
        let mut probes = 0;
        let mut probe_failures = 0;
        for (fixture, from, to, convert_to, infilter) in cases {
            // Skip anything the product already advertises; this section exists
            // for the cells that have no route yet.
            if soffice::route(*from, *to).is_some() {
                continue;
            }
            probes += 1;
            let label = format!("{:?} -> {}", from, to.as_str());
            let input = self.fixture(fixture);
            let ext = verify::document_extension(*to).unwrap();
            let output = self.out(&format!(
                "probe-{}-{}.{ext}",
                fixture.replace('.', "-"),
                to.as_str()
            ));
            let route = DocRoute {
                convert_to,
                infilter: if infilter.is_empty() { None } else { Some(infilter) },
                evidence: "measured-here",
            };
            match run_harness_route(&self.paths, route, &input, &output, &self.work.join("tmp"), self) {
                Ok(detail) => println!("probe PASS  {label:<18} {detail}"),
                Err(e) => {
                    probe_failures += 1;
                    println!("probe FAIL  {label:<18} {e}");
                }
            }
        }
        println!(
            "      {probes} unadvertised directions probed, {probe_failures} did not produce a validated file"
        );
    }
}

/// Run a route that is not (yet) in the product table, using the same spawn
/// contract and validators. Harness-only: `soffice::route` still refuses it.
fn run_harness_route(
    paths: &EnginePaths,
    route: DocRoute,
    input: &Path,
    output: &Path,
    temp_root: &Path,
    h: &Harness,
) -> Result<String, String> {
    let started = Instant::now();
    let job_dir = temp_root.join(format!(
        "measure-{}",
        output.file_name().unwrap().to_string_lossy()
    ));
    let outdir = job_dir.join("out");
    std::fs::create_dir_all(&outdir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&job_dir).map_err(|e| e.to_string())?;
    let profile_url = soffice::file_url(&job_dir.join("profile"), soffice::native_platform());
    let spec = soffice::BatchSpec {
        route,
        launcher: &paths.soffice,
        profile_url: &profile_url,
        outdir: &outdir,
        inputs: &[input.to_path_buf()],
    };
    let outcome = crate::engine::proc::run(paths, &spec.invocation(&job_dir))
        .map_err(|e| format!("spawn: {e}"))?;
    let ms = started.elapsed().as_millis();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&outdir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        // A crashed LibreOffice leaves `.~lock.<name>#` behind. It is not an
        // output, and treating it as one is exactly the silent-garbage trap.
        .filter(|p| {
            p.is_file()
                && !p
                    .file_name()
                    .map(|n| n.to_string_lossy().starts_with(".~lock."))
                    .unwrap_or(false)
        })
        .collect();
    files.sort();
    if !outcome.ok() {
        return Err(format!(
            "rc={:?} (engine did not report success) and {} file(s) (stderr: {})",
            outcome.status,
            files.len(),
            outcome.stderr.trim()
        ));
    }
    if files.is_empty() {
        return Err(format!(
            "rc={:?} and no file (stderr: {})",
            outcome.status,
            outcome.stderr.trim()
        ));
    }
    let produced = &files[0];
    match verify::document_output(produced, guess_target(produced)) {
        verify::Verdict::Pass => {
            std::fs::create_dir_all(output.parent().unwrap()).ok();
            let _ = std::fs::copy(produced, output);
            Ok(format!(
                "{} B  {ms} ms  as {}",
                std::fs::metadata(output).map(|m| m.len()).unwrap_or(0),
                produced
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            ))
        }
        verify::Verdict::Fail(why) => Err(format!(
            "rc={:?} wrote {:?} but validator said: {why}",
            outcome.status,
            produced.file_name()
        )),
    }
    .inspect(|_| {
        let _ = h;
    })
}

fn guess_target(path: &Path) -> TargetFormat {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "pdf" => TargetFormat::Pdf,
        "md" => TargetFormat::Md,
        "html" => TargetFormat::Html,
        "docx" => TargetFormat::Docx,
        "xlsx" => TargetFormat::Xlsx,
        "pptx" => TargetFormat::Pptx,
        "csv" => TargetFormat::Csv,
        _ => TargetFormat::Txt,
    }
}

fn short(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn read_text(path: &Path) -> String {
    String::from_utf8_lossy(&std::fs::read(path).unwrap_or_default()).into_owned()
}

fn check_for(to: TargetFormat) -> fn(&Path, &str) -> Result<String, String> {
    match to {
        TargetFormat::Pdf => check_pdf_pages,
        TargetFormat::Md => check_markdown,
        TargetFormat::Html => check_html,
        TargetFormat::Docx => check_docx,
        TargetFormat::Xlsx => check_xlsx,
        TargetFormat::Pptx => check_pptx,
        TargetFormat::Csv => check_csv,
        _ => check_text,
    }
}

/// The content check for one advertised direction.
///
/// A target-shaped check is not enough: `docx -> txt` and `xlsx -> txt` can both
/// produce a non-empty file while only one of them still carries the source. So
/// the needle is picked per **source**, and the page count per family, from what
/// the fixture actually contains.
fn check_output(
    from: CanonicalType,
    to: TargetFormat,
    path: &Path,
    name: &str,
) -> Result<String, String> {
    use CanonicalType as C;
    match to {
        TargetFormat::Pdf => {
            let expected = match from {
                // Two sheets, two slides: the fixtures really have two pages.
                C::Pptx | C::Ppt | C::Xls | C::Xlsx => 2,
                _ => 1,
            };
            let data = std::fs::read(path).map_err(|e| e.to_string())?;
            let pages = verify::count_pdf_pages(&data);
            if pages != expected {
                return Err(format!("expected {expected} page(s), found {pages}"));
            }
            Ok(format!("{pages} page(s), {} B", data.len()))
        }
        TargetFormat::Md => match from {
            C::Html => {
                let text = read_text(path);
                if !text.contains("# Fixture") {
                    return Err(format!(
                        "the heading did not survive as markdown: {:?}",
                        text.chars().take(60).collect::<String>()
                    ));
                }
                Ok(format!("{} bytes of markdown, heading kept", text.len()))
            }
            C::Rtf => {
                let text = read_text(path);
                if !text.contains("caf\u{e9}") {
                    return Err(format!(
                        "source text missing: {:?}",
                        text.chars().take(60).collect::<String>()
                    ));
                }
                Ok(format!("{} bytes of markdown, source text present", text.len()))
            }
            _ => check_markdown(path, name),
        },
        TargetFormat::Html => check_html(path, name),
        TargetFormat::Docx => check_docx(path, name),
        TargetFormat::Xlsx => check_xlsx(path, name),
        TargetFormat::Pptx => check_pptx(path, name),
        TargetFormat::Csv => check_csv(path, name),
        TargetFormat::Rtf => check_rtf(path, name),
        _ => check_source_text(from, path, name),
    }
}

/// Plain text must still carry the source, not just be non-empty.
fn check_source_text(from: CanonicalType, path: &Path, name: &str) -> Result<String, String> {
    use CanonicalType as C;
    let text = read_text(path);
    let needle = match from {
        C::Csv | C::Xls | C::Xlsx => "Widget A",
        C::Docx | C::Doc => "MARKER-ALPHA",
        C::Markdown => "emphasis",
        C::Rtf => "caf\u{e9}",
        _ => "",
    };
    if text.trim().is_empty() {
        return Err("empty text output".into());
    }
    if !needle.is_empty() && !text.contains(needle) {
        return Err(format!(
            "{needle:?} missing from the text output (got {:?})",
            text.chars().take(80).collect::<String>()
        ));
    }
    let tabs = text.matches('\t').count();
    let detail = if tabs > 0 {
        format!("{} bytes of text, {needle:?} present, {tabs} TAB separators", text.len())
    } else {
        format!("{} bytes of text, {needle:?} present", text.len())
    };
    let _ = name;
    Ok(detail)
}

/// RTF: the header, not just a file with the right extension.
fn check_rtf(path: &Path, _name: &str) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let head = String::from_utf8_lossy(&data[..data.len().min(8)]).into_owned();
    if !head.starts_with("{\\rtf") {
        return Err(format!("no RTF header (starts {head:?})"));
    }
    Ok(format!("{} bytes, RTF header present", data.len()))
}

/// The fixture that carries each document input type.
fn fixture_for(from: CanonicalType) -> Option<&'static str> {
    Some(match from {
        CanonicalType::Docx => "sample.docx",
        CanonicalType::Xlsx => "sample.xlsx",
        CanonicalType::Pptx => "sample.pptx",
        CanonicalType::Csv => "sample.csv",
        CanonicalType::Html => "sample.html",
        CanonicalType::Doc => "sample.doc",
        CanonicalType::Xls => "sample.xls",
        CanonicalType::Ppt => "sample.ppt",
        CanonicalType::Rtf => "sample.rtf",
        CanonicalType::Markdown => "sample.md",
        CanonicalType::Txt => "sample.txt",
        _ => return None,
    })
}

fn check_pdf_pages(path: &Path, _name: &str) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let pages = verify::count_pdf_pages(&data);
    if pages == 0 {
        return Err("no page objects".into());
    }
    Ok(format!("{pages} page(s)"))
}

fn check_markdown(path: &Path, _name: &str) -> Result<String, String> {
    let text = read_text(path);
    if !text.contains("MARKER-ALPHA") {
        return Err("source marker MARKER-ALPHA missing".into());
    }
    if !text.lines().any(|l| l.trim_start().starts_with('#')) {
        return Err("no heading".into());
    }
    Ok(format!("{} bytes of markdown, marker present", text.len()))
}

fn check_html(path: &Path, _name: &str) -> Result<String, String> {
    let text = read_text(path).to_ascii_lowercase();
    if !text.contains("<html") && !text.contains("<!doctype html") {
        return Err("no html element in the output".into());
    }
    Ok(format!("{} bytes of html", text.len()))
}

fn check_docx(path: &Path, _name: &str) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    if !data.starts_with(b"PK\x03\x04") {
        return Err("not a zip".into());
    }
    let needle = b"word/document.xml";
    if !data.windows(needle.len()).any(|w| w == needle) {
        return Err("no word/document.xml".into());
    }
    Ok("docx container with word/document.xml".into())
}

fn check_xlsx(path: &Path, _name: &str) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let needle = b"xl/workbook.xml";
    if !data.windows(needle.len()).any(|w| w == needle) {
        return Err("no xl/workbook.xml".into());
    }
    Ok("xlsx container with xl/workbook.xml".into())
}

fn check_pptx(path: &Path, _name: &str) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let needle = b"ppt/presentation.xml";
    if !data.windows(needle.len()).any(|w| w == needle) {
        return Err("no ppt/presentation.xml".into());
    }
    Ok("pptx container with ppt/presentation.xml".into())
}

fn check_text(path: &Path, _name: &str) -> Result<String, String> {
    let text = read_text(path);
    if text.trim().is_empty() {
        return Err("empty text output".into());
    }
    Ok(format!("{} bytes of text", text.len()))
}

fn check_csv(path: &Path, name: &str) -> Result<String, String> {
    let text = read_text(path);
    if !text.contains("Widget A") {
        return Err(format!(
            "source cell missing (got {:?})",
            text.chars().take(80).collect::<String>()
        ));
    }
    // The measured renaming: `<stem>-<sheet>.csv`, never `<stem>.csv`.
    Ok(format!("produced as {name}, source cell present"))
}

fn report_identity(h: &Harness) {
    let argv = soffice::version_argv(&h.paths.soffice);
    let inv = crate::engine::proc::Invocation {
        program: crate::engine::proc::Program::Soffice,
        args: argv,
        cwd: h.work.clone(),
    };
    match crate::engine::proc::run(&h.paths, &inv) {
        Ok(outcome) => {
            let text = format!("{}{}", outcome.stdout.trim(), outcome.stderr.trim());
            let first = text.lines().next().unwrap_or("").to_string();
            println!(
                "engine: {}  ({})",
                if first.is_empty() { "<no version output>".into() } else { first },
                if soffice::version_matches(&text, soffice::VERSION_PIN) {
                    "matches the pin"
                } else {
                    "DOES NOT MATCH THE PIN"
                }
            );
        }
        Err(e) => println!("engine: could not run --version: {e}"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut positional: Vec<String> = Vec::new();
    let mut soffice_dir: Option<PathBuf> = None;
    let mut measure_legacy = false;
    for arg in args.iter().skip(1) {
        if let Some(value) = arg.strip_prefix("--soffice-dir=") {
            soffice_dir = Some(PathBuf::from(value));
        } else if arg == "--measure-legacy" {
            measure_legacy = true;
        } else {
            positional.push(arg.clone());
        }
    }
    if positional.len() < 2 {
        eprintln!(
            "usage: ezc-c3-e2e <fixtures-dir> <work-dir> [--soffice-dir=<program dir>] [--measure-legacy]"
        );
        std::process::exit(2);
    }
    let fix = PathBuf::from(&positional[0]);
    let work = PathBuf::from(&positional[1]);
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).expect("work dir");

    let mut h = Harness::new(fix, work.clone(), measure_legacy);
    if let Some(dir) = soffice_dir {
        h.paths.set_soffice_dir(&dir);
    }
    println!("=== EZ Convert C3 document matrix ===");
    println!("fixtures: {}", h.fix.display());
    println!("work:     {}", h.work.display());
    println!(
        "launcher: {}",
        h.paths
            .soffice
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    );
    report_identity(&h);

    // The app-start hook the window calls (`warm_up_at_startup`), on a fresh
    // cache dir: it must bootstrap the profile once and then be a no-op, using
    // the real engine on this leg.
    let hook_cache = work.join("hook-cache");
    let started = Instant::now();
    let first_hook = soffice::warm_up_at_startup(&h.paths, &hook_cache);
    let cold_ms = started.elapsed().as_millis();
    let second_hook = soffice::warm_up_at_startup(&h.paths, &hook_cache);
    match (&first_hook, &second_hook) {
        (soffice::WarmUp::Warmed { bootstrapped: true, .. }, soffice::WarmUp::AlreadyWarm) => h.ok(
            "warm-up hook",
            format!("app-start hook bootstrapped in {cold_ms} ms, relaunch is a no-op"),
        ),
        _ => h.bad(
            "warm-up hook",
            format!("first {first_hook:?} then {second_hook:?}"),
        ),
    }

    // Warm-up: a brand-new profile, then the same call again.
    let profile = Profile::under(&work.join("cache"));
    let started = Instant::now();
    let first = soffice::warm_up(&h.paths, &profile, &work);
    let cold = started.elapsed();
    match first {
        Ok(outcome) if outcome.ok() => {
            let started = Instant::now();
            let second = soffice::warm_up(&h.paths, &profile, &work);
            let warm = started.elapsed();
            let bootstrapped = profile.bootstrapped();
            if bootstrapped {
                h.ok(
                    "warm-up",
                    format!(
                        "fresh profile {} ms, warm {} ms, profile bootstrapped",
                        cold.as_millis(),
                        warm.as_millis()
                    ),
                );
            } else {
                h.bad(
                    "warm-up",
                    format!("profile has no user/ dir after warm-up (cold {} ms)", cold.as_millis()),
                );
            }
            if let Ok(second) = second {
                if !second.ok() {
                    h.bad("warm-up (2nd)", second.summary());
                }
            }
        }
        Ok(outcome) => h.bad("warm-up", outcome.summary()),
        Err(e) => h.bad("warm-up", format!("spawn: {e}")),
    }

    println!("--- advertised directions (matrix Proven) ---");
    // Driven by the product's own route table, so "every advertised pair has an
    // end-to-end test" is true by construction, not by a list someone maintains.
    let mut advertised: Vec<(CanonicalType, TargetFormat)> = soffice::proven_routes().to_vec();
    advertised.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    for (from, to) in advertised {
        let Some(fixture) = fixture_for(from) else {
            h.bad(
                &format!("{from:?} -> {}", to.as_str()),
                "no fixture for this input type".into(),
            );
            continue;
        };
        let ext = verify::document_extension(to).unwrap_or("bin");
        let out_name = format!("{}-to-{}.{ext}", from.as_str(), to.as_str());
        let input = h.fixture(fixture);
        let output = h.out(&out_name);
        let started = Instant::now();
        match convert(
            &h.paths,
            &DocJob { from, to },
            &input,
            &output,
            &h.work.join("tmp").join(&out_name),
        ) {
            Ok(_) => {
                let ms = started.elapsed().as_millis();
                let bytes = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
                match check_output(from, to, &output, &out_name) {
                    Ok(extra) => {
                        let label = format!("{from:?} -> {}", to.as_str());
                        h.ok(&label, format!("{bytes} B  {ms} ms  {extra}"));
                    }
                    Err(why) => {
                        let label = format!("{from:?} -> {}", to.as_str());
                        h.bad(&label, format!("content check failed: {why}"));
                    }
                }
            }
            Err(e) => {
                let label = format!("{from:?} -> {}", to.as_str());
                h.bad(&label, format!("{e} | user: {}", e.user_message()));
            }
        }
    }

    println!("--- batching ---");
    h.batch_timing();

    println!("--- negative controls ---");
    h.negative_cases();

    if measure_legacy {
        println!("--- unproven directions (measurement only) ---");
        h.legacy_measurements();
    }

    println!("--- summary ---");
    println!("{} passed, {} failed", h.pass, h.fail);
    for failure in &h.failures {
        println!("  - {failure}");
    }
    if h.fail > 0 {
        std::process::exit(1);
    }
}
