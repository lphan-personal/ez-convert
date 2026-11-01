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
