# P2 — Windows NSIS per-user installer with bundled engines

Card: `t_466cc032`. Date: 2026-11-02. Build host: Debian 13 container (cross-compile,
`cargo-xwin` + LLVM 18) → bundle on the `win11-dev` VM. Test host: `win11-dev`
(Windows 11 Pro 10.0.26200, no developer tools installed).

## 1. Bundle configuration

`src-tauri/tauri.windows.conf.json` (merged over `tauri.conf.json` for Windows targets):

| key | value |
|---|---|
| `bundle.targets` | `["nsis"]` |
| `bundle.windows.nsis.installMode` | `currentUser` (no UAC) |
| `bundle.windows.webviewInstallMode` | `offlineInstaller`, `silent: true` |
| `bundle.windows.bundleVCRuntime` | `true` |
| `bundle.externalBin` | `binaries/ffmpeg`, `binaries/ffprobe` |
| `bundle.resources` | `resources/engines` → `engines`, `resources/licenses` → `licenses` |

Field names were checked against the published schema `https://schema.tauri.app/config/2`
(`$id .../config/2.12.5`), not against memory: `bundleVCRuntime` (bool, default false),
`webviewInstallMode` (`offlineInstaller` + `silent`), `nsis.installMode` (`currentUser` is
already the default but is stated explicitly), `signCommand` (`{cmd, args:["%1"]}`).

## 2. Engine staging

`scripts/stage-engines.sh` produced the bundle inputs from the shipping artifacts:

| input | files | bytes | check |
|---|---|---|---|
| vips dist (spec-aom final) | 43 | 35,824,560 | 43/43 as the S4 manifest; no `libjbig*` |
| ffmpeg.exe win64 | 1 | 136,180,736 | `--enable-gpl` count 0 |
| ffprobe.exe win64 | 1 | 135,966,208 | `--enable-gpl` count 0 |
| LibreOffice pack (Windows 26.8.2.2) | 13,836 → **8,327** after strip | 734,515,634 → **709,858,114** | strip decision `t_20fae747` |
| licences folder | 16 | — | every file is a byte-identical copy of an attributed notice |

Strip result on the Windows pack (measured here, first time on this platform):
`gallery` 175 files / 13,489,879 B, `help` 5,334 files / 11,167,641 B,
`culmus` **0 files** — the Windows pack does not carry the five GPL-2.0 CLM families, so
there was nothing to remove for that group; the manifest records the 5,509 removed files.

Gate: `scripts/dist-license-gate.sh --scan` run over each shipped tree
(vips dist root, soffice pack root, and the sidecar projection named as the installer
writes them) — results in §6.

## 3. Tauri signs externalBin sidecars on Windows

Method: the bundler was run with `bundle.windows.signCommand` pointed at a stub that
appends each path it is asked to sign to `C:\p2\sign.log` and exits 0. What the log
contained is recorded below — that is the empirical answer, not a reading of the docs.

RESULT_PLACEHOLDER

## 4. Installer

RESULT_PLACEHOLDER

## 5. Acceptance run on a clean Windows 11 machine

RESULT_PLACEHOLDER

## 6. Licence gate over the shipped trees

RESULT_PLACEHOLDER

## 7. Gaps

RESULT_PLACEHOLDER
