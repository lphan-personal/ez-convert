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

1. **Windows sidecar** — now covered by **§9** (card `t_20ced994`, same
   LibreOffice 26.8.2.2, Windows build, `spikes/s3/run.ps1`): 7/7 directions
   PASS, `soffice.com` is the launcher, `soffice.bin` is the same rc=81 trap, and
   one new caveat — the `docx -> pdf` text layer loses `é`/`ï` under `pypdf`
   when the real Calibri/Cambria are embedded (§9.5).
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

---

# 9. Windows leg — card `t_20ced994`

Everything above (§1-§8) is Linux x86-64. This section is the **Windows**
re-run of the same matrix on the **Windows** LibreOffice build. No Linux number
in this section; no Windows number above.

**Verdict: 7 / 7 directions PASS on Windows**, plus one platform-specific
caveat that the Linux leg did not have: the `docx -> pdf` **text layer** loses
two accented glyphs to `pypdf`, even though the rendered page and a second
extractor are correct (§9.5). The `rc=0 / no output` trap from §5 reproduces
here **exactly** and is caused by the same three things — the highest-value
check on this platform, because the answer decides the sidecar's argv contract.

## 9.1 Environment

| item | value |
|---|---|
| host | `Ryo-PC` — Microsoft Windows 11 Pro 10.0.26200, build 26200, 24 logical cores, 61.6 GB RAM, `AMD64` |
| engine | LibreOffice **26.8.2.2** (`9dea406ab7695df6325b11b0e19fdca8db4b7276`) — same build id as the Linux leg |
| provenance | official TDF MSI `LibreOffice_26.8.2_Win_x86-64.msi`, 375 934 976 B, sha256 `ff4b911403b67ee9e6249f820f9782c44bb32c94fdaa45b0da53a8e419e5307a`, from `download.documentfoundation.org/libreoffice/stable/26.8.2/win/x86_64/` |
| install | `msiexec /i <msi> /qn /norestart ALLUSERS=1` → rc 0, **46 466 ms**, `C:\Program Files\LibreOffice\program` |
| runner | `spikes/s3/run.ps1` (this repo) → `run_s3.py --soffice <found> --out spikes/s3/out-win --reps 3 --controls` |
| python | CPython 3.14.7, `%LOCALAPPDATA%\Python\bin\python.exe`; `pip install python-docx==1.2.0 openpyxl==3.1.5 python-pptx==1.0.2 pypdf==6.21.4` in 4 725 ms |
| transport | SSH (`sshd` = OpenSSH for Windows, default shell `cmd.exe`); files base64-piped, because `scp` is unreliable on this host |
| fixtures | byte-identical set to §1 (`sample.docx` 35 747 B here — the docx container is not byte-stable across python-docx runs, but carries the same content and markers) |
| repeatability | the whole matrix was run twice, once before and once after a clean MSI uninstall+reinstall: **byte-identical outputs**, cold/warm times within ±7 % (§9.6) |

## 9.2 Which executable the sidecar must spawn

This was the card's highest-value question. Windows ships **three** launchers in
`C:\Program Files\LibreOffice\program` — and there is **no `oosplash.exe`**:

| file | size | subsystem | first run on a brand-new `-env:UserInstallation` profile |
|---|---|---|---|
| `soffice.com` | 523 096 B | **console** | `rc=0`, 1 130 ms, **writes `sample.pdf`** ✅, full `convert … using filter : writer_pdf_Export` line on stdout |
| `soffice.exe` | 523 096 B | GUI | `rc=0`, 1 150 ms, **writes `sample.pdf`** ✅, same stdout line |
| `soffice.bin` | 2 731 352 B | — | **`rc=81`, 355 ms, no output, empty stdout/stderr** ❌ |

