# EZ Convert

Local file conversion desktop app. Tauri 2 shell, Rust core, radial "wheel" UI.

**Status: P0 skeleton.** This repo exists to unblock the OS-specific spike tasks (S1/S2/S3) by
giving them a real GitHub Actions surface. Nothing converts files yet — the UI is a static
placeholder and the Rust core is an empty contract.

Prior art, reference-policy and licensing rules live in `docs/PRIOR_ART.md`,
`docs/REFERENCE_POLICY.md` and `docs/LICENSES.md`. They are binding on every card: read
`docs/REFERENCE_POLICY.md` before pushing.

---

## Repo map

```
.
├── ui/index.html            # the wheel placeholder (static, no build step)
├── src-tauri/
│   ├── Cargo.toml           # tauri = "2", tauri-build = "2"
│   ├── Cargo.lock           # committed — CI builds with --locked
│   ├── build.rs             # tauri_build::build()
│   ├── tauri.conf.json      # frontendDist = ../ui, bundle.active = false (P2 owns bundling)
│   ├── icons/               # generated icon set (icon.png + sizes + icon.ico)
│   └── src/
│       ├── main.rs          # thin binary entry
│       ├── lib.rs           # ez_convert_lib::run() + command registry
│       └── engine.rs        # placeholder engine contract + unit tests
├── scripts/license-assert.sh# ffmpeg LGPL gate (CI-enforced)
├── scripts/make-icons.py    # regenerates src-tauri/icons (stdlib + ffmpeg only)
├── spikes/                  # one directory per spike, each with its own run.sh
└── .github/workflows/build.yml
```

**Icon gotcha:** `tauri::generate_context!()` requires `src-tauri/icons/icon.png` at
compile time even when `bundle.active` is `false` and `bundle.icon` is empty — a missing
icon fails the build with `proc macro panicked: failed to open icon …`. Regenerate the set with:

```bash
python3 scripts/make-icons.py src-tauri/icons   # needs ffmpeg on PATH
```

Bundle config, code signing, installers and the vendored motion layer are **not** here:
they belong to P2 (bundling) and U1 (apple-design motion vendoring).

---

## Running a spike on a runner

Spikes that need Windows, macOS, MS Office (`soffice`), or a cargo build **cannot** run in the
Linux agent container. They run on GitHub Actions instead. Every spike is dispatched through the
same workflow, `.github/workflows/build.yml`, using `workflow_dispatch` with a `spike_id`.

### Contract for a spike

A spike is a directory with an executable script:

```
spikes/<spike_id>/run.sh      # required
spikes/<spike_id>/out/        # optional: artifacts the job uploads
```

`run.sh` runs on `ubuntu-latest` with `rustc` (stable) and the Tauri Linux build deps already
installed. It must exit non-zero on failure. Anything written to `spikes/<spike_id>/out/` is
attached to the run as an artifact named `spike-<spike_id>`.

If `spikes/<spike_id>/run.sh` does not exist, the job fails loudly — that means the spike card
was dispatched before its script was written.

### Dispatching

```bash
# via API
curl -s -X POST \
  -H "Authorization: token $GITHUB_TOKEN" \
  -H "Accept: application/vnd.github+json" \
  https://api.github.com/repos/lphan-personal/ez-convert/actions/workflows/build.yml/dispatches \
  -d '{"ref":"main","inputs":{"spike_id":"s1"}}'
```

Or in the browser: **Actions → build → Run workflow → spike_id → Run workflow**.

Registered `spike_id` values: `none` (CI only, skip the spike job), `s0-runner-smoke`
(reference spike, proves the wiring), `s1`, `s2`, `s3`.

Adding a new spike id means adding it to the `options:` list under `workflow_dispatch` in
`build.yml` *and* creating `spikes/<id>/run.sh`.

### What every dispatch run also does

The dispatch run always runs `build` (3 OS matrix jobs) and `license-assert` in addition to the
requested spike, so a spike can never land on a broken trunk.

---

## CI jobs

