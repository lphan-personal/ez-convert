# S4, second host — both items repeated on the dedicated test VM (`win11-dev`)

The first pass (`EVIDENCE.md`) ran on **Ryo-PC** (10.0.0.103). The Windows target was then moved to
the dedicated VM, where synthetic input and GUI automation are allowed:
`ryo@10.0.0.191`, Windows 11 Pro 10.0.26200, 6 vCPU / 8 GB, autologon console session 2
(`converter/win11-dev/VM.md`).

Both items were repeated there. Item 1 is a **second, independent run of the same unchanged suite
against the same dist**; item 2 gained the viewer the first pass could not test — **real Adobe
Acrobat**, plus a real Firefox GUI instead of pdf.js under Node.

Everything below is tool output. Harness copies are in `artifacts-vm2/`:

| path | what |
|---|---|
| `artifacts-vm2/avif/avif-final-vm.log` | the full 18-avif-tests.sh run (34 assertions) |
| `artifacts-vm2/viewers/` | per-viewer logs, find-bar UIA dumps, clipboard text, screenshots |
| `artifacts-vm2/viewers/preflight.sh` | proves the toolchain the suite needs was present |

---

## 1. avif suite on the final dist — second host: **PASS, 34 assertions, 0 failures**

### 1.1 The dist, verified on the VM by hash

`C:\build\dist` was tarred on Ryo-PC, moved by `scp`, and then checked **on the VM** with
PowerShell `Get-FileHash` against the committed manifest `artifacts/FINAL-dist-spec-aom-sha256.txt`
(43 entries):

```
manifest entries : 43
files on disk    : 43
hash mismatches  : 0
missing          : 0
not in manifest  :
total bytes      : 35824560
libvips-42.dll   : edea7dc5ea5fae8479d4a057e51f9ed12052b7a371c7f60895895d62eb586410
libheif.dll      : 92f1c402a412ae8e8129af7da81fc6639b3afeb5f7b352d17365e96815e0d4a9
VERDICT          : dist is byte-identical to the manifest
```

Same two hashes the card names in its "final dist" column. The check is per-file, so the pairing
between the artifact and the run cannot drift.

### 1.2 The script, unchanged

```
18-avif-tests.sh on the VM : d5314fbfa29e9b21a826a2835a35417eab9dccf6e7b90a8dfdd5a63ccbb74224
repo copy                  : d5314fbfa29e9b21a826a2835a35417eab9dccf6e7b90a8dfdd5a63ccbb74224
```

Byte-identical to the copy the S2b card signed off and to `build/windows/scripts/18-avif-tests.sh`.
Not one line was edited for this host.

### 1.3 Prerequisites (the VM had none of them)

The VM is a bare Windows 11 install — no MSYS2, no bash, no python. `msys2-base-x86_64-20260927`
was installed at `C:\msys64`; its base already ships `python3`, `objdump` 2.47 (binutils), and the
coreutils the script uses. `preflight.sh` output:

```
objdump     : GNU objdump (GNU Binutils) 2.47.20260726  -> /usr/bin/objdump
stat/awk/sed/grep/od/head/ls/mkdir/rm/date/sha256sum/base64 : all present in /usr/bin
icc         : 3144 bytes (C:\Windows\System32\spool\drivers\color\sRGB Color Space Profile.icm)
objdump on vips.exe: 13 DLL imports
```

Fixtures (`base.png`, `sample1.heic`, `orient6.heic`, `orient6.jpg`, `mkjpeg_exif2.py`) were copied
into `C:\build\fx` — the paths the script expects.

### 1.4 The run

```
bash -lc "D=/c/build/dist bash /c/build/s4/18-avif-tests.sh"
```

```
dist: /c/build/dist
vips-8.18.7
date(UTC): Sun, Nov  1, 2026 10:08:21 PM
...
######## SUMMARY ########
  PASS=34  FAIL=0
```

rc=0. **34 assertions, 0 failures — the same count as the Ryo-PC run.** Shell rc and summary agree.

### 1.5 The cases the card asked for, quoted from this run

