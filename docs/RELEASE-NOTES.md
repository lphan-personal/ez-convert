# Release notes — EZ Convert v1 (0.0.1)

Released 2026-11-02. Internal build. Not published, not notarised, not a store package —
see `docs/DISTRIBUTION.md`.

---

## What v1 is

A local, offline file converter with two surfaces in one app: a radial overlay that appears
at the pointer on a global hotkey, and a normal window with a batch queue. It converts
images, audio, video and office documents by spawning engine processes on the same machine.

- **148 conversion directions** advertised and measured: 37 image, 49 audio, 20 video,
  42 document.
- **Content-first detection** — the extension is recorded and compared, never trusted. A
  JPEG renamed `.png` converts as JPEG. A text file renamed `.docx` is refused, because
  LibreOffice would otherwise answer `rc=0` and write a plausible-looking PDF from it.
- **Output rules as rules** — same folder by default, `converted/` for batches of three or
  more (threshold, name and on/off are settings), never overwrite (`-1`, `-2`…), atomic
  placement, verified 10-second Undo that deletes only what the job created.
- **Metadata kept by default** (EXIF/ICC/dates/audio tags) with a per-batch and per-app
  strip toggle; the strip is verified, not assumed.
- **Offline by construction** — no network code, no telemetry, no update check, engines
  spawned with a cleared environment.

---

## Installation

| platform | artifact | measured |
|---|---|---|
| Windows 11 x64 | `EZ Convert_0.0.1_x64-setup.exe`, per-user NSIS, no UAC | 512,319,647 B (488.59 MiB), sha256 `d49a012ae521f73a24485e69a6cacdf88f2c7cb96464687c9bf9ce7c97c3ed5d` |
| Windows 11 x64 | same installer, first bundle (kept because it is the one with the clean-machine transcript) | 512,304,426 B (488.57 MiB), sha256 `5432C8AEFF26B7AF81B3CF3389ABED010E8438EDB2B1CB32D05AFFD9635C7BFF` |
| macOS 11+ | `EZ-Convert-0.0.1-macos-universal.dmg` (arm64 + x86_64 slices, ad-hoc signed) | sha256 `9002d00350965cec7ea8d7edc837be1568ed8390e82303efcc69a6a74e987144` |
| macOS, Apple Silicon | `EZ-Convert-Engine-Pack-arm64.dmg` (separate install) | 362,201,061 B, sha256 `5f6c41050dfa91d04aa91ed7d6b09ba47598b4bdb9c45ecfb66dd1c9e998eb9c` |

Both macOS DMGs install and convert on Apple Silicon; the engine pack must be installed
(`Install Engine Pack.command`, which verifies `MANIFEST.sha256` before copying). The app
bundle contains no engines by design — Tauri's macOS bundler cannot sign them there.

**Unsigned builds:** Windows SmartScreen → *More info → Run anyway*; macOS Gatekeeper →
right-click → *Open*, or `xattr -d com.apple.quarantine "/Applications/EZ Convert.app"`.
These are documented steps in v1, not defects. `docs/USER-GUIDE.md` §2 has the full install
walkthrough.

---

## Measured evidence behind the format claims

| leg | suite | result |
|---|---|---|
| Windows 11, shipping engine dist | `scripts/e2e.sh` | **154/154 rows pass, 0 fail, 0 skip**, 13/13 planted-fault controls caught (`evidence/avif-strip.md`) |
| Linux test dist | `scripts/e2e.sh` | 148/148 runnable directions pass, 6 `→ avif` skips (that dist has no AV1 encoder), 13/13 controls (`evidence/q3-e2e.md`) |
| macOS 26, arm64 | one conversion per family | 4/4 pass; the x86_64 slice under Rosetta 4/4 (`evidence/p3-macos-validation.md`) |
| Windows 11, clean machine, offline | installed build | `SUMMARY families=4 pass=4 fail=0` (cold 198 s, warm 3 s) (`evidence/p2-acceptance-clean-win11.txt`) |

Each row asserts existence, content sanity (decoded geometry and non-blank mean for images,
`ffprobe` codec/rate/duration for audio and video, page count or container part or source
marker for documents) and the metadata policy. The suite also plants faults — deleted
output, zero-byte output, wrong container, wrong geometry, text file named `.docx`, PDF with
no pages, metadata surviving a strip, WAV offered as mp3, duration drift, missing video
stream — and fails if it does not catch them.

