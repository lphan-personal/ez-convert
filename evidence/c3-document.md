# EVIDENCE — C3: document pipeline (LibreOffice sidecar)

Card: `t_ea95c826` (C3). Date: 2026-11-01.
Engine: **LibreOffice 26.8.2.2** (`9dea406ab7695df6325b11b0e19fdca8db4b7276`) on **both** legs —
the pinned version from `SPEC.md` §6.

Two legs, the **same Rust code**:

| leg | host | engine location | how it was run |
|---|---|---|---|
| Linux | this container, Debian 13, no display | TDF tarball unpacked to `/opt/data/converter/vendor/libreoffice-26.8.2` + `lo-deps` for the X11 libs `oosplash` links | `rustc` harness, `spikes/c3/run-e2e.sh` |
| Windows | `win11-dev` VM, 10.0.0.191, Windows 11 Pro 26200, 6 vCPU | `C:\Program Files\LibreOffice\program` | the **same** harness cross-compiled with `x86_64-pc-windows-gnu` + a Linux-hosted mingw, run over SSH |

The Windows leg is not a PowerShell re-implementation: it is `engine::doc::convert`, the same
function the job worker calls, compiled for `x86_64-pc-windows-gnu` and executed on the guest.
That is what makes the `soffice.com` launcher rule and the `file:///C:/…` profile URL real
rather than asserted.

## 1. Advertised directions (matrix `Proven`) — 22/22 PASS on both legs

The advertised set is driven by `engine::soffice::proven_routes()`, so this table is the whole
product surface for documents, not a sample. Each case runs `engine::doc::convert` and then a
content check that a broken conversion cannot fake: page count, OOXML container part
(`word/document.xml`, `xl/workbook.xml`, `ppt/presentation.xml`), the `<html` element, the source
marker `MARKER-ALPHA` for Markdown, the source cell `Widget A` for CSV, and the measured
`<stem>-<sheet>` naming.

```
direction        Linux                    Windows
csv  -> xlsx     5,804 B    694 ms       5,796 B  2,411 ms
doc  -> docx     8,771 B    497 ms       8,791 B  1,845 ms
doc  -> html     4,352 B    482 ms       4,427 B  1,589 ms
doc  -> md         385 B    497 ms         400 B  1,570 ms
doc  -> pdf     26,460 B    512 ms      46,489 B  1,678 ms
doc  -> txt        341 B    482 ms         349 B  1,614 ms
docx -> html     4,064 B    573 ms       4,134 B  2,063 ms
docx -> md         389 B    527 ms         404 B  1,846 ms
docx -> pdf     26,460 B    542 ms      46,489 B  1,906 ms
html -> docx     5,358 B    467 ms       5,380 B  1,448 ms
ppt  -> pdf     33,915 B    618 ms      79,102 B  2,118 ms
ppt  -> pptx    10,252 B    543 ms      10,257 B  1,722 ms
pptx -> pdf     33,957 B    678 ms      79,145 B  1,982 ms
rtf  -> docx     5,015 B    498 ms       5,041 B  1,815 ms
rtf  -> html       668 B    483 ms         687 B  1,797 ms
rtf  -> pdf     13,100 B    498 ms      13,335 B  1,799 ms
rtf  -> txt         33 B    483 ms          34 B  1,784 ms
xls  -> csv        147 B    498 ms         151 B  1,575 ms
xls  -> html     5,235 B    498 ms       5,330 B  1,571 ms
xls  -> pdf     26,212 B    513 ms      41,142 B  1,933 ms
xls  -> xlsx     7,327 B    513 ms       7,330 B  1,571 ms
xlsx -> csv        147 B    528 ms         151 B  1,603 ms
```

Page-count assertions: `docx -> pdf`, `doc -> pdf`, `rtf -> pdf` are 1 page 612×792 pt;
`pptx -> pdf` and `ppt -> pdf` are 2 pages 720×540 pt; `xls -> pdf` is 2 pages (two sheets).

