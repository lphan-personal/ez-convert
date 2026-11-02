# macOS packaging — universal DMG, engine pack, and the signing order

Status: v1 is **unsigned/ad-hoc**. Everything below was built to run on a GitHub
Actions macOS runner (`.github/workflows/macos-dmg.yml`); the Developer ID path
is documented in §4 and is a configuration change, not a redesign.

## 1. What ships

| Artifact | Contents | Size (runner-measured) |
|---|---|---|
| `EZ-Convert-0.0.1-macos-universal.dmg` | `EZ Convert.app`, universal (arm64 + x86_64) | ~12 MB |
| `EZ-Convert-Engine-Pack-arm64.dmg` | `engines/vips`, `engines/ffmpeg`, `engines/soffice` + licences + `MANIFEST.sha256` + `MANIFEST.json`, built for Apple Silicon | 362 MB |
| `EZ-Convert-Engine-Pack-arm64.dmg.sha256` | digest of the published DMG | 1 line |
| `EZ-Convert-Engine-Pack-arm64.dmg.manifest.json` | pack id/version/arch, artifact digest, `MANIFEST.json` digest | 1 file |
| `EZ-Convert-Engine-Pack-x86_64.dmg` | the same payload built for Intel Macs: LibreOffice `x86-64`, the `/usr/local` Homebrew bottle of libvips, ffmpeg configured for x86_64 | 380 MB |
| `EZ-Convert-Engine-Pack-x86_64.dmg.sha256` | digest of the published DMG | 1 line |
| `EZ-Convert-Engine-Pack-x86_64.dmg.manifest.json` | pack id/version/arch, artifact digest, `MANIFEST.json` digest | 1 file |

**One pack per architecture; they are not interchangeable.** Each pack carries
`arch` in its `MANIFEST.json` (`arm64` or `x86_64`), and
`packaging/macos/install-engine-pack.command` compares it with `uname -m` and
**refuses** to install a mismatched pack, before it copies anything:

```
REFUSED: this engine pack is for arm64 Macs; this Mac is x86_64.
         Download EZ-Convert-Engine-Pack-x86_64.dmg and run that one instead.
```

Without that check the arm64 pack installs happily on Intel and the app then
fails every family with `Bad CPU type in executable (os error 86)` — a wrong
download read as an app bug (§7).

The app bundle contains **no engines**. On macOS every engine ships in the pack,
for three reasons that are all properties of Tauri's macOS bundler:

1. It signs `Contents/MacOS`, `Frameworks`, `Plugins`, `Helpers`, `XPCServices`
   and `Libraries` — **never `Contents/Resources`**. An engine placed in
   `Resources` is unsigned, and an unsigned nested executable in a signed bundle
   fails `codesign --verify --deep --strict`.
2. `copy_resources` copies file-by-file, which flattens the `.app` symlinks and
   extended attributes a LibreOffice or libvips tree depends on.
3. Re-signing third-party code inside the app bundle means re-signing it on every
   app release. A separate pack is versioned, signed and updated on its own.

The Windows installer keeps the engines inside the installer (P2) — that is a
different platform with a different bundler, and the asymmetry is deliberate
(`/opt/data/converter-packaging-strategy.md` §3.4).

## 2. Layout the app resolves

`engine::proc::EnginePaths::from_root(<resource dir>)` looks for:

```
engines/vips/{vips,vipsheader,lib/*.dylib,vips-modules-*}
engines/ffmpeg/{ffmpeg,ffprobe,lib/*.dylib}
engines/soffice/LibreOffice.app/Contents/MacOS/soffice   (real bundle binary)
engines/soffice/program -> LibreOffice.app/Contents/MacOS (compatibility link)
```

The pack root is the directory **containing** `engines/`:

```
~/Library/Application Support/app.ezconvert.desktop/engines/...
```

Resolution order, `EnginePaths::discover` then `adopt_pack`:

1. bundle resources (`<App>.app/Contents/Resources/engines/...`) — empty on macOS;
2. `EZC_ENGINE_PACK` (absolute path) — test/CI override, same contract as
   `EZC_VIPS_DIR`/`EZC_FFMPEG_DIR`/`EZC_SOFFICE_DIR`;
3. `app_data_dir()` — `~/Library/Application Support/app.ezconvert.desktop`.

