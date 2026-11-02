# macOS packaging — universal DMG, engine pack, and the signing order

Status: v1 is **unsigned/ad-hoc**. Everything below was built to run on a GitHub
Actions macOS runner (`.github/workflows/macos-dmg.yml`); the Developer ID path
is documented in §4 and is a configuration change, not a redesign.

## 1. What ships

| Artifact | Contents | Size (runner-measured) |
|---|---|---|
| `EZ-Convert-0.0.1-macos-universal.dmg` | `EZ Convert.app`, universal (arm64 + x86_64) | ~12 MB |
| `EZ-Convert-Engine-Pack-arm64.dmg` | `engines/vips`, `engines/ffmpeg`, `engines/soffice` + licences + `MANIFEST.sha256` + `MANIFEST.json` | ~700 MB |
| `EZ-Convert-Engine-Pack-arm64.dmg.sha256` | digest of the published DMG | 1 line |
| `EZ-Convert-Engine-Pack-arm64.dmg.manifest.json` | pack id/version/arch, artifact digest, `MANIFEST.json` digest | 1 file |

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
engines/ffmpeg/{ffmpeg,ffprobe}
engines/soffice/program/soffice        (console launcher, not soffice.bin)
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

**LibreOffice layout normalisation.** The macOS build is an `.app`; the pack
stores `Contents/MacOS` as `program/` and `Contents/Resources` at the pack root,
which is the same shape as the Linux (`opt/libreoffice/{program,share}`) and
Windows (`program/`) packs. That is what lets `scripts/strip-engine-pack.py` and
`scripts/dist-license-gate.sh --scan` work unchanged on all three platforms.
`scripts/mac-engine-pack.sh` detects the real layout and fails loudly with the
`Contents` listing if TDF ships something else.

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
EZ-Convert-Engine-Pack-arm64.dmg
EZ-Convert-Engine-Pack-arm64.dmg.sha256          # the digest
EZ-Convert-Engine-Pack-arm64.dmg.manifest.json   # name, bytes, sha256, pack id/version/arch,
                                                 # + sha256 of the MANIFEST.json inside
```

The installer checks `MANIFEST.sha256` before copying and carries `MANIFEST.json`
into the install root, so the installed pack still says what it is. To check a
pack the way the app uses it (macOS):

```bash
# an installed pack, plus proof the app bundle carries no engines
bash scripts/verify-engine-pack.sh \
  --pack "$HOME/Library/Application Support/app.ezconvert.desktop" \
  --app "/Applications/EZ Convert.app" --expect-arch arm64

# a downloaded DMG, before installing anything (mounts read-only)
bash scripts/verify-engine-pack.sh --dmg EZ-Convert-Engine-Pack-arm64.dmg --expect-arch arm64
```

That verifier checks the sidecar digest against the artifact, the manifest pair
against the tree, the symlink inventory (with `engines/soffice/Resources` as the
named spot check), `codesign --verify` on **every** Mach-O object in the pack, and
that `vips`, `ffmpeg` and `soffice` actually run from inside the pack — then, with
`--app`, that `Contents/Resources` holds no engine tree. `scripts/mac-engine-pack.sh`
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
friction, not a footnote).

## 4. When the $99/yr Developer ID is funded

> **NOT IN SCOPE FOR v1.** The stakeholder ruling of 2026-11-02
> (`docs/DISTRIBUTION.md`) makes EZ Convert internal-only: ad-hoc signing, no
> Developer ID, no notarization, no store. Everything below is the *reference
> sequence* the card asked to be written down — the exact order that must be
> followed **if** publishing is ever decided. It is not a plan of work, and CI
> does not run any of it. The v1 consequence stays as §3 states: Gatekeeper
> needs one right-click → Open (or `xattr -d com.apple.quarantine`), and the
> engine-pack installer carries the same note.

The `$99` Apple Developer Program gives a **Developer ID Application**
certificate (not "Mac App Distribution", not "Apple Development" — either of
those is rejected at notarization with *"The binary is not signed with a valid
Developer ID certificate"*).

Prerequisites, once:

1. Export a **Developer ID Application** certificate + private key from the
   Keychain as `.p12`.
2. Create an app-specific password (or an App Store Connect API key) for
   `notarytool`.
3. Store, as repository secrets:
   `APPLE_CERT_P12` (base64), `APPLE_CERT_PASSWORD`, `APPLE_ID`,
   `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD`.

Then the exact sequence, per release (the numbers are the order; skipping one
breaks the seal of everything above it):

```bash
# 0. toolchain: import the identity, keep it in a throwaway keychain
security create-keychain -p "$KEYCHAIN_PW" build.keychain
security import cert.p12 -k build.keychain -P "$APPLE_CERT_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple: -k "$KEYCHAIN_PW" build.keychain
ID="Developer ID Application: NAME (TEAMID)"
OPTS="--options runtime --timestamp"