Byte counts match the S3 spike on Linux exactly for the seven it measured (26,460 / 389 / 4,064
/ 147 / 33,957) and on Windows for the text/container outputs (404 / 4,134 / 151). The Windows
PDFs are larger than S3's Windows leg recorded (39,946 B and 56,471 B): this VM's font set
differs from the stakeholder's PC. Sizes are not a contract; page counts and content markers
are.

## 2. Batching — measured, and it is the reason the pipeline is batch-first

8 identical `sample.docx` inputs, one route, one outdir, **one** spawn, versus 3 inputs each with
its own spawn (same route, same profile):

```
Linux    batch 8:   830 ms  = 104 ms/file     3 individual: 1,630 ms = 543 ms/file   5.24x
Windows  batch 8: 3,716 ms  = 465 ms/file     3 individual: 5,659 ms = 1,886 ms/file  4.06x
```

The card's planning numbers (0.20 s/file batched vs 0.52 s/file spawned) are the same effect
measured on a faster box; the direction and the magnitude class reproduce.

## 3. Warm-up (`--terminate_after_init`), and where it is called

A brand-new `-env:UserInstallation` profile, then the same call again:

```
Linux    fresh profile  90 ms    warm  90 ms   profile bootstrapped (user/ present)
Windows  fresh profile 262 ms    warm 170 ms   profile bootstrapped (user/ present)
```

The install-cold **first conversion** is the number that hurts (S3 measured 5,111 ms on Windows);
the warm-up exists so that cost is paid at startup, not on the user's first job. It is not a
capability probe and its failure is never fatal — the job still runs.

It is wired, not just available: `lib.rs`'s `.setup()` spawns a thread that calls
`soffice::warm_up_at_startup(paths, cache_dir)` once per launch, and that function returns
`NotInstalled` / `AlreadyWarm` / `Warmed { millis, bootstrapped }` / `Failed(reason)`. Two rules
make it safe to call unconditionally:

- **bootstrap once** — `Profile::bootstrapped()` (the `user/` directory LibreOffice writes) is
  the cheap "nothing to pay" signal, so a relaunch spawns nothing at all (asserted);
- **`rc=0` is not the claim** — the result is read back from the profile, so a launcher that
  exits 0 and writes nothing is `Failed`, never `Warmed` (asserted with a stub launcher).

The e2e's `warm-up hook` case runs the real app-start function against the real engine on both
legs: Linux 90 ms, Windows 201 ms, and the second call is `AlreadyWarm` on both.

App-start call site verification, stated exactly: the Tauri crate is **compile-checked** for
`x86_64-pc-windows-gnu` (`cargo check --locked --all-targets`, clean, after proving the checker
reaches the new closure with a deliberately-broken line), and the decision logic is unit-tested
on both legs and exercised against real LibreOffice by the e2e. The GUI itself was **not** run
here — this box has no WebKitGTK, so "the window starts and prints the warm-up line" is a gap,
not a claim.

## 4. Negative controls — the acceptance criteria

| # | situation | what the pipeline did |
|---|---|---|
| N3 | `text_renamed.docx` (plain text, `.docx` name) — the measured silent-garbage trap: LibreOffice converts it to a valid-looking 12,388 B PDF at `rc=0` with **empty stderr** | detection says `Txt`, the matrix offers **no** document targets, and the pipeline refuses with `NotRouted` **before spawning**. Asserted on both legs. |
| N5 | engine exits 0 and writes no file | `DocError::NoOutput` with the outdir listing; nothing is published. Unit test with a stub engine. |
| — | engine exits 0 and writes a file that is not what it claims | `OutputInvalid`; the file is deleted, nothing is published. Unit test with a stub engine. |
| N2 | missing input | refused at path admission, `rc=1` never reached. |
| — | structurally damaged docx (`truncated.docx`, central directory cut) | detection returns `Unknown` (no container directory), so there is no route and nothing is spawned. The **residual** case stays open: a ZIP with a valid directory and a broken `word/document.xml` passes detection and can still produce a plausible PDF — not claimed as caught (`ARCHITECTURE.md` §6.0.5). |