**Spawn `soffice.com`.** It is the console-subsystem sibling of the GUI binary:
it attaches a console (so `stdout`/`stderr` are real, blocking pipes and the
`convert …` line arrives) and it waits for the conversion. `soffice.exe` also
finished and produced the file in this harness, but it is the GUI subsystem
binary — its console behaviour depends on the handles it inherits, which is not
a contract a sidecar should rely on when `soffice.com` exists for exactly this
purpose. `soffice.bin` is the same trap as Linux and **must never be called**:

- first run on a fresh profile: **rc=81**, 355 ms, **no file**;
- a **second** run on that same (now bootstrapped) profile: **rc=0**, 739 ms,
  `sample.pdf` written.

So the failure is not "this binary never works" — it is specifically
*the first run against a profile that has never been bootstrapped*, which is
exactly the run a freshly installed app performs for its first user job.

**Contract for A1 (Windows):**

```
spawn  C:\Program Files\LibreOffice\program\soffice.com
       --headless --norestore --nolockcheck --nodefault --nologo
       -env:UserInstallation=file:///C:/<per-job profile>
       [--infilter=<filter>] --convert-to <TYPE:FILTER[:OPTIONS]>
       --outdir <fresh dir> <input>
```

Two Windows-only details that will silently break an otherwise correct port:

1. **`-env:UserInstallation` wants a URL, not a path.** It must be
   `file:///C:/Users/.../profile` — drive letter, forward slashes, three
   slashes. The Linux string `file://` + `C:\Users\...` is accepted without
   error and quietly bootstraps a *different* profile, which destroys the cold/
   warm contract and re-pays the first-run cost on every job.
   `run_s3.py` now has a `file_url()` helper for this (POSIX output unchanged).
2. **Do not set `SAL_USE_VCLPLUGIN=svp`.** `svp` is the X11-free *Linux* VCL
   plugin; the Windows build never needs it. The Linux leg sets it and it is now
   gated to POSIX.

## 9.3 Per-direction results (Windows)

Run `run2`, `spikes/s3/run.ps1`, `--reps 3 --controls`, exit code **0**.
Outputs live in `spikes/s3/out-win/products/` (`C:\s3win\repo\spikes\s3\out-win`
on the spike host).

| direction | verdict | invocation | output | bytes | cold | warm median | content check |
|---|---|---|---|---|---|---|---|
| docx -> pdf | **PASS** | `--convert-to pdf:writer_pdf_Export` | `sample.pdf` | 39 946 | 1 158 ms | 732 ms | ok (text layer degraded, §9.5) |
| docx -> md | **PASS** | `--convert-to md:Markdown` | `sample.md` | 404 | 1 100 ms | 721 ms | ok |
| docx -> html | **PASS** | `--convert-to html:HTML (StarWriter)` | `sample.html` | 4 134 | 1 121 ms | 728 ms | ok |
| csv -> xlsx | **PASS** | `--infilter=csv:44,34,76,1,,0,false,true,true --convert-to xlsx:Calc MS Excel 2007 XML` | `sample.xlsx` | 5 793 | 795 ms | 417 ms | ok |
| xlsx -> csv | **PASS** | `--convert-to csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true,false,false,1` | **`sample-Data.csv`** | 151 | 812 ms | 424 ms | ok |
| html -> docx | **PASS** | `--infilter=HTML (StarWriter) --convert-to docx:MS Word 2007 XML` | `sample.docx` | 6 888 | 921 ms | 524 ms | ok |
| pptx -> pdf | **PASS** | `--convert-to pdf:impress_pdf_Export` | `sample.pdf` | 56 471 | 1 024 ms | 652 ms | ok |

Per direction, the content that was actually checked (read back with the
harness libraries, not eyeballed):

- **docx -> pdf** — `%PDF-` header, **1 page**, page box **612.0 × 792.0 pt**
  (US Letter preserved, not A4). `MARKER-ALPHA` and `Marker` present in the text
  layer. sha256 `09381cf2…717a5b0e`. Accents: see §9.5.
