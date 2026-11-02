# EZ Convert

A local file converter for Windows and macOS: images, audio, video and office documents,
converted by engines that run on your own machine. Tauri 2 shell, Rust core, radial "wheel"
overlay plus a normal window.

**Status: v1 (version 0.0.1), internal build.** Both surfaces work, four format families
convert, and every advertised direction has a passing end-to-end run against real engines.
The build is unsigned and internal-only by stakeholder decision (`docs/DISTRIBUTION.md`).

- **Users** → [`docs/USER-GUIDE.md`](docs/USER-GUIDE.md) (install, formats, fidelity caveats,
  output rules, privacy, known limitations).
- **What changed in this version** → [`docs/RELEASE-NOTES.md`](docs/RELEASE-NOTES.md).
- **What converts today, with per-route evidence** → [`docs/DETECTION.md`](docs/DETECTION.md).

Everything runs offline: no update check, no telemetry, no crash reporting, no network
capability at run time (`docs/ARCHITECTURE.md` §1, §16 S8).

---

## What it does

- **Wheel** (`ui/wheel`, `src-tauri/src/shell/`): a global hotkey pops a 520 × 520 ring at
  the pointer without stealing focus; drop files from Explorer/Finder, pick a target, done.
  Six segments maximum, overflow behind a "more" sub-ring, only valid targets shown.
- **Window** (`ui/`): queue with per-file state, multi-file and folder drop, "Convert all
  to", per-batch metadata toggle, settings generated from the schema, completion toast with
  **Show** and a 10-second **Undo**.
- **Detection** (`src-tauri/src/model/`): content decides, never the extension. A JPEG
  renamed `.png` resolves to JPEG; a text file renamed `.docx` is refused rather than sent
  to a document engine that would produce a plausible-looking PDF from it.
- **Output rules** (`src-tauri/src/job/`): same folder by default, `converted/` for batches
  of three or more, never overwrite (`-1`, `-2`…), atomic placement, verified Undo.
- **Engines as sidecars**: libvips for still images, ffmpeg/ffprobe for audio and video,
  LibreOffice for documents, each a separate process with a cleared environment. Nothing is
  linked into the app binary and nothing is downloaded at run time.

148 conversion directions are advertised and measured: 37 image, 49 audio, 20 video and 42
document directions. The wheel can only render routes the matrix marks `PROVEN`; `planned`
rows are invisible and `cut` rows are measured dead.

---

## Requirements

| | Windows | macOS |
|---|---|---|
| OS | Windows 11 x64 (measured); Windows 10 untested | macOS 11+ |
| CPU | x64 | **Apple Silicon for conversion** — the engine pack is arm64 only; the app is universal but Intel conversions fail `Bad CPU type in executable` |
| Disk | ~1 GB installed | app ~6 MB + engine pack 362,201,061 B |
| Network | none required | none required |

---

## Build

```bash
# Linux prerequisites (Debian/Ubuntu) for the Tauri shell
sudo apt-get install -y build-essential pkg-config libssl-dev libgtk-3-dev \
  libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev \
  libayatana-appindicator3-dev librsvg2-dev

cd src-tauri
cargo build            # --locked, Cargo.lock is committed
cargo test
cargo run              # needs a desktop session
```

Containerised agents usually cannot run the full `cargo test` (the shell needs GTK/WebKit);
the pure model and engine tests do not, and these are the local paths:

```bash
bash scripts/test-model.sh          # detection + matrix, plain rustc
bash scripts/test-engine.sh         # argv builders, planner, spawn contract, verifiers
bash scripts/c4-tests.sh            # output rules: plan → convert → publish → undo (+ e2e)
bash scripts/e2e.sh                 # one real conversion per advertised direction
bash scripts/check-model-purity.sh  # model/ stays Tauri-free (ARCHITECTURE §2.2)
bash scripts/hebrew-fidelity.sh     # CLM → open-licence twin substitution
```

`scripts/e2e.sh` needs real engines; point it at them with `EZC_VIPS_DIR`,
`EZC_FFMPEG_DIR`, `EZC_SOFFICE_DIR` and (for an unpacked vips tree) `EZC_VIPS_PLUGIN_DIR`.
`--strict` turns a skip into a failure, `--inject=drop-output` proves the gate goes red.
See [`e2e/q3/README.md`](e2e/q3/README.md).

### Regenerating generated files