## 5. What was NOT measured

- macOS: no LibreOffice leg exists for it (`SPEC.md` §6 pins a macOS engine, `P2` owns it).
- Encrypted/password-protected documents: no fixture, no route claim.
- Very large documents: every fixture here is < 40 KB; the 120 s timeout is a hang guard, not a
  measurement.
- Legacy `.doc`/`.xls`/`.ppt`/`.rtf`: measured (§6) and now advertised for the directions in §6;
  the remaining legacy cells were not measured and stay `Planned`.

## 6. Legacy Office and rtf — measured on both legs, now advertised

These were `Planned` (`PLANNED_LEGACY`, owner C3) with the hypothesis "same LibreOffice
filters as the OOXML rows, no legacy fixture on either platform". The fixtures exist
(`fixtures/sample.doc`, `.xls`, `.ppt`, `.rtf`) and both legs ran them through the same
pipeline and validators. Each line below is the marker `model::matrix` cites, so a stale claim
fails the matrix test rather than rotting in a document.

```
doc -> pdf PASS    26,460 B / 542 ms (Linux)    46,489 B / 1,922 ms (Windows)
doc -> md PASS        385 B / 527 ms                400 B / 1,614 ms
doc -> html PASS    4,352 B / 512 ms              4,427 B / 1,614 ms
doc -> docx PASS    8,771 B / 497 ms              8,791 B / 1,613 ms
doc -> txt PASS       341 B / 557 ms                349 B / 1,982 ms   (txt:Text (encoded):UTF8)
xls -> pdf PASS    26,212 B / 512 ms             41,142 B / 1,996 ms   (pdf:calc_pdf_Export)
xls -> csv PASS       147 B / 482 ms                151 B / 1,538 ms   (named <stem>-<sheet>.csv)
xls -> xlsx PASS    7,327 B / 512 ms              7,330 B / 1,660 ms
xls -> html PASS    5,235 B / 497 ms              5,330 B / 1,675 ms   (html:HTML (StarCalc))
ppt -> pdf PASS    33,915 B / 558 ms             79,102 B / 1,751 ms   (pdf:impress_pdf_Export)
ppt -> pptx PASS    produced at rc=0             produced at rc=0      (pptx:Impress MS PowerPoint 2007 XML)
rtf -> pdf PASS    13,100 B / 512 ms             13,335 B / 1,994 ms
rtf -> docx PASS    5,015 B / 497 ms              5,041 B / 1,817 ms
rtf -> html PASS      668 B / 512 ms                687 B / 1,858 ms
rtf -> txt PASS        33 B / 497 ms                 34 B / 1,934 ms
```

The filter is per **family**, and that is not cosmetic: a Calc document exported with
`pdf:writer_pdf_Export` is not what produced these files — `xls -> pdf` measured with
`pdf:calc_pdf_Export`, `ppt -> pdf` with `pdf:impress_pdf_Export`, `xls -> html` with
`html:HTML (StarCalc)`. This is also why the single shared `R_DOC_LEGACY` row was split into
`R_DOC` / `R_XLS` / `R_PPT`: one row would have advertised `xls -> csv` for a `.doc`.

Still not advertised, and still `Planned` with owner C3. These were probed with the same
family filters the proven rows use and **did not produce a validated file**, so the hypothesis
"same filters, just no fixture" is falsified for them:

- `xls -> md`, `xls -> txt`, `ppt -> md`, `ppt -> html`, `ppt -> txt`, `csv -> txt` —
  LibreOffice answers `rc=1` and writes nothing:
  `Error: Please verify input parameters... (SfxBaseModel::impl_store … failed: 0xc10 (Error
  Area:Io Class:Write Code:16))`. That is the *writer* filter name being handed to a Calc or
  Impress document — the filter is per family, so these need a family filter nobody has
  measured yet, not a re-run.
- `doc -> csv` stays `Cut`: a Word document is not a table.

## 7. Directions measured but deliberately left un-advertised