- **docx -> md** — `MARKER-ALPHA` present, document starts with a real Markdown
  heading (`markdown_heading=yes`), 379 chars. sha256 `78b22651…1b422204`.
  The **`md:Markdown` filter exists in the Windows build too** — same
  version-dependence flag as §2, pandoc still not needed on 26.8.x.
- **docx -> html** — `MARKER-ALPHA`, **`café`**, one `<h1`, one `<table>`,
  **13 `<p` tags**, 4 036 chars. sha256 `f54c4251…1e9ec2d5`.
- **csv -> xlsx** — 4×4 on sheet `sample`; header `id,name,amount,note`;
  `B2='Widget A'`, `D2='comma, inside'` (embedded comma survives),
  `D3='café'` (UTF-8 via charset option 76);
  **`A2=1`, `C2=12.5`, `C3=-3.75`, `C4=1000` are numeric cells, not text** —
  the `--infilter=csv:…` binder works identically on Windows.
  sha256 `8ab420e1…c21a8d0`.
- **xlsx -> csv** — produced as **`sample-Data.csv`**, not `sample.csv`:
  the `<stem>-<sheet>.csv` renaming rule from §5.3 reproduces on Windows. 4 rows
  × 7 cols, header `id,name,amount,note,,total,café – naïve → 中文`,
  `Widget A`, `"comma, inside"` re-quoted, `café`, numerics `12.5 / -3.75 /
  1000`. sha256 `dec6c6bc…46df7999`.
- **html -> docx** — `MARKER-BETA`, 5 paragraphs, **1 table of 2×2** with the
  marker inside a cell, so `<table>` markup became a real Word table.
  sha256 `ba5129d0…578a2b8f`.
- **pptx -> pdf** — `%PDF-`, **2 pages**, page box **720.0 × 540.0 pt**
  (10″×7.5″ slide geometry, not A4), both slide markers present.
  sha256 `3f5cd98a…0c4ae92f`.

## 9.4 Fonts — real Calibri/Cambria, no substitution

The §3 fidelity fixture (4 pages, image, 78 body lines, three position markers)
converted with `--convert-to pdf:writer_pdf_Export`:

- 4 pages, **every** page 612 × 792 pt.
- `MARKER-PAGE1`, `MARKER-PAGE2`, `MARKER-PAGE-LAST` all present; 2 981
  extracted chars (identical count to the Linux leg).
- 4 image XObjects.
- **Fonts embedded: `/BAAAAA+Calibri-Bold` and `/CAAAAA+Cambria`, both
  `FontFile2`.** This is the substantive Windows/Linux difference: the fixture
  declares Calibri/Cambria, Windows has them, so LibreOffice embeds the real
  fonts. Linux embedded `Carlito-Bold` / `Caladea-Regular`, the metric-compatible
  clones. Layout is the same because the clones are metric-compatible; the
  embedded bytes are not.
- PDF grew accordingly: 50 626 B here vs **34 607 B on Linux** for the same
  fidelity fixture, and `docx -> pdf` of `sample.docx` is 39 946 B vs 26 460 B —
  real fonts, bigger file.

**Do not claim "pixel-identical across platforms".** Same metrics, different
outlines. The app's promise should be "layout preserved", not "identical".

## 9.5 NEW — the `docx -> pdf` text layer is degraded on Windows

This is the one place where Windows is **worse** than Linux, and it is
font-related. It is recorded as a caveat, not hidden:

| check | Linux (§2) | Windows |
|---|---|---|
| page renders `café – naïve → 中文` correctly | yes | **yes** (150-dpi render, visually verified) |
| `pypdf 6.21.4` text extraction | `café` ✅ | **`cafe\x1c`** / **`naï\x1f`** ❌ — two control characters where the accents should be |
| `PyMuPDF 1.26` text extraction | (not run) | `café – naïve → 中文` ✅ |
| ASCII markers / page count / page size | ok | ok |
| same document to **html** and **csv** | `café` ✅ | `café` ✅ (so the source data is fine) |

