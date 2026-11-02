# P2 — Windows NSIS per-user installer with bundled engines

Card: `t_466cc032`. Date: 2026-11-02. Build host: Debian 13 container (cross-compile with
`cargo-xwin` + LLVM 18) → the installer is bundled on the `win11-dev` VM.
Test host: `win11-dev`, Windows 11 Pro 10.0.26200, **no developer tools installed**.

Verdict: **DONE and measured.** The installer was produced, installed silently on a clean
Windows 11 VM and converted one file per family offline — see §5. The NSIS provisioning
blocker recorded in §4 is resolved and kept below as history.

**Result:** `EZ Convert_0.0.1_x64-setup.exe`, 512,304,426 B (488.57 MiB),
sha256 `5432C8AEFF26B7AF81B3CF3389ABED010E8438EDB2B1CB32D05AFFD9635C7BFF`.

## 1. Bundle configuration (done)

`src-tauri/tauri.windows.conf.json` — the platform overlay Tauri merges over
`tauri.conf.json` for a Windows target, so the macOS overlay is unaffected:

| key | value |
|---|---|
| `bundle.targets` | `["nsis"]` |
| `bundle.windows.nsis.installMode` | `currentUser` (no UAC) |
| `bundle.windows.webviewInstallMode` | `offlineInstaller`, `silent: true` |
| `bundle.windows.bundleVCRuntime` | `true` |
| `bundle.externalBin` | `binaries/ffmpeg`, `binaries/ffprobe` |
| `bundle.resources` | `resources/engines` → `engines`, `resources/licenses` → `licenses` |

Field names were taken from the published schema (`https://schema.tauri.app/config/2`,
`$id` …/config/2.12.5), not from memory: `bundleVCRuntime` (bool, default false),
`webviewInstallMode` (`offlineInstaller` + `silent`), `nsis.installMode`, `signCommand`
(`{cmd, args:["%1"]}`).

## 2. Engine staging (done, gate green)

`scripts/stage-engines.sh` assembled the bundle inputs from the shipping artifacts:

| input | files | bytes | check |
|---|---|---|---|
| vips dist (spec-aom final) | 43 | 35,824,560 | 43/43 identical to the S4 manifest; no `libjbig*` |
| ffmpeg.exe win64 | 1 | 136,180,736 | `-buildconf`: `--enable-gpl` count 0 |
| ffprobe.exe win64 | 1 | 135,966,208 | same |
| LibreOffice pack Windows 26.8.2.2 | 13,836 → **8,327** | 734,515,634 → **709,858,114** | stripped per `t_20fae747` |
| licences folder | 16 | — | every file a byte-identical copy of an attributed notice |

Strip result on the **Windows** pack (first time measured on this platform): `gallery`
175 files / 13,489,879 B, `help` 5,334 files / 11,167,641 B, `culmus` **0 files** — the
Windows pack does not carry the five GPL-2.0 CLM families at all, so that group removed
nothing; 5,509 files left the tree in total (`evidence/engine-pack-strip-manifest.tsv`).

Gate (`scripts/dist-license-gate.sh --scan`) over each shipped tree: vips dist root
43/43 attributed, 0 failures; sidecar projection 2/2, 0 failures; soffice pack root
8,327 attributed with **1 failure still open**:
`program/python-core-*/lib/libffi-8.dll` — the gate still resolves the bare
`libffi-8.dll` row from the vips dist first, so the LO build's pin is compared against
the wrong hash even though a path-specific row now exists. The matcher needs to prefer
the more specific pattern; that is a one-function change in `match_row`, not an
inventory gap. (The `update-settings.ini` failure this scan surfaced *was* an inventory
typo and is fixed: the re-run reports it clean.)
Three Windows-pack rows were missing and were added: `license.txt`, `update-settings.ini`,
and the pack's own `libffi-8.dll`.

## 3. Does Tauri sign `externalBin` sidecars on Windows? — YES (measured)

Method: the bundler was run with `bundle.windows.signCommand` pointing at a stub, and
`TAURI_WINDOWS_SIGNTOOL_PATH` at a stub `signtool.exe` (needed because the bundler calls
`verify` on sidecars before deciding to sign). `C:\p2\sign.log` after the run:

```
binaries\ffmpeg-x86_64-pc-windows-msvc.exe
binaries\ffprobe-x86_64-pc-windows-msvc.exe
binaries\ffmpeg-x86_64-pc-windows-msvc.exe
binaries\ffprobe-x86_64-pc-windows-msvc.exe
C:\p2\src-tauri\target\release\ez-convert.exe
```

So on Windows the bundler passes **both sidecars and the main binary** through the sign
command (the duplicate pair is the verify pass followed by the sign pass), with the
sidecar paths still carrying the `-<triple>` suffix. The bundler log confirms the wording:
`Signing C:\p2\src-tauri\target\release\ez-convert.exe with a custom signing command`.
Corroborating source (pinned `tauri-bundler` 2.7.1, `src/bundle/windows/sign.rs`):
`sign_command()` dispatches to `sign_command_custom` when `signCommand` is set, and
`verify()` short-circuits already-signed sidecars via `signtool verify /pa`.

Caveat stated plainly: this proves the *call*, not a signature — no code-signing
certificate was used. A real signing certificate must be attached on the release build.