A pack never overrides an engine the bundle already provides, so the same
binary resolves the Windows layout and the macOS layout without a platform
branch in `engine/`.

**LibreOffice ships as the `.app` TDF builds.** `Contents/MacOS` holds the
launchers, `Contents/Resources` the data and `Contents/Frameworks` the libraries;
the bootstrap resolves both relative to the executable *inside* the bundle. A
flattened copy (program/ = `Contents/MacOS`, Resources content at the pack root,
`Resources -> .`) starts, converts nothing, and aborts with a UNO
`DeploymentException` (`Abort trap: 6`, empty log — CI run 41384142116), so the
bundle is copied with `ditto` and never unpacked. `engines/soffice/program` stays
a symlink to `LibreOffice.app/Contents/MacOS` so the flat `program/soffice` path
keeps resolving for tooling, and `engine::proc::from_root` prefers the real
bundle path on macOS. The strip step therefore runs against
`LibreOffice.app/Contents/Resources` (`scripts/strip-engine-pack.py --program`
relaxes its layout guard) and the licence decision is asserted on the bundle's own
paths (`Resources/share/gallery`, `Resources/gallery`, `Resources/help`, the five
GPL Culmus families).

## 2.1 The pack manifest (version + checksum)

Two files travel inside the pack, at its root next to `engines/`:

| File | Answers |
|---|---|
| `MANIFEST.sha256` | *are these the bytes that were built?* — one `sha256  ./path` line per file, `shasum -a 256 -c`-compatible |
| `MANIFEST.json` | *which pack is this?* — id, pack version, arch, install root, per-engine version + entry point + file count, source digests (LibreOffice image, ffmpeg buildconf), and the **symlink inventory** |

They cover each other. The order in `scripts/pack-manifest.py seal` is what keeps
that from being circular:

1. hash every file except the two manifests → `MANIFEST.sha256`;
2. `checksums.files_list_sha256` = sha256 of that list as written;
3. write `MANIFEST.json` carrying that value;
4. append `MANIFEST.json`'s own hash line to `MANIFEST.sha256`.

`verify` reproduces it exactly: remove the `MANIFEST.json` line, re-hash the
remaining bytes and compare with `files_list_sha256`; then check every listed file
(the manifest itself included) and every file on disk that is *not* listed; then
compare the recorded symlink inventory with the tree. A flattened `.app` — what
Tauri's resource copy or a dereferencing `cp -R` produces — is a failure, not a
cosmetic difference.

A DMG cannot contain its own digest, so the published bytes are pinned beside it:

```
EZ-Convert-Engine-Pack-<arch>.dmg                 # <arch> = arm64 | x86_64
EZ-Convert-Engine-Pack-<arch>.dmg.sha256          # the digest
EZ-Convert-Engine-Pack-<arch>.dmg.manifest.json   # name, bytes, sha256, pack id/version/arch,
                                                  # + sha256 of the MANIFEST.json inside
```

Each architecture's pack publishes its own trio (same shape, different bytes).

The installer checks `MANIFEST.sha256` before copying and carries `MANIFEST.json`
into the install root, so the installed pack still says what it is. To check a
pack the way the app uses it (macOS):

```bash
# an installed pack, plus proof the app bundle carries no engines
bash scripts/verify-engine-pack.sh \
  --pack "$HOME/Library/Application Support/app.ezconvert.desktop" \
  --app "/Applications/EZ Convert.app" --expect-arch "$(uname -m)"

# a downloaded DMG, before installing anything (mounts read-only)
bash scripts/verify-engine-pack.sh --dmg EZ-Convert-Engine-Pack-x86_64.dmg --expect-arch x86_64
```

That verifier checks the sidecar digest against the artifact, the manifest pair
against the tree, the symlink inventory (with `engines/soffice/program` and
`LibreOffice.app/Contents/MacOS/soffice` as the named spot checks), `codesign --verify` on **every** Mach-O object in the pack,
that no Mach-O object still references the build machine (`/opt/homebrew`,
`/usr/local` — the pack has to run on a Mac with no Homebrew), and that `vips`,
`ffmpeg` and `soffice` actually run from inside the pack — then, with `--app`,
that `Contents/Resources` holds no engine tree. `scripts/mac-engine-pack.sh`
runs it on the DMG it just built (step 8b), so a pack that does not survive
`hdiutil create` fails in the build, not in the user's hands.

