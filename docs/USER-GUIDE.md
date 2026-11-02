# EZ Convert — user guide

Version 0.0.1 (v1, internal build). Last updated 2026-11-02.

EZ Convert converts files on your own machine: images, audio, video and office documents.
It has no account, no cloud, and no network connection at run time (`docs/ARCHITECTURE.md`
§1, §16 S8). Everything below describes what this build actually does, not what it is
planned to do; §12 lists what it does not do yet.

---

## 1. The two surfaces

One app, two ways to start a conversion. Both feed the same queue, the same output rules
and the same Undo history, so a file converts identically whichever surface you use.

| | **Wheel** | **Window** |
|---|---|---|
| How it opens | global hotkey, at the pointer | from the tray icon / Start menu / Dock |
| Best for | one or two files you are looking at | a batch, or when you want to see the queue |
| Intake | drag files from Explorer/Finder onto the ring | drag and drop, or **Add files…** / **Add folder…** |
| Sizing | always-on-top ring, 520 × 520, never takes focus | normal resizable window |

Both are measured on Windows 11. On macOS the window path is measured; the wheel's
focus-free overlay behaviour is not (see §12).

---

## 2. Installing

### Windows 11 (x64)

1. Run `EZ Convert_0.0.1_x64-setup.exe` (about 489 MiB; measured 512,319,647 B on
   2026-11-02, `evidence/p2-windows-installer.md` §10).
2. **Windows SmartScreen will warn you.** The installer is not code-signed (a deliberate
   decision, `docs/DISTRIBUTION.md`). Click **More info → Run anyway**. That is a
   documented step in this build, not a fault you need to fix.
