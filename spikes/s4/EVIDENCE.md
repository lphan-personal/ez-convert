# S4 — the engine pack that actually ships

Two release-gate questions nothing had measured, both on the Windows box:

1. is **avif output** still true on the **final** `spec-aom` vips dist (it was proven on the
   S2b dist, then the binaries were relinked by the libtiff/JBIG remediation)?
2. is the Windows `docx → pdf` **text layer** good enough in a **real viewer**, given that
   `pypdf` reads `cafe\x1c` where the page renders `café`?

Card `t_61e55863`. Host: `Ryo-PC` (Windows 11 Pro 26200, 24 logical cores, 61.6 GB RAM),
LibreOffice 26.8.2.2, MSYS2/UCRT64. Every number below is tool output, not an estimate.
Artifacts for both items are in `spikes/s4/artifacts/`.

**Verdicts**

| question | verdict |
|---|---|
| avif save/decode/EXIF/ICC/orientation on the **final** dist | **PASS — 34 assertions, 0 failures.** Avif output stays `Proven`; nothing changes in the wheel. |
| real-viewer fidelity of accented text in `docx → pdf` on Windows | **(b) some viewers are affected.** Rendering and search are fine everywhere; the *text layer* (copy, screen readers) loses `é` outside PDFium. A1 must **not** promise searchable accented text on Windows. |

> **Second host.** Both items were repeated on the dedicated test VM (`win11-dev`, 10.0.0.191)
> after the Windows target moved there: see **`EVIDENCE-VM.md`** — same dist re-verified 43/43 and
> the same suite at 34/34 on a second machine, plus **real Acrobat 26.2** and a real Firefox GUI
> instead of pdf.js under Node. The verdict does not change; the Acrobat row is new and it is
> affected too.

---

## 1. avif suite on the final dist

### 1.1 Which artifact, exactly

The dist on disk is the one the card calls "final", proven by hash, not by byte count:

```
C:\build\dist  →  43 files, 35,824,560 B (35,733,055 B of that outside licenses/)
sha256 of every file: spikes/s4/artifacts/FINAL-dist-spec-aom-sha256.txt
```

Cross-checked against the manifest S2c recorded (`t_65624d81/FINAL-dist-spec-aom-sha256.txt`):

```
s2c entries: 43   now entries: 43
only in s2c: []          only now: []
hash mismatches: 0
```

So the directory tested is **byte-identical** to the dist the S2c remediation signed off. The two
hashes named in the card match too:

| file | S2b (where avif was proven) | dist tested now |
|---|---|---|
| `libvips-42.dll` | `618c9aa0…c2f3` | `edea7dc5ea5fae8479d4a057e51f9ed12052b7a371c7f60895895d62eb586410` |
| `libheif.dll` | `54f9e58e…6102` | `92f1c402a412ae8e8129af7da81fc6639b3afeb5f7b352d17365e96815e0d4a9` |

One correction to the card's own figure, recorded so nobody re-derives it wrong later: the
**35,731,757 B** number is S2c's *binaries-only* total. With `licenses/` included the dist is
**35,824,560 B** today (S2c's own `du`-including-licences figure was 35,823,740 B; the licence
texts were edited again after that measurement). File count (43) and every hash are unchanged.
Nothing about the avif result depends on the byte total.

### 1.2 The run

```
D=/c/build/dist bash /c/build/18-avif-tests.sh > /c/build/s4/avif-final.log 2>&1
```

- script: `18-avif-tests.sh`, sha256 `d5314fbfa29e9b21a826a2835a35417eab9dccf6e7b90a8dfdd5a63ccbb74224`
  — **byte-identical** to `build/windows/scripts/18-avif-tests.sh` in this repo and to the
  `18-avif-tests.TESTED-d5314fbf.sh` the S2b card signed off. Unchanged, as the card required.
- `vips-8.18.7`, run 2026-11-01 06:08:56 UTC, full log `spikes/s4/artifacts/avif-final.log`.

