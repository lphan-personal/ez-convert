# EVIDENCE — S3: LibreOffice document directions

Card: `t_d910a4c8` (S3 spike). Date: 2026-11-01.
Question: a prior benchmark reported `rc=0` with **no output file** for
`csv->xlsx`, `html->docx`, `docx->html`, `xlsx->csv`. Are the document
directions real, and what exactly is the invocation that makes them work?

**Verdict: 7 / 7 directions PASS.** Every direction below produced a file on
disk whose *content* was checked (not just its existence), plus cold and warm
wall times. Nothing has to be declared impossible. The `rc=0 / no file` report
was reproduced and traced to three specific causes (see §5) — none of them a
missing capability.

```
direction      verdict  output bytes  cold      warm median
docx -> pdf    PASS         26 460 B   950 ms     754 ms
docx -> md     PASS            389 B   914 ms     761 ms
docx -> html   PASS          4 064 B   957 ms     713 ms
csv  -> xlsx   PASS          5 802 B   704 ms     311 ms
xlsx -> csv    PASS            147 B   801 ms     408 ms
html -> docx   PASS          6 863 B   855 ms     620 ms
pptx -> pdf    PASS         33 957 B  1217 ms     990 ms
```

## 1. What was actually run

| item | value |
|---|---|
| engine | LibreOffice **26.8.2.2** (`9dea406ab7695df6325b11b0e19fdca8db4b7276`) — the version SPEC §77 pins |
| provenance | official TDF release tarball `LibreOffice_26.8.2_Linux_x86-64_deb.tar.gz` (221 006 208 B), `.deb`s extracted with `dpkg-deb -x` into a local prefix — no system install, no root |
| invocation | `program/soffice` (the **wrapper**, not `soffice.bin` — see §5.1) `--headless --norestore --nolockcheck --nodefault --nologo -env:UserInstallation=file://<per-run profile> --convert-to <filter> [--infilter=<filter>] --outdir <dir> <input>` |
| host | Debian 13 container, Linux 7.0.0-38, 4 cores, no display |
| driver | `spikes/s3/run_s3.py` + `spikes/s3/fixtures.py` (this repo); raw output in `result.json`, `controls.json`, `logs/`, `products/` |
| fixtures | `sample.docx` (37 058 B: heading, paragraphs, 3×3 table, unicode, tab), `sample.xlsx` (5 542 B: 2 sheets, numbers, `=SUM()`), `sample.pptx` (29 310 B: 2 slides), `sample.csv` (101 B: quoted comma, negative, unicode), `sample.html` (474 B: h1, paragraphs, table, list) |

Timing method: **cold** = first conversion ever with a brand-new
`-env:UserInstallation` profile (includes LibreOffice profile bootstrap);
**warm** = three more conversions on that same profile, median reported. All
runs are sequential, one soffice process at a time.

## 2. Per-direction evidence (run `run4`)

Exact argv used for each direction (profile path and outdir elided):

### docx -> pdf — PASS
```
--convert-to pdf:writer_pdf_Export
```
- output `sample.pdf`, 26 460 B, sha256 `2a60687a…3f4fffce`
- cold 949.9 ms / warm 759.9, 680.4, 753.7 ms (median 753.7)
- content: `%PDF-` header; **1 page**; page box **612.0 × 792.0 pt** (US Letter
  preserved, not A4 default); extracted text contains `MARKER-ALPHA`, `café`,
  and the table cell text `Marker`.

### docx -> md — PASS
```
--convert-to md:Markdown
```
- output `sample.md`, 389 B, sha256 `cfdbdcf8…6dacd5a`
- cold 913.7 ms / warm 761.3, 852.6, 691.0 ms (median 761.3)
- content: `# **EZ Convert S3 fixture**` heading, `MARKER-ALPHA`, unicode
  `café – naïve → 中文`, tabs preserved, the 3×3 table rendered as a GitHub
  markdown table, soft break as trailing double space.
