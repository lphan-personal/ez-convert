# macOS validation — install, launch, engine discovery, conversion (2026-11-02)

Acceptance evidence for the two shipped macOS artifacts. Everything below is a
measurement on a macOS runner, not a claim from a build log: the DMGs are
downloaded from the run that produced them, mounted, installed the way a user
installs them, and then exercised.

- **Artifacts**: `macos-app-dmg` + `macos-engine-pack` from run **41388618088**
  (`macos-dmg`, head `220db32`). App DMG sha256
  `9002d003…e987144`, pack DMG sha256 `5f6c4105…998eb9c`, both re-checked
  against their published sidecars inside the install path on every run below.
- **Workflow**: `.github/workflows/macos-validate.yml` (workflow_dispatch,
  `source_run` + `intel_label` inputs). It reuses artifacts, so a validation
  costs minutes instead of another build.
- **Runs**: **41392415715** (head `c61a7fc`) — the architecture matrix;
  **41392784882** (head `9252832`) — the same matrix plus the image-digest pin
  check (warden F10). Both are green on all three jobs
  (`arm64`, `intel`, `report`).
- **Evidence channel**: branch `macos-status`, directories
  `macos-status/run-41392784882/validate-arm64/`, `…/validate-intel/`,
  `…/validate/` (facts.json), including the screenshots.

## Verdict

| Check | arm64 native (macOS 26.6.2) | arm64, app forced to x86_64 (Rosetta) | Intel native (macOS 15.7.9) |
|---|---|---|---|
| image digest pinned before mount | PASS (`app-dmg.sha256`) | — | PASS |
| pack digest pinned before mount | PASS (`…dmg.sha256`) | — | PASS |
| installs from DMG to `/Applications` | PASS, 12 MB | — | PASS, 12 MB |
| `codesign --verify --strict` on installed bundle | PASS | — | PASS |
| `lipo -archs` on installed binary | `x86_64 arm64` | `x86_64 arm64` | `x86_64 arm64` |
| app bundle carries no engines | PASS | — | PASS |
| 24 pack symlinks survive install, none dangling | PASS | — | PASS (`verify` PASS ×24) |
| `engines/soffice/program` → `LibreOffice.app/Contents/MacOS` | PASS | — | PASS |
| engine discovery (5 of 5 resolved) | PASS | PASS | PASS |
| 339 Mach-O signatures + no build-machine paths | PASS | — | PASS (codesign works cross-arch) |
| conversion: image (png→jpg) | PASS 2,781 B | PASS 2,781 B | **FAIL** `Bad CPU type in executable (os error 86)` |
| conversion: audio (wav→mp3) | PASS 8,579 B | PASS 8,579 B | **FAIL** same |
| conversion: video (mp4→webm) | PASS 2,278 B | PASS 2,278 B | **FAIL** same |
| conversion: document (docx→pdf, LibreOffice) | PASS 25,546 B | PASS 25,546 B | **FAIL** `The result didn't pass a check, so it wasn't saved` |
| windowed app launches from `/Applications` | PASS (`lsappinfo` registers `EZ Convert`, in front) | — | PASS |
| screenshot | captured, 1024×768 | — | captured, 1920×1080 |

`VALIDATION` lines as published by the runs:

```
run 41392784882 / validate-arm64
  VALIDATION arm64 install=pass launch=pass engine-discovery=pass conversion=pass (4/4)
  VALIDATION rosetta-x86_64 launch=pass conversion=pass
  VALIDATION gui-window screenshot=captured

run 41392784882 / validate-intel
  VALIDATION intel install=pass launch=pass engine-discovery=pass conversion=blocked
```

## The Rosetta result is the interesting one

The universal app's **x86_64 slice converts 4/4 on Apple Silicon
(`sysctl.proc_translated` = 1 in the child)**. The engines are arm64-only and the
translated parent spawns them natively, so an Apple Silicon Mac that ends up
running the Intel slice still converts. Evidence: `rosetta.log`
(`proc_translated(shell)=0`, `proc_translated(child)=1`) and `diag-rosetta.log`
(`SUMMARY families=4 pass=4 fail=0`).

That is *not* the same thing as Intel hardware, which is why the second job runs
on a real Intel runner.

## The Intel limitation, measured (not assumed)

On `macos-15-intel` (macOS 15.7.9, x86_64) everything that does not execute an
engine passes: digest pin, install, signature, symlink inventory, manifest of
9,304 files, 339 Mach-O signature/linkage checks, and discovery of all five
engine paths. Execution is what fails, because the shipped pack is **arm64
only**:

```
FAIL  pack vips does not run (exit 126): …/engines/vips/vips: Bad CPU type in executable
FAIL  pack ffmpeg does not run (exit 126): … Bad CPU type in executable
FAIL  pack soffice does not run (exit 126): … Bad CPU type in executable
      shipped-ffmpeg licence probe skipped: no buildconf output (exit 126)
FAMILY image  FAIL 0 1 Bad CPU type in executable (os error 86)
FAMILY audio  FAIL 0 0 Bad CPU type in executable (os error 86)
FAMILY video  FAIL 0 0 Bad CPU type in executable (os error 86)
FAMILY document FAIL 0 1 The result didn't pass a check, so it wasn't saved. The original is unchanged.
SUMMARY families=4 pass=0 fail=4
```

So on an Intel Mac today: the app installs, launches, and finds the pack; it
cannot convert, and it says so per family instead of exiting 0 over an empty
output. An `x86_64` engine pack (`scripts/mac-engine-pack.sh --arch x86_64`,
never run) is the missing artifact, filed as follow-up work.

## Defect this card found and fixed

The first Intel run exposed a **false PASS** in `scripts/verify-engine-pack.sh`:
vips and ffmpeg were reported as failures, while soffice printed

```
PASS  pack soffice runs: … Bad CPU type in executable
```

because that check only required non-empty output. `check_exec` now asserts the
**exit status** for all three engines and refuses to PASS a licence probe it
could not run. Regression test: `scripts/test-verify-engine-pack.sh` (extracts
`check_exec` from the shipped script and proves three cases — runs, cannot
execute, silent exit 0) — it runs in `macos-dmg.yml`'s pack job and in this
workflow, on every run.

## What is asserted vs recorded

- arm64 native is **asserted**: install, `codesign`, both slices, symlink count,
  5/5 discovery, 4/4 conversions. Any regression fails the job.
- Rosetta and Intel are **classified**: 4/4 → `pass`, 0/4 → `blocked` (the
  documented arm64-only-pack limitation), anything in between → `::error` and a
  red job, because a partial result is not a documented state.

## Re-running

```
python3 ghx.py dispatch macos-validate.yml main source_run=<run id> intel_label=macos-15-intel
```

`macos-13` no longer exists (retired Dec 2025); the current Intel labels are
`macos-15-intel` (until Aug 2027) and `macos-26-intel`.