```bash
python3 scripts/gen-fixtures.py                        # fixtures/ + fixtures/PROVENANCE.md
python3 scripts/gen-media-fixtures.py --vips … --ffmpeg … --icc …
rustc --edition 2021 -o /tmp/model-tables scripts/model-tables.rs
/tmp/model-tables --write                              # docs/DETECTION.md + evidence/ROUTES.md
python3 scripts/gen-licenses.py                        # docs/LICENSES.md from the locked graph
python3 scripts/make-icons.py src-tauri/icons           # needs ffmpeg on PATH
```

`docs/DETECTION.md` and `evidence/ROUTES.md` are generated from `model/matrix.rs` and
`cargo test` compares them byte for byte, so a route cannot be added without its table row.

**Icon gotcha:** `tauri::generate_context!()` requires `src-tauri/icons/icon.png` at compile
time even when `bundle.active` is `false`; a missing icon fails the build with
`proc macro panicked: failed to open icon …`.

---

## Repo map

```
.
├── ui/                        # the window surface (no build step)
│   ├── index.html             # shell (no inline script: CSP default-src 'self')
│   ├── style.css              # Apple design language, reduced-motion/transparency fallbacks
│   ├── logic.js, app.js       # pure window logic + DOM/IPC wiring
│   ├── tests/                 # node --test, no dependencies
│   └── wheel/                 # the radial overlay: model.js, spring.js, wheel.js
│       ├── tests/             # layout, hit-test, springs
│       └── checks/            # real-Chromium interaction check over CDP
├── src-tauri/
│   ├── Cargo.toml, Cargo.lock # locked; CI builds --locked
│   ├── tauri.conf.json        # base config (platform-neutral)
│   ├── tauri.windows.conf.json# NSIS per-user, offline WebView2, VC runtime
│   ├── tauri.bundle.windows.conf.json  # engine payload, packaging command only
│   ├── tauri.macos.conf.json
│   └── src/
│       ├── engine/            # proc (spawn contract), vips, ffmpeg, soffice, doc,
│       │                      #   plan, convert, verify, stderr_rules, tags
│       ├── model/             # detect, matrix, types, settings, settings_schema, naming
│       ├── job/               # queue, outputs, publish, journal, paths, hash
│       ├── shell/             # ipc, summon, geometry, screen, hotkey, accelerator,
│       │                      #   grab, tray
│       ├── diag.rs            # --diagnostics: engine discovery + one conversion per family
│       └── lib.rs, main.rs
├── e2e/q3/                    # the end-to-end suite (one run per advertised direction)
├── e2e/hebrew/                # CLM substitution driver
├── fixtures/                  # 46-byte-verified fixtures + generated PROVENANCE.md
├── evidence/                  # measurement transcripts per card, q3-e2e/, ROUTES.md
├── docs/                      # see docs index below
├── scripts/                   # gates, generators, packaging, per-family test runners
├── packaging/macos/           # install-from-dmgs.sh, verify-pins.sh, sign-app.sh,
│                              #   install-engine-pack.command, README-pack.txt
├── spikes/                    # one directory per measurement spike, each with run.sh
└── .github/workflows/         # build.yml, macos-dmg.yml, macos-validate.yml
```

### Docs index

| file | what it is |
|---|---|
| `docs/USER-GUIDE.md` | the user-facing manual: install, formats, fidelity, output rules, privacy, limits |
| `docs/RELEASE-NOTES.md` | v1 release notes and the open-items list |
| `docs/DETECTION.md` | detection rules, fixture corpus, generated type → target matrix with evidence |
| `docs/ARCHITECTURE.md` | engineering contract: measured engine argv, pipeline, error model, settings |
| `docs/LICENSES.md` | generated dependency licences (R6 drift-gated) |
| `docs/licenses/THIRD-PARTY-NOTICES.md` | shipped notice text |
| `docs/shipped-inventory.tsv`, `docs/SHIPPED-INVENTORY.md` | every shipped binary, attributed |
| `docs/PACKAGING-WINDOWS.md` | NSIS installer, engine staging, licence gate |
| `docs/MACOS-PACKAGING.md` | universal DMG + engine pack, ad-hoc signing order, validation |
| `docs/MACOS-SIGNING.md` | the Developer ID / notarization order — documented, not used in v1 |
| `docs/DISTRIBUTION.md` | the 2026-11-02 internal-only signing decision |
| `docs/PRIOR_ART.md`, `docs/REFERENCE_POLICY.md` | binding rules for this project: reference-only prior art, mandatory cross-check |