These pass on Linux and are outside C3's card body (they belong to a card that will also decide
the wheel-side ordering): `docx -> txt` (341 B), `xlsx -> pdf` (26,211 B), `xlsx -> html`
(5,655 B), `html -> pdf` (17,997 B), `html -> txt` (27 B), `md -> pdf` (40,527 B),
`md -> html` (1,336 B), `txt -> pdf` (14,522 B), `txt -> docx` (5,117 B), `rtf -> md` (30 B),
`csv -> pdf` (19,362 B — note `pdf:calc_pdf_Export`, not the writer filter).
`csv -> txt` fails with the writer text filter (`Class:Write Code:16`), i.e. it needs the Calc
text filter, same finding as §6.

## 8. Reproduce

```bash
spikes/c3/run-e2e.sh                    # linux leg
spikes/c3/run-e2e.sh --windows          # cross-compiled, run on win11-dev over ssh
spikes/c3/run-e2e.sh --measure-legacy   # add the still-Planned probes
spikes/c3/run-win-unit-tests.sh         # the unit tests themselves, compiled for
                                        # x86_64-pc-windows-gnu and run on the guest
scripts/test-engine.sh                  # unit tests, no LibreOffice needed (stub engines)
```

Unit-test result on the guest (same code, `--target x86_64-pc-windows-gnu`):
**159 passed, 0 failed** — 4 fewer than Linux because the stub-engine tests are `cfg(unix)`.
That run also caught a genuine test-portability bug: `traversal_is_refused` asserted on a
hard-coded `/tmp/../etc/passwd`, which is not absolute on Windows, so the test was asserting
`NotAbsolute` instead of `Traversal`. Fixed to build the path from `temp_dir()`.

## 9. Re-verification (2026-11-01T21:40Z, `main` @ `bb93d5a` + the app-start hook)

Both legs and the Windows unit-test run were re-run at the pushed head, not trusted from the
earlier session's log, and the transcripts in `spikes/c3/transcripts/` are those runs:

| run | result |
|---|---|
| `spikes/c3/run-e2e.sh` (Linux) | `30 passed, 0 failed`; batch 8 = 107 ms/file vs 543 ms/file spawned (5.05x); app-start hook 90 ms |
| `spikes/c3/run-e2e.sh --windows` (win11-dev, 10.0.0.191) | `30 passed, 0 failed`; batch 8 = 475 ms/file vs 1,954 ms/file (4.12x); app-start hook 201 ms |
| `spikes/c3/run-win-unit-tests.sh` | `160 passed, 0 failed` |
| `scripts/test-engine.sh` (Linux) | `167 passed, 0 failed` |
| `scripts/test-model.sh` (Linux) | `69 passed, 0 failed` |
| `cargo check --locked --all-targets --target x86_64-pc-windows-gnu` | clean (exit 0) |

The 30th e2e case is the app-start warm-up hook added in this round; the earlier 29 numbers are
unchanged. The only other change since the first run is a `RouteStatus::Planned` hypothesis
string, so the conversion numbers are expected to match — they do.

## 10. C3b — the remaining document cells: 20 flipped, 13 cut, 0 left Planned (2026-11-01)

Card `t_f8d19f60`. Scope: the 33 cells that were still `Planned` after C3. Same two legs, same
Rust code, same pinned engine as §1 (`LibreOffice 26.8.2.2`):

```bash
spikes/c3/run-e2e.sh --measure-legacy              # Linux   50 passed, 0 failed
spikes/c3/run-e2e.sh --windows --measure-legacy    # Windows 50 passed, 0 failed (10.0.0.191)
```

Transcripts of exactly these runs: `spikes/c3/transcripts/c3b-linux-e2e.txt` and
`spikes/c3/transcripts/c3b-windows-e2e.txt`.

The advertised set is still driven by `soffice::proven_routes()`, which is now **42** directions
(22 from C3 + 20 from C3b), so the e2e's per-direction list *is* the product surface. The probe
section (13 cases) is the `Cut` set: it runs the same spawn contract and the same validator, and
its failures are the measurement behind each cut.