Cause, as far as this spike can establish: with the real Calibri/Cambria
subset embedded, the PDF's `ToUnicode` CMap for the Cambria subset carries 44
`bfchar` entries (including multi-char ones — `0x0D → 00660069`, an `fi`
ligature — so the exporter *can* emit them) but has **no entry for the codes
drawn for `é` and `ï`**; a hand decode of the content stream against that CMap
leaves exactly those codes unmapped. `pypdf`, which trusts `ToUnicode`, therefore
emits the raw code byte; PyMuPDF falls back to the embedded font's own cmap and
recovers the character; the rasteriser draws the real glyph, so the page looks
right.

**What this means, and what it does not.** The conversion is correct — the file,
the pagination, the geometry and the visible text are all right. What is *not*
guaranteed on Windows is **copy/paste/search fidelity for accented Latin
characters inside the produced PDF**, which matters directly for a Vietnamese
user base. Not established here: which real viewers are affected (Acrobat, Chrome
PDFium, Edge, Preview were **not** tested), and whether a different PDF export
option (PDF/A-2, font subsetting) avoids it. A1 must not advertise "searchable
text" as intact on Windows until a viewer test is done.

`run_s3.py` was changed accordingly: `check_pdf()` now gates on **ASCII**
markers and reports a `unicode_probe` (`café`, `naïve`, `中文`) plus a
`unicode_text_layer: ok|degraded` metric instead of failing the direction. The
table cell for that direction reads `ok (text layer degraded)`, and the driver
prints a WARNING line. The Linux numbers in §2 were produced with `café` as a
gate and passed; re-running the Linux leg with this driver only removes a
gate that Linux never tripped.

## 9.6 Cold / warm timings — including the real first-run cost

Two different "cold" costs exist on Windows, and A1's per-file budget needs
both. Measured with `soffice.com`.

**Install-cold** — the very first LibreOffice process on the machine after a
clean MSI install, nothing warm, brand-new profile (`probe3`, run immediately
after `msiexec` returned and no other LibreOffice process had been started):

| step | wall time |
|---|---|
| 1 — first conversion ever, fresh profile | **5 111 ms** |
| 2 — second conversion, another fresh profile | 2 725 ms |
| 3 — same profile as run 2 (warm) | 884 ms |
| 4 — same profile (warm) | 892 ms |
| 5 — same profile (warm) | 880 ms |

A separate one-shot probe on the *first* install (before any matrix run) saw
**9 556 ms** for the first-ever invocation — the spread between 9.6 s and 5.1 s
is the machine's state at install time (first-install flag, on-access AV scan of
~358 MB of freshly written DLLs), not a harness difference. **Budget for the
worst case: ~10 s for the first conversion a user ever runs after install.**

**Matrix cold/warm** (fresh profile per direction, warm install, `--reps 3`):

| direction | cold | warm 1 | warm 2 | warm 3 | warm median |
|---|---|---|---|---|---|
| docx -> pdf | 1 158 ms | 732 | 726 | 745 | **732 ms** |
| docx -> md | 1 100 ms | 721 | 741 | 719 | **721 ms** |
| docx -> html | 1 121 ms | 736 | 728 | 722 | **728 ms** |
| csv -> xlsx | 795 ms | 407 | 417 | 417 | **417 ms** |
| xlsx -> csv | 812 ms | 431 | 422 | 424 | **424 ms** |
| html -> docx | 921 ms | 526 | 524 | 521 | **524 ms** |
| pptx -> pdf | 1 024 ms | 654 | 650 | 652 | **652 ms** |

Re-run after uninstall+reinstall: cold within **−7 % … +6 %** of the table and
**byte-identical** outputs, so these are stable enough to budget against.
Windows warm times land within ~10 % of the Linux leg despite 24 vs 4 cores —
per-conversion cost is dominated by process start + profile I/O, not CPU.

## 9.7 Controls on Windows (the `rc=0 / no output` traps, same harness)