3. No administrator password is requested and no UAC prompt appears. The app installs
   per user into `%LOCALAPPDATA%\EZ Convert\`.
4. Launch **EZ Convert** (the executable is `%LOCALAPPDATA%\EZ Convert\ez-convert.exe`).
   The installer carries the WebView2 runtime, so it works on a machine that has never
   been online.

The installer contains the conversion engines (libvips, ffmpeg/ffprobe, LibreOffice).
Unpacked they occupy roughly 1 GB (staged payload 970.9 MiB, `evidence/p2-windows-installer.md`
§5). The first conversion after installation is slow — Windows Defender scans the freshly
unpacked engine tree; measured cold first image conversion was 113 s, warm conversions are
under a second to a few seconds (`evidence/p2-acceptance-clean-win11.txt`).

To uninstall: Windows **Settings → Apps → EZ Convert → Uninstall**.

### macOS (Apple Silicon and Intel)

Both architectures are supported. The one step that differs is step 3, which picks the engine
pack for your Mac.

1. Open `EZ-Convert-0.0.1-macos-universal.dmg` and drag **EZ Convert** into
   `/Applications`. Use `/Applications`, not a desktop copy: a copy run from another
   location may be moved by App Translocation and fail to find its engine pack.
2. **Gatekeeper will block the first launch.** The app is ad-hoc signed, not notarised
   (`docs/DISTRIBUTION.md`). Either right-click the app and choose **Open**, or run:

   ```bash
   xattr -d com.apple.quarantine "/Applications/EZ Convert.app"
   ```

   This is a documented one-time step in this build.
3. Open the engine-pack DMG for your Mac — `EZ-Convert-Engine-Pack-arm64.dmg` on Apple
   Silicon, `EZ-Convert-Engine-Pack-x86_64.dmg` on Intel (`uname -m` prints `arm64` or
   `x86_64`) — and double-click **Install Engine Pack.command**. If macOS refuses to open it,
   right-click → **Open**. The installer compares the pack's `arch` with `uname -m` and
   **refuses a mismatched pack** before verifying or copying anything; the wrong pack would
   otherwise leave every conversion failing with `Bad CPU type in executable (os error 86)`
   (`docs/MACOS-PACKAGING.md` §1). It then checks every file against `MANIFEST.sha256` and
   installs to:

   ```
   ~/Library/Application Support/app.ezconvert.desktop/engines
   ```

   The published packs are 361,154,840 B (arm64) and 380,435,751 B (x86_64); each contains the
   LibreOffice tree, libvips and ffmpeg built for that architecture
   (`evidence/p3-engine-pack-manifest.md`, `evidence/p3-x86-pack.md`). The pack is installed
   once, separately from the app, because Tauri's macOS bundler cannot sign engines placed
   inside the app bundle (`docs/MACOS-PACKAGING.md` §1).
4. Open **EZ Convert** normally.

To verify an installation from Terminal:

```bash
"/Applications/EZ Convert.app/Contents/MacOS/ez-convert" --diagnostics --fixtures .
```

To uninstall: delete `/Applications/EZ Convert.app` and
`~/Library/Application Support/app.ezconvert.desktop`.

**Intel Macs:** supported, with the x86_64 pack from step 3. The two packs are separate builds
and are not interchangeable, so the installer refuses a pack whose `arch` is not your
`uname -m` — that check is what stops the old `Bad CPU type in executable (os error 86)`
failure mode from being mistaken for an app bug (`docs/MACOS-PACKAGING.md` §1). Measured on
Intel hardware, macOS 15.7.9 (build 24G830): install, `codesign --verify --strict`, 5/5
engines resolved, `SUMMARY families=4 pass=4 fail=0` — one real conversion per family
(`evidence/p3-x86-pack.md`; matrix in `docs/MACOS-PACKAGING.md` §7).

### Upgrading

There is no auto-update in this build (`SPEC.md` §7). To upgrade, install the new build
over the old one; settings and the Undo journal survive in the per-user folders below.

| what | Windows | macOS |
|---|---|---|
| app + engines | `%LOCALAPPDATA%\EZ Convert\` | app in `/Applications`, engines in the pack folder above |
| settings | `%APPDATA%\app.ezconvert\settings.json` | `~/Library/Application Support/app.ezconvert/settings.json` |
| job temp, journal, logs | `%LOCALAPPDATA%` app cache under `app.ezconvert` | `~/Library/Caches/app.ezconvert` |

---

## 3. The window

- **Queue** — one row per file: name, detected type, target, state, size. Rows report what
  is happening (detecting, queued, converting, done, failed) rather than a spinner.
- **Convert all to** — picks the target format for every queued file. Only formats valid
  for the file that is actually queued appear; a JPEG renamed `.png` is offered JPEG's
  targets because detection reads content, not the extension (`docs/DETECTION.md` §1).
- **Strip metadata** — a per-batch switch that overrides the setting for this run only.
- **Add files… / Add folder…** — a dropped folder contributes its own files; tick
  *Include subfolders on folder drops* in Settings to go deeper.
- **Settings** — generated from the app's schema, so every rule in §8 has a control:
  where files go, batch threshold and folder name, metadata, wheel segments and order,
  hotkey, smart grab, sound, Undo window, PDF page, diagnostics, limits.
- **Toast** — appears when a batch finishes, with **Show** (scrolls to the finished rows and
  highlights them) and **Undo** (delete what this job created, for the Undo window).

Nothing in the window is an account, a sign-in, an update check or a usage report: there
is no such UI because there is no such code.

---

## 4. The wheel

Press the hotkey (default **Ctrl+Alt+Shift+D**) and a ring appears at the pointer. If
another application already owns that chord, the app records the OS error, falls back to
**Ctrl+Alt+D**, and saves the working chord — it does not pretend to have registered
(`evidence/u3-intake.md` §3).

- Drag one or more files from Explorer onto the ring (the Finder drag is unmeasured on
  macOS — see §12). Drop a folder to take its files.
- The ring shows at most six targets; anything beyond that lives behind a **more**
  segment that opens a sub-ring. Labels never shrink below the legibility floor — the
  layout is refused rather than rendered too small to read (`ui/wheel/README.md`).
- The ring does not take focus, and outside the circle there is no window at all, so
  clicks and drops go to whatever is behind it. **Esc** or a click outside dismisses it.
- While a conversion runs the ring stays open and shows per-file state; another drop
  queues more work.
- **Smart grab** (Settings → *Smart grab*, off by default) is documented as taking the
  current Explorer/Finder selection when the hotkey fires with nothing dragging. The
  consent half is implemented and tested, but the selection reader is not implemented in
  this build, so turning it on does not yet grab anything (`evidence/u3-intake.md` §4).

The wheel's placement, hotkey conflict ladder and dismiss rules are measured on Windows 11.
On macOS the equivalent window primitives have not been measured — see §12.

---

## 5. Formats you can convert

A format appears as a target only if there is a measured, passing end-to-end conversion for
that direction. The table below is the shipped matrix (`docs/DETECTION.md` §4); the
columns are targets, and a dash means the input is recognised but offers no conversions yet.

### Images

| input | targets |
|---|---|
| jpg | jpg, png, webp, tiff, avif, gif |
| png | jpg, webp, tiff, avif, gif |
| heic | jpg, png, webp, tiff, avif, gif |
| avif | jpg, png, webp, tiff, gif |
| webp | jpg, png, tiff, avif, gif |
| gif | jpg, png, webp, tiff, avif (first frame only) |
| tiff | jpg, png, webp, avif, gif |
| bmp, svg | — |

### Audio

`mp3`, `wav`, `flac`, `m4a`, `aac`, `ogg`, `opus` — each converts to any of those seven,
including to itself (a genuine re-encode, not a copy).

### Video

| input | targets |
|---|---|
| mp4, mkv, mov, webm | mp4, mkv, mov, webm, gif |

### Documents

| input | targets |
|---|---|
| docx | pdf, md, txt, html, rtf |
| doc | pdf, md, txt, docx, html, rtf |
| rtf | pdf, md, txt, docx, html |
| md | pdf, txt, docx, html, rtf |
| html | pdf, md, docx, rtf |
| xlsx | pdf, csv, txt, html |
| xls | pdf, csv, xlsx, txt, html |
| csv | pdf, xlsx, txt |
| pptx | pdf, html |
| ppt | pdf, pptx, html |
| txt, pdf | — |

A **PDF is input-only in this build**: it is detected, but no target is offered, because
the raster path is not packaged yet. Plain **txt** is detected and deliberately offers
nothing: the app refuses rather than route text through a document engine that would
produce a plausible-looking PDF from a file that is not a document at all
(`docs/DETECTION.md` §5).

---

## 6. What "converted" means, per family

### Images

- Pixel dimensions are preserved (an EXIF orientation is applied, so a rotated phone photo
  comes out upright with the orientation flag cleared, not rotated twice).
- **ICC profiles and EXIF are kept by default**, where the target container has a field for
  them. With *Strip metadata* on, they are removed — and the app verifies the strip
  actually happened, so a strip run that leaves metadata behind fails instead of reporting
  success.
- **HEIC output is not offered.** HEVC encoding needs a GPL encoder or an OS codec pack, so
  HEIC is input-only in this build.
- **GIF input converts its first frame only.** Animated GIF is labelled that way in the UI;
  GIF-to-GIF re-encoding is not offered.
- Still images never pass through ffmpeg. ffmpeg only packages the palette for a GIF
  target, from a normalised PNG that libvips wrote.

### Audio and video

- Encoder and sample rate are set explicitly for each target, so you get a real file of the
  format you picked, not a container rename.
- Audio tags (title/artist/album and the rest) travel with the file by default and are
  dropped when you strip metadata. Video carries no metadata promise in this build.
- Video output is constant frame rate, with the duration checked against the source.
- **Transcoding a lossy format to another lossy format loses quality.** mp3 → mp3,
  aac → ogg, mp4 → webm and similar are re-encodes at the app's fixed settings. There is no
  quality, bitrate, trim or resize control in this build.

### Documents

- `.docx/.pptx/.xlsx/.doc/.ppt/.xls → PDF` is **layout-faithful, not Word-identical**. Page
  geometry, text, tables and images carry over; nothing promises a pixel-for-pixel match
  with Microsoft Word (`SPEC.md` §4).
- **Documents set in Aptos will reflow.** Aptos is Word's newer default font; it is not
  bundled with the app and has no metric-compatible substitute, so line breaks and page
  counts can differ from what Word shows (`docs/ARCHITECTURE.md` §6.2.3). Documents set in
  Calibri, Cambria or Times New Roman are measured to keep their layout.
- **On Windows, copying accented text out of a generated PDF can be wrong in some
  viewers.** Rendering and in-viewer search are correct, but the extracted text layer
  drops or mangles characters such as `é`/`ï` in Edge, Acrobat and Firefox's pdf.js;
  Chrome and Brave copy correctly (`docs/ARCHITECTURE.md` §6.2.4). Do not rely on
  copy-paste of accented text from a converted PDF on Windows.
- **Hebrew in documents that name the Culmus (CLM) typefaces** is rendered with shipped
  open-licence twins (David CLM → David Libre, Frank Ruehl CLM → Frank Ruhl Hofshi,
  Miriam CLM → Miriam Libre, Miriam Mono CLM → Liberation Mono, Nachlieli CLM → Alef).
  The text is drawn, not dropped, but character widths differ by roughly 4–8 %, so lines
  may rebreak (`evidence/hebrew-fidelity.md`).
- **Spreadsheets → csv/txt are named per sheet** — `<stem>-<sheet>.csv` — because a workbook
  has one table per sheet. A one-sheet workbook gives `<stem>-<sheet>.csv`, not
  `<stem>.csv`.
- `md` and `html` output are extraction-grade: headings, lists, tables and links survive;
  fonts, columns and page layout do not.
- What is deliberately *not* offered: spreadsheet or slide deck → markdown (the document
  engine has no such export), html → txt (markdown keeps the heading instead), and any
  legacy `.doc/.xls/.ppt` as an output format. The reasons are measured and recorded in
  `docs/DETECTION.md` §3.

---

## 7. Metadata

| family | kept by default | removed by *Strip metadata* |
|---|---|---|
| images | EXIF, ICC profile, dates (where the target format has the field) | yes, verified |
| audio | tags (title, artist, album, …) | yes, verified |
| video | nothing claimed | nothing claimed |
| documents | nothing claimed — document properties are outside the policy | — |

GIF has no EXIF/ICC field of its own, so the app reports the metadata check as not
applicable for GIF targets rather than claiming a pass.

---

## 8. Where files go, and Undo

Defaults, all of them changeable in Settings:

1. **Same folder as the original.** Choose *Chosen folder* to send everything elsewhere.
2. **Batches of 3 or more go into a `converted/` subfolder** next to the originals. Change
   the threshold, rename the folder, or turn the rule off entirely.
3. **Never overwrite.** If `photo.webp` exists, the app writes `photo-1.webp`, then
   `photo-2.webp`. The original is never replaced and you are never asked.
4. **Atomic placement.** The engine writes into a job temp folder; the verified result is
   moved into place in one step. A crash or a full disk leaves your folder as it was.
5. **Undo (10 seconds by default).** The toast's **Undo** deletes the files this job
   created — and any empty folder it created — and nothing else. Undo refuses (and keeps
   everything) if the output has changed since it was written, if the Undo window has
   passed, if the target is now a symlink, or if the app has been closed.
6. **Folders a job creates are removed only while empty.**

Reserved Windows names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`),
trailing dots or spaces, paths over 260 characters and network paths are refused at
admission with a message that says what to change.