### 10.1 Flipped to `Proven` — 20 directions, both legs

Each line below is the marker `model::matrix` cites for that direction, so a stale claim fails
the matrix test instead of rotting in a document.

```
docx -> txt PASS                 341 B /   558 ms (Linux)         349 B /  1888 ms (Windows)   (txt:Text (encoded):UTF8)
docx -> rtf PASS              22,392 B /   558 ms (Linux)      22,622 B /  1934 ms (Windows)   (rtf:Rich Text Format)
doc -> rtf PASS               23,116 B /   513 ms (Linux)      23,338 B /  1632 ms (Windows)   (rtf:Rich Text Format)
rtf -> md PASS                    30 B /   498 ms (Linux)          31 B /  1826 ms (Windows)   (md:Markdown)
md -> pdf PASS                40,527 B /   498 ms (Linux)      40,507 B /  1646 ms (Windows)   (pdf:writer_pdf_Export)
md -> txt PASS                   124 B /   497 ms (Linux)         130 B /  1542 ms (Windows)   (txt:Text (encoded):UTF8)
md -> docx PASS                6,363 B /   498 ms (Linux)       6,384 B /  1602 ms (Windows)   (docx:MS Word 2007 XML)
md -> html PASS                1,336 B /   513 ms (Linux)       1,541 B /  1554 ms (Windows)   (html:HTML (StarWriter))
md -> rtf PASS                 5,505 B /   512 ms (Linux)       5,715 B /  1556 ms (Windows)   (rtf:Rich Text Format)
html -> pdf PASS              18,453 B /   467 ms (Linux)      18,442 B /  1524 ms (Windows)   (pdf:writer_pdf_Export + infilter HTML (StarWriter))
html -> md PASS                   27 B /   467 ms (Linux)          30 B /  1494 ms (Windows)   (md:Markdown + infilter HTML (StarWriter))
html -> rtf PASS               3,254 B /   482 ms (Linux)       3,427 B /  1526 ms (Windows)   (rtf:Rich Text Format + infilter HTML (StarWriter))
xlsx -> pdf PASS              26,211 B /   558 ms (Linux)      41,077 B /  1982 ms (Windows)   (pdf:calc_pdf_Export)
xlsx -> txt PASS                 145 B /   543 ms (Linux)         149 B /  1650 ms (Windows)   (calc text filter, TAB (9))
xlsx -> html PASS              5,655 B /   543 ms (Linux)       5,750 B /  1554 ms (Windows)   (html:HTML (StarCalc))
xls -> txt PASS                  145 B /   497 ms (Linux)         149 B /  1542 ms (Windows)   (calc text filter, TAB (9))
csv -> pdf PASS               19,362 B /   528 ms (Linux)      19,352 B /  1937 ms (Windows)   (pdf:calc_pdf_Export)
csv -> txt PASS                  105 B /   498 ms (Linux)         109 B /  1515 ms (Windows)   (calc text filter, TAB (9))
pptx -> html PASS                917 B /   693 ms (Linux)         928 B /  1875 ms (Windows)   (html:impress_html_Export)
ppt -> html PASS                 932 B /   528 ms (Linux)         943 B /  1714 ms (Windows)   (html:impress_html_Export)
```

Four filters are **not** interchangeable, and each was measured the hard way before it was
adopted:

- **`xls/xlsx/csv -> txt` is the Calc text filter, not the Writer one.** `txt:Text (encoded):UTF8`
  answers `rc=1 … Class:Write Code:16` on a spreadsheet; the working literal is
  `txt:Text - txt - csv (StarCalc):9,34,76,1,,0,false,true,true,false,false,1`, i.e. the Calc text
  filter with a **TAB** field separator (9), not the comma (44) `csv -> xlsx` uses. The engine
  factorises it as `soffice::CALC_TEXT_FILTER`, and the output keeps LibreOffice's measured
  `<stem>-<sheet>.txt` naming (`sample-Data.txt`, `sample-sample.txt`).