| Job | Runner(s) | What it proves |
|---|---|---|
| `build (ubuntu-latest)` | ubuntu-latest | Tauri 2 + webkit2gtk-4.1 deps install, `cargo build --all-targets`, `cargo test` |
| `build (windows-latest)` | windows-latest | same on MSVC toolchain |
| `build (macos-latest)` | macos-latest | same on Apple silicon runner |
| `license-assert` | ubuntu-latest | the ffmpeg licence gate works (`--selftest`) and no shipped ffmpeg is GPL (`--scan`) |
| `policy-check` | ubuntu-latest | R1-R3 deny-list (prior-art names only in the policy docs), no C#/.NET or submodule, `docs/LICENSES.md` in sync with the locked graph (R6) |
| `spike (<id>)` | ubuntu-latest | the dispatched spike script (only on `workflow_dispatch` with `spike_id != none`) |
| `report` | ubuntu-latest | publishes the run's own verdict to the `ci-status` branch |

### Reading a run's verdict without Actions API access

Every run ends with the `report` job, which commits `ci-status/latest.json` and
`ci-status/<run_id>.json` to the `ci-status` branch, and every matrix leg commits its own
`ci-evidence/latest.json` to a `ci-evidence-<os>` branch:

```json
{
  "run_id": 1234567890,
  "run_url": "https://github.com/lphan-personal/ez-convert/actions/runs/1234567890",
  "event": "push",
  "ref": "main",
  "sha": "…",
  "results": {
    "build_matrix_3_os": "success",
    "license_assert": "success",
    "policy_check": "success",
    "spike": "skipped"
  }
}
```

```json
{
  "os": "windows-latest",
  "run_id": 1234567890,
  "job_status": "success",
  "cargo_build_ok": true,
  "test_summary": "test result: ok. 8 passed; 0 failed; …",
  "rustc": "rustc 1.99.1 …"
}
```

That's the machine-readable twin of the Actions UI: a credential with only Contents access can
verify a run — `build_matrix_3_os` is the matrix aggregate, and the three `ci-evidence-<os>`
branches prove each OS leg individually. When a leg fails, its `ci-evidence-<os>` branch also
carries the raw `build.log` / `test.log`.


### Cargo.lock

`src-tauri/Cargo.lock` is committed (locked against the stable toolchain), so CI builds with
`--locked`. If a dependency is added, regenerate it locally with
`cargo generate-lockfile --manifest-path src-tauri/Cargo.toml` and commit the result; the ubuntu
job also exports the lock file it built with as the `Cargo.lock` artifact, as a drift check.

---

## ffmpeg licensing gate

Distribution target is a signed consumer app, so we ship **LGPL-only** ffmpeg builds. The gate
treats an ffmpeg whose configuration contains `--enable-gpl` or `--enable-nonfree` as
"`enable-gpl` non-zero" and **fails the build**:

```bash
bash scripts/license-assert.sh --scan       # gate the repo (CI)
bash scripts/license-assert.sh --selftest   # prove the gate rejects a GPL build
```

Scanned locations (Tauri sidecar / vendored binary conventions):

```
vendor/ffmpeg/**    src-tauri/binaries/**    binaries/**    resources/ffmpeg/**
```

If no bundled ffmpeg exists yet (current skeleton state) `--scan` passes and says so. The
`--selftest` step is what keeps the gate honest meanwhile: it builds a fake GPL ffmpeg and a
fake LGPL ffmpeg and asserts the gate rejects the first and accepts the second.

Per policy R6 the engine stays a **separate-process sidecar** — no static link, no bundling into
the app binary.

**Open decision for P2:** if a conversion path genuinely requires a GPL component (e.g. `libx264`),
the policy has to be re-decided explicitly — either ship the GPL binary plus its source
obligations, or drop the codec. Do not flip the gate silently.

## Dependency licenses

`docs/LICENSES.md` is generated from the locked dependency graph, never hand-edited:

```bash
python3 scripts/gen-licenses.py          # regenerate after adding a dependency
python3 scripts/gen-licenses.py --check  # CI drift check
```

Rust dependency graph today: 407 packages, no GPL-only crate. The app's own licence is still a
stakeholder decision (recorded as TBD in that file).

---

## Local development

```bash
# Linux prerequisites (Debian/Ubuntu)
sudo apt-get install -y build-essential pkg-config libssl-dev libgtk-3-dev \
  libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev \
  libayatana-appindicator3-dev librsvg2-dev

cd src-tauri
cargo build
cargo test
cargo run        # opens the placeholder window (needs a desktop session)
```

Containerised agents cannot run this (no cargo, no desktop); use the CI dispatch above.

---

## Conventions

- No force-push to `main`. No secrets in the tree — CI reads no repository secrets.
- Private repo: `lphan-personal/ez-convert`.