# 1. app: build without signing, then sign inside-out (order from §3)
tauri build --target universal-apple-darwin --no-sign --config src-tauri/tauri.macos.conf.json
find "EZ Convert.app/Contents" -type f -print0 | xargs -0 file | grep Mach-O | cut -d: -f1 \
  | awk -F/ '{print NF" "$0}' | sort -rn | cut -d' ' -f2- |
  while read -r f; do codesign --force --sign "$ID" $OPTS --entitlements entitlements.plist "$f"; done
find "EZ Convert.app/Contents" -mindepth 1 \( -name '*.framework' -o -name '*.appex' -o -name '*.xpc' \) -type d \
  | awk -F/ '{print NF" "$0}' | sort -rn | cut -d' ' -f2- |
  while read -r b; do codesign --force --sign "$ID" $OPTS --entitlements entitlements.plist "$b"; done
codesign --force --sign "$ID" $OPTS --entitlements entitlements.plist "EZ Convert.app"
codesign --verify --deep --strict --verbose=2 "EZ Convert.app"     # 1b. must pass

# 2. engine pack: same inside-out pass over engines/**, then the DMG
EZC_SIGN_IDENTITY="$ID" EZC_SIGN_OPTIONS="$OPTS" scripts/mac-engine-pack.sh --out dist
# (sign-app.sh takes the bundle as an argument: sign-app.sh "EZ Convert.app")

# 3. the app: zip it (notarytool takes zip/pkg/dmg, not a bare .app), submit, staple
ditto -c -k --keepParent "EZ Convert.app" "EZ Convert.zip"
xcrun notarytool submit "EZ Convert.zip" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" \
  --password "$APPLE_APP_PASSWORD" --wait
xcrun stapler staple "EZ Convert.app"

# 4. the DMG: build it from the stapled app, sign it, submit it, staple it
hdiutil create -volname "EZ Convert" -srcfolder stage -ov -format UDZO "EZ-Convert-0.0.1-macos-universal.dmg"
codesign --force --sign "$ID" $OPTS "EZ-Convert-0.0.1-macos-universal.dmg"
xcrun notarytool submit "EZ-Convert-0.0.1-macos-universal.dmg" --apple-id "$APPLE_ID" \
  --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" --wait
xcrun stapler staple "EZ-Convert-0.0.1-macos-universal.dmg"

# 5. gate before publish — both must pass
spctl -a -vvv -t exec "EZ Convert.app"
spctl -a -vvv -t open --context context:primary-signature "EZ-Convert-0.0.1-macos-universal.dmg"
```

Repeat 2–5 for the engine pack DMG (`spctl -t open` on the pack, and
`codesign --verify --deep --strict` over the installed `engines/` tree).

Entitlements: keep them empty unless a restricted capability is required. Tauri
passes the app's entitlements to sidecars (tauri#16211), and a *restricted*
entitlement inherited by a sidecar gets it `Killed: 9` while every signature and
the notarization ticket still verify. Do **not** add
`com.apple.security.cs.disable-library-validation` unless libvips is loading
unsigned plugins — our plugins are signed with the same identity, so it is not
needed.

What changes in the workflow when the cert exists: `signingIdentity` in
`src-tauri/tauri.macos.conf.json` becomes the Developer ID string,
`hardenedRuntime` becomes `true`, the two `sign-*` steps take
`EZC_SIGN_IDENTITY`/`EZC_SIGN_OPTIONS`, and the notarize+staple block from step 3
and 4 is appended. Nothing else moves.

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

- the pack is **arm64 only**. The app is universal and runs on Intel, but media
  and document conversion on an Intel Mac needs an `x86_64` pack; the script
  takes `--arch x86_64` and has not been run on Intel hardware. Filed as a
  follow-up rather than claimed.
- unsigned: Gatekeeper friction on first launch (§3), and `spctl` will reject
  the DMG until §4 is funded.
- no release upload: the DMGs are build artifacts on the run (`macos-app-dmg`,
  `macos-engine-pack`); publishing them is a release-engineering card.