- **Flag:** LibreOffice 26.8.2 has a real Markdown export filter, so pandoc is
  **not** required for `docx -> md`. Availability is version-dependent — an
  older image (e.g. LibreOffice 24.2) may not ship it. The matrix treats this
  direction as a probe, not a hard gate, precisely because of that; pandoc
  remains the fallback in SPEC.

### docx -> html — PASS
```
--convert-to html:HTML (StarWriter)
```
- output `sample.html`, 4 064 B, sha256 `380af156…c98b7e115`
- cold 957.1 ms / warm 752.4, 712.8, 679.1 ms (median 712.8)
- content: `MARKER-ALPHA`, `café`, one `<h1`, one `<table`, **13 `<p` tags**.

### csv -> xlsx — PASS
```
--infilter=csv:44,34,76,1,,0,false,true,true  --convert-to xlsx:Calc MS Excel 2007 XML
```
- output `sample.xlsx`, 5 802 B, sha256 `82100b6a…3d7b2031c`
- cold 704.2 ms / warm 372.9, 309.5, 310.5 ms (median 310.5)
- content (read back with `openpyxl`): 4 rows × 4 cols; header
  `id,name,amount,note`; `B2='Widget A'`; `D2='comma, inside'` (embedded comma
  survives quoting); `D3='café'` (UTF-8 via charset option 76);
  **`A2=1`, `C2=12.5`, `C3=-3.75`, `C4=1000` are numeric cells, not text.**

### xlsx -> csv — PASS
```
--convert-to csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true,false,false,1
```
- output **`sample-Data.csv`** (see naming rule below), 147 B,
  sha256 `cfe53c2a…1829a1758`
- cold 801.1 ms / warm 410.8, 408.0, 354.7 ms (median 408.0)
- content: 4 rows × 7 cols; header `id,name,amount,note,,total,café – naïve → 中文`;
  `Widget A`, `"comma, inside"` re-quoted correctly, `café`, numeric
  `12.5 / -3.75 / 1000`; the `=SUM(C2:C4)` cell exported as its **evaluated
  value `1008.75`**.
- **Naming rule (verified):** with an explicit sheet index in the filter
  options LibreOffice writes `<stem>-<sheet>.csv` (`sample-Data.csv`), not
  `<stem>.csv`. With the 12th option set to `-1` it writes one file per sheet:
  `sample-Data.csv` **and** `sample-Notes.csv`. A caller that asserts
  `outdir/<stem>.csv` will report "rc=0, no output" while the file exists under
  a different name — see §5.3.

### html -> docx — PASS
```
--infilter=HTML (StarWriter)  --convert-to docx:MS Word 2007 XML
```
- output `sample.docx`, 6 863 B, sha256 `a073c214…92f6193e6c`
- cold 855.2 ms / warm 620.0, 615.8, 625.4 ms (median 620.0)
- content (read back with `python-docx`): `MARKER-BETA`, 5 paragraphs, **1
  table of 2×2** with the marker inside a cell, so `<table>` markup became a
  real Word table.

### pptx -> pdf — PASS
```
--convert-to pdf:impress_pdf_Export
```
- output `sample.pdf`, 33 957 B, sha256 `d32d1d91…5e951f4fe`
- cold 1217.3 ms / warm 989.6, 1117.2, 987.3 ms (median 989.6)
- content: `%PDF-`; **2 pages** (one per slide); page box **720 × 540 pt** (10″×7.5″
  slide geometry, not A4); text of both slides present (`MARKER-GAMMA`,
  `MARKER-DELTA`).

## 3. Fidelity of docx -> pdf (the substantive question)

A 4-page fixture (`Fidelity fixture`, heading + image + 78 body lines + markers
at start/middle/end) converted with `--convert-to pdf:writer_pdf_Export`:

- 4 pages, **all** pages 612 × 792 pt — page geometry is preserved page by page,
  no reflow to A4, no page collapse.