---

## CI

`.github/workflows/build.yml` runs on every push:

| job | runner | proves |
|---|---|---|
| `build (ubuntu-latest)` | ubuntu | webkit2gtk deps, `cargo build --all-targets`, `cargo test` |
| `build (windows-latest)` | windows | same on MSVC |
| `build (macos-latest)` | macos | same on Apple silicon |
| `license-assert` | ubuntu | the ffmpeg gate rejects a GPL build (`--selftest`) and no shipped ffmpeg is GPL (`--scan`) |
| `policy-check` | ubuntu | deny-list scope, no C#/.NET or submodule, `docs/LICENSES.md` in sync, model boundary, generated tables in sync |
| `e2e` | ubuntu | the Q3 suite with test-only engines + the fault-injection gate |
| `report` | ubuntu | publishes the run's own verdict to the `ci-status` branch |

`.github/workflows/macos-dmg.yml` builds the app DMG and the engine pack on a macOS runner
(with a bundle-shape gate and a licence gate over the DMG payload).
`.github/workflows/macos-validate.yml` re-downloads those artifacts, pins their digests,
installs them and converts one file per family on arm64 and on an Intel runner.

Every job carries a `timeout-minutes` cap.

### Reading a run without Actions API access

Every run ends with the `report` job, which commits `ci-status/latest.json` and
`ci-status/<run_id>.json` to the `ci-status` branch; each matrix leg commits its own
`ci-evidence/latest.json` to its `ci-evidence-<os>` branch. A credential with only Contents
access can therefore verify a run:

```json
{
  "run_id": 1234567890,
  "sha": "…",
  "results": {
    "build_matrix_3_os": "success",
    "license_assert": "success",
    "policy_check": "success",
    "e2e": "success"
  }
}
```

---

## Packaging

**Windows** — NSIS, per user, no UAC:

```bash
scripts/stage-engines.sh --vips <spec-aom dist> --ffmpeg <dir> --lo-pack <LibreOffice pack> --out src-tauri
tauri build --target x86_64-pc-windows-msvc --config src-tauri/tauri.bundle.windows.conf.json
```

The `--config` overlay is what carries `externalBin` (ffmpeg, ffprobe) and `resources`
(`engines/`, `licenses/`). Without it you get an app-only installer (~210 MiB) whose every
conversion reports `MissingEngine`. With it, the installer is ~489 MiB and works offline.
The supported way to produce the payload is `scripts/stage-engines.sh`, and it refuses to
finish on a licence-gate failure (`docs/PACKAGING-WINDOWS.md`).

**macOS** — universal app DMG plus a separately signed arm64 engine pack, built by
`macos-dmg.yml`; the app bundle deliberately contains no engines, and a CI shape gate fails
the run if one appears in `Contents/Resources` (`docs/MACOS-PACKAGING.md` §1, §6).

No auto-update, no store listing, no notarization — see `docs/DISTRIBUTION.md`.

---

## Licensing posture

- Engines stay **separate processes** (LGPL/MPL sidecars). No engine crate is linked into
  the app binary; `Cargo.toml`/`Cargo.lock` carry no engine.
- The shipped ffmpeg is built without `--enable-gpl` / `--enable-nonfree`; the gate fails
  the build otherwise:

  ```bash
  bash scripts/license-assert.sh --scan       # gate the repo (CI)
  bash scripts/license-assert.sh --selftest   # prove the gate rejects a GPL build
  ```

- Every shipped file must attribute to a `docs/shipped-inventory.tsv` row (fail-closed):

  ```bash
  bash scripts/dist-license-gate.sh --scan <tree>
  bash scripts/dist-license-gate.sh --check-inventory
  ```

- `docs/LICENSES.md` is generated from the locked graph, never hand-edited:

  ```bash
  python3 scripts/gen-licenses.py          # regenerate
  python3 scripts/gen-licenses.py --check  # CI drift check
  ```

The app's own licence is a stakeholder decision recorded as TBD in `docs/LICENSES.md`.

---

## Working on this repo

`docs/PRIOR_ART.md` and `docs/REFERENCE_POLICY.md` are binding on every change: prior art is
a reference, never a source, and every work unit ends with the cross-check block from
`docs/REFERENCE_POLICY.md` §1. Read the policy before pushing.

- No force-push to `main`. No secrets in the tree — CI reads no repository secrets.
- Private repo: `lphan-personal/ez-convert`.