### 1.3 Result: 34 assertions, 0 failures

```
######## SUMMARY ########
  PASS=34  FAIL=0
```

33 → 34 is not a new test: S2b counted 33 because `libjbig-0.dll` was *present* in that dist and
its check printed a report instead of an assertion. The final dist has no jbig file, so that
branch now asserts — `PASS  libjbig-0.dll not in the closure`. The assertion set is identical.

### 1.4 The assertions the card asked for, quoted from the log

**jpg → avif → jpg, EXIF IFD0 + ICC surviving**

```
  PASS  jpg -> avif (2 MP)
  PASS  big2mp.avif kept icc-profile-data
  PASS  big2mp.avif kept exif-ifd0-Make
  PASS  big2mp.avif kept exif-ifd0-Model
  PASS  big2mp.avif kept exif-ifd0-Software
  PASS  big2mp.avif kept exif-ifd0-DateTime
  PASS  avif->jpg kept the ICC profile
  PASS  avif->jpg kept EXIF IFD0
  PASS  avif geometry 400x200
```

`icc-profile-data: 3144 bytes of binary data` survives into the avif *and* back out to jpg;
the four IFD0 tags (`EZConvertTest` / `AVIF Proving Rig` / `libvips 8.18.7 spec-aom` /
`2026:11:01 03:00:00`) survive unmodified.

**orientation-6, no double rotation**

```
  PASS  orientation-6 jpg -> avif
  PASS  avif round trip lands correctly oriented at 1152x1728
  PASS  no stale EXIF Orientation in the avif (libheif rewrote it; irot carries the rotation)
  PASS  orientation-6 avif still carries the rest of the EXIF IFD0
  PASS  decoded avif presents Orientation=1 (irot already applied -> autorot is a no-op)
```

Source 1728×1152 with `Orientation: 6`; the container decodes 1152×1728 with no Orientation tag
left behind, so no viewer can rotate it twice.

**avif → jpg decode (dav1d)**

```
  PASS  avif(400x200) -> jpg
  PASS  avif(2 MP) -> jpg
  PASS  avif -> jpg (dav1d decode of the avif we just wrote)
```

**Everything else that still holds**

```
  PASS  png -> avif (400x200)                 small.avif 3,863 B, brand ftypavif
  PASS  explicit --encoder aom accepted
  PASS  8-bit source stays 8-bit without an explicit --bitdepth
  PASS  0 banned imports across the whole dist
  PASS  libjbig-0.dll not in the closure
  PASS  HEIC orientation test: 200x400 (unchanged from S2)
  PASS  top-left still blue (30 59 221)
  PASS  no 'bad seek' warning on orient6.heic
  PASS  no 'bad seek' warning on sample1.heic
```

The two traps from S2b reproduce unchanged on the final dist:

- `--encoder rav1e` still returns **rc=0 and writes a 3,853 B file** while printing
  `VIPS-WARNING: heifsave: could not find rav1e` — the option cannot be used as a capability
  signal (`--vips-config` said avif save `true` on the build where save failed).
- a heic→avif conversion of the 2,345 B fixture still produces output with empty stderr.

### 1.5 Measured latency and size (same host, 3 runs each)

```
  0.08 MP  Q60 effort6   0.072s 0.069s 0.072s     3,863 B
  2 MP     Q60 effort6   1.09s  1.10s  1.09s    302,068 B
  2 MP     Q80 effort6   1.078s 1.101s 1.086s    527,245 B
  8 MP     Q60 effort6   2.132s 2.118s 2.116s    637,278 B
  8 MP     Q60 effort0   0.200s 0.198s 0.195s    785,574 B
  8 MP     Q60 effort9   6.194s 6.124s 6.110s    633,409 B
--------------------------------------------------------------
  8 MP     jpeg Q80      0.144s               1,376,541 B
  8 MP     webp Q80      0.581s                 974,066 B
  8 MP     png           0.492s              19,389,662 B
  8 MP avif -> jpg (dav1d) 0.122s
  2 MP avif -> jpg (dav1d) 0.075s
```