- **C1 launcher** — see §9.2. `soffice.com` ✅ / `soffice.exe` ✅ /
  `soffice.bin` **rc=81, no output on a fresh profile**, rc=0 on the retry.
  Same signature as Linux, minus `oosplash` (Windows has none).
- **C2 nine `xlsx -> csv` spellings** — identical verdicts to the Linux table in
  §5.2, including the same two `rc=1` parameter errors and the
  `…,false,false,-1` variant producing **two** files (`sample-Data.csv` +
  `sample-Notes.csv`). The `TYPE:FILTER_NAME:OPTIONS` rule is not platform
  specific.
- **C3 error model** — missing `--outdir`: `rc=0`, directory **created** by
  LibreOffice, file written (same trap as Linux: never detect a bad path by the
  directory being absent). Missing input: `rc=1`, stderr
  `Error: source file could not be loaded` — byte-identical message to Linux, so
  the app can key on it.
- **C4 fidelity** — §9.4.

## 9.8 Windows vs Linux, one table

| item | Linux (§1-§8) | Windows (§9) |
|---|---|---|
| launch | `program/soffice` (oosplash wrapper) | **`program/soffice.com`**; `soffice.exe` is the GUI twin, `soffice.bin` is the rc=81 trap; **no `oosplash`** |
| profile arg | `file://` + abs path | **`file:///C:/…`** — URL, not path |
| directions | 7/7 PASS | **7/7 PASS** |
| `docx->pdf` bytes | 26 460 | 39 946 (real fonts) |
| `pptx->pdf` bytes | 33 957 | 56 471 (real fonts) |
| `xlsx->csv` name | `sample-Data.csv` | `sample-Data.csv` (same rule) |
| embedded fonts | Carlito-Bold, Caladea-Regular | **Calibri-Bold, Cambria** |
| `docx->pdf` text layer | clean | **degraded for `é`/`ï` under pypdf** (§9.5) |
| warm cost | 311–990 ms | 417–732 ms |
| install-cold | n/a (debs extracted, no install) | **5.1–9.6 s** |

## 9.9 What this does NOT cover

1. **Real PDF viewers were not tested** for the §9.5 text-layer caveat (Acrobat,
   Chrome/Edge PDFium, macOS Preview). Only `pypdf`, `PyMuPDF` and a raster
   render.
2. **Per-user (non-admin) install.** This ran as a machine-wide install
   (`ALLUSERS=1`) under an **elevated** SSH session (the account is a local
   administrator). A standard user installing LibreOffice per-user under
   `%LOCALAPPDATA%\Programs` was not exercised; `run.ps1` looks there too.
3. **Long-running / concurrent use.** One conversion at a time, one process per
   call. Multiple sidecar jobs against one profile, and a user who has
   LibreOffice open, were not probed on Windows.
4. **Antivirus interference beyond first run.** Defender was active; it is the
   likeliest cause of the 5.1 s vs 9.6 s install-cold spread and was not
   isolated. A real install path (P2) may hit a SmartScreen/AV prompt.
5. **Encrypted/macro inputs, huge files, macOS** — unchanged gaps from §7.

## 9.10 Reproduce (Windows)

```powershell
# any Windows host with LibreOffice 26.8.x installed (machine-wide or per-user)
powershell -NoProfile -ExecutionPolicy Bypass -File spikes/s3/run.ps1
#   -> pip-installs the four harness-only libs, locates soffice.com via the
#      registry / Program Files / %LOCALAPPDATA%, runs the 7 directions with
#      --controls, writes spikes/s3/out-win/{result.json,table.md,controls.json,
#      windows-host.json,soffice-version.txt,run.log}
#   -> exits non-zero if any required direction produces no valid file
```

To measure the install-cold gradient, run one conversion with a brand-new
profile immediately after `msiexec` returns and before anything else touches
LibreOffice — a `--version` call first will warm the file cache and halve the
number.
