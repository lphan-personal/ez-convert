# macOS signing — the Developer ID order (reference) and the v1 audit

Status: **v1 ships ad-hoc, per the stakeholder ruling in `docs/DISTRIBUTION.md`**
(internal-only; no Developer ID, no notarization, no store — explicitly out of
scope). This document is the *reference sequence* for that ruling's revisit
condition ("fund Developer ID → sign engine pack → notarize the DMG → staple →
document the change"): the exact inside-out order, parameters, gotchas and
verification to follow **if** publishing is ever decided. **CI runs none of
§4–§8 today.** The v1 ad-hoc order itself is `docs/MACOS-PACKAGING.md` §3; the
bundle layout and the engine-pack contract are §1–§2 there.

Companion scripts (the executable encoding of §5–§6 — keep doc and script
together): `packaging/macos/sign-app.sh`, `scripts/mac-engine-pack.sh` (stage 5),
`scripts/verify-engine-pack.sh`. §11 audits what v1 actually does.

## 1. The two artifacts

| Artifact | Built by | Signed by (v1) | Signed by (funded flow) |
|---|---|---|---|
| `EZ-Convert-<v>-macos-universal.dmg` (`EZ Convert.app` inside) | `.github/workflows/macos-dmg.yml` app job | ad-hoc (`-`) | §5 |
| `EZ-Convert-Engine-Pack-<arch>.dmg` (`engines/` + licences + manifests) | pack job (`scripts/mac-engine-pack.sh`) | ad-hoc (`-`) | §6 |

Every step below is ordered. A signature covers the exact bytes of everything
inside it, so an object modified after its parent was signed invalidates the
parent's seal — `codesign --verify --deep --strict` fails, notarization rejects,
and the user gets *"the app is damaged and can't be opened"*.

## 2. Why LibreOffice cannot live in `Contents/Resources`

Two properties of Tauri's macOS bundler make it impossible, and one property of
our release cadence makes it undesirable:

1. **Tauri never signs `Contents/Resources`.** Its bundler signs
   `Contents/MacOS`, `Frameworks`, `Plugins`, `Helpers`, `XPCServices` and
   `Libraries` — never `Resources`. An engine placed there ships **unsigned**;
   an unsigned nested executable inside a signed bundle fails
   `codesign --verify --deep --strict`, and notarization rejects the submission
   outright ("the binary is not signed at all" class).
2. **Its resource copy flattens `.app` symlinks.** `copy_resources` copies
   file-by-file and drops the symlinks and extended attributes a LibreOffice or
   libvips tree depends on. LibreOffice's bootstrap resolves
   `../Resources/fundamentalrc` and `../Frameworks` relative to the executable
   *inside* the bundle: a flattened LibreOffice starts, converts nothing, and
   aborts with a UNO `DeploymentException` (`Abort trap: 6`, empty log — measured
   on the 26.8.2 macOS build, CI run 41384142116). The pack therefore ships the
   pristine `.app`, copied with `ditto`, with `engines/soffice/program` kept as a
   symlink into it (`docs/MACOS-PACKAGING.md` §2).
3. **Third-party code inside the app bundle would be re-signed on every app
   release.** The engine pack is versioned, signed and updated on its own
   cadence (`docs/MACOS-PACKAGING.md` §1).

Consequence enforced in CI: the app bundle's `Contents/Resources` holds
`icon.icns` and nothing else — asserted by the bundle shape gate (fail on any
engine marker anywhere in the bundle), because the Windows bundle config shares
this tree and does copy `resources/engines`.

## 3. Parameters

Every command in §4–§8 uses exactly these; nothing else is literal except tool
names and the paths our layout defines.

```bash
# identity / account (the $99 Apple Developer Program gives a "Developer ID
# Application" certificate — not "Mac App Distribution", not "Apple Development";
# either of those is rejected at notarization: "not signed with a valid
# Developer ID certificate")
DEVELOPER_NAME="..."                      # as printed on the certificate
TEAMID="..."
ID="Developer ID Application: ${DEVELOPER_NAME} (${TEAMID})"
APPLE_ID="..."                            # Apple ID for notarytool (or an API key)
APPLE_APP_PASSWORD="..."                  # app-specific password, NOT the account pw

# signing behaviour
SIGN_OPTS="--options runtime --timestamp" # hardened runtime + secure timestamp
ENT=""                                    # entitlements plist path; empty by default (§7)

# artifacts
APP="src-tauri/target/universal-apple-darwin/release/bundle/macos/EZ Convert.app"
APP_DMG="dist/EZ-Convert-${EZC_VERSION}-macos-universal.dmg"
PACK_DMG="dist/EZ-Convert-Engine-Pack-arm64.dmg"   # or --arch x86_64
```

Repository secrets the CI would need when funded: `APPLE_CERT_P12` (base64
`.p12` of cert + key), `APPLE_CERT_PASSWORD`, `APPLE_ID`, `APPLE_TEAM_ID`,
`APPLE_APP_PASSWORD`.

## 4. One-time prerequisites (CI keychain)

```bash
security create-keychain -p "$KEYCHAIN_PW" build.keychain
security set-keychain-settings -lut 21600 build.keychain   # else it relocks mid-job
security unlock-keychain -p "$KEYCHAIN_PW" build.keychain
security import cert.p12 -k build.keychain -P "$APPLE_CERT_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple: -k "$KEYCHAIN_PW" build.keychain
```

Gotcha: without `set-key-partition-list`, `codesign` fails with
`errSecInternalComponent` / *"user interaction is not allowed"* — the key is
there but the partition list does not allow the non-interactive tool to use it.

## 5. Order 1 — the app bundle, inside-out

Objects in **exactly this order** (each row before the row that contains it):

| # | Object | Command |
|---|---|---|
| 1 | every loose Mach-O under `Contents/`, **deepest path first**: nested helper tools, dylibs (incl. anything in `Contents/Frameworks`) | `codesign --force --sign "$ID" $SIGN_OPTS <file>` |
| 2 | nested code containers, **deepest first**: `.framework`, `.bundle`, `.xpc`, `.appex`, nested `.app` helpers | `codesign --force --sign "$ID" $SIGN_OPTS <bundle>` |
| 3 | the main Tauri app (its `Contents/MacOS/ez-convert` is signed as part of the bundle) | `codesign --force --sign "$ID" $SIGN_OPTS [--entitlements "$ENT"] "$APP"` |
| 4 | verification | `codesign --verify --deep --strict --verbose=2 "$APP"` |
| 5 | notarize the app, staple the app | §7 |
| 6 | build the app DMG **from the stapled app** | `hdiutil create ... -format UDZO "$APP_DMG"` |
| 7 | sign the DMG, notarize it, staple it | §7 |
| 8 | digest sidecars **after** stapling | `shasum -a 256 "$APP_DMG" \| tee dist/app-dmg.sha256` |

Rows 1–4 are what `packaging/macos/sign-app.sh` runs with
`EZC_SIGN_IDENTITY="$ID" EZC_SIGN_OPTIONS="$SIGN_OPTS"` — the script strips the
per-architecture `file(1)` lines of universal binaries (gotcha 3, §9) and
refuses an empty Mach-O list. Entitlements (row 3) are applied to the outer
bundle only (§7; the script passes none, which is the correct default here).

**Row 6 is a rebuild, not a copy.** Today's v1 workflow packages Tauri's own DMG,
which `tauri build` created *before* the signing pass (§11, F1). In the funded
flow the DMG must be created from the signed **and stapled** app — build the
`stage/` folder (app + `Applications` symlink) and `hdiutil create` from it,
exactly as the pack script does. Sign the DMG with `--timestamp` (the `runtime`
flag is for Mach-O; a disk image is not Mach-O):

```bash
mkdir -p stage && ditto "$APP" "stage/EZ Convert.app" && ln -s /Applications stage/Applications
hdiutil create -volname "EZ Convert" -srcfolder stage -ov -format UDZO "$APP_DMG"
codesign --force --sign "$ID" --timestamp "$APP_DMG"
```

## 6. Order 2 — the engine pack, inside-out

`scripts/mac-engine-pack.sh` already encodes this order (stage 5 → 6 → 7 → 8);
setting `EZC_SIGN_IDENTITY`/`EZC_SIGN_OPTIONS` upgrades it in place. The object
order, for reference:

| # | Object | Notes |
|---|---|---|
| 1 | every Mach-O under `engines/**`, deepest path first | vips dylibs + modules, ffmpeg/ffprobe + codec dylibs, **LibreOffice nested binaries** (launchers in `LibreOffice.app/Contents/MacOS`, dylibs in `Contents/Frameworks`) |
| 2 | nested code containers, deepest first | `LibreOffice.app` and anything nested inside it (`.framework`, `.bundle`, `.xpc`, `.appex`) |
| 3 | verification | `codesign --verify --deep --strict` on `LibreOffice.app` and the engines roots (funded flow — see gotcha 11) |
| 4 | licence gate + engine smoke | unchanged |
| 5 | **manifest seal** (`scripts/pack-manifest.py seal`) | after signing — `MANIFEST.sha256` covers the *signed* bytes (gotcha 10) |
| 6 | `hdiutil create` the pack DMG, then sign it | `codesign --force --sign "$ID" --timestamp "$PACK_DMG"` |
| 7 | notarize + staple the pack DMG | §7 |
| 8 | sidecar `.sha256` + `seal-artifact` **after** stapling | gotcha 6 |

## 7. Notarization, stapling, entitlements, hardened runtime

```bash
# app: notarytool takes zip/pkg/dmg, not a bare .app
ditto -c -k --keepParent "$APP" "${APP%.app}.zip"
xcrun notarytool submit "${APP%.app}.zip" --apple-id "$APPLE_ID" --team-id "$TEAMID" \
  --password "$APPLE_APP_PASSWORD" --wait --output-format json | tee notarize-app.json
grep -q '"status": "Accepted"' notarize-app.json        # gate on the verdict (gotcha 7)
xcrun stapler staple "$APP" && xcrun stapler validate "$APP"

# app DMG (after the rebuild in §5), and the pack DMG (after §6 step 6):
xcrun notarytool submit "$APP_DMG" --apple-id "$APPLE_ID" --team-id "$TEAMID" \
  --password "$APPLE_APP_PASSWORD" --wait --output-format json | tee notarize-appdmg.json
grep -q '"status": "Accepted"' notarize-appdmg.json
xcrun stapler staple "$APP_DMG" && xcrun stapler validate "$APP_DMG"
# then recompute the digest sidecars (§5 row 8, §6 step 8)
```

**Staple the app before building the DMG** so the ticket travels inside the
image: a user who copies the app out of the DMG still passes Gatekeeper offline.

**Entitlements — keep them empty unless a restricted capability is required.**
This app needs none under hardened runtime: WKWebView runs in WebKit's own XPC
services (which carry their own entitlements) and the engines are spawned
separate processes. Rules when that changes:

- Entitlements belong to the **main executable** of a process: apply `--entitlements`
  when signing the outer `.app` (its main binary), and to a nested helper
  *app/xpc/appex* that is its own process and needs its own — never to dylibs or
  frameworks (their embedded entitlements are ignored at runtime).
- Tauri passes the app's entitlements to sidecars (tauri#16211): a *restricted*
  entitlement inherited by a sidecar gets it `Killed: 9` while every signature
  and the notarization ticket still verify.
- Do **not** add `com.apple.security.cs.disable-library-validation` unless an
  engine loads plugins signed by a different team — ours are same-identity
  signed, so library validation passes.

**Hardened runtime** (`--options runtime`) goes on **every** Mach-O, engine CLIs
included: notarization requires it of all executables. **Secure timestamps**
(`--timestamp`) go on every Developer ID signature; ad-hoc v1 passes
`--timestamp=none` (ad-hoc signatures cannot be timestamped). A timestamped
release keeps validating after the certificate later expires or is revoked.

## 8. Verification gate (run before publishing; all must pass)

| Check | Expected |
|---|---|
| `codesign --verify --deep --strict --verbose=2` on `$APP`, `LibreOffice.app`, installed `engines/` | `accepted`, `satisfies its Designated Requirement` |
| `codesign -dvvv "$APP" \| grep -E 'Authority\|TeamIdentifier\|flags\|Timestamp'` | `Authority=Developer ID Application: ...`, `TeamIdentifier=TEAMID`, `flags=0x10000(runtime)`, a `Timestamp=` line |
| `lipo -archs "$APP/Contents/MacOS/ez-convert"` | `x86_64 arm64` |
| `xcrun stapler validate "$APP"` and each DMG | `The validate action worked!` |
| `xcrun notarytool info <submission-id>` | `status: Accepted` (record the id in the release notes) |
| `spctl -a -vvv -t exec "$APP"` | `accepted`, `source=Notarized Developer ID` |
| `spctl -a -vvv -t open --context context:primary-signature "$APP_DMG"` (and the pack DMG) | `accepted` |
| `scripts/verify-engine-pack.sh --dmg "$PACK_DMG" --expect-arch arm64` | all pass (manifest pair, symlinks, per-Mach-O signatures, engines run) |

## 9. Gotchas

1. **Never sign with `codesign --deep`.** `--deep` while *signing* guesses the
   nested-code set and stamps one identity/options over everything; Apple's
   guidance is the manual inside-out order used here. `--deep` is fine for
   *verification*.
2. **Seal invalidation.** Any byte change after the parent is signed — `chmod`,
   `xattr`, `install_name_tool`, `strip`, a re-sign — breaks the seal. Sign last;
   `chmod +x` before signing, never after. Copy bundles with `ditto`, not
   `cp -R` (Finder info/xattrs make `--strict` fail: *"resource fork, Finder
   information, or similar detritus not allowed"*).
3. **`file(1)` and universal binaries.** BSD `file` prints one line **per
   architecture** of a fat binary (`path (for architecture arm64): ...`), which
   is not a path — codesign then fails "No such file or directory" (CI run
   41376620641). `sign-app.sh` strips the parenthetical; `cut -d: -f1` also
   truncates any path containing a colon. Use the script's pipeline, not a naive
   one.
4. **Entitlements scope** — §7. The failure mode is silent: `Killed: 9` at
   runtime with every signature and ticket verifying.
5. **Staple the app first, then build the DMG** (§7) — offline Gatekeeper for
   users who copy the app out of the image.
6. **Stapling mutates bytes.** `stapler staple` on a DMG changes the image file,
   so any `.sha256` sidecar computed *before* stapling is stale. Digests and
   `seal-artifact` run **after** stapling (v1 has no stapling, so the pack
   script's current order — sign DMG → sidecar — is correct *today*; move the
   sidecar after the staple step when notarizing).
7. **Gate on `Accepted`, not on the exit code.** `notarytool submit --wait`
   reports the verdict in its output; parse the JSON `status` (or grep
   `status: Accepted`) and treat anything else as failure before stapling.
   `stapler validate` is the final gate. The engine-pack DMG is ~700 MB —
   `--wait` can poll for many minutes; keep the submission id.
8. **CI keychain** — §4: `set-key-partition-list` or codesign dies with
   "user interaction is not allowed"; `set-keychain-settings -lut` or the
   keychain relocks mid-job.
9. **Universal binaries**: one signature covers both slices — no per-architecture
   pass; verify with `lipo -archs` that both slices are present in what ships.
10. **Manifest vs signature ordering**: `MANIFEST.sha256` is sealed over the
    *signed* bytes (pack script stage 8 runs after stage 5). Re-sign anything →
    re-seal the manifest. The two cover different questions (signature = who and
    integrity of the seal; manifest = are these the bytes that were built).
11. **LibreOffice containers are checked loosely today.**
    `verify-engine-pack.sh` runs `codesign --verify` per Mach-O object, never
    `--deep --strict` on the `.app` container. Notarization scans nested
    containers; run `codesign --verify --deep --strict` on `LibreOffice.app`
    before the first submission and extend the verifier then (audit F4).
12. **The `.command` installer is a shell script**: it cannot carry a Mach-O
    signature and needs none. Under the notarized DMG, Gatekeeper judges it via
    the image; in v1 it carries the same right-click → Open / `xattr -d
    com.apple.quarantine` note as the app (`docs/DISTRIBUTION.md`).
13. **One signing authority.** If `signingIdentity` in
    `src-tauri/tauri.macos.conf.json` is ever set, Tauri's bundler signs first;
    our inside-out pass must still run **last** (`--force` replaces the outer
    seal — a nested object left with the bundler's seal is a mixed state). The
    recommended funded configuration keeps the bundler at `"-"` and lets
    `sign-app.sh` / `mac-engine-pack.sh` do all signing via
    `EZC_SIGN_IDENTITY`/`EZC_SIGN_OPTIONS` — entitlements exactly once, one
    identity, one order.

## 10. What changes in CI when the cert is funded — **currently disabled**

These steps do not exist in `.github/workflows/macos-dmg.yml`; the stakeholder
ruling of 2026-11-02 (`docs/DISTRIBUTION.md`) keeps them out until its revisit
condition fires. Listed so "disabled" is explicit, not implied:

1. Secrets `APPLE_CERT_P12`, `APPLE_CERT_PASSWORD`, `APPLE_ID`,
   `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD` (§3).
2. Keychain import steps (§4) at the start of the app and pack jobs.
3. `EZC_SIGN_IDENTITY="$ID" EZC_SIGN_OPTIONS="--options runtime --timestamp"`
   on the "Sign inside-out" step and on `scripts/mac-engine-pack.sh`.
4. App job: rebuild the DMG from the signed+stapled app (§5 row 6), then the
   notarize+staple block (§7) for app and DMG.
5. Pack job: `--deep --strict` on `LibreOffice.app` (gotcha 11), then
   notarize+staple for the pack DMG, then sidecar digests (gotcha 6).
6. install-test additions: `stapler validate` and `spctl` on the installed app
   and mounted DMGs (§8).
7. `src-tauri/tauri.macos.conf.json` stays `signingIdentity: "-"`,
   `hardenedRuntime: false` on purpose (gotcha 13) — the scripts carry the
   identity. If the config is switched instead, Tauri's signer becomes the first
   pass and the inside-out pass must still run after it.

## 11. v1 audit (warden, 2026-11-02, head 89744ac)

Raw scans: `evidence/p3-signing-audit-scans.txt`.

**Confirmed: v1 remains ad-hoc/unsigned by Developer ID, as intended.**
`src-tauri/tauri.macos.conf.json` sets `signingIdentity: "-"` and
`hardenedRuntime: false`; `sign-app.sh` and `mac-engine-pack.sh` both default to
identity `-` (ad-hoc, `--timestamp=none`); the workflow's only signing commands
are `codesign --force --sign -`. No workflow step, script, or secret reference
touches Developer ID, `notarytool`, `stapler`, `spctl` or any `APPLE_*`
credential (whole-tree scan A/B in the evidence file; the only hits are prose in
docs and comments). This matches `docs/DISTRIBUTION.md` exactly.

What v1 actually signs: every Mach-O in the app bundle and in the pack
(`sign-app.sh`; `mac-engine-pack.sh` stage 5), the nested `LibreOffice.app`, and
both DMGs. Verification on every run: `codesign --verify` (app job and
install-test), per-Mach-O `codesign --verify` across the pack (339 objects,
`verify-engine-pack.sh`), plus the shape gate (Resources = `icon.icns`) and the
fail-closed licence gate.

Findings (severity — evidence — recommended action):

| # | Severity | Finding | Action |
|---|---|---|---|
| F1 | MEDIUM (funded-flow trap) | The app job runs `tauri build` (targets `app`,`dmg`) **before** `sign-app.sh`, so the shipped DMG contains the bundler's signature, not the inside-out pass. v1 is equivalent (single Mach-O, valid ad-hoc either way — the artifact's slices do carry `LC_CODE_SIGNATURE` and install-test runs the installed app), but naively setting `EZC_SIGN_IDENTITY` would sign a tree the DMG never sees | §5 row 6: in the funded flow the DMG is rebuilt from the signed+stapled app via `hdiutil`. No v1 change needed |
| F2 | LOW | `MACOS-PACKAGING.md` §4's sketch passed `--entitlements` to every Mach-O and nested bundle | Fixed here: §7 applies entitlements to the outer main executable only (tauri#16211 `Killed: 9` mode) |
| F3 | LOW | §4's sketch Mach-O pipeline lacked the per-architecture `file(1)` strip — reproducing the bug CI already hit (run 41376620641) | Fixed here: gotcha 3; commands delegate to `sign-app.sh` |
| F4 | LOW | No `codesign --verify --deep --strict` on code *containers* (`LibreOffice.app`, incl. `Frameworks`/`PlugIns`) — `verify-engine-pack.sh` checks per-Mach-O only | Gotcha 11: run it before first notarization; extend the verifier then |
| F5 | LOW | Digest sidecars are computed before any stapling; `stapler staple` mutates the DMG | Gotcha 6: digests after stapling |
| F6 | INFO | The pins (`app-dmg.sha256`, pack `.dmg.sha256`/manifest sidecar) are unsigned in v1 — trust root is the GitHub HTTPS download channel. CI never exercises Gatekeeper quarantine (no quarantine xattr on the runner), so the right-click → Open path is documented, not proven | Accepted for internal-only v1 (`docs/DISTRIBUTION.md`); notarization replaces the trust root when funded |
| F7 | LOW | `permissions: contents: write` is workflow-global; only the evidence job pushes to `macos-status`. Third-party actions are tag-pinned (`checkout@v4`, `upload/download-artifact@v4`, `dtolnay/rust-toolchain`, `Swatinem/rust-cache`), not SHA-pinned | Least-privilege per job; SHA-pin the actions if this repo ever publishes |
| F8 | INFO | The app job's `codesign --verify --deep --strict ... \|\| true` is advisory; the fatal gate is `--strict` without `--deep` | Fine while the bundle is one Mach-O; make deep-strict fatal if nested code ever ships in the bundle |
| F9 | INFO | `macos-latest` is a mutable runner image; signed bytes drift with the toolchain | Pin `macos-14`/`macos-15` when signed releases matter |
| F10 | LOW (mid-audit landing 89744ac, owner: t_fa5721fd) | `packaging/macos/install-from-dmgs.sh` mounts and installs both DMGs without checking the sidecar digests (`app-dmg.sha256`, `*.dmg.sha256`); the pack's inner `MANIFEST.sha256` is checked by the installer, the image-level pins never are. `macos-validate.yml` adds no Developer ID/notarization surface (spot-checked — consistent with v1) | One `shasum -a 256 -c` per DMG before mounting; filed to the owning card, not fixed here |

Nothing release-blocking for v1: the release is ad-hoc by ruling, the two CI
gates (shape + licence) and the install/convert acceptance run on the shipped
bytes, and F1–F5 are exactly the traps the funded flow must not walk into —
which is what §5–§9 sequence around.