- All three position markers present in extracted text: start (`MARKER-PAGE1`),
  middle (`MARKER-PAGE2`) and last body line (`MARKER-PAGE-LAST`).
- Text is live text (2 981 extracted chars), not a raster.
- Fonts: `/BAAAAA+Carlito-Bold` and `/CAAAAA+Caladea-Regular`, **both embedded
  (FontFile2)**. These are the metric-compatible substitutes: the fixture
  declares Calibri/Cambria, which are not installed on Linux, so LibreOffice
  substituted Carlito/Caladea with identical metrics.
- The inserted PNG survives as image XObjects in the PDF.

**Fidelity caveat for the app:** layout is faithful for the fixtures, but
"faithful" depends on the fonts being available. On a host without the
document's fonts LibreOffice substitutes metric-compatible clones (fine for
Calibri/Cambria here) and, for an exotic font, will use a non-metric substitute
and reflow. The Windows sidecar should rely on the system font set (Windows
documents usually name Windows fonts) and must not claim "pixel-identical".

## 4. Cost per file

Cold ~0.7–1.2 s, warm 0.31–0.99 s for these fixtures. `pptx->pdf` is the most
expensive direction (more layout work), `csv->xlsx` the cheapest. Cold is
dominated by LibreOffice profile bootstrap, not by the document: a second run
on the same profile is 1.2–2.3× faster. Keep one warm profile per worker and
serialise conversions through it.

## 5. The `rc=0` / no-output failure mode — reproduced, cause by cause

### 5.1 Launching `soffice.bin` directly instead of the `soffice` wrapper
Same input, same fresh profile, same flags, only the binary differs:

| binary | first run on a fresh profile | output |
|---|---|---|
| `program/soffice` (wrapper) | `rc=0`, 923 ms | `sample.pdf` ✅ |
| `program/soffice.bin` | **`rc=81`**, 230 ms | **none** ❌ |

A second run on the same profile with `soffice.bin` succeeds (`rc=0`, 762 ms).
Exit 81 is the "crash with restart" code: the first-ever profile bootstrap
crashes, and the `oosplash` wrapper is what relaunches the process. **Call the
wrapper.** A sidecar that calls `soffice.bin` will see a hard failure on the
first conversion after install — the worst possible time.

### 5.2 Filter-name syntax errors are loud, except when they are not
Nine `--convert-to` spellings for `xlsx -> csv` were probed:

| `--convert-to` argument | rc | files |
|---|---|---|
| `csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true,false,false,1` | 0 | `sample-Data.csv` |
| `csv:Text - txt - csv (StarCalc):44,34,76` | 0 | `sample.csv` |
| `csv:Text - txt - csv (StarCalc)` | 0 | `sample.csv` |
| `csv:Text - txt - csv (StarCalc):44,34,76,1` | 0 | `sample.csv` (every text field quoted) |
| `csv` | 0 | `sample.csv` |
| `csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true,false,false,-1` | 0 | `sample-Data.csv` + `sample-Notes.csv` |
| `csv:44,34,76` | **1** | none — `Error: Please verify input parameters… Class:Parameter Code:26` |
| `csv:44,34,76,1,,0,false,true,true` | **1** | none — same parameter error |
| `Text - txt - csv (StarCalc):44,34,76,1,…` (no `csv:` type prefix) | **1** | none — it tries to write a file literally named `sample.Text - txt - csv (StarCalc)` |

Rule proved: the argument is `TYPE:FILTER_NAME:OPTIONS`. **Both** the type token
(`csv`) and the full filter name (`Text - txt - csv (StarCalc)`) are required
before the option list; short token + bare options is a parameter error, and a
filter name without the type token is parsed as the *filename*. Quote characters
inside the argument (e.g. `csv:"Text - txt - csv (StarCalc)"`) are literal bytes
to `execve`/`Command::arg` and also fail — never embed quotes.

