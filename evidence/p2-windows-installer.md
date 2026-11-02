# P2 — Windows NSIS per-user installer with bundled engines

Card: `t_466cc032`. Date: 2026-11-02. Build host: Debian 13 container (cross-compile with
`cargo-xwin` + LLVM 18) → the installer is bundled on the `win11-dev` VM.
Test host: `win11-dev`, Windows 11 Pro 10.0.26200, **no developer tools installed**.

Verdict: **config, staging, engine paths and the sidecar-signing question are DONE and
measured. The installer itself was NOT produced in this run** — the bundler's NSIS
toolchain could not be provisioned on the VM (see §4). The acceptance run in §5 therefore
did not happen, and the card is handed back blocked on that single step.

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

## 5. Acceptance run — not executed

`ez-convert.exe --diagnostics --fixtures C:\p2 --work C:\p2\work` was prepared (the four
fixtures `sample.png/.wav/.mp4/.docx` are on the VM) but needs an installed build. Nothing
is claimed here that was not run.

## 6. Engine path resolution at runtime (code, unit-level)

`EnginePaths::from_root` now accepts the layout the installer actually writes: ffmpeg and
ffprobe as `externalBin` siblings of the app executable, falling back only when
`engines/ffmpeg/` is absent, so a staged pack still wins. Without this the installed app
would have looked for `<install>\engines\ffmpeg\ffmpeg.exe` and reported the audio/video
families as `MissingEngine`.

## 7. Gaps

- The NSIS installer was not built; §4 is the exact failure and the fix is network access
  to GitHub release assets from the build host (or pre-placing `nsis_tauri_utils.dll`).
- `pdfium` is not staged: the first-party `ezc-pdf` sidecar (`ARCHITECTURE.md` §17.2) does
  not exist yet, so there is no binary to package and the PDF routes stay unshipped. The
  staging script accepts `--pdfium` and the gate will reject it until its inventory row
  carries a sha256 pin.
- Tauri signs the sidecars through `signCommand`, but no certificate was used, so the
  installer that ships must be built on a machine with a real signing identity.
