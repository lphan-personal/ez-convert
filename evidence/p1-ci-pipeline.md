# P1 — the Windows/macOS CI pipeline, the 2026-11-01 blackout, and the green run that closed it

Owner: smith (card `t_648a071a`). Status at time of writing: **done** — green `build
(windows-latest)` and `build (macos-latest)` on run
[41367754978](https://github.com/lphan-personal/ez-convert/actions/runs/41367754978)
(sha `9cd3686f`), both unsigned artifacts downloaded and their digests re-verified here. §4 keeps
the blackout record because it is the reason the evidence is branch-published; §6 is the green-run
proof and §7 the fix it forced.

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

## 3. Free-tier cost of a run (measured on run 59, plus the multiplier arithmetic)

Wall-clock per job on run `41367754978` (all jobs start within a second of 00:34:02Z):

| job | wall clock | billed multiplier (private repo) |
|-----|-----------|----------------------------------|
| `license-assert` | 29 s | 1× |
| `policy-check` | 20 s | 1× |
| `build (ubuntu-latest)` | 1 min 49 s | 1× |
| `build (macos-latest)` | 2 min 24 s | 10× → 24 min |
| `build (windows-latest)` | 12 min 18 s | 2× → 25 min |

So a push with the platforms green costs ≈ **51 billed minutes** — the macOS 10× multiplier and the
cold-ish Windows leg dominate, not the Linux legs. That is ~40 pushes against the 2,000-minute
private-repo allowance; `Swatinem/rust-cache@v2` keeps it from growing with repeat pushes.

The cost that actually threatened the budget was not the matrix: run 59's `e2e` leg sat in one step
from 00:34:43Z and was still running at 02:10Z (**95+ minutes at 1×**) with no `timeout-minutes`
anywhere outside the build matrix — see §7.

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

## 5. What closed this card

A green run in the Actions UI with `build (windows-latest)` and `build (macos-latest)` green, and
the artifacts `ez-convert-Windows-unsigned` / `ez-convert-macOS-unsigned` downloadable. That
happened on run 59 — §6 has the per-step and per-digest evidence.

The blackout ended by itself: GitHub began assigning runners again at ~00:34Z on 2026-11-02 and the
first commit it ran (the `9cd3686f` typo fix) went green on all three OSes. The owner actions that
*would* have unblocked it remain the lever for the next time the allowance runs out:

1. GitHub → Settings → Billing → Actions: raise the spending limit / add included minutes.
2. Make the repo public (standard runners are free for public repos) — a disclosure decision,
   not an infrastructure one; R1-R6 policy applies.
3. Accept a cheaper matrix: macOS only on release/dispatch, Ubuntu+Windows on every push.

None of the three was needed, and none is applied: the pipeline still runs both platforms on every
push, as the card asked.

## 6. The green run: `41367754978` (run 59), sha `9cd3686f`

Run: <https://github.com/lphan-personal/ez-convert/actions/runs/41367754978>
Event `push` on `main`, attempt 1, created 2026-11-02T00:33:10Z.

| job | html_url | conclusion | wall clock |
|-----|----------|-----------|------------|
| `build (windows-latest)` | …/job/125031674371 | **success** | 12 min 18 s |
| `build (macos-latest)` | …/job/125031674100 | **success** | 2 min 24 s |
| `build (ubuntu-latest)` | …/job/125031674130 | success | 1 min 49 s |
| `license-assert` | …/job/125031674190 | success | 29 s |
| `policy-check` | …/job/125031674118 | success | 20 s |
| `e2e` | …/job/125031673943 | still running at 02:10Z | uncapped — §7 |

Every step on both shipping platforms succeeded, in this order: job-start marker → Rust toolchain
→ cargo cache → `cargo build` → `cargo test` → pinned Tauri CLI → licence scan → licence **selftest**
→ `cargo tauri build --no-bundle --ci` → stage + manifest → upload → artifact record → evidence.

**The licence assertion, on the runners** (from each job's own log, and from the
`license-selftest.log` the job ships inside the artifact):

```text
selftest 1/2: GPL build must be REJECTED
FAIL vendor/ffmpeg/gpl/ffmpeg: enable-gpl/nonfree is non-zero — not shippable
       --enable-gpl
OK   vendor/ffmpeg/lgpl/ffmpeg: LGPL-only configuration
license-assert: 2 bundled ffmpeg checked, 1 rejected
selftest 1/2: PASS (gate exited 1 on a GPL build)
selftest 2/2: LGPL build must be ACCEPTED
OK   vendor/ffmpeg/lgpl/ffmpeg: LGPL-only configuration
license-assert: 1 bundled ffmpeg checked, 0 rejected
selftest 2/2: PASS (gate exited 0 on an LGPL build)
```

That is the card's "demonstrated failing on a deliberately bad ffmpeg", executed on Windows and on
macOS. **Honest limit:** the accompanying `--scan` step prints `no bundled ffmpeg under [vendor/ffmpeg
src-tauri/binaries binaries resources/ffmpeg] — nothing to gate yet (skeleton stage)` — a CI checkout
carries no engine binaries, so the scan is vacuous there and the *selftest* is what makes the gate
non-vacuous. The real sidecars are gated by `scripts/dist-license-gate.sh --check-inventory` in the
`license-assert` job (also green in this run) against `docs/shipped-inventory.tsv`.

**Artifacts, downloaded and re-hashed here** (not just asserted):

| artifact | id | zip bytes | file | file bytes | sha256 |
|---|---|---|---|---|---|
| `ez-convert-Windows-unsigned` | 13108112038 | 2 082 168 | `ez-convert-windows-x86_64.exe` | 4 698 624 | `5da491368bd0c816af0408e403b2dffa4705b42ecc86c44889d48acdf2f24b66` |
| `ez-convert-macOS-unsigned` | 13107514457 | 2 177 233 | `ez-convert-macos-arm64` | 5 418 352 | `0ad2299cfd86347c1baf79bfb51f626503c508b8ea970952e40dda2d3bcf7805` |

Both `GET …/actions/artifacts/<id>/zip` returned HTTP 200; each zip contains the binary, a
`manifest.json` (`signed: false`, `bundle: false`, tauri-cli 2.12.4, rustc 1.99.1, run id/url, sha)
and `SHA256SUMS.txt`. The recomputed binary digest and size equal the manifest in both cases, so the
artifact is the binary the job staged. Each platform's own record is on its branch:
`ci-artifacts-windows-latest/latest.json` and `ci-artifacts-macos-latest/latest.json`.

Not green, stated plainly: the run's overall conclusion is still `in_progress` because the `e2e` job
(another card's suite) had not finished after 95 minutes; the `report` job needs it, so `ci-status`
still holds run 58's verdict. `macos-dmg.yml` is a separate workflow and a separate card.

## 7. Fix forced by the run: no job outside the matrix had a time cap

Only `build` carried `timeout-minutes` (30). `e2e`, `license-assert`, `policy-check`, `spike` and
`report` had none, and run 59 proved the consequence: one step of `e2e` ran, uncapped, for 95+
minutes on a 1× runner, which also means `report` (needs: e2e) could not publish a verdict for the
run at all. Every job now has a cap — `build` 30, `e2e` 30, `policy-check` 30, `spike` 30,
`license-assert` 20, `report` 15 — verified by parsing the file (`yaml.safe_load`, six jobs, none
without a cap). Pushed as a docs-and-timeout commit with `[skip ci]` so it costs no matrix minutes;
it takes effect on the next ordinary push. The e2e job itself is another card's to fix (why it takes
that long / whether it hangs), and it is reported on this card rather than changed here.