**jpg → avif → jpg with EXIF IFD0 + ICC surviving**

```
  PASS  big2mp.avif kept icc-profile-data
  PASS  big2mp.avif kept exif-ifd0-Make
  PASS  big2mp.avif kept exif-ifd0-Model
  PASS  big2mp.avif kept exif-ifd0-Software
  PASS  big2mp.avif kept exif-ifd0-DateTime
  PASS  avif->jpg kept the ICC profile
  PASS  avif->jpg kept EXIF IFD0
```

**orientation-6, no double rotation**

```
  PASS  orientation-6 jpg -> avif
  PASS  avif round trip lands correctly oriented at 1152x1728
  PASS  no stale EXIF Orientation in the avif (libheif rewrote it; irot carries the rotation)
  PASS  orientation-6 avif still carries the rest of the EXIF IFD0
  PASS  decoded avif presents Orientation=1 (irot already applied -> autorot is a no-op)
```

Source 1728×1152 at `Orientation: 6` decodes 1152×1728 with the tag gone, so nothing can rotate it
twice.

**avif → jpg decode**

```
  PASS  avif(400x200) -> jpg
  PASS  avif(2 MP) -> jpg
  PASS  avif -> jpg (dav1d decode of the avif we just wrote)
  PASS  HEIC decode (orient6.heic) still works
  PASS  top-left still blue (30 59 221)
```

**closure / copyleft**

```
  PASS  0 banned imports across the whole dist
  PASS  libjbig-0.dll not in the closure
```

### 1.6 Latency on this host (6 vCPU — not the build box)

```
  0.08 MP  Q60 effort6    0.114s  0.111s  0.106s      3,863 B
  2 MP     Q60 effort6    3.057s  2.868s  2.941s    294,804 B
  2 MP     Q80 effort6    3.603s  3.760s  3.536s    525,271 B
  8 MP     Q60 effort6    6.971s  6.984s  6.844s    634,158 B
  8 MP     Q60 effort0    0.353s  0.353s  0.367s    781,161 B
  8 MP     Q60 effort9   19.596s 21.708s 20.203s    630,409 B
--------------------------------------------------------------
  8 MP     jpeg Q80       0.211s                  1,376,541 B
  8 MP     webp Q80       0.775s                    974,066 B
  8 MP     png (lossless) 0.715s                 19,389,662 B
  8 MP avif -> jpg (dav1d)  0.182s
  2 MP avif -> jpg (dav1d)  0.115s
```

8 MP Q60 is **3.3× slower here than on Ryo-PC** (6.93 s vs 2.12 s) with 4× fewer cores. The sizes
match the PC run to within a few hundred bytes; the *ratios* (avif ≈ 2.2× smaller than JPEG Q80 at
~33× the encode time on this host, 7× on Ryo-PC) are what the UI's progress copy should use. Ship
numbers must come from the build host, not from this VM.

### 1.7 Consequence

Nothing moves: no assertion regressed on either host, so avif output stays `Proven` and the wheel is
unchanged. Two independent hosts, two independent dist verifications, 34/34 both times.

---

## 2. Real-viewer text fidelity — this time including real Acrobat

### 2.1 The artifact under test

Fresh `docx → pdf` on the VM with the S3 argv (`soffice.com`, `file:///C:/` profile,
`writer_pdf_Export`):

```
rc=0  10,515 ms  %PDF-1.7
sample.docx  37,058 B  sha256=C760304BAD00D1AFBA8A4289E84CAAC2C38DA84094B9F5418BD5D41BE32C6347
sample.pdf   46,489 B  sha256=75F3E2199CE1C4F6D0C70C89774179856BA873672512D5B639521829972F5FC2
```

The docx hash is the S3 fixture exactly. The PDF is **larger than the Ryo-PC one** (46,489 vs
39,946 B) even though both hosts run LibreOffice 26.8.2.2 with Cambria present — so this is a
*different PDF*, which is why the `pypdf` control was re-run on it instead of assumed:

