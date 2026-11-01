# Q3 — end-to-end suite: one real run per advertised direction

Card `t_9a4c622d`. Suite: `scripts/e2e.sh` (code in `e2e/q3/`, README there).
Transcripts: `evidence/q3-e2e/` (`transcript.txt`, `report.tsv`, `report.json`,
`matrix.md`, `fault-injection.txt`).

**Result on this container (Linux leg, 2026-11-01):**

```
advertised=154  pass=148  fail=0  skip=6  controls=13/13 caught
SUMMARY GREEN for the 148 directions this dist can run; 6 SKIP (engine availability)
```

| family | advertised | pass | fail | skip |
|---|---|---|---|---|
| image | 37 | 31 | 0 | 6 |
| audio | 49 | 49 | 0 | 0 |
| video | 20 | 20 | 0 | 0 |
| document | 42 | 42 | 0 | 0 |

The 6 SKIPs are the six `→ avif` directions: this test dist has `libheif` and an
AV1 *decoder* but no AV1 *encoder*, so `heifsave` answers `Unsupported compression`.
They stay `Proven` on the Windows dist (`V8`, `spikes/c2/EVIDENCE-WINDOWS.md`) and
the row says so; nothing is silently green. `--strict` turns any SKIP into a red run.

## What "advertised" means here, mechanically

The suite has no direction list. It reads the product:

- media rows come from `model::matrix::routes_for(ty)` filtered on `Proven`;
- document rows come from `engine::soffice::proven_routes()`;
- preflight asserts those two document sets are **equal**, that every advertised
  direction has a fixture on disk, that `plan::engine_for` routes it with the
  engine the matrix names, and that every route's evidence file still contains
  its marker literal.

Add a `Proven` cell and the suite grows a row. Break a fixture or a transcript
and the run goes red before a single conversion happens.

## Per-direction assertions

Each row proves three things (details per family in `e2e/q3/README.md`):

1. **existence** — the output file exists and is not empty;
2. **content sanity** — a property a broken conversion cannot fake: decoded
   geometry + non-blank mean (images), `ffprobe` codec + rate + duration
   (audio/video), page count / OOXML container part / source text marker /
   the measured `<stem>-<sheet>.csv` naming (documents);
3. **metadata policy** (`SPEC.md` §5) — EXIF/ICC kept by default and gone with
   `strip` on every image direction whose input type can carry them (measured on
   a metadata-bearing source of the same type, built from `fixtures/exif_icc.jpg`
   at run time), audio tags carried and read back from the container vs absent
   after `strip`. Where the policy cannot apply the row says
   `meta=n/a: <reason>` (GIF has no ICC field; SPEC §5 names no video metadata
   field; document properties are not the policy's fields).

## The harness is proven able to fail

13 planted faults, one per assertion family, each required to be **caught**
(`transcript.txt`, "harness controls"):

| control | planted fault | caught by |
|---|---|---|
| missing output | file deleted | existence |
| zero-byte output | empty file | existence |
| wrong container | JPEG bytes named `.png` | container check |
| wrong geometry | 64×48 PNG, source says 200×400 | geometry |
| text file named `.docx` | plain text | document validator |
| PDF with no pages | `%PDF` header only | page count |
| metadata dropped by a keep run | stripped copy, "should carry ICC" | metadata keep |
| metadata surviving a strip run | kept copy, "must be empty" | metadata strip |
| wrong audio codec | WAV offered as mp3 | audio codec |
| duration drift | 2 s source described as 10 s | audio duration |
| no video stream | WAV offered to the video check | video check |
| advertised direction with no fixture | `Unknown` type | fixture resolution |
| one-sided document advertisement | pair added to one table only | document set equality |

And the gate itself: `scripts/e2e.sh --media-only --inject=drop-output` deletes one
converted output before it is checked, so the **real** loop goes red —
`FAIL image jpeg->jpg output missing or empty`, `SUMMARY RED`, exit **1**
(`fault-injection.txt`). CI runs that as a separate step; a green suite that
cannot go red is not evidence.

## Two real defects this card found and fixed

1. **Twelve advertised directions could not convert.** `plan()` returned
   `PlanError::SameFormat` for `from == to`, while the matrix advertised jpg→jpg,
   all 7 audio identities and all 4 video identities as `Proven` (the wheel showed
   them). Preflight check 4 flagged all twelve on the first run. Fix: the planner
   plans identity like any other target (`ARCHITECTURE.md` §17.15); the capability
   is real (measured: every identity direction converts and passes its content +
   metadata assertions). `identity_is_planned_as_a_real_re_encode` pins it.
2. **HEIC/AVIF decode was unreachable from the engine on an unpacked dist.** The
   child process runs with `env_clear()`, so `libheif` never saw
   `LIBHEIF_PLUGIN_PATH` and answered *"no decoding plugin installed"* even though
   the codecs were present. Fix: `EZC_VIPS_PLUGIN_DIR` (absolute, dev/CI only) →
   `proc::plugin_env` sets `VIPS_PLUGINDIR` + `LIBHEIF_PLUGIN_PATH` for vips
   children (`ARCHITECTURE.md` §6.0.2, unit-tested by
   `plugin_env_maps_an_unpacked_tree_and_stays_empty_otherwise`). Nine directions
   (heic/avif *decode*) went from unreachable to green.

## How to run it

```bash
export EZC_VIPS_DIR=/path/to/vips-and-vipsheader
export EZC_FFMPEG_DIR=/path/to/ffmpeg-and-ffprobe
export EZC_SOFFICE_DIR=/path/to/libreoffice/program
bash scripts/e2e.sh                 # add --strict to fail on SKIP
```

On the CI runner (`ubuntu-latest`) the workflow job `e2e` installs test-only
engines (`libvips-tools`, `ffmpeg`, `libreoffice-core/writer/calc/impress`), runs
the suite, runs the fault-injection gate, and uploads `e2e-report/` + `e2e.log`.
Windows parity remains `forge`'s card (`t_44676171`, the shipping dist).

## Gaps, stated

- **AVIF output** (6 directions) is measured on the Windows dist only; this box has
  no AV1 encoder. SKIP, never PASS.
- **macOS** is unmeasured for every direction: nothing here ran on a Mac.
- The **CI verdict for this head** is not observable from this container (Contents
  scope only; Actions API 403). The suite is wired into `.github/workflows/build.yml`
  and the local transcript is what is claimed here.
- Animated GIF input stays `Planned` (`ARCHITECTURE.md` §17.12); the suite tests
  only what the matrix advertises, so it neither hides nor claims that call.