## 4. Installer — NOT produced (the blocker)

What worked, in order: the Windows binary compiled cross-platform
(`cargo xwin build --release --target x86_64-pc-windows-msvc`, `ez-convert.exe`
4,702,720 B, `cargo` exit 0) and the bundler ran far enough to sign the sidecars and the
main binary. It then failed on NSIS provisioning:

```
Warn NSIS directory is missing some files. Recreating it.
Info Verifying NSIS package
Downloading https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.13/nsis-3.13.zip
failed to bundle project: `timeout: global`
```

Root cause, measured: the `win11-dev` VM reaches nodejs.org and static.rust-lang.org but
**cannot reach GitHub release assets** (`Invoke-WebRequest … /nsis-3.13.zip` →
*"Unable to connect to the remote server"*), so the CLI's own NSIS download times out.
Hand-placing the zip in `%LOCALAPPDATA%\tauri\NSIS` still trips the "missing some files"
check: the bundle also wants `Plugins/x86-unicode/additional/nsis_tauri_utils.dll`
(downloaded separately from `tauri-apps/nsis-tauri-utils` releases), which could not be
fetched from the VM in the remaining time.

## 5. Acceptance run — executed (clean Windows 11 VM, offline)

Full log: `evidence/p2-acceptance-clean-win11.txt`.

The VM's portable dev toolchain was moved out of the way (`C:\p2` → `p2_offdev`,
`C:\ezc` → `ezc_offdev`) and `where.exe` then returned *absent* for cargo, rustc, node,
npm, cl, link, makensis, windres, lld-link, python, gcc and git; no Visual Studio
directory exists. IPv4 gateway `10.0.0.1` was unreachable, so both the install and the
conversions ran with no network. (`C:\msys64` and `C:\build` could not be renamed — they
are locked and on no PATH; stated for precision.)

Installer `/S`: exit 0 in 112 s, no UAC prompt, payload in
`C:\Users\ryo\AppData\Local\EZ Convert` — `ez-convert.exe` 4,702,720 B, `ffmpeg.exe`
136,180,736 B and `ffprobe.exe` 135,966,208 B as externalBin siblings, `engines/vips`
(43 files) and `engines/soffice` (stripped pack), `licenses/`, seven VC-runtime DLLs and
`uninstall.exe`.

`ez-convert.exe --diagnostics --fixtures C:\anchor\fixtures --work C:\anchor\work2`:

```
engine vips found …\engines\vips\vips.exe
engine vipsheader found …\engines\vips\vipsheader.exe
engine ffmpeg found …\ffmpeg.exe
engine ffprobe found …\ffprobe.exe
engine soffice found …\engines\soffice\program\soffice.com
FAMILY image PASS 2781 113086 …\work2\diag-image.jpg
FAMILY audio PASS 8579 51729 …\work2\diag-audio.mp3
FAMILY video PASS 2206 282 …\work2\diag-video.webm
FAMILY document PASS 46489 8296 …\work2\diag-document.pdf
SUMMARY families=4 pass=4 fail=0            (198 s, cold cache)
```

A second run on the warm cache is 4/4 in 3 s.

**Defect the run caught (fixed, `31e3a1f`):** `EnginePaths::from_root` routed every
program through `exe()`, which appends `.exe` on Windows, so the LibreOffice console
launcher resolved as `soffice.com.exe`. Installer v1 therefore reported the document
family `MissingEngine`; v2, rebuilt with `soffice_dir.join("soffice.com")`, passes.

**Flagged, not fixed:** the *cold* image conversion took 113 s against the 120 s engine
timeout (manual `vips copy` cold = 72 s, warm = 0 s — Defender scanning the freshly
unpacked vips tree). It passes, but the margin is ~6 % and a slower disk would fail the
first image conversion after install.

## 6. Engine path resolution at runtime (code, unit-level)

`EnginePaths::from_root` now accepts the layout the installer actually writes: ffmpeg and
ffprobe as `externalBin` siblings of the app executable, falling back only when
`engines/ffmpeg/` is absent, so a staged pack still wins. Without this the installed app
would have looked for `<install>\engines\ffmpeg\ffmpeg.exe` and reported the audio/video
families as `MissingEngine`.

## 7. Gaps

- The installer ships **unsigned**: the externalBin signing *path* is proven (§3), but no
  code-signing certificate was used, so a release build must attach a real identity.
- The installer now carries seven Microsoft VC-runtime DLLs (v14.44.35211). They have no
  `docs/shipped-inventory.tsv` row and no `THIRD-PARTY-NOTICES.md` entry yet — required
  before release (policy R6), recorded here rather than assumed.
- `scripts/dist-license-gate.sh --scan` over the soffice pack still reports one failure:
  `program/python-core-*/lib/libffi-8.dll` resolves the bare `libffi-8.dll` row from the
  vips dist first; `match_row` must prefer the more specific pattern.
- `pdfium` is not staged: the first-party `ezc-pdf` sidecar (`ARCHITECTURE.md` §17.2) does
  not exist yet, so there is no binary to package and the PDF routes stay unshipped. The
  staging script accepts `--pdfium` and the gate will reject it until its inventory row
  carries a sha256 pin.
- Tauri signs the sidecars through `signCommand`, but no certificate was used, so the
  installer that ships must be built on a machine with a real signing identity.