---

## 9. Limits and refusals

These are deliberate refusals, not failures:

| situation | what happens |
|---|---|
| file larger than 2 GiB | refused before any conversion |
| still image declaring more than 250,000,000 pixels | refused (a decompression bomb is not caught by a byte limit) |
| ZIP/Office container declaring more than 8 GiB uncompressed, or more than 10,000 entries | refused |
| ZIP64 archive | reported unsupported; not guessed |
| JBIG-compressed TIFF | refused — this build has JBIG disabled for licence reasons |
| password-protected file | refused; the app cannot open encrypted files |
| unrecognised content | no targets offered; the file is untouched |
| truncated JPEG/PNG/GIF/PDF/RTF | flagged incomplete; the original is left as it is |

Every refusal states whether the original was modified. It never is.

---

## 10. Privacy

- **No network calls at run time.** No update check, no telemetry, no crash reporting, no
  font download, no licence phone-home. The app has no HTTP dependency and the Tauri
  capability set grants it no network permission (`docs/ARCHITECTURE.md` §1, §16 S8).
- **Engines run as separate processes** with a cleared environment: no proxy variables, no
  credentials inherited, each conversion its own child process. An engine crash is a failed
  job, not a dead app.
- **Everything stays on the machine.** Output files land next to the originals (or in the
  folder you chose); the job temp area, the Undo journal and up to 50 job logs (a setting)
  live in the app's own cache folder.