## 3. Ad-hoc signing (v1)

`packaging/macos/sign-app.sh` and stage 5 of `scripts/mac-engine-pack.sh` use
one order. It is not stylistic: **a signature covers the bytes of everything
inside it**, so an inner object modified after its parent was signed leaves the
parent's seal invalid — `codesign --verify --deep --strict` fails, notarization
rejects the submission, and a user sees *"the app is damaged and can't be
opened"*.

Inside-out, in exactly this order:

| # | Object | Command |
|---|---|---|
| 1 | every loose Mach-O in the bundle, **deepest path first** | `codesign --force --sign - <file>` |
| 2 | nested bundles, deepest first (`.framework`, `.app`, `.appex`, `.xpc`, `.bundle`) | `codesign --force --sign - <bundle>` |
| 3 | the outer `.app` | `codesign --force --sign - "EZ Convert.app"` |
| 4 | `codesign --verify --deep --strict --verbose=2` | must pass before packaging |
| 5 | build the DMG (`hdiutil create -format UDZO`) | — |
| 6 | sign the DMG itself | `codesign --force --sign - <dmg>` |

For the engine pack the same rule applies one level down: the LibreOffice tree,
the vips dylibs and the ffmpeg binaries are signed *before* `hdiutil create`, and
the DMG last. Nothing is signed twice after a modification — `chmod +x` runs
before signing, never after (a post-signing mode change invalidates the seal).

Ad-hoc signatures have **no timestamp and no hardened runtime**: they prove
integrity, not identity, and Gatekeeper still requires the right-click → Open
dance on first launch. That is the accepted v1 trade (Sequoia removed the
Control-click shortcut for quarantined *unsigned* downloads, so this is a real
friction, not a footnote). The Developer ID variant of this order — hardened
runtime, entitlements, secure timestamps, notarization, stapling — is
`docs/MACOS-SIGNING.md`.

## 4. When the $99/yr Developer ID is funded

> **NOT IN SCOPE FOR v1.** The stakeholder ruling of 2026-11-02
> (`docs/DISTRIBUTION.md`) makes EZ Convert internal-only: ad-hoc signing, no
> Developer ID, no notarization, no store. The exact reference sequence —
> parameterized commands, hardened runtime, entitlements, secure timestamps,
> `codesign` verification, `notarytool submit --wait`, stapling to app and DMG,
> `spctl` assessment, gotchas, and the v1 release audit — lives in
> **`docs/MACOS-SIGNING.md`**, the single source of truth for the funded order.
> CI runs none of it. The v1 consequence stays as §3 states: Gatekeeper needs
> one right-click → Open (or `xattr -d com.apple.quarantine`), and the
> engine-pack installer carries the same note.

In one line: import the Developer ID certificate into a throwaway keychain,
sign inside-out with `--options runtime --timestamp`
(`sign-app.sh` / `mac-engine-pack.sh` via `EZC_SIGN_IDENTITY`/`EZC_SIGN_OPTIONS`),
rebuild the app DMG from the signed **and stapled** app, `notarytool submit
--wait` + `stapler` on the app and both DMGs, then `spctl` each. The traps that
make a naive port ship the wrong bytes — the DMG is bundled *before* the signing
pass, `stapler staple` invalidates digest sidecars, entitlements belong on the
main executable only — are sequenced around in `docs/MACOS-SIGNING.md` §5–§9.

## 5. What the runner proves (and what it cannot)

`install-test` in `.github/workflows/macos-dmg.yml` is the acceptance test, run
on Apple Silicon with no developer tools:

1. mount the app DMG, `sudo ditto` the app into `/Applications` (the classic
   App-Translocation trap: a binary that works from `target/` can die here);
2. `codesign --verify --strict` on the installed bundle, `lipo -archs` on the
   installed binary (both slices present);
3. mount the pack DMG, run `Install Engine Pack.command` exactly as a user
   would (it verifies `MANIFEST.sha256` before copying);
4. re-verify the **installed** tree with
   `scripts/verify-engine-pack.sh --pack … --app "…"`: the manifest pair, the
   symlink inventory, `codesign --verify` on every Mach-O object, all three
   engines running from the installed location, and no engine tree under the
   app's `Contents/Resources`;