8 MP AVIF at Q60 is 2.16× smaller than JPEG Q80 and costs ~15× JPEG's encode time. That ratio is
what the UI's progress/ETA copy should be built on; it is measured, not modelled.

### 1.6 Consequence for the wheel

No regression, so nothing moves: `AVIF output` stays `Proven` on the same evidence footing as the
other image pairs, and the release gate item "avif must be re-proven on the final dist" is now
closed (see `ARCHITECTURE.md` §6.1, §19, §20).

---

## 2. Real-viewer text fidelity for `docx → pdf` on Windows

### 2.1 The artifact under test

Converted on the Windows box with the S3-proven argv (`soffice.com`, file-URL profile, fresh
`-env:UserInstallation`; `spikes/s4/artifacts/docx2pdf-run.txt`):

```
rc=0   2,653 ms
convert ...\sample.docx as a Writer document -> ...\out\sample.pdf using filter : writer_pdf_Export
sample.pdf  39,946 B  sha256=D962480477BC053B9DA5CFA0A706D14818D93A16D1EDD32D01FDEB4772FA3534
```

The docx is the S3 fixture (`spikes/s3/fixtures.py`, `sample.docx`, 37,058 B,
`C760304B…C6347`); it contains `café – naïve → 中文` plus `MARKER-ALPHA` markers. Size matches the
S3 Windows leg exactly (39,946 B); the hash differs only because the PDF carries a creation date.

### 2.2 Control: the finding reproduces on this exact file

`pypdf 6.21.4` on the PDF above (`artifacts/pypdf-control.json`):

```
'… MARKER-ALPHA this paragraph proves text survives the conversion. cafe\x1c – naï\x1fve → 中文 …'
contains_cafe_accent: false      contains_cafe_0x1c: true
ctrl_codes: ["\u001c", "\u001f"]
/ CAAAAA+Cambria  bfchar codes: 00 FF 01 … 1A 1B 1D 1E 20 21 … 2E     ← 1C and 1F missing
```

So the object of the test is confirmed: the `ToUnicode` CMap of the Cambria subset skips exactly
`0x1C` and `0x1F`, and a library that trusts `ToUnicode` produces control characters where the
page draws `é` / `ï`.

### 2.3 How the viewers were driven (and why it needed a launcher)

An SSH session on this host lands in **session 0**, which cannot see the logged-on desktop
(`OpenInputDesktop` → `err=1`, `CopyFromScreen` → invalid handle). The harness therefore runs
through a SYSTEM scheduled task that calls `WTSQueryUserToken(active console session)` and
`CreateProcessAsUser` into session 11 (`win/launcher.ps1`), where it can:

- launch each browser on `file:///C:/build/s4/pdf/out/sample.pdf` with a throwaway profile,
- screenshot the window (`chrome-render.png`, `edge-render.png`),
- type into the find box and read the **find bar's own result string** out of UI Automation
  (`Result 1 of 1` / `No results`) — `*-find-cafe-uia.txt`,
- select the page text and read back what actually landed on the **clipboard**, via Win32
  `OpenClipboard`/`GetClipboardData(CF_UNICODETEXT)` (`*-clipboard.txt`).

Two harness bugs were found and fixed before the numbers below were taken, and are recorded so
nobody repeats them: (i) a `.ps1` with non-ASCII literals is read as ANSI by PowerShell 5.1, so
the first run pasted `cafÃ©` and every find bar correctly said "No results" — the source is now
ASCII-only and builds `é` from `[char]0x00E9`; (ii) `Get-Clipboard` is a poor reader from this
context, so the clipboard is read with `CF_UNICODETEXT` directly. A clipboard self-test
(`set 'SELFTEST-café' → read back`) runs at the start of each harness run.

### 2.4 Results

