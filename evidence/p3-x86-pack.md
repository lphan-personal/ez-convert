# x86_64 engine pack — build, signature, execution proof (2026-11-02)

Intel Macs could install the app, verify its signature, resolve 5/5 engines and
launch the window, but converted **0/4** with `Bad CPU type in executable
(os error 86)` because the only pack was arm64 (`evidence/p3-macos-validation.md`).
This is the measurement that the second pack exists, is signed, and executes its
three engines on Intel hardware.

- **Pack build**: run **41394172979** (`macos-dmg`, head `f20bba08`) — the `pack`
  job is a two-architecture matrix, `macos-latest` (arm64) and `macos-15-intel`
  (x86_64). All jobs green: `app`, `pack (arm64)`, `pack (x86_64)`,
  `install-test`, `evidence`.
- **Acceptance**: run **41397186245** (`macos-validate`, `source_run=41394172979`,
  `intel_label=macos-15-intel`). All jobs green: `arm64`, `intel`, `report`.
- **Artifacts**: `macos-engine-pack-x86_64` (374,217,541 B zip) and
  `macos-engine-pack-x86_64`'s DMG, 380,510,014 B, sha256
  `4941922215d89362dfae12d43ed74fb14440b67ae9973d8f43491866cb9b5d7a` —
  recomputed here from the downloaded artifact and equal to both
  `EZ-Convert-Engine-Pack-x86_64.dmg.sha256` and the `artifact.sha256` field of
  `EZ-Convert-Engine-Pack-x86_64.dmg.manifest.json`
  (`pack.id = ezconvert-engine-pack-macos-x86_64`, `arch = x86_64`,
  `version = 1.0.0`).
- **Evidence channel**: branch `macos-status`, `macos-status/run-41397186245/`
  (`validate-arm64/`, `validate-intel/`, `facts.json`), screenshots included.

## The pack itself (build log, x86_64 leg)

```
=== 1/8 LibreOffice 26.8.2 (x86_64)
    symlinks preserved: 23 in the bundle
=== 3/8 libvips (x86_64) — Homebrew bottle, relocated to the pack
    vips 8.18.5 from /usr/local/opt/vips
=== 4/8 ffmpeg 8.1.1 (x86_64) — built here, --disable-gpl
manifest: pack ezconvert-engine-pack-macos-x86_64 version 1.0.0 (9305 files, 24 symlinks)
verify: OK ezconvert-engine-pack-macos-x86_64 version 1.0.0 arch x86_64
=== 8b/8 verify the artifact as a user receives it
PASS  artifact digest matches EZ-Convert-Engine-Pack-x86_64.dmg.sha256
PASS  artifact digest matches the sidecar manifest
PASS  sidecar version=1.0.0 arch=x86_64
PASS  verify: 9304 files hashed, MANIFEST.json covered by the list
PASS  verify: 24 symlinks intact
PASS  24 symlinks present after unpack
PASS  engines/soffice/program is a symlink -> LibreOffice.app/Contents/MacOS
PASS  339 Mach-O objects verify (ad-hoc v1 signatures)
PASS  339 Mach-O objects reference only system or in-pack libraries
PASS  pack ffmpeg runs: ffmpeg version 8.1.1 Copyright (c) 2000-2026 the FFmpeg developers
PASS  shipped ffmpeg is not GPL
PASS  pack soffice runs: LibreOffice 26.8.2.2 9dea406ab7695df6325b11b0e19fdca8db4b7276
verify-engine-pack: PASS
engine pack: OK arch=x86_64 dmg=dist/EZ-Convert-Engine-Pack-x86_64.dmg bytes=380510014
```

`vips --version`'s first line on this runner is a glib duplicate-class warning
(the runner also has Homebrew glib installed), which is what the verifier's
`PASS pack vips runs: …` line quotes; the check asserts exit status, and the
warning does not appear on a Mac without Homebrew. The arm64 pack built in the
same run reports the same shape (9,305 files, 24 symlinks, 339 Mach-O,
362,254,999 B).

## Intel acceptance (installed from the published DMG)

```
host arch: x86_64
sw_vers 15.7.9            (build 24G830)
app binary lipo: x86_64 arm64
engine vips/vips                                      Mach-O 64-bit executable x86_64
engine ffmpeg/ffmpeg                                  Mach-O 64-bit executable x86_64
engine soffice/LibreOffice.app/Contents/MacOS/soffice Mach-O 64-bit executable x86_64
selected engine pack: EZ-Convert-Engine-Pack-x86_64.dmg (host x86_64)
architecture: pack x86_64, this Mac x86_64 — OK
PASS  339 Mach-O objects verify (ad-hoc v1 signatures)
verify-engine-pack: PASS
installed symlinks: 24  (manifest declares 24)
dangling symlinks: 0
Contents/Resources carries icon.icns only: OK
engine vips found …/engines/vips/vips          (5 of 5 resolved: vips, vipsheader, ffmpeg, ffprobe, soffice)
SUMMARY families=4 pass=4 fail=0
INTEL_CONVERSION=pass (4/4)
VALIDATION intel install=pass launch=pass engine-discovery=pass conversion=pass
```

## arm64 is unchanged (same run)

```
selected engine pack: EZ-Convert-Engine-Pack-arm64.dmg (host arm64)
architecture: pack arm64, this Mac arm64 — OK
SUMMARY families=4 pass=4 fail=0
ROSETTA_CONVERSION=pass (4/4)
VALIDATION arm64 install=pass launch=pass engine-discovery=pass conversion=pass (4/4)
```

`install-test` in the build run (arm64, from the fresh DMGs) also reports
`SUMMARY families=4 pass=4 fail=0`, so the matrix change did not disturb the
packaging acceptance path.

## What had to be fixed for this to be measurable

1. `scripts/mac-engine-pack.sh --arch x86_64` had never run and pointed at
   `…/mac/x86-64/…`; TDF serves the directory as `mac/x86_64/` and the file as
   `…_MacOS_x86-64.dmg`. The old mapping 404'd on the directory.
2. ffmpeg's x86 assembly needs `nasm`; without it `configure` stops with
   "nasm/yasm not found or too old". Installed on the x86_64 leg.
3. `upload-artifact@v4` refuses one artifact name twice in a run, so the packs
   upload as `macos-engine-pack-arm64` / `macos-engine-pack-x86_64`.
4. `install-from-dmgs.sh` picked a pack with `find … | head -1` — filesystem
   order, i.e. a coin flip with two packs. It now selects
   `EZ-Convert-Engine-Pack-$(uname -m).dmg`; the installer inside the image
   re-checks `MANIFEST.json`'s `arch` against `uname -m` and refuses a mismatch
   before verifying or copying anything (`scripts/test-install-engine-pack-arch.sh`
   covers both paths and gates both macOS workflows).