- **`ppt/pptx -> html` is `html:impress_html_Export`.** The writer name
  (`html:HTML (StarWriter)`) answers `Class:Write Code:16`, and the documented UI name
  `HTML (StarImpDoc)` answers `Parameter Code:26`. The internal filter name is what works, and it
  writes a single self-contained `<file>.html` (932 B / 917 B), verified by the `<!doctype html`
  check.
- **Every `.html` input carries `--infilter=HTML (StarWriter)`**, for all four targets, because
  that is how the three new ones were measured (and D6 already worked that way).
- **`xlsx -> pdf` and `csv -> pdf` need `pdf:calc_pdf_Export`**, the same family rule C3 measured
  for `xls -> pdf`. `xlsx -> pdf` is 2 pages (two sheets), `csv -> pdf` is 1.

The content checks were made per **source**, not per target, or the pass would be vacuous:
`docx -> txt` and `xlsx -> txt` both produce text, so the e2e looks for `MARKER-ALPHA` and
`Widget A` respectively; `xlsx/xls -> txt` also asserts the TAB separators exist; `rtf -> md`
looks for the source string (a 30 B file has no heading to look for); `html -> md` asserts the
`# Fixture` heading survived; and every `rtf` output must start with `{\rtf` **and** contain a
document body (`\fonttbl`/`\par`) — a validator that did not exist before this card and that
`verify::document_output` now enforces, with a stub test proving `{\rtf1}` alone fails.

### 10.2 Cut on a measurement — 13 cells

Verbatim, from the probe section of the two transcripts above (paths elided to `…`, which is the
only edit; the full lines are in the committed transcripts):

```
Linux
probe FAIL  Xls -> md          rc=Some(1) (engine did not report success) and 0 file(s) [stderr: … 0xc10(Error Area:Io Class:Write Code:16)]
probe FAIL  Xlsx -> md         rc=Some(1) (engine did not report success) and 0 file(s) [stderr: … 0xc10(Error Area:Io Class:Write Code:16)]
probe FAIL  Pptx -> md         rc=Some(1) (engine did not report success) and 0 file(s) [stderr: … 0xc10(Error Area:Io Class:Write Code:16)]
probe FAIL  Ppt -> md          rc=Some(1) (engine did not report success) and 0 file(s) [stderr: … 0xc10(Error Area:Io Class:Write Code:16)]
probe FAIL  Ppt -> txt         rc=Some(1) (engine did not report success) and 0 file(s) [stderr: … 0xc10(Error Area:Io Class:Write Code:16)]
probe FAIL  Pptx -> txt        rc=Some(1) (engine did not report success) and 0 file(s) [stderr: … 0xc10(Error Area:Io Class:Write Code:16)]
probe FAIL  Pdf -> txt         rc=Some(1) (engine did not report success) and 0 file(s) [stderr: … 0xc10(Error Area:Io Class:Write Code:16)]
probe FAIL  Docx -> xlsx       rc=Some(134) (engine did not report success) and 1 file(s) (stderr: Unspecified Application Error)
probe PASS  Html -> txt        27 B  467 ms  as sample.txt
probe PASS  Txt -> md          174 B  497 ms  as sample.md
probe PASS  Txt -> pdf         14522 B  497 ms  as sample.pdf
probe PASS  Txt -> docx        5117 B  497 ms  as sample.docx
probe PASS  Txt -> rtf         3423 B  512 ms  as sample.rtf

Windows
probe FAIL  Xls -> md / Xlsx -> md / Pptx -> md / Ppt -> md / Ppt -> txt / Pptx -> txt / Pdf -> txt
            rc=Some(1), 0 files, stderr … 0xc10(Error Area:Io Class:Write Code:16)   (same seven)
probe FAIL  Docx -> xlsx       rc=Some(0) wrote Some("lu765651v5f.tmp") but validator said: … is zero bytes
probe PASS  Html -> txt        29 B  1585 ms  as sample.txt
probe PASS  Txt -> md          181 B  1659 ms  as sample.md
probe PASS  Txt -> pdf         14513 B  1706 ms  as sample.pdf
probe PASS  Txt -> docx        5143 B  1645 ms  as sample.docx
probe PASS  Txt -> rtf         3604 B  1632 ms  as sample.rtf
```

