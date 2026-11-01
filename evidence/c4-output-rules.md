# EVIDENCE — C4: output-rules engine (naming, location, metadata, undo)

Card: `t_b2723c61` (C4). Date: 2026-11-01.
Host: this container (Debian 13, no display, no GTK/WebKit → the app crate cannot build here).
Engine for the end-to-end leg: **ffmpeg 7.x from `/usr/local/bin`** (audio family) plus
**lofty 0.22** for the tag merge — the same crates the product ships.

## 1. How it was run, and why it is legitimate

```bash
EZC_FFMPEG_DIR=/usr/local/bin bash scripts/c4-tests.sh
```

`scripts/c4-tests.sh` generates a throwaway cargo package in `$TMPDIR` that compiles the
repo's Tauri-free modules **by absolute path** — `model/{types,detect,matrix,settings,naming}`,
`engine/*`, `job/*` — so the tests that ran are the repo's own `#[cfg(test)]` modules and the
committed e2e file. Nothing was copied, re-implemented, or run from a parallel tree. CI runs
the same modules through `cargo test --locked` on Linux, Windows and macOS; this script is the
local path that exists because `src-tauri` needs GTK/WebKit to build.

## 2. Unit leg — 244 tests, 0 failed

```
--- unit leg: model + job suites ---
test result: ok. 244 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.34s
```

The C4 modules carry 52 of those:

| module | tests | what it owns |
|---|---|---|
| `job::outputs` | 14 | plan → convert → publish → journal, partial failure, toggles |
| `job::journal` | 13 | journal lifecycle, verified delete, expiry, startup sweep |
| `job::publish` | 9 | no-clobber placement, per-directory lock, temp area, dir creation |
| `job::queue` | 12 | folder drop depth, `include_subfolders`, admission (pre-existing, still green) |
| `job::paths` | 2 | cache override |
| `job::hash` | 2 | NIST SHA-256 vectors + streaming over a chunk boundary |

### Acceptance: collision suffixing across 1000 simulated names

Three tests, all green, in `model/naming.rs`:

- `a_thousand_colliding_names_are_all_distinct` — 1000 inputs all called `photo.png` from
  1000 subfolders landing in one chosen folder: 1000 unique names, `photo.webp`, `photo-1.webp`
  … `photo-999.webp`, then written to disk and the directory counted (1000 entries).
- `a_thousand_probes_skip_taken_names_and_fill_the_gaps` — 1000 probes against a taken-set
  with holes (plain name + every even suffix occupied): every result free, 1000 unique.
- `a_thousand_real_files_are_stepped_over` — 1000 real files on disk → `photo-1000.png`, and
  the original file is byte-identical afterwards.

### Acceptance: undo removes only files this job created

`job/outputs::undo_removes_only_the_files_this_job_published` (green) runs job A, then job B
into the same folder with a user file created between them, and asserts undo of B deletes
exactly `other.jpg` while job A's `photo.jpg`, the user's `user-note.jpg`, the decoy
`keep.txt` and both inputs survive. The journal tests add the four refusal cases: an edited
output (`Changed`, nothing deleted), an input path (`Protected`), a symlinked output
(`Protected`), a re-pointed output folder (`Protected`), and a closed window (`Expired`,
files kept).

## 3. End-to-end leg — real ffmpeg, 13/13 checks

```
PASS E2E-1a a single file lands next to its source — published=1 path=.../keep/in/tone.wav failures=[]
PASS E2E-1b the output is a validated WAV, and the input is untouched — size=105970 first4=[82, 73, 70, 70] input_unchanged=true
PASS E2E-1c metadata is kept by default (tags merged by lofty) — tags_merged=Some(5)
PASS E2E-1d the journal is committed with the published hash — state=Committed outputs=1 sha256=507c302326467f753982d5a74061ea9d5f020b7f72ef00332413a9549413b556
PASS E2E-2a settings.metadata.strip=true reaches the engine (no tag merge) — published=1 tags_merged=None failures=[]
PASS E2E-2b the stripped output is a different, still-valid WAV — strip_len=105918 keep_len=105970 identical=false
PASS E2E-3a an existing file is stepped over, never replaced — planned_renamed=true published=.../collide/in/tone-1.wav user_file_intact=true
PASS E2E-3b undo deletes the file this job wrote and nothing else — deleted=Ok([".../collide/in/tone-1.wav"]) user_file_kept=true input_kept=true
PASS E2E-4a a 3-file batch goes into converted/ — published=3 wavs_in_converted=3 created_dirs=[".../batch/in/converted"]
PASS E2E-4b undo removes the outputs and the empty converted/ folder — deleted=3 converted_gone=true inputs_kept=true
PASS E2E-5a undo inside the window deletes the output and keeps the input — deleted=Ok([".../keep/in/tone.wav"]) input_kept=true
PASS E2E-6 a second undo has nothing to delete — second_undo=Err(NotFound(".../c4_e2e_1-1793568771372.json"))
PASS E2E-5b after the 10-second window undo refuses and the file is kept — result=Err(Expired { job_id: "c4_e2e_5", .. }) kept=true

13 passed, 0 failed (13 checks)
```