- **The only thing that leaves is what you send yourself** — the app has no upload path.
- Engine messages are stored locally for diagnostics and are shown in the window only when
  you ask for the job log.

---

## 11. If something goes wrong

1. Read the row. Failures appear in plain language and say whether the original file was
   touched (it is not).
2. Open the job log from the window's diagnostics area if the message points at an engine.
3. Run the built-in check from a terminal — the same binary the window uses, resolving
   engines the same way:

   ```bash
   # Windows
   "%LOCALAPPDATA%\EZ Convert\ez-convert.exe" --diagnostics --fixtures <folder-with-test-files>

   # macOS
   "/Applications/EZ Convert.app/Contents/MacOS/ez-convert" --diagnostics --fixtures .
   ```

   It prints one line per engine it found, then one real conversion per family and a
   `SUMMARY families=4 pass=4 fail=0` line. A `MissingEngine` line means the engine pack or
   installer did not complete.
4. Common causes: engine pack not installed (macOS), installer run without its payload
   (Windows: reinstall), or a source file that is damaged.

---

## 12. Known limitations in this build

These are current, measured limits — not bugs waiting to be discovered:

1. **The builds are unsigned.** Windows SmartScreen needs *More info → Run anyway*; macOS
   Gatekeeper needs right-click → Open or `xattr -d com.apple.quarantine`. `docs/DISTRIBUTION.md`
   records the decision (internal-only distribution).
