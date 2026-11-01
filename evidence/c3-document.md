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

## 1. Advertised directions (matrix `Proven`) — 14/14 PASS

`docx -> pdf` and `pptx -> pdf` assert a page count; `docx -> md` asserts the source marker
`MARKER-ALPHA` survives; `docx -> html` asserts a `<table>`; `html -> docx` and `csv -> xlsx`
assert the OOXML container part (`word/document.xml`, `xl/workbook.xml`); `xlsx -> csv` asserts
the source cell `Widget A` and the measured renaming `<stem>-<sheet>.csv`.

```
direction      Linux                          Windows
docx -> pdf    PASS  26,460 B   588 ms  1 page  PASS  46,489 B  3,822 ms  1 page
docx -> md     PASS     389 B   543 ms  marker  PASS     404 B  1,921 ms  marker
docx -> html   PASS   4,064 B   543 ms  <table> PASS   4,134 B  1,966 ms  <table>
csv  -> xlsx   PASS   5,804 B   558 ms  workbook PASS  5,796 B  1,843 ms  workbook
xlsx -> csv    PASS     147 B   543 ms  cell    PASS     151 B  1,665 ms  cell
html -> docx   PASS   5,358 B   483 ms  part    PASS   5,380 B  1,708 ms  part
pptx -> pdf    PASS  33,957 B   724 ms  2 pages PASS  79,145 B  2,493 ms  2 pages
```

Byte counts match the S3 spike on Linux exactly (26,460 / 389 / 4,064 / 147 / 33,957) and on
Windows for the text/container outputs (404 / 4,134 / 151). The two Windows PDFs are larger than
the S3 Windows leg recorded (39,946 B and 56,471 B): this VM's font set differs from the
stakeholder's PC. The asserted properties — one page 612×792 pt, two pages 720×540 pt, `%PDF-`
header — hold on both legs. Sizes are not a contract; page counts and content markers are.

## 2. Batching — measured, and it is the reason the pipeline is batch-first

8 identical `sample.docx` inputs, one route, one outdir, **one** spawn, versus 3 inputs each with
its own spawn (same route, same profile):

```
Linux    batch 8:   845 ms  = 106 ms/file     3 individual: 1,720 ms = 573 ms/file   5.43x
Windows  batch 8: 3,820 ms  = 477 ms/file     3 individual: 5,886 ms = 1,962 ms/file  4.11x
```

The card's planning numbers (0.20 s/file batched vs 0.52 s/file spawned) are the same effect
measured on a faster box; the direction and the magnitude class reproduce.

## 3. Warm-up (`--terminate_after_init`)

A brand-new `-env:UserInstallation` profile, then the same call again:

```
Linux    fresh profile  105 ms   warm  90 ms   profile bootstrapped (user/ present)
Windows  fresh profile  246 ms   warm 199 ms   profile bootstrapped (user/ present)
```

The install-cold **first conversion** is the number that hurts (S3 measured 5,111 ms on Windows);
the warm-up exists so that cost is paid at startup, not on the user's first job. It is not a
capability probe and its failure is never fatal — the job still runs.

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
- Legacy `.doc`/`.xls`/`.ppt`/`.rtf` and the unproven document cells: measured separately by the
  harness (`--measure-legacy`), results in §6, but they are **not** advertised by the matrix
  until their rows are flipped.

## 6. Unproven directions, measured anyway (`--measure-legacy`)

These run the same spawn contract and the same validators through a harness-only route table.
They are evidence for the next matrix change, not capability claims: the product refuses them
until `model::matrix` says `Proven`.

```
direction        Linux                        Windows
doc -> pdf       PASS  26,460 B  542 ms      PASS  46,489 B  1,922 ms
doc -> md        PASS     385 B  527 ms      PASS     400 B  1,614 ms
doc -> html      PASS   4,352 B  512 ms      PASS   4,427 B  1,614 ms
doc -> docx      PASS   8,771 B  497 ms      PASS   8,791 B  1,613 ms
xls -> pdf       PASS  26,212 B  512 ms      PASS  41,142 B  1,996 ms
xls -> csv       PASS     147 B  482 ms      PASS     151 B  1,538 ms
xls -> xlsx      PASS   7,327 B  512 ms      PASS   7,330 B  1,660 ms
xls -> html      PASS   5,235 B  497 ms      PASS   5,330 B  1,675 ms
ppt -> pdf       PASS  33,915 B  558 ms      PASS  79,102 B  1,751 ms
rtf -> pdf       PASS  13,100 B  512 ms      PASS  13,335 B  1,994 ms
rtf -> docx      PASS   5,015 B  497 ms      PASS   5,041 B  1,817 ms
rtf -> html      PASS     668 B  512 ms      PASS     687 B  1,858 ms
rtf -> txt       PASS      33 B  497 ms      PASS      34 B  1,934 ms
docx -> txt      PASS     341 B  557 ms      PASS     349 B  1,982 ms
xlsx -> html     PASS   5,655 B  542 ms      PASS   5,750 B  1,631 ms
xlsx -> pdf      PASS  26,211 B  557 ms      PASS  41,077 B  1,982 ms
html -> pdf      PASS  17,997 B  497 ms      PASS  18,022 B  1,660 ms
html -> txt      PASS      27 B  482 ms      PASS      29 B  1,704 ms
md -> pdf        PASS  40,527 B  527 ms      PASS  40,507 B  2,208 ms
md -> html       PASS   1,336 B  573 ms      PASS   1,541 B  1,827 ms
txt -> pdf       PASS  14,522 B  603 ms      PASS  14,513 B  1,643 ms
txt -> docx      PASS   5,117 B  587 ms      PASS   5,143 B  1,600 ms
ppt -> pptx      produced a file at rc=0     produced a file at rc=0
                 (harness had no pptx validator yet; see §7)
```

## 7. Follow-ups this measurement list produces

- `ppt -> pptx` needs a `pptx` structural validator (zip + `ppt/presentation.xml`) before it can
  be judged; the harness reported "no document validator for pptx" rather than guessing.
- Flipping any of §6 into `model::matrix` is a separate change with per-cell evidence ids; the
  numbers above are the input to it, not the flip.

## 8. Reproduce

```bash
spikes/c3/run-e2e.sh                    # linux leg
spikes/c3/run-e2e.sh --windows          # cross-compiled, run on win11-dev over ssh
spikes/c3/run-e2e.sh --measure-legacy   # add §6
scripts/test-engine.sh                  # unit tests, no LibreOffice needed (stub engines)
```