```
'… MARKER-ALPHA this paragraph proves text survives the conversion. cafe\x1c – na\xef\x1fve → 中文 …'
contains_cafe_0x1c: true    contains_naive_0x1f: true
/CAAAAA+Cambria ToUnicode: 00 01 … 1B 1D 1E 20 21 …  → 0x1C and 0x1F absent
```

The defect is in the VM's PDF too, in the same two codes.

### 2.2 Results — four real viewers, each driven in a real GUI session

Session 2 via `WTSQueryUserToken`/`CreateProcessAsUser`; screenshots, find-bar UIA strings and
Win32 `CF_UNICODETEXT` clipboard reads. No library was used to decide any of these.

| viewer | version | renders `café – naïve → 中文` | find `café` | copy / text layer |
|---|---|---|---|---|
| **Chrome** (PDFium) | 156.0.8078.80 | **yes** (`chrome-1-render.png`) | **Result 1 of 1** | **`caf[00E9] [2013] na[00EF]ve [2192] [4E2D][6587]`** — 335 chars, correct (`chrome-3-clipboard.txt`) |
| **Firefox** (pdf.js in the real browser) | 159.0 | **yes** (`v2-firefox-1-render.png`) | **1 of 1 match** | **`cafe[001C] … na[00EF][001F]ve`** — `é` becomes `0x1C`, a stray `0x1F` glyph (`v2-firefox-3-clipboard-1.txt`) |
| **Acrobat** (full app, not Reader) | 26.2.22031.0 | **yes** (`acrobat10-1-render.png`) | not captured — see 2.3 | **`caf[00E9] [2013] ve [2192] [4E2D][6587]`** — `é` survives, **`naï` is dropped** (`acrobat10-3-clipboard-1.txt`, identical on two attempts) |
| **Edge** (Adobe-Acrobat-branded PDF engine) | 156.0.4314.58 | **yes** (`v2-edge-1-render.png`) | **Result 1 of 1** | not captured in the UI; accessible document text (Ryo-PC) is `cafe ` — `é` dropped (`v2-edge-2-find-cafe-uia.txt`) |
| `pypdf` 6.21.1 (control) | — | n/a | n/a | `cafe[001C] … na[00EF][001F]ve` |

Arrangement of the numbers: every viewer **renders** correctly and every viewer that was asked to
**search** finds `café` (`1 of 1`). What differs is the **text layer**, and it differs in three
distinct ways: PDFium reconstructs the glyphs from the embedded font and is correct; pdf.js trusts
`ToUnicode` and emits the same control codes as `pypdf`; Acrobat's extraction drops the glyph run
whose mapping is broken (`naï` disappears, `café` and `中文` survive); Edge's Acrobat-branded engine
renders the missing code as a space.

### 2.3 What is still not measured — stated plainly

- **Acrobat's find result.** Acrobat draws its own find UI; it exposes no counter to UI Automation
  ("no counter in UIA (see screenshot)"), and the Ctrl+F → Ctrl+V route put the query into the
  wrong field (Acrobat's top-bar search box was not focused). Render and copy *are* measured and
  were reproduced twice. Finding `café` in Acrobat is therefore **unverified**, not "failed".
- **Edge's clipboard.** Ctrl+A does not bind in Edge's Acrobat-branded PDF viewer here: the
  clipboard still held the harness sentinel after both attempts (`v2-edge-summary.txt`), and a
  mouse-drag selection landed on the address bar instead of the page. What is measured for Edge is
  its render, its find result, and (on Ryo-PC) its accessible document text, which drops `é`.
- **macOS Preview**: no Mac on this project. **Vietnamese stacked diacritics** (`ế`, `ộ`): not
  tested — one page of accented Latin plus CJK is all the fixture carries.

### 2.4 Verdict

**(b) — some viewers are affected**, now on a second host and with the reference viewer tested.
Rendering and in-viewer search are fine everywhere measured; **copying accented text out is only
correct under PDFium**, and Acrobat — the viewer most users will open — loses part of the accented
word. A1's user-facing copy must not promise searchable/selectable accented text on Windows, and
no validator may read text out of a PDF (confirmed now by Chrome/Brave vs Firefox vs Acrobat vs
pypdf, four independent implementations).