What those checks actually prove, and what would have passed if the code were wrong:

- **E2E-1b** runs the shared `engine::verify` validator (RIFF/WAVE magic + ffprobe), not a
  file-exists check; a zero-byte or mislabelled output fails here.
- **E2E-1c vs E2E-2a** is the metadata toggle with teeth: keeping merges the source's 5 ID3
  tags, stripping skips the merge (`None`) and the bytes differ by 52 bytes. A toggle that
  never reached the engine would report the same tag count twice.
- **E2E-3a/3b** pre-create the user's own `tone.wav` and prove it survives byte-for-byte,
  that the job wrote `tone-1.wav`, and that undo deleted only that file.
- **E2E-4a/4b** prove the 3-file batch rule and that undo also removes the empty `converted/`
  it created — while leaving all three inputs.
- **E2E-5b** proves expiry with an injected timestamp, not a sleep, so the guard cannot be
  tested-into-passing by a fast machine.

## 4. Toggles change behaviour — where each is proven

| `SPEC.md` §5 rule | default | toggle | proof |
|---|---|---|---|
| same folder | yes | `output.location = chosen_folder` | `the_chosen_folder_toggle_redirects_every_output`; E2E-4a (batch) |
| batch → `converted/` at 3+ | on | `output.use_batch_subfolder = false`, `batch_threshold` | `three_files_go_to_the_batch_folder_and_the_toggle_moves_them_back`, `two_files_stay_in_the_same_folder_even_with_the_batch_rule_on` |
| never overwrite | always | `never_overwrite` is locked on (`ARCHITECTURE.md` §15) | `an_existing_output_is_suffixed_and_the_original_is_untouched`; E2E-3a |
| keep metadata | yes | `metadata.strip` + per-row override | `the_metadata_toggle_reaches_the_engine_for_every_file`; E2E-1c/E2E-2a/E2E-2b |
| folder drop 1 level deep | yes | `folder_drop.include_subfolders` | `a_dropped_folder_is_enumerated_one_level_deep_unless_subfolders_are_included` + existing queue tests |
| 10-second undo | 10 s | `undo.window_seconds` (clamped 1–60) | `undo_is_refused_once_the_window_closed`, `undo_inside_the_window_still_works_at_ten_seconds`; E2E-5b |

## 5. What is NOT proven here

- **Windows and macOS**: the placement uses `hard_link` + `create_new`, both of which behave
  per-platform (NTFS hard links, Windows `AlreadyExists`), and the tests are written with
  platform-absolute paths — but this host is Linux. The three-OS `cargo test --locked` CI run
  is the evidence for those platforms; at the time of writing the run for pushed head
  `87eac51` had not yet published a verdict to `ci-status` (the last recorded run is
  `41283721257` for `8608125`, which predates this change). **Unverified for this head.**
- **vips image targets and LibreOffice documents through this path**: the e2e uses the audio
  family because ffmpeg is the only engine installed on this host. The output-rules code is
  engine-agnostic (the converter is a closure), so the same path is exercised, but a
  `png → webp` batch `undo` on a real vips build is not measured here.
- **Multi-file outputs** (xlsx → one CSV per sheet): `publish::publish_exact` exists for the
  engine-chosen-name case and is unit-tested, but no engine leg produces multiple outputs yet,
  so `job::outputs::run` only handles one output per input today. Recorded as a gap, not
  glossed: the C3 document path's per-sheet naming needs wiring into `run` when the worker
  lands.
- **Concurrency across processes**: the per-directory lock is in-process by design
  (documented in `ARCHITECTURE.md` §9); another program racing us is handled by the
  no-clobber placement, which is tested, but not by a cross-process lock.

## 6. Reproduce

```bash
EZC_FFMPEG_DIR=/usr/local/bin bash scripts/c4-tests.sh           # unit + e2e
bash scripts/c4-tests.sh --unit-only                             # unit only
spikes/c4/run-e2e.sh                                             # e2e only
python3 scripts/gen-licenses.py --check                          # R6 drift gate
```