Two defects were found by those runs and fixed before this release:

- **AVIF with `strip metadata` wrote no file at the requested path** (the saver bracket
  became a literal file name, and `--keep all` overrode `--strip`). Fixed; the Windows leg
  then went 154/154.
- **The installed Windows build resolved the LibreOffice launcher as `soffice.com.exe`**,
  so every document conversion reported `MissingEngine`. Fixed and re-bundled; the accepted
  installer is the second bundle.

---

## Document fidelity, stated plainly

- `.docx/.pptx/.xlsx/.doc/.ppt/.xls → PDF` is **layout-faithful, not Word-identical**.
- **Aptos is not bundled and has no metric clone**, so documents set in Word's newer default
  font reflow. Calibri, Cambria and Times New Roman are measured to keep their layout.
- **Windows PDF text layer:** rendering and in-viewer search are correct, but copying
  accented text out of a generated PDF can drop or mangle characters in Edge, Acrobat and
  Firefox's pdf.js. Chrome and Brave copy correctly. Do not rely on accented copy-paste from
  a converted PDF on Windows.
- **Hebrew Culmus typefaces** are replaced by shipped open-licence twins (David Libre,
  Frank Ruhl Hofshi, Miriam Libre, Liberation Mono, Alef). The text is drawn, not dropped,
  but widths differ by roughly 4–8 %, so lines can rebreak.

---

## Known issues in this build

1. Unsigned builds — Gatekeeper / SmartScreen friction as described above.
2. **Intel Macs cannot convert**: the engine pack is arm64 only. Install, signature and
   engine discovery pass; execution fails `Bad CPU type in executable`. An x86_64 pack is
   open (`t_31f9e38d`).
3. **The macOS wheel overlay is unmeasured** — hotkey, focus-free placement, click-through
   and dismiss rules are measured on Windows 11 only.
4. **Smart grab does nothing**: the toggle and its consent behaviour exist; the selection
   reader is not implemented.
5. **No resize, trim, bitrate, quality or codec controls** — encoder settings are fixed.
6. **Multi-sheet workbooks → csv/txt** are not fully wired: the engine writes one file per
   sheet (`<stem>-<sheet>.csv`), the app's publish step records one output per input
   (`evidence/c4-output-rules.md` §5).
7. **Recognised but not convertible:** bmp, svg, pdf as input, txt. **Not offered as
   outputs:** HEIC, SVG, PDF, legacy Office formats.
8. **GIF input is first-frame only**; GIF-to-GIF animation re-encoding is not offered.
9. **Cold-start image conversion is 113 s against a 120 s engine timeout** on a fresh
   Defender-scanned install. Warm conversions are fast; a slower disk could time out the
   first one.
10. **No auto-update** — upgrade by reinstalling.

Engineering follow-ups open at release: MSVC runtime inventory/notices rows (`t_34f3a93b`),
the x86_64 macOS engine pack (`t_31f9e38d`), the `ezc-pdf` sidecar (PDF as an image input is
not packaged), and the multi-sheet publish wiring. All are recorded on the board rather than
smoothed over here.

---

## Upgrading and uninstalling

- **Windows:** run the new installer over the old one; then *Settings → Apps → EZ Convert →
  Uninstall* to remove it. Settings live in `%APPDATA%\app.ezconvert\settings.json`.
- **macOS:** replace `EZ Convert.app` in `/Applications`; the engine pack stays installed.
  To remove everything, delete the app and
  `~/Library/Application Support/app.ezconvert.desktop`.

## Verifying an installation

```bash
# Windows
"%LOCALAPPDATA%\EZ Convert\ez-convert.exe" --diagnostics --fixtures <dir>

# macOS
"/Applications/EZ Convert.app/Contents/MacOS/ez-convert" --diagnostics --fixtures .
```

Five `engine … found` lines and `SUMMARY families=4 pass=4 fail=0` mean the install is
complete. The same binary the window uses resolves the engines the same way.

## Licences

Engines are separate-process sidecars: libvips (LGPL-2.1+), libheif/libde265 (LGPL-3.0),
ffmpeg/ffprobe (LGPL build, `--enable-gpl` count 0), LibreOffice (MPL-2.0), with notice
texts in the installed `licenses/` folder. Every shipped file is attributed in
`docs/shipped-inventory.tsv`; `docs/LICENSES.md` is generated from the locked dependency
graph. The app's own licence is a stakeholder decision, recorded as TBD.
