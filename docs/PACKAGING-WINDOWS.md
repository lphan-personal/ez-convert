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
explicitly by the packaging command:

| file | contents | read by |
|---|---|---|
| `tauri.windows.conf.json` | `bundle.targets: ["nsis"]`, `nsis.installMode: currentUser`, `webviewInstallMode`, `bundleVCRuntime` | every Windows build |
| `tauri.bundle.windows.conf.json` | `bundle.externalBin: [ffmpeg, ffprobe]`, `bundle.resources: {engines, licenses}` | the packaging command |

The split exists because `tauri-build` validates `externalBin`/`resources` paths at **cargo
build** time. With the payload in the platform overlay, the CI matrix — which never stages
engines — failed on every push with
`resource path binaries\ffmpeg-x86_64-pc-windows-msvc.exe doesn't exist`
(runs `41381217360`, `41381593048`). Keeping the platform overlay payload-free means the
matrix builds the app with no engine staging, while the packaging command opts into the
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

## The engine pack (built without a network-connected Windows host)

The Windows *test* host (`win11-dev`) has no outbound network — no TCP to
`1.1.1.1`/`8.8.8.8`/github, no DNS via any server — so it cannot fetch the engine
sources, and the installer it can produce carries the app shell only. The engines
are therefore built on a GitHub-hosted Windows runner by
`.github/workflows/windows-engine-pack.yml`, which runs
`scripts/win-engine-pack.sh` and publishes `windows-engine-pack-x86_64.zip`.

The zip is the same layout `EnginePaths::from_root` resolves (`src-tauri/src/engine/proc.rs`),
i.e. exactly what `adopt_pack()` accepts, and what `stage-engines.sh` consumes:

```
engines/vips/{vips.exe, vipsheader.exe, vipsthumbnail.exe, *.dll, licenses/}
engines/ffmpeg/{ffmpeg.exe, ffprobe.exe}
engines/soffice/program/soffice.com + share/ readmes/ presets/ LICENSE* NOTICE …
licenses/        notices + build record (configure output, MSI digest, strip manifest, gate output)
MANIFEST.sha256  every shipped file, hashed
README.txt       what the pack is and how to install it
```

| input | pinned by |
|---|---|
| libvips 8.18.7 "spec-aom" dist (43 files, the S2 build) | tarball sha256 in the script; every per-file digest in `docs/shipped-inventory.tsv` |
| BtbN FFmpeg-Builds LGPL, dated tag `autobuild-2026-11-01-12-49` | zip + `ffmpeg.exe` + `ffprobe.exe` digests (the two exe digests are the inventory rows) |
| LibreOffice 26.8.2 Win x86-64 MSI (TDF) | MSI sha256, measured by S3 |

The pack is the **installed** LibreOffice program directory, not an `msiexec /a`
extraction: the administrative install lays out 19,418 files / 1.60 GB — including
`Fonts/` (the copy the installer drops into `C:\Windows\Fonts`, which carries the
GPL-2.0 Culmus families the licence decision strips) and `System{,64}/` — while the
installed tree is 13,836 files / 734 MB and is the tree the licence gate was made
green against (`evidence/p2-windows-installer.md` §2).

Gates, all fail-closed, all in the job log:

1. **Pinned inputs** — every download is digest-checked before it is unpacked.
2. **Engines run from the pack** — `vips.exe --version` from the relocated tree,
   `vips-config.txt` shows `image quantisation … false` and `… with libheif: true`,
   `ffmpeg -buildconf` has no `--enable-gpl`/`--enable-nonfree`,
   `soffice.com --version` reports 26.8.2.
3. **Strip** — `gallery`, `help` and the five Culmus families are asserted gone
   independently of the script that removed them.
4. **Dependency closure** — the Windows counterpart of the macOS `otool` gate: every
   imported DLL in every shipped PE object must resolve to a file in the pack or to a
   Windows system DLL, and a separate banned-import audit refuses
   imagequant/poppler/magick/x265/x264/jbig.
5. **Licence gate** — `scripts/dist-license-gate.sh --check-inventory` plus `--scan`
   over each engine tree, from its own root, against `docs/shipped-inventory.tsv`.
6. **Smoke, judged by the product** — `png → jpg` (dimensions compared with the
   source), `heic → jpg` (libheif decode), `wav → mp3` (ffprobe says `mp3`),
   `mp4 → webm` (ffprobe says `vp9`, the app's own route: `libvpx-vp9` + `libopus`),
   `docx → pdf` (header, `%%EOF`, page object) and `docx → txt` carrying
   `MARKER-ALPHA` and `café` from the fixture. LibreOffice's exit code is never
   consulted — its *output* is.

To reproduce on any Windows machine with MSYS2:

```bash
bash scripts/win-engine-pack.sh --out dist          # ~10 min: 535 MB of downloads
```

To feed the pack into the installer instead of a bare installer build:

```bash
scripts/stage-engines.sh \
  --vips   <pack>/engines/vips \
  --ffmpeg <pack>/engines/ffmpeg \
  --lo-pack <pack>/engines/soffice \
  --out src-tauri
```

## Verifying that Tauri signs the sidecars

`bundle.windows.signCommand` (schema 2.12.5) is the hook: Tauri runs the command for each
file it signs, with `%1` replaced by the file path. Evidence that external binaries go
through it is recorded in `evidence/p2-windows-installer.md`, both by source inspection of
the pinned `tauri-bundler` and by a build whose sign command logs every path it is handed.

## Acceptance (what "done" means)

1. Stage the engines (`scripts/stage-engines.sh`, above), then build from the repo root with
   the payload overlay: `tauri build --target x86_64-pc-windows-msvc --config
   src-tauri/tauri.bundle.windows.conf.json` (or natively on Windows; the `--config` path is
   resolved against the working directory, so from `src-tauri/` it is
   `tauri.bundle.windows.conf.json`). Leaving the `--config` off produces a bundle with no
   engines — the installer would then report every family `MissingEngine`, which is why the
   acceptance run below is the gate, not the build.
2. Install on a Windows 11 machine with **no developer tools** and no internet.
3. Run `ez-convert.exe --diagnostics --fixtures <dir>` from the installed location. It
   resolves the engines the way the app does and converts one file per family (image,
   audio, video, document) through the same entry points the job worker uses, printing
   `FAMILY <name> PASS|FAIL <bytes> <ms>` and `SUMMARY families=4 pass=4 fail=0`.
4. Record the installer size, the per-family lines and the engine resolution lines in
   `evidence/p2-windows-installer.md`.