5. run the installed app's own binary:
   `"/Applications/EZ Convert.app/Contents/MacOS/ez-convert" --diagnostics --fixtures <repo>/fixtures`
   — one real conversion per engine family (png→jpg, wav→mp3, mp4→webm,
   docx→pdf) through `engine::convert` / `engine::doc`, with "exit 0 is not
   success" enforced (a reported conversion with no file on disk is a FAIL).

`--diagnostics` is not a test-only binary: it is the app binary, in the shipped
bundle, resolving engines exactly as the window does.

**Gaps this card does not close:**

- the `install-test` job runs on Apple Silicon only and therefore exercises the
  arm64 pack. The Intel pack has its own execution proof, on Intel hardware:
  `.github/workflows/macos-validate.yml`'s `intel` job installs
  `EZ-Convert-Engine-Pack-x86_64.dmg`, runs
  `scripts/verify-engine-pack.sh --expect-arch x86_64` (every Mach-O signature,
  the pack's symlink inventory, all three engines executing) and requires 4/4
  conversions — see §7.
- unsigned: Gatekeeper friction on first launch (§3), and `spctl` will reject
  the DMG until §4 is funded.
- no release upload: the DMGs are build artifacts on the run (`macos-app-dmg`,
  `macos-engine-pack-arm64`, `macos-engine-pack-x86_64`); publishing them is a
  release-engineering card.

## 6. The app DMG, measured off the runner (2026-11-02)

The `app` job's own log says what it built; this section is the same claim
re-measured on the downloaded artifact instead of the log, because a green step
is not the same as the bytes it uploaded being the bytes we think they are.

Source: artifact `macos-app-dmg` from run `41381599797` (tag `v0.0.1-p3.4-app`,
head `ac2fb8f`), the first run that executed the two gates further down — the
`app` job and the workflow verdict are green, with `pack`/`install-test` skipped
on purpose by the tag suffix. The DMG is 6,299,142 B and its sha256 is
`e8ac81f21eff646dc53e3256843dcd526f86415ac867f30814953208562c6361`, identical to
the `dist/app-dmg.sha256` the job recorded and to the job log — the artifact is
the file the job hashed, not a rebuild. (The first measurement, on the artifact
from run `41379802059` at `3a60afa`, gave the same architecture result:
`0f9dacc8784edd24a5627d508caf402a005320c49d3794af15d40fb56023f740`.)

| Claim | Measurement |
|---|---|
| universal | fat header `0xcafebabe`, `nfat_arch = 2` |
| x86_64 | slice at offset 16,384, 5,978,912 B, `LC_BUILD_VERSION platform=1 minos=11.0.0` |
| arm64 | slice at offset 5,996,544, 5,405,008 B, `LC_BUILD_VERSION platform=1 minos=11.0.0` |
| ad-hoc signed | both slices carry an embedded `LC_CODE_SIGNATURE` (64,480 B / 28,800 B) |
| bundle | 4 files, 12,349,211 B total |

The bundle's entire file list — this is the whole `Contents` tree, not a
summary:

```
   1,058  Contents/Info.plist
11,401,552  Contents/MacOS/ez-convert
  944,161  Contents/Resources/icon.icns
    2,440  Contents/_CodeSignature/CodeResources
```

`Contents/Resources` holds `icon.icns` and nothing else, so LibreOffice, vips and
ffmpeg are provably not in the bundle (§1: they ride in the engine pack). The
DMG volume adds only the `Applications` symlink and `.VolumeIcon.icns`.

Two gates now enforce that shape on every run instead of leaving it to the
config that happens to be correct today — the Windows bundle config *does* copy
`resources/engines` and shares this source tree:

1. **Bundle shape gate** (before packaging) — fails if `Contents/Resources`
   holds anything but `icon.icns`, or if any `soffice`/`libreoffice`/`vips`/
   `ffmpeg`/`ffprobe`/`engines` entry appears anywhere in the bundle. Proven
   non-vacuous against two planted fixtures: a `Resources/engines/soffice/...`
   tree and a `Contents/Frameworks/libvips.42.dylib` both fail the step.
2. **Licence gate over the DMG payload** (after packaging, before upload) —
   `scripts/dist-license-gate.sh --scan` over the `.app` on the mounted volume,
   fail-closed; this is warden's Q2 condition. The five rows it needs live in
   `docs/shipped-inventory.tsv` under "macOS app DMG payload". Before this step
   the app DMG had never been through the gate: all four bundle files were
   unattributed and the scan failed.

## 7. Cross-architecture validation (2026-11-02)

`.github/workflows/macos-validate.yml` is the acceptance run for the *shipped*
artifacts. It rebuilds nothing: it downloads `macos-app-dmg` and both
`macos-engine-pack-{arm64,x86_64}` artifacts from a named run, pins each image
against its published digest sidecar (`packaging/macos/verify-pins.sh`,
fail-closed), installs the pack for the host architecture
(`packaging/macos/install-from-dmgs.sh`, which selects
`EZ-Convert-Engine-Pack-$(uname -m).dmg`), and then exercises the installed app
on two runners:

| | arm64 native | arm64, x86_64 slice via Rosetta | Intel native |
|---|---|---|---|
| runner | `macos-latest` (macOS 26.6.2) | same | `macos-15-intel` (macOS 15.7.9, build 24G830) |
| install + `codesign --verify --strict` + symlinks | PASS | — | PASS, 24 symlinks / 0 dangling |
| 5/5 engines resolved | PASS | PASS | PASS |
| pack verification | PASS, 339 Mach-O | — | PASS, 339 Mach-O (`--expect-arch x86_64`) |
| conversions | **4/4** | **4/4** | **4/4** |
| windowed app + screenshot | PASS | — | PASS |

The x86_64 pack that makes the Intel column possible was built in run
`41394172979` (the `pack` job is now a two-architecture matrix:
`macos-latest`/arm64 and `macos-15-intel`/x86_64):

| pack | bytes | files / symlinks | sha256 |
|---|---|---|---|
| `EZ-Convert-Engine-Pack-arm64.dmg` | 362,254,999 | 9,305 / 24 | see `.dmg.sha256` of that artifact |
| `EZ-Convert-Engine-Pack-x86_64.dmg` | 380,510,014 | 9,305 / 24 | `4941922215d89362dfae12d43ed74fb14440b67ae9973d8f43491866cb9b5d7a` |

The digest above was recomputed from the downloaded workflow artifact, and it
matches both `EZ-Convert-Engine-Pack-x86_64.dmg.sha256` and
`…dmg.manifest.json` (`pack.id = ezconvert-engine-pack-macos-x86_64`,
`arch = x86_64`). Acceptance run `41397186245` (source run `41394172979`,
`intel_label=macos-15-intel`) is green on both validation jobs and prints
`VALIDATION intel install=pass launch=pass engine-discovery=pass conversion=pass`
with `INTEL_CONVERSION=pass (4/4)`; logs are on the `macos-status` branch under
`macos-status/run-41397186245/validate-{arm64,intel}/`. Full matrix, commands and
log excerpts: `evidence/p3-x86-pack.md`.

Two things this section is careful about:

- **Rosetta is not Intel.** The x86_64 slice converts 4/4 on Apple Silicon
  because the translated process spawns the arm64 engines natively. It proves
  the Intel *slice* is sound; it says nothing about Intel *hardware*, which is
  why the second job runs on one.
- **`0/4` on Intel is a failure now.** While there was a single (arm64) pack,
  the Intel job classified `0/4` as the documented architecture limitation. It no
  longer does: the job requires 4/4, so a pack that is not built, not picked, not
  executable or not installed fails the run instead of hiding behind the old
  note. The same run asserts what used to be arm64-only — 5/5 discovery,
  `codesign --verify --strict`, the pack's own symlink inventory, and
  `Contents/Resources` = `icon.icns` only.
- **Rosetta is not part of the Intel job.** That job runs the app binary
  directly on Intel hardware; translation only appears in the arm64 job, where
  the same universal binary has to work under it.

`macos-13` no longer exists (retired December 2025); the current Intel labels are
`macos-15-intel` (through August 2027) and `macos-26-intel`.

The first Intel run also earned its keep by exposing a **false PASS** in
`scripts/verify-engine-pack.sh`: the soffice smoke check only required non-empty
output, so `Bad CPU type in executable` on stderr counted as success. It now
asserts exit status for all three engines and declines to PASS a licence probe it
could not run, guarded by `scripts/test-verify-engine-pack.sh` in both macOS
workflows.