### 5.3 The output file may not be called what you asked for
Explicit sheet option ⇒ `<stem>-<sheet>.csv`. This bit *this* spike first: an
early run of the matrix logged `xlsx -> csv` as "rc=0, exists=False" purely
because the checker looked for `sample.csv` while LibreOffice had written
`sample-Data.csv`. A caller must: create a fresh outdir per job, then **glob the
outdir for the expected extension** and treat "more than one produced file" as
a per-sheet export. Asserting a fixed filename manufactures a false negative —
and is the most likely explanation for the original `rc=0 / no output` report on
`xlsx->csv`.

### 5.4 Not reproduced: concurrency
A conversion launched while another `soffice` held the same
`-env:UserInstallation` profile **still produced its file** (`rc=0`, 457 ms;
holder verified alive before the run, `ps`-checked). LibreOffice 26.8 tolerated
the shared profile in this test. A per-job profile is still the recommended
contract (it is the only way to make "cold" and "warm" deterministic and to
avoid two jobs racing on one lock), but a busy profile is not, on this evidence,
a silent-no-output cause.

## 6. Error model inputs (for the app's user-facing messages)

| situation | rc | stdout/stderr | output |
|---|---|---|---|
| input file does not exist | 1 | stderr `Error: source file could not be loaded` | none |
| `--outdir` does not exist | 0 | normal convert line on stdout | **directory is created by LibreOffice** and the file written |
| filename with spaces + non-ASCII (`my report café.docx`) | 0 | normal convert line | `my report café.pdf` |
| successful run | 0 | `convert <in> as a Writer document -> <out> using filter : <FILTER>` | file |

Two consequences: (a) `rc=0` carries no information about success — always
stat the output; (b) do not rely on the outdir being absent to detect a bad
path, LibreOffice will happily create it. Create the outdir yourself and assert
the produced file(s) afterwards.

## 7. What this evidence does NOT cover (honest gaps)

1. **Windows sidecar unverified.** All numbers are Linux x86-64, LibreOffice
   26.8.2. LibreOffice on Windows uses the same filter code, but the wrapper is
   `soffice.exe`/`soffice.com`, and profile paths, font substitution (real
   Calibri vs Carlito) and start-up cost will differ. This is a separate spike.
2. **macOS unverified.** Not tested at all.
3. **CI re-run pending.** `spikes/s3/run.sh` is committed and the workflow
   already offers `spike_id: s3`, but the `GITHUB_TOKEN` available to this agent
   has **no Actions scope** (`actions=read` refused, HTTP 403), so the job cannot
   be dispatched from here — it needs *Actions → build → Run workflow →
   spike_id=s3* clicked by a human. The GitHub runner's preinstalled
   LibreOffice is an older release than 26.8.2, so its `docx->md` result is the
   interesting variable.
4. **Encrypted/password-protected and macro-enabled inputs** (`docm`, `xlsm`,
   password PDF/docx) were not tested. Neither were corrupt/truncated inputs
   (fuzzing is Q1/Q3 work).
5. **Very large files** (100+ pages, 50 MB xlsx) were not measured; the timings
   here are for small fixtures and should not be extrapolated linearly.
6. **Embedded objects / tracked changes / footnotes / headers** fidelity was not
   probed; the fidelity fixture covers text, a table, an image, pagination and
   fonts only.

## 8. Reproduce

```bash
# any host with a LibreOffice 26.8.x build (Linux)
python3 -m pip install python-docx==1.2.0 openpyxl==3.1.5 python-pptx==1.0.2 pypdf==6.21.4
python3 spikes/s3/run_s3.py --soffice "$(command -v soffice)" --out spikes/s3/out --reps 3 --controls
# or on a GitHub runner, via workflow_dispatch with spike_id=s3
bash spikes/s3/run.sh
```

`run_s3.py` exits non-zero if any required direction produces no valid file, so
a silent no-output conversion can never pass. `out/result.json`,
`out/table.md`, `out/controls.json`, `out/logs/` and `out/products/` are the
evidence bundle for this card.