What that means, per cell (the matrix carries these as the `Cut` reason):

| cell(s) | measured finding | verdict |
|---|---|---|
| `xls -> md`, `xlsx -> md`, `ppt -> md`, `pptx -> md` | `rc=1`, `Class:Write Code:16` on both legs — only Writer registers the `md:Markdown` export filter, so a Calc or Impress document cannot become markdown through the sidecar | **Cut** |
| `ppt -> txt`, `pptx -> txt` | `rc=1`, `Class:Write Code:16` — Impress registers no plain-text export filter (tried the writer text filter *and* the Calc text filter) | **Cut** |
| `pdf -> txt` | `rc=1`, `Class:Write Code:16` — a PDF imports as a Draw document, so neither text filter applies; PDF text extraction is a different product (`§8.3`) | **Cut** |
| `docx -> xlsx` | not slow, not empty: **Linux aborts** (`rc=134`, `Unspecified Application Error`, only a `.~lock.` file left), **Windows answers rc=0 and leaves a zero-byte `.tmp`**. No workbook on either leg | **Cut** |
| `html -> txt` | PASS 27 B / 29 B, but it is `html -> md` with the heading stripped (`# Fixture` is kept by md, measured in the same run) | **Cut** (product) |
| `txt -> md` | PASS 174 B / 181 B — LibreOffice's markdown export wraps the *entire* file in a fenced code block, which is not a txt→md conversion | **Cut** (product) |
| `txt -> pdf` 14,522 / 14,513 B, `txt -> docx` 5,117 / 5,143 B, `txt -> rtf` 3,423 / 3,604 B | PASS on both legs, and held back on purpose: offering a `.txt` source turns the N3 trap (plain text renamed `.docx`, detected as `Txt`) from a **refusal** into a conversion, and that refusal is a shipped C3 acceptance | **Cut**, pending the stakeholder decision in §10.4 |

The probe itself had to be fixed before it could be trusted: on Linux the aborted `docx -> xlsx`
leaves a `.~lock.sample.xlsx#` behind, and the first version of `run_harness_route` accepted that
lock file as "a produced file that passes the text validator" — a false PASS. The probe now
(a) ignores `.~lock.*` and (b) treats a non-zero exit as a failure regardless of what is in the
outdir. `doc::list_with_extension` was already safe (it matches on the exact target extension),
so no engine change was needed for that case.

### 10.3 The wheel: what is offered, and in what order

`offered_targets` returns `Proven` targets in matrix order, so **row order is wheel order**. That
made the ordering question decidable in data instead of prose: `model::matrix::wheel_rank(from,
to)` ranks targets by intent **inside the source's family**, and
`document_wheel_order_follows_the_intent_rank` fails if a row drifts or a source exceeds the
6-segment cap (`ARCHITECTURE.md` §10.2, `SPEC.md` §2):

```
Writer  (docx, doc, rtf, md, html):  pdf, md, txt, docx, html, rtf
Calc    (xlsx, xls, csv):            pdf, csv, xlsx, txt, html
Impress (pptx, ppt):                 pdf, pptx, html
```

- `docx` → **pdf, md, txt, html, rtf** (5 segments)
- `doc` → **pdf, md, txt, docx, html, rtf** (6 — the one source that fills the cap)
- `xlsx` → **pdf, csv, txt, html**; `xls` → **pdf, csv, xlsx, txt, html**; `csv` → **pdf, xlsx, txt**
- `html` → **pdf, md, docx, rtf**; `rtf` → **pdf, md, txt, docx, html**; `md` → **pdf, txt, docx,
  html, rtf**
- `pptx` → **pdf, html**; `ppt` → **pdf, pptx, html**
- `txt` → **nothing** (the N3 decision above); `pdf` → nothing (unchanged)

### 10.4 Decisions this card makes, and the one that needs a signature

