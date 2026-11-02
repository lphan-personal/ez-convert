# Windows packaging (P2) — NSIS per-user installer with bundled engines

Status: bundle configuration and engine staging land here; the installer itself is
produced by `tauri build` on a Windows build host and its acceptance run is recorded in
`evidence/p2-windows-installer.md`.

## What the installer is

| setting | value | why |
|---|---|---|
| target | `nsis` | one `.exe`, no MSI/WiX toolchain needed |
| `bundle.windows.nsis.installMode` | `currentUser` | installs to `%LOCALAPPDATA%\EZ Convert\`, **no UAC prompt** (`SPEC.md` §7) |
| `bundle.windows.webviewInstallMode` | `offlineInstaller` (+127 MB) | the install must work on a machine with no internet; the bootstrapper modes need one |
| `bundle.windows.bundleVCRuntime` | `true` | the WebView2 loader and the sidecars must run without a separately installed VC++ redistributable |
| `bundle.targets` | `["nsis"]` | `"all"` on Windows also builds WiX/MSI, which is not a supported install path here |
| `bundle.externalBin` | `binaries/ffmpeg`, `binaries/ffprobe` | single-file executables; the bundler signs them and strips the `-<triple>` suffix |
| `bundle.resources` | `resources/engines` → `engines/`, `resources/licenses` → `licenses/` | multi-file trees (vips needs its DLL siblings; LibreOffice needs its whole tree) |

These live in `src-tauri/tauri.windows.conf.json`, the platform overlay Tauri merges on
top of `tauri.conf.json` for a Windows target, so the base config stays platform-neutral
and the macOS overlay (`tauri.macos.conf.json`) is not affected.

## Install layout produced

```
%LOCALAPPDATA%\EZ Convert\            (per-user, no elevation)
├── ez-convert.exe
├── ffmpeg.exe                        externalBin  (LGPL-2.1 build; licence gate)
├── ffprobe.exe                       externalBin
├── engines\
│   ├── vips\                         vips.exe + vipsheader.exe + DLL/module closure
│   └── soffice\program\              LibreOffice tree (stripped pack, see below)
└── licenses\                         THIRD-PARTY-NOTICES.md + per-engine texts
```

**Why `engines/` and not more `externalBin`s:** Tauri strips the triple suffix and drops
sidecars next to the app exe, which loses the directory relationship `vips.exe` needs for
its 32 DLL siblings and would scatter ~8,500 LibreOffice files into the install root.
vips is therefore launched by absolute path with its CWD set to its own directory
(`engine/proc.rs`), the same rule as `ARCHITECTURE.md` §18.

## Two config files: app vs. payload

`src-tauri/tauri.windows.conf.json` is the **platform** overlay Tauri applies to every
Windows build — the CI matrix's `cargo build` included. It carries the NSIS settings and
nothing that must exist on disk.

The engine payload lives in `src-tauri/tauri.bundle.windows.conf.json` and is passed
explicitly, only by the packaging command:

| file | contents | read by |
|---|---|---|
| `tauri.windows.conf.json` | `bundle.targets: ["nsis"]`, `nsis.installMode: currentUser`, `webviewInstallMode`, `bundleVCRuntime` | every Windows build |
| `tauri.bundle.windows.conf.json` | `bundle.externalBin: [ffmpeg, ffprobe]`, `bundle.resources: {engines, licenses}` | the packaging command only |

The split exists because `tauri-build` validates `externalBin`/`resources` paths at **cargo
build** time. With the payload in the platform overlay, the CI matrix — which never stages
engines — failed on every push with
`resource path binaries\ffmpeg-x86_64-pc-windows-msvc.exe doesn't exist`
(runs `41381217360`, `41381593048`). Keeping the platform overlay payload-free means the
matrix builds the app with no engine staging, and only the packaging command opts into the
payload. Staging stubs in CI would have made the matrix green while destroying the one thing
it checks — that the declared payload paths are real.

## Engine staging

`scripts/stage-engines.sh` assembles the bundle inputs. It is the only supported way to
produce `src-tauri/binaries/` and `src-tauri/resources/engines/`; it refuses to finish on
a licence-gate failure.

```bash
scripts/stage-engines.sh \
  --vips   <vips dist dir | spec-aom-dist.tar.gz> \
  --ffmpeg <dir holding ffmpeg.exe + ffprobe.exe> \
  --lo-pack <LibreOffice Windows pack root> \
  --out    src-tauri
```

What it does, in order:

1. **vips** — extract/copy the 43-file dist, assert 43 files and the absence of
   `libjbig-0.dll` (the historical GPL contaminant), and keep its own `licenses/`.
2. **ffmpeg/ffprobe** — copy to `binaries/ffmpeg-x86_64-pc-windows-msvc.exe` and
   `binaries/ffprobe-x86_64-pc-windows-msvc.exe` (the triple suffix is what Tauri
   requires for an `externalBin` entry) and assert `--enable-gpl` count = 0 by running
   the binary's own `-buildconf`.
3. **LibreOffice** — copy the pack, then **strip it** with
   `scripts/strip-engine-pack.py` (5 Culmus GPL-2.0 font families, `share/gallery/**`,
   `help/**`; decision `t_20fae747`, commit `6adcb35`). Packaging the unstripped pack is
   a licence defect, and the gate below fails on it by design.
4. **licences** — `docs/licenses/THIRD-PARTY-NOTICES.md`, `docs/LICENSES.md` and each
   engine's own licence files into `resources/licenses/`.
5. **gate** — `scripts/dist-license-gate.sh --scan` over `resources/engines` and over
   `binaries`; every shipped file must attribute to a `docs/shipped-inventory.tsv` row
   (fail-closed, policy R6). Then writes `evidence/p2-stage-manifest.tsv`
   (`sha256<TAB>bytes<TAB>path`) and prints the size report.

`pdfium` is **not** staged. The PDF route ships through the first-party `ezc-pdf` sidecar
(`ARCHITECTURE.md` §17.2), which does not exist yet; its inventory row is `planned`.
`stage-engines.sh --pdfium <exe>` exists so the binary can be added the day that sidecar
lands, and the gate rejects it until the sha256 row is in the inventory.

## Verifying that Tauri signs the sidecars

`bundle.windows.signCommand` (schema 2.12.5) is the hook: Tauri runs the command for each
file it signs, with `%1` replaced by the file path. Evidence that external binaries go
through it is recorded in `evidence/p2-windows-installer.md`, both by source inspection of
the pinned `tauri-bundler` and by a build whose sign command logs every path it is handed.

## Acceptance (what "done" means)

1. Stage the engines (`scripts/stage-engines.sh`, above), then build with the payload
   overlay: `tauri build --target x86_64-pc-windows-msvc --config
   src-tauri/tauri.bundle.windows.conf.json` (or natively on Windows). Leaving the
   `--config` off produces a bundle with no engines — the installer would then report every
   family `MissingEngine`, which is why the acceptance run below is the gate, not the build.
2. Install on a Windows 11 machine with **no developer tools** and no internet.
3. Run `ez-convert.exe --diagnostics --fixtures <dir>` from the installed location. It
   resolves the engines the way the app does and converts one file per family (image,
   audio, video, document) through the same entry points the job worker uses, printing
   `FAMILY <name> PASS|FAIL <bytes> <ms>` and `SUMMARY families=4 pass=4 fail=0`.
4. Record the installer size, the per-family lines and the engine resolution lines in
   `evidence/p2-windows-installer.md`.