| viewer | version | renders `café – naïve → 中文` | search `café` | text layer / copy |
|---|---|---|---|---|
| **Chrome** | 156.0.8078.76 | **yes** (`chrome-render.png`) | **Result 1 of 1** | **`caf[00E9] – na[00EF]ve → 中文`** — 335 chars, byte-correct (`chrome-clipboard.txt`) |
| **Edge** | 156.0.4314.50 | **yes** (`edge-render.png`) | **Result 1 of 1**, match highlighted (`edge-find-cafe.png`) | **`cafe [2013] na[00EF]ve [2192][4E2D][6587]`** — the `é` is *gone*, replaced by a space (`edge-uia-doc-text.txt`) |
| **Brave** | 156.1.98.63 | **yes** | **Result 1 of 1** (ASCII control `MARKER-ALPHA` → 1 of 2) | **`caf[00E9] – na[00EF]ve → 中文`** — 335 chars, byte-correct (`brave-clipboard.txt`) |
| **Firefox text engine** (`pdf.js` 5.7.284, Node) | — | n/a (text layer only) | — | **`caf e[1C] – na ï[1F] ve → 中文`** — same failure as pypdf (`pdfjs-text-layer.txt`) |
| `pypdf` 6.21.4 (control) | — | n/a | — | `cafe[1C] – naï[1F]ve` |

Read the Edge line carefully — it is the interesting one. **Both** Edge engines that expose a text
layer disagree with each other: Edge 156 renders and searches the document through an
Adobe-Acrobat-branded PDF engine (`Powered by Adobe Acrobat` in its own accessibility tree) and
its document text reads `cafe ` — the accent is dropped, exactly like pypdf's `cafe\x1c` but
sanitised to a space instead of a control byte. Chrome and Brave, both on PDFium, recover
`caf[00E9]` correctly. Firefox's `pdf.js` text layer reproduces the raw `caf e[1C]`.

Screenshots: `chrome-render.png` (all four strings render correctly), `edge-render.png`,
`edge-find-cafe.png` (find bar `café`, counter `1/1`, hit highlighted).

### 2.5 What is *not* measured, stated plainly

- **Acrobat Reader itself is not installed on this host** and macOS is unavailable, so the two
  viewers named in the card as "if available" are missing from the table. Edge's PDF engine
  identifies itself as Adobe-derived, which is why its row is worth reading, but it is **not** a
  substitute for testing Acrobat Reader.
- **The Edge clipboard copy was not captured.** Two attempts were made; Edge's first-run
  "we've signed you in" toast took the selection both times, so the clipboard held the toast text
  rather than the PDF. What is measured for Edge is its accessible document text (the string a
  screen reader gets and the same text a copy is built from), not a literal paste. Its render and
  its find result *are* measured directly.
- The fixture is one page of accented Latin plus CJK. It does not attempt Vietnamese
  stacked diacritics (`ế`, `ộ`), which is the audience this matters most for.
- No viewer on Linux, no Preview: everything here is Windows.

### 2.6 Verdict

**(b) — some viewers are affected.** Concretely:

- the conversion, the pagination and the **rendering** are correct in every viewer tested;
- **searching** for `café` succeeds in all three browsers (all three say `1 of 1`);
- **copy / select / screen-reader** text is correct under **PDFium** (Chrome, Brave) and **loses
  the accent** under Edge's Adobe-derived engine and under `pdf.js` (Firefox's text layer), which
  is the same defect `pypdf` reports, in the same place.

Consequences, to be applied in A1 (`ARCHITECTURE.md` §6.2.4, §20):

1. The caveat **stands**. User-facing copy must not promise "searchable/still selectable accented
   text" on Windows.
2. The rule "validators may not depend on text extraction of non-ASCII characters" is confirmed
   by two independent engines, not just pypdf, and stays load-bearing.
3. This is a **LibreOffice Windows exporter** behaviour (Cambria subset `ToUnicode`), not
   something EZ Convert introduces at packaging time. Nothing in our argv changes it.
