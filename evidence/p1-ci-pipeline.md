# P1 — the Windows/macOS CI pipeline, and why no job has run since 2026-11-01T07:13Z

Owner: smith (card `t_648a071a`). Status at time of writing: **pipeline built and committed,
blocked on the account being able to start a job** (see §4).

## 1. What the pipeline does now (`.github/workflows/build.yml`, commit `7b7bef1`)

`build` is a three-OS matrix. Ubuntu keeps the original build+test legs; **windows-latest and
macos-latest additionally**:

| step | what it proves |
|------|----------------|
| `Record job start to ci-start-<os>` | the job actually started. Pushed before any build work, so a run that dies is distinguishable from a run that never started (see §4). |
| `Install the Tauri CLI (pinned prebuilt)` | the CLI is a *build-time tool*, never shipped. Fetched as the official prebuilt release `tauri-cli-v2.12.4` and sha256-verified (`3e8687fb…` win x86_64, `f71a34ad…` mac aarch64, `164d184c…` mac x86_64) instead of `cargo install` burning minutes on every run. |
| `Licence assertion - bundled ffmpeg must be enable-gpl=0` | `scripts/license-assert.sh --scan` over anything this checkout would bundle. |
| `Licence assertion - a deliberately GPL ffmpeg must be rejected` | `scripts/license-assert.sh --selftest`: the gate must exit 1 on a `--enable-gpl` ffmpeg and 0 on an LGPL one. This is the demonstration the card asks for, on both shipping platforms. |
| `Build the Tauri app (release, unsigned, no installer yet)` | `cargo tauri build --no-bundle --ci`. `bundle.active` is false in `tauri.conf.json`, so the artifact is the app binary; installers/DMG are the P3 packaging cards, not this one. |
| `Stage the unsigned artifact + manifest` | copies the binary, writes `SHA256SUMS.txt` and `manifest.json` (os, file, size, sha256, `signed: false`, tauri-cli version, rustc, run id/url, commit). |
| `Upload the unsigned artifact` | `actions/upload-artifact@v4`, name `ez-convert-<OS>-unsigned`, `if-no-files-found: error`, 30-day retention. |
| `Publish the artifact record to ci-artifacts-<os>` | records the upload step's own `artifact-id`/`artifact-url` plus the file digest on a branch, so the upload is auditable without the Actions API. |

`Swatinem/rust-cache@v2` (already present) caches the registry and `src-tauri/target` for both
platforms, and the job carries `timeout-minutes: 30` so a hung runner cannot burn an hour. The
`report` job now folds the e2e job into `ci-status`.

### Reading CI evidence with a Contents-scope token

The Actions REST/GraphQL run APIs return `403 Resource not accessible by personal access token`
for this account's token, so every job publishes its own evidence to a branch:

| branch | written by | content |
|--------|-----------|---------|
| `ci-start-<os>` | build matrix, first step | `{"os", "run_id", "run_url", "sha", "started_at"}` |
| `ci-evidence-<os>` | build matrix, last step (`if: always()`) | job status, test summary, cargo log on failure |
| `ci-artifacts-<os>` | build matrix, after upload | artifact name, id, url, file size + sha256, job status |
| `ci-status` | `report` job | one JSON per run plus `latest.json` with every job result |

## 2. What was verified locally (this container, no CI minutes spent)

- `yaml.safe_load` of the workflow parses; all six jobs and every step are well-formed.
- `bash -n` over **every** `run:` block in the file: 0 syntax failures.
- The staging step was executed for both OS branches against a fake
  `src-tauri/target/release/ez-convert[.exe]`: it refuses to stage when the binary is missing
  (exit 1, `::error::`), and produces valid `manifest.json`, `SHA256SUMS.txt` and `$GITHUB_ENV`
  entries when it is present.
- The artifact-record step was executed against a stubbed `git`: valid JSON on both the success
  and the failure path (empty artifact id, `job_status: failure`).
- The three pinned Tauri CLI digests were re-downloaded and `sha256sum -c`-verified.
- `scripts/license-assert.sh --selftest` was run here (2/2, GPL fixture rejected) and `--scan`
  reports "no bundled ffmpeg … nothing to gate yet" — this repo carries no engine binaries;
  the shipped sidecars live in `docs/shipped-inventory.tsv` and are gated by
  `scripts/dist-license-gate.sh` in the `license-assert` job.

## 3. Free-tier cost of a run (estimate, stated as an estimate)

Public billing data is not readable with this token, so this is arithmetic from the matrix
shape, not a measurement: ubuntu ≈ 1× and windows ≈ 2× against macOS ≈ **10×** the wall-clock
minutes. A ~6 min ubuntu leg + ~10 min windows leg + ~10 min macOS leg bills roughly
`6 + 20 + 100 ≈ 126` minutes per push, of which ~80% is macOS. On the 2,000-minute free
allowance for a private repo that is ~16 pushes/month.

## 4. The blackout: runs are created, jobs never start

Facts, in order:

1. The last run that published anything is `41283721257` (run 24), sha `8608125`, `ci-status`
   written 2026-11-01T07:13:56Z, all jobs `success`.
2. Between then and 2026-11-02T00:21Z, main received **39** commits (u1/u2/u3, c1..c4, q1..q4,
   q2-dec, q3-win, q3-avif, p3-mac …) and **not one** updated `ci-status`, `ci-evidence-*` or
   `ci-artifacts-*`. Only local evidence was produced for those cards.
3. Workflow *runs* are nevertheless being created: GitHub GraphQL
   `repository.object(expression:"<sha>") … checkSuites { totalCount }` returns 2 for
   `8608125`, `ab17563`, `2c2c4ba`, `7b7bef1`, `6888ca4` and 3 for `4339541` (the commit that
   added a second workflow file, `macos-dmg.yml`). Node-level detail is `FORBIDDEN` for this
   token, so conclusions cannot be read, only the suite count.
4. A job-start marker was added as the **first** step of the build matrix (commit `6888ca4`,
   committed 2026-11-02T00:16:29Z). Four commits from other cards landed after it, all with run
   suites created; by 00:24:45Z no `ci-start-ubuntu-latest`, `-windows-latest` or `-macos-latest`
   branch existed — no job was ever handed a runner.
5. `GET /repos/lphan-personal/ez-convert/actions/permissions` → `{"enabled": true,
   "allowed_actions": "all"}`: Actions is enabled at the repo level. The repo is private, not
   archived, not disabled.

Reading: the trigger fires and runs are created, but **no job is ever handed a runner**, which
is the observable shape of the account-level Actions allowance/billing refusing jobs (private
repo, macOS billed at 10×, 24 runs burned on the first day of the month). It is *not* a YAML
problem — the workflow parses, and the failure predates this card by 17 hours.

## 5. What closes this card

A green run in the Actions UI with `build (windows-latest)` and `build (macos-latest)` green,
and the artifacts `ez-convert-Windows-unsigned` / `ez-convert-macOS-unsigned` downloadable.
The moment jobs run again, `ci-start-*`, `ci-evidence-*`, `ci-artifacts-*` and `ci-status` all
update by themselves and the run URL is in every one of them.

Owner actions that unblock it (any one):

1. GitHub → Settings → Billing → Actions: raise the spending limit / add included minutes.
2. Make the repo public (standard runners are free for public repos) — a disclosure decision,
   not an infrastructure one; R1-R6 policy applies.
3. Accept a cheaper matrix: macOS only on release/dispatch, Ubuntu+Windows on every push
   (~40 billed minutes per push instead of ~126). This contradicts the card's acceptance as
   written (both platforms on a green run), so it needs the owner's call.