2. **The macOS wheel overlay is unmeasured.** Hotkey, placement, click-through and dismiss
   rules are measured on Windows 11; the macOS window primitives behind them have not been
   run on a Mac.
3. **Smart grab does nothing yet.** The toggle and its consent behaviour exist; the code
   that reads the current Explorer/Finder selection is not implemented.
4. **Aptos documents reflow** — Word's newer default font is not bundled and has no metrical
   clone.
5. **Copying accented text out of a Windows-generated PDF can be wrong** in Edge, Acrobat
   and pdf.js; rendering and search are fine.
6. **Hebrew Culmus typefaces are substituted** with open-licence twins whose widths differ
   by up to ~8 %, so lines can rebreak.
7. **No resize, trim, bitrate, quality or codec controls.** Conversions use fixed encoder
   settings.
8. **Multi-sheet workbooks → csv/txt are not fully wired**: the document engine writes one
   file per sheet (`<stem>-<sheet>.csv`), while the app's publish step records one output
   per input in this build (`evidence/c4-output-rules.md` §5). Treat multi-sheet csv/txt
   output as unverified.
9. **Recognised but not convertible:** bmp, svg, pdf (as input), txt.
10. **Not offered as outputs:** HEIC, SVG, PDF (as an image output), any legacy Office
    format as an output.
11. **GIF input is first-frame only**, and GIF-to-GIF animation re-encoding is not offered.
12. **No auto-update** — upgrade by reinstalling.
13. **First image conversion after a fresh engine-pack unpack is slow** on a Defender-scanned
    install: the scanner reads the whole vips tree (96.5 s once it has seen the tree,
    216.9 s on the genuine first touch). The engine cap is 600 s so this completes instead
    of being killed; warm conversions are ~0.1 s. A very slow disk could still be
    noticeable, but no longer fails.

---

## 13. How these claims are checked

A format is advertised only when the end-to-end suite converts a real file in that
direction and checks the output's existence, content and metadata — not when the code
merely compiles (`e2e/q3/README.md`).

| leg | result |
|---|---|
| Windows 11, shipping engine dist | **154/154 rows pass, 0 skip, 13/13 planted-fault controls caught** (`evidence/avif-strip.md`, `windows.txt`) |
| Linux test dist | 148/148 runnable directions pass; the six `→ avif` directions skip because that test dist has no AV1 encoder (`evidence/q3-e2e.md`) |
| macOS 26 arm64 | one conversion per family: 4/4 pass, and 4/4 through the x86_64 slice under Rosetta (`evidence/p3-macos-validation.md`) |
| macOS 15.7.9 Intel x86_64, native | install from the published DMGs, 5/5 engines resolved, one conversion per family: 4/4 pass (`evidence/p3-x86-pack.md`) |

The suite plants faults too — a deleted output, zero-byte output, JPEG bytes named `.png`,
a text file named `.docx`, a PDF with no pages, metadata that survives a strip, a WAV
offered as mp3 — and fails if it does not catch them. A suite that cannot go red is not
evidence.
