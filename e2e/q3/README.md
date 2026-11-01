# Q3 — the end-to-end suite

One automated run per **advertised direction**, on real files, against real
engines. This is the evidence behind the claim "every advertised direction
works": a direction is advertised when `model::matrix` says `Proven` (images,
audio, video) or `engine::soffice::proven_routes()` lists it (documents), and the
suite iterates exactly those tables — there is no hand-maintained list to drift.

```
scripts/e2e.sh                 # media + documents
scripts/e2e.sh --strict        # a SKIP fails the run too
scripts/e2e.sh --media-only    # images/audio/video only
scripts/e2e.sh --docs-only     # LibreOffice only
scripts/e2e.sh --inject=drop-output   # prove the gate goes red
```

Engines are the engine layer's own overrides (`EZC_VIPS_DIR`, `EZC_FFMPEG_DIR`,
`EZC_SOFFICE_DIR`, `EZC_ENGINE_LIB_DIR`, `EZC_VIPS_PLUGIN_DIR`). Reports land in
`EZC_E2E_REPORT` (`report.tsv`, `report.json`, `matrix.md`).

## What a row asserts

| family | existence | content sanity | metadata policy (`SPEC.md` §5) |
|---|---|---|---|
| image | file present, non-empty | container matches the target; decoded geometry equals the source's (autorot applied); mean pixel is not blank | ICC kept by default where the container has a field, gone with `strip`; a stale EXIF orientation never survives an autorot |
| audio | file present, non-empty | `ffprobe`: codec, sample rate, channels, duration within 10 % of the source | tags carried and readable back from the container; no title tag after `strip` |
| video | file present, non-empty | `ffprobe`: codec, geometry, constant frame rate, duration within 20 % | `n/a` — SPEC §5 names no video metadata field |
| document | file present, non-empty | structural validator + page count / OOXML part / source marker / measured CSV naming | `n/a` — the policy's fields are image EXIF/ICC and audio tags |

Rows whose metadata policy cannot apply say `meta=n/a: <reason>`; they never
claim a check ran. Metadata policy is measured on a metadata-bearing source of
the same input type (built once from `fixtures/exif_icc.jpg` at run time), so the
assertion is real even when the plain fixture carries no EXIF/ICC.

## Why the controls exist

A suite that only ever prints PASS proves nothing. `controls()` plants a fault in
each assertion family and requires the check to reject it:

| control | planted fault | must be caught by |
|---|---|---|
| missing output | the output file is deleted | existence |
| zero-byte output | empty file | existence |
| wrong container | JPEG bytes under `.png` | container check |
| wrong geometry | a real 64×48 PNG, source says 200×400 | geometry check |
| text file named `.docx` | plain text | document validator |
| PDF with no pages | `%PDF` header, no page objects | page check |
| metadata dropped by a keep run | stripped copy offered as "should carry ICC" | metadata (keep) |
| metadata surviving a strip run | kept copy offered as "must be empty" | metadata (strip) |
| wrong audio codec | WAV offered as mp3 | audio codec |
| duration drift | 2 s source described as 10 s | audio duration |
| no video stream | a WAV offered to the video check | video stream |
| advertised direction with no fixture | `Unknown` type | fixture resolution |
| one-sided document advertisement | a pair added to one table only | document set equality |

If a control is not caught the suite prints `FAIL(control)` and exits non-zero.

## Preflight: the suite cannot run a stale claim

Before converting anything it asserts, and fails the run if any is untrue:

1. `Options::default()` keeps metadata (the policy's default);
2. no direction is listed twice;
3. every advertised input type has a fixture file in `fixtures/`;
4. every advertised media direction is routed by the planner, with the engine
   the matrix names;
5. the matrix's document rows and `soffice::proven_routes()` are the same set;
6. every advertised route's evidence file still exists and still contains its
   marker literal.

Check 4 is the one that caught a real defect on the first run: twelve advertised
identity targets (`jpg→jpg`, the 7 audio and 4 video identities) were refused by
`plan()` while the wheel offered them (`ARCHITECTURE.md` §17.15).
