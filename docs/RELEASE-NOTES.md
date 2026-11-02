# Release notes — EZ Convert v1 (0.0.1)

Released 2026-11-02 as the internal GitHub release **v1.0.0**
(<https://github.com/lphan-personal/ez-convert/releases/tag/v1.0.0>) — a download link only,
no GitHub login needed. Not notarised, no Developer ID, no Authenticode, not a store package;
see `docs/DISTRIBUTION.md`. The binaries inside self-report version `0.0.1` (`EZC_VERSION`)
while the tag says `v1.0.0`, which is why the file names carry `0.0.1`.

---

## Unreleased — the next build carries the wheel overlay

v1.0.0 shipped **without** a working overlay window: `summon()` looked up a window labelled
`wheel` that nothing ever created, so the hotkey produced nothing. That gap was listed as a
known issue in the v1.0.0 notes and is deleted now that the window and its measurement exist
(`docs/OVERLAY.md`):

- The wheel is a real, transparent, non-activating, always-on-top window built at runtime;
  the square corners outside the disc are click-through (an elliptic window region on
  Windows, an `ignoresMouseEvents` watcher on macOS), and summoning it never takes the key
  window from the app the user is working in.
- Measured on a real desktop by the app itself — the same summon path the hotkey, the tray
  item and the `wheel_summon` command call — with the screen photographed and the ring's
  rectangle checked against a blank-capture control (`evidence/overlay/README.md`).
  `overlay-measure` on `main` ([41494299571](https://github.com/lphan-personal/ez-convert/actions/runs/41494299571)),
  both jobs green, 15/15 asserted checks each:

| platform | summon → visible | first summon after a fresh launch | screenshot |
|---|---|---|---|
| macOS 15 arm64, 1024×768, scale 1 | 39 ms | 2427 ms | ring visible in the crop |
| Windows Server 2025 x64, 1024×768 | 28 ms | 38 ms | ring visible in the crop |

- **The first summon on macOS can take ~2.4 s** — measured twice (2427 ms from the build tree,
  2321 ms from the installed app). The asserted budget covers the summon path (39–55 ms); the
  cold figure is the first webview construction after launch. It is reported here rather than
  omitted: the cold path is the one a user actually meets first.
- **Measured from the installed app, not from a build tree.** `macos-validate`
  ([41499554761](https://github.com/lphan-personal/ez-convert/actions/runs/41499554761))
  installs the DMG built from this state (`EZ-Convert-0.0.1-macos-universal.dmg`, 6,369,880 B,
  sha256 `7972d9bf9bfbc8b87f80e12caa59bc26a603a71c62530486d1556433febdada0`) and runs
  `--overlay-probe` against `/Applications/EZ Convert.app/Contents/MacOS/ez-convert`; the job
  fails unless the probe reports `ok=true`. Installed result: **55 ms warm, 2321 ms cold, 15/15
  checks**, `VALIDATION overlay installed-binary=true`, alongside arm64 4/4 and Rosetta 4/4
  conversions and a native Intel 4/4 leg.
- **The same is true on Windows.** `windows-installer`
  ([41506163113](https://github.com/lphan-personal/ez-convert/actions/runs/41506163113))
  builds the NSIS installer from this state (`EZ Convert_0.0.1_x64-setup.exe`, 219,983,879 B,
  sha256 `3b6fafb8dfe01b5b61fce66d17860e9c671d7c2430a3e022dbb009e3a21baff1`), installs it
  silently and per-user into `%LOCALAPPDATA%\EZ Convert\` with no elevation, and runs
  `--overlay-probe` against the installed `ez-convert.exe`; the job fails unless the probe
  reports `ok=true`, 15/15. Installed result: **33 ms warm, 75 ms cold, 15/15 checks**, with
  the wheel photographed over the installed app (`evidence/overlay/README.md`). The installer
  digest is per-build: a release rebuilds it and publishes that build's bytes.
- **What is not yet true:** neither the DMG above nor an NSIS installer is attached to a
  published release yet. Publishing the release is the next step.

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

| platform | artifact | published size and sha256 |
|---|---|---|
| macOS 11+, arm64 + x86_64 | `EZ-Convert-0.0.1-macos-universal.dmg` — universal app, ad-hoc signed | 6,299,468 B (6.0 MiB), sha256 `adbe945dcd1b8eb6847a921feaaf2516f2b442be0245756812ce7189bc132a54` |
| macOS, Apple Silicon | `EZ-Convert-Engine-Pack-arm64.dmg` — engine pack, separate install | 361,154,840 B (344.4 MiB), sha256 `26ca81b521ba8158429988000e1cb40510bb591fbfcab5334e6e3dc5cfb44067` |
| macOS, Intel | `EZ-Convert-Engine-Pack-x86_64.dmg` — engine pack, separate install | 380,435,751 B (362.8 MiB), sha256 `cbbcd97731b13d6673d7d7c16573fa66d36ab71da84339cd4859a10f7fa3dfb2` |
| macOS, either | `EZ-Convert-Engine-Pack-{arm64,x86_64}.dmg.manifest.json`, `SHA256SUMS.txt` | pack metadata; sums of every file above |
| Windows 11 x64 — measured internal build, **not in the v1.0.0 release** | `EZ Convert_0.0.1_x64-setup.exe`, per-user NSIS, no UAC | 512,319,647 B (488.59 MiB), sha256 `d49a012ae521f73a24485e69a6cacdf88f2c7cb96464687c9bf9ce7c97c3ed5d` |
| Windows 11 x64 — first bundle of the same build, **not in the v1.0.0 release** | `EZ Convert_0.0.1_x64-setup.exe` (kept because it is the one with the clean-machine transcript) | 512,304,426 B (488.57 MiB), sha256 `5432C8AEFF26B7AF81B3CF3389ABED010E8438EDB2B1CB32D05AFFD9635C7BFF` |

v1.0.0 publishes the **macOS** deliverables. The Windows rows are the measured internal build
(`docs/PACKAGING-WINDOWS.md`, `evidence/p2-windows-installer.md`); the release page states that
the Windows installer and engine pack follow in a separate release, because an installer
without its engine pack would install an app that cannot convert anything.

Install **one** engine pack, the one that matches your Mac: `uname -m` prints `arm64` on
Apple Silicon and `x86_64` on Intel, each pack declares its `arch` in `MANIFEST.json`, and
`Install Engine Pack.command` refuses a mismatched pack before verifying or copying anything
(`docs/MACOS-PACKAGING.md` §1). The app bundle contains no engines by design — Tauri's macOS
bundler cannot sign them there.

Verify the download before installing — `shasum -a 256 -c SHA256SUMS.txt` reports OK for
every line.

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
| macOS 15.7.9, Intel x86_64 native | install from the published DMGs, `codesign --verify --strict`, 5/5 engines resolved, one conversion per family | 4/4 pass, `SUMMARY families=4 pass=4 fail=0` (macos-validate run [41427529926](https://github.com/lphan-personal/ez-convert/actions/runs/41427529926); `evidence/p3-x86-pack.md`) |
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
2. **Smart grab does nothing**: the toggle and its consent behaviour exist; the selection
   reader is not implemented.
3. **No resize, trim, bitrate, quality or codec controls** — encoder settings are fixed.
4. **Multi-sheet workbooks → csv/txt** are not fully wired: the engine writes one file per
   sheet (`<stem>-<sheet>.csv`), the app's publish step records one output per input
   (`evidence/c4-output-rules.md` §5).
5. **Recognised but not convertible:** bmp, svg, pdf as input, txt. **Not offered as
   outputs:** HEIC, SVG, PDF, legacy Office formats.
6. **GIF input is first-frame only**; GIF-to-GIF animation re-encoding is not offered.
7. **Cold-start image conversion is 113 s against a 120 s engine timeout** on a fresh
   Defender-scanned install. Warm conversions are fast; a slower disk could time out the
   first one.
8. **No auto-update** — upgrade by reinstalling.

Engineering follow-ups open at release: the Microsoft VC++ runtime inventory and notice rows
for the DLLs the Windows installer now carries, the `ezc-pdf` sidecar (PDF as an image input
is not packaged), and the multi-sheet publish wiring. They are recorded on the project board
rather than smoothed over here.

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
