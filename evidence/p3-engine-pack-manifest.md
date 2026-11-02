# Engine pack: version manifest, artifact verification, and the .app fix

Run: **41388618088** (`macos-dmg`, dispatch on `main` @ `220db32`), results
`{"app_dmg":"success","engine_pack":"success","install_test":"success"}` —
`macos-status/run-41388618088/facts.json`.

Before this card the last recorded pack run was `run-41365601536 pack=failure`
with `artifacts.absent`: no engine-pack artifact had ever been produced. This run
produces one, verifies it, installs it as a user does, and converts with it.

## 1. The artifact

```
./EZ-Convert-Engine-Pack-arm64.dmg              362 201 061 B
./EZ-Convert-Engine-Pack-arm64.dmg.sha256       5f6c41050dfa91d04aa91ed7d6b09ba47598b4bdb9c45ecfb66dd1c9e998eb9c
./EZ-Convert-Engine-Pack-arm64.dmg.manifest.json 558 B
```
(evidence `macos-status/run-41388618088/pack/artifacts.ls`)

Uploaded as the `macos-engine-pack` artifact, with `if-no-files-found: error`;
the `Assert the published pins are next to the DMG` step fails the job if the
sidecar or the `.sha256` is missing, so the DMG is never published without them.

## 2. Inside the artifact (manifest + integrity)

`pack.log` (`macos-status/run-41388618088/pack/pack.log`):

```
manifest: pack ezconvert-engine-pack-macos-arm64 version 1.0.0 (9305 files, 24 symlinks)
manifest: engine ffmpeg 8.1.1 -> engines/ffmpeg/ffmpeg (18 files)
manifest: engine soffice 26.8.2 -> engines/soffice/LibreOffice.app/Contents/MacOS/soffice (8 files in Contents/MacOS)
manifest: engine vips 8.18.7 -> engines/vips/vips (103 files)
artifact manifest: ...dmg.manifest.json sha256=5f6c41…9c bytes=362201061
```

`MANIFEST.json` carries the pack id/version/arch, per-engine versions and entry
points, the source digests (LibreOffice image, ffmpeg buildconf) and the symlink
inventory; `MANIFEST.sha256` covers every file **including** `MANIFEST.json`,
whose own `checksums.files_list_sha256` pins the list without that line (no
circular hash — `scripts/pack-manifest.py` header states the order).

## 3. Artifact verification (step 8b, on the DMG as published)

`verify-engine-pack.sh --dmg … --expect-arch arm64`, from `pack.log`:

```
PASS  artifact digest matches EZ-Convert-Engine-Pack-arm64.dmg.sha256
PASS  artifact digest matches the sidecar manifest
PASS  sidecar version=1.0.0 arch=arm64
PASS  volume has Install Engine Pack.command / README.txt / payload/MANIFEST.json / payload/MANIFEST.sha256
PASS  verify: OK ezconvert-engine-pack-macos-arm64 version 1.0.0 arch arm64
PASS  verify: 9304 files hashed, MANIFEST.json covered by the list
PASS  verify: 24 symlinks intact
PASS  24 symlinks present after unpack
PASS  engines/soffice/program is a symlink -> LibreOffice.app/Contents/MacOS
PASS  LibreOffice.app/Contents/MacOS/soffice is executable
PASS  339 Mach-O objects verify (ad-hoc v1 signatures)
PASS  339 Mach-O objects reference only system or in-pack libraries
PASS  pack vips runs: vips-8.18.7
PASS  pack ffmpeg runs: ffmpeg version 8.1.1
PASS  shipped ffmpeg is not GPL
PASS  pack soffice runs: LibreOffice 26.8.2.2 9dea406ab7695df6325b11b0e19fdca8db4b7276
verify-engine-pack: PASS
```

## 4. Installed pack + the app that uses it

`install-test` mounts both DMGs, `sudo ditto`s the app to `/Applications`, runs
`Install Engine Pack.command` exactly as a user would (it verifies
`MANIFEST.sha256` before copying), then:

`verify-pack.log` (`macos-status/run-41388618088/install-test/verify-pack.log`):

```
PASS  verify: 24 symlinks intact
PASS  24 symlinks present after unpack
PASS  339 Mach-O objects verify (ad-hoc v1 signatures)
PASS  339 Mach-O objects reference only system or in-pack libraries
PASS  pack soffice runs: LibreOffice 26.8.2.2 …
PASS  app bundle signature verifies (EZ Convert.app)
PASS  app bundle carries no Contents/Resources/engines
PASS  app bundle carries no Contents/Resources/LibreOffice.app
PASS  app bundle carries no Contents/Resources/soffice|vips|ffmpeg
verify-engine-pack: PASS
```

`diag.log` — the **installed** app binary, resolving engines the way the window
does, one conversion per family:

```
engine vips/ffmpeg/soffice found  (~/Library/Application Support/app.ezconvert.desktop/engines/…)
FAMILY image PASS, audio PASS, video PASS, document PASS
SUMMARY families=4 pass=4 fail=0
```

## 5. Why the pack is not a Tauri resource — and why LibreOffice is a real .app

The app bundle carries no engines (§4, three `carries no` lines). The pack lives
under `~/Library/Application Support/app.ezconvert.desktop/`, resolved through
`engine::proc::EnginePaths::from_root` → `discover` → `adopt_pack`
(`docs/MACOS-PACKAGING.md` §2), with `EZC_ENGINE_PACK` as the test/CI override.

LibreOffice is shipped as the **pristine `.app`**, not flattened. The flattened
layout (program/ = `Contents/MacOS`, Resources content at the pack root,
`Resources -> .`) passes every static check and then aborts with a UNO
`DeploymentException` on the first conversion: run 41384142116 converted
png→jpg and wav→mp3 from the pack and died in soffice with `Abort trap: 6` and an
empty log. Its bootstrap resolves data relative to the executable *inside* the
bundle, so the bundle is copied with `ditto` and never unpacked;
`engines/soffice/program` remains a symlink to `Contents/MacOS` so the flat path
still resolves for tooling, and the app prefers the bundle path on macOS.

## 6. What this card does not close

- **arm64 only.** The pack is built for Apple Silicon; `--arch x86_64` exists but
  has not been run on Intel hardware.
- **Ad-hoc signing.** Integrity, not identity: `spctl` still rejects the DMG and
  Gatekeeper needs the one right-click → Open (`docs/DISTRIBUTION.md`).
- **No release upload.** The DMGs are workflow artifacts on the run; publishing
  them is release engineering, not this card.
- The pack is ~362 MB compressed; the installer does not prune the previous
  install beyond moving it beside the pack root (documented, not measured).
