# Spikes

OS-specific validation work that cannot run in the Linux agent container.
Each spike is one directory with an executable `run.sh`; CI executes it on
`ubuntu-latest` via `workflow_dispatch` (see the top-level README).

```
spikes/<spike_id>/run.sh    required — exit non-zero on failure
spikes/<spike_id>/out/      optional — uploaded as artifact spike-<spike_id>
```

## Contract

A `run.sh` receives:

| Var | Meaning |
|---|---|
| `SPIKE_ID` | the dispatched id, e.g. `s1` |
| `SPIKE_ARGS` | free-form string from the `spike_args` dispatch input |

The runner already has: `rustc` (stable), the Tauri Linux system deps,
`ffmpeg`/`ffprobe`, `python3`, `git`, `curl`. Anything else must be installed
inside `run.sh` (apt/npm/cargo), and the script must be idempotent.

`run.sh` should:

1. write machine-readable results to `out/result.json` (what was tested, versions, verdicts);
2. copy or synthesize any proof (probe output, timings, sample output) into `out/`;
3. exit 0 only on a real pass — no `|| true` around the assertion under test.

`out/` is git-ignored.

## Registering a new spike

1. Add `spikes/<id>/run.sh` (chmod +x — CI calls `bash`, so the bit is cosmetic but conventional).
2. Add `<id>` to the `spike_id` `options:` list in `.github/workflows/build.yml`.
3. Dispatch it, then paste the run URL into the spike card.

Dispatching an id whose `run.sh` is missing fails the job on purpose.

## Harness-only tooling (never shipped)

`spikes/s3` verifies converted output by reading it back with Python libraries.
These are installed inside the spike job only — they are **not** app
dependencies and must never enter the Tauri bundle or `docs/LICENSES.md`'s
generated graph (which covers the Rust build only):

| package | version | licence | used for |
|---|---|---|---|
| `python-docx` | 1.2.0 | MIT | build the docx fixture, read back `html -> docx` |
| `openpyxl` | 3.1.5 | MIT | build the xlsx fixture, read back `csv -> xlsx` |
| `python-pptx` | 1.0.2 | MIT | build the pptx fixture |
| `pypdf` | 6.21.4 | BSD-3-Clause | page count / text / font extraction from PDFs |

`warden` sees this list in the S3 hand-off; adding a harness dependency means
adding a row here in the same commit.

## Present spikes

| id | purpose |
|---|---|
| `s0-runner-smoke` | reference implementation: proves the dispatch → runner → artifact path works |
| `s1` | reserved — attach when the card lands |
| `s2` | reserved — attach when the card lands |
| `s3` | LibreOffice document direction matrix: 7 directions, output asserted + content-checked, cold/warm timings, no-output controls (`spikes/s3/run.sh`) |
