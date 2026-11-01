# C4 spikes — output rules, publication and the undo journal

Everything here exists so the `SPEC.md` §5 rules are proven against real files, not just
against reasoning.

## What is proven

| claim | where |
|---|---|
| collision suffixing across 1000 simulated names, plus 1000 real files on disk | `src-tauri/src/model/naming.rs` tests |
| 8 concurrent publishers of the same name all get distinct files, none clobbered | `src-tauri/src/job/publish.rs` tests |
| undo deletes only this job's outputs (other jobs' outputs, user files and inputs survive) | `src-tauri/src/job/journal.rs` + `job/outputs.rs` tests |
| undo refuses a changed file, an input, a symlink, a re-pointed folder, and a closed window | `src-tauri/src/job/journal.rs` tests |
| the toggles change behaviour: same-folder vs `converted/`, chosen folder, keep vs strip | `job/outputs.rs` tests and `e2e.rs` E2E-2/E2E-4 |
| the strip toggle reaches a real engine and changes the bytes | `e2e.rs` E2E-1c vs E2E-2a |
| a batch undo also removes the empty `converted/` it created | `e2e.rs` E2E-4b |

## How to run

```bash
# unit + end-to-end (needs ffmpeg + ffprobe; default EZC_FFMPEG_DIR=/usr/local/bin)
scripts/c4-tests.sh

# end-to-end only
spikes/c4/run-e2e.sh

# unit only
scripts/c4-tests.sh --unit-only
```

`scripts/c4-tests.sh` generates a throwaway cargo package under `$TMPDIR` that compiles the
Tauri-free modules **by absolute path** (`model/`, `engine/`, `job/`), because `src-tauri`
cannot build on a host without GTK/WebKit dev packages. The test code is the repo's own
`#[cfg(test)]` modules — nothing is copied or re-implemented — and CI runs the same modules
through `cargo test` in the real crate on Linux, Windows and macOS.

## Files

- `e2e.rs` — the end-to-end scenario, included by the generated `tests/e2e.rs`. Uses the
  real `engine::convert::convert` (probe → plan → spawn → verify → tags), the real
  `job::outputs::run`, and a real journal on disk.
- `run-e2e.sh` — wrapper around `scripts/c4-tests.sh --e2e-only`.

## Traps this harness encodes

- The strip toggle is only observable through the **engine's** behaviour, so the fake-engine
  unit test marks the output and the e2e compares bytes and merged-tag counts from the real
  ffmpeg+lofty path. An assertion that only checks "a file exists" would pass even if the
  toggle were ignored.
- `std::fs::rename` overwrites on Unix. Any test that used it to "publish" would pass while
  the product silently destroyed files, which is why `publish` uses `hard_link`/`create_new`
  and why the e2e pre-creates a user file at the destination name.
- Expiry is tested by passing a timestamp, never by sleeping: a flaky time-based test would
  teach the team to ignore failures on the one guard that protects user data.