1. **`md -> html` and `html -> md` do not use an in-process crate.** Both cells were `Planned`
   against an unmeasured comrak/htmd path; the sidecar writes and reads markdown on both legs
   (1,336 B / 1,541 B, `# Fixture` preserved), so the matrix now says `Soffice` and the crate rows
   are gone. Fewer dependencies, one measured mechanism.
2. **`html -> txt` is cut in favour of `html -> md`**, and **`txt -> md` is cut as no-gain**
   (§10.2). Both are one-line flips if the stakeholder disagrees.
3. **A `.txt` source offers nothing in v1 — this one needs a signature.** The measurement says
   text→pdf/docx/rtf all work. The blocker is a design promise, not a filter: `ARCHITECTURE.md`
   §6.0.5 says detection stops the N3 trap at the door, and `engine::doc`'s
   `a_text_file_renamed_docx_never_reaches_soffice` asserts the wheel offers no document target
   for it. Offering `txt -> pdf` replaces that refusal with a conversion (the text would be
   imported as text, so the output is *correct* — the trap is only that the user is no longer
   told the file is misnamed). That is a product call, so the cells are `Cut` and this card does
   not change the acceptance unilaterally.
4. **`docx -> xlsx` is cut as a crash, not a "not yet".** Recording it as `Planned` would have
   implied a filter nobody has measured is the only obstacle; the measurement is an abort.

### 10.5 What this card did not measure

- macOS: still no LibreOffice leg (`SPEC.md` §6 pins the engine, `P2` owns it).
- Very large documents, encrypted documents, Vietnamese stacked diacritics: unchanged from §5.
- The `.rtf` **output** path is validated structurally (`{\rtf` + body) but no real RTF consumer
  was asked to open the file — `S4`'s viewer-fidelity harness covers PDF only.
- `xls -> md` on a document with a single sheet was not re-run; the four `-> md` failures are the
  same missing filter on both legs and on both legacy and OOXML Calc inputs, which is enough to
  cut the pair, not enough to claim any spreadsheet can never be markdown.

### 10.6 Verification runs (all at the tree this section describes)

| run | result |
|---|---|
| `spikes/c3/run-e2e.sh --measure-legacy` (Linux) | **50 passed, 0 failed** — 42 advertised directions + warm-up ×2 + batch ×2 + 4 negative controls; 13 cut probes, 8 failing as designed (`spikes/c3/transcripts/c3b-linux-e2e.txt`) |
| `spikes/c3/run-e2e.sh --windows --measure-legacy` (win11-dev 10.0.0.191) | **50 passed, 0 failed**, same 42 directions, same 13 probes (`spikes/c3/transcripts/c3b-windows-e2e.txt`) |
| `spikes/c3/run-win-unit-tests.sh` | **164 passed, 0 failed** (`c3b-windows-unit-tests.txt`) |
| `scripts/test-engine.sh` (Linux) | **171 passed, 0 failed** (`c3b-linux-engine-tests.txt`) |
| `scripts/test-model.sh` (Linux) | **71 passed, 0 failed** (`c3b-linux-model-tests.txt`) |
| `cargo check --locked --all-targets --target x86_64-pc-windows-gnu` | clean (exit 0; only the pre-existing dead-code warnings) |
| `scripts/check-model-purity.sh` | ok — after the first push, CI's policy gate caught two `use … as …` aliases I had put in `matrix::wheel_rank`; they are gone and the script now passes locally too |
| CI on the pushed head `59d5016` (run 41356206344) | `build_matrix_3_os=success`, `license_assert=success`, `policy_check=success` |

Two engine tests had to be re-pointed because their fixture pair became a real route, and that is
worth stating rather than hiding: `an_unrouted_pair_is_refused_before_any_spawn` used `rtf -> md`
(now proven) and moved to `xls -> md` (cut); `document_jobs_only_exist_for_document_types`
asserted `docx -> txt` is not a document job (it is now) and moved to `docx -> xlsx` (cut as a
crash). Both still test the same rule — a cut cell is not routable — on a pair that is still cut.
