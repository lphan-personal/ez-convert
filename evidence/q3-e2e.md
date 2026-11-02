# Q3 — end-to-end suite: one real run per advertised direction

Card `t_9a4c622d`. Suite: `scripts/e2e.sh` (code in `e2e/q3/`, README there).
Transcripts: `evidence/q3-e2e/` (`transcript.txt`, `report.tsv`, `report.json`,
`matrix.md`, `fault-injection.txt`).
Windows leg (card `t_4b6ee179`): `evidence/q3-e2e/windows.txt`,
`windows-report.tsv`, `windows-report.json`, `windows-matrix.md`,
`windows-fault-injection.txt`.

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

**Result on the shipping Windows dist (VM `win11-dev`, 2026-11-01, card
`t_4b6ee179`)** — same suite, same fixtures, the 43-file `spec-aom` vips dist and
a LGPL-only ffmpeg:

```
advertised=154  pass=150  fail=4  skip=0  controls=13/13 caught
SUMMARY RED  (4 direction failure(s), 0 control(s) not caught)
```

| family | advertised | pass | fail | skip |
|---|---|---|---|---|
| image | 37 | 33 | 4 | 0 |
| audio | 49 | 49 | 0 | 0 |
| video | 20 | 20 | 0 | 0 |
| document | 42 | 42 | 0 | 0 |

Zero SKIP: the AVIF encoder is present, so all six `→ avif` directions actually
run. Two of them pass; four are **red** — not because AVIF is broken (the encode,
geometry and mean all verify) but because the `strip` half of the metadata policy
cannot complete on an AVIF target. That is a real engine defect the Linux leg
could not reach, measured and written up in *The Windows leg* below. Nothing here
flips a matrix cell: the four rows are a finding for the matrix owner.

Note on arithmetic: the suite's summary line counts its six `policy`/`preflight`
rows together with the direction rows, which is where `154` comes from on both
legs (`148` directions + `6` preflight). The per-family table counts directions
only: 148 directions, 144 PASS, 4 FAIL, 0 SKIP.

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

## The Windows leg — the shipping dist (card `t_4b6ee179`)

The Linux leg cannot answer one question about itself: does the suite still
describe the product when the AVIF encoder *is* present? This leg runs the same
`scripts/e2e.sh` against the dist the app actually ships, on a dedicated Windows
11 VM.

### Environment (measured on the VM, not assumed)

| item | value |
|---|---|
| host | `win11-dev` (pve-ryo VM 110), Windows 11 Pro 10.0.26200.6584, 6 vCPU, 8 GB |
| vips | 8.18.7, `spec-aom` dist at `C:\ezc\dist` — 43 files / 35,824,560 B, **43/43 sha256 + size identical** to `spikes/s4/artifacts/FINAL-dist-spec-aom-sha256.txt`, re-hashed on the VM |
| ffmpeg | `N-128273-g836436cfb9-20261101`, BtbN `win64-lgpl`, `C:\ezc\ffmpeg` |
| LibreOffice | 26.8.2.2 (`9dea406ab7695df6325b11b0e19fdca8db4b7276`), `C:\Program Files\LibreOffice\program` |
| checkout | origin/main tree of `cb05606` at `C:\msys64\ezc\repo2`; tree hash over every file (`.git` excluded) `78a84f470a56ecef410ff76ca9d759050e50a886a5d56bb4f7e688742351ccc3`, computed independently on the Linux and Windows sides and equal |
| rust | rustc/cargo 1.99.1, host `x86_64-pc-windows-gnu` |
| shell | MSYS2 bash 5.3.20 |
| fixtures | 47 files — the repo `fixtures/` at that commit |

Two setup facts worth recording, because they are not obvious and they are not a
change to the product:

- The VM has no Rust, no MSVC and no mingw. Rust was installed with
  `rustup --default-host x86_64-pc-windows-gnu`, and the windows-gnu link uses
  the toolchain's own `rust-lld` against its self-contained mingw import
  libraries (`RUSTFLAGS="-Clinker=rust-lld -Clink-self-contained=y"`). No
  compiler install and no MSVC build tools, so the linked code is the same Rust
  the suite is built from anywhere else.
- The runner is POSIX bash: MSYS2 base 5.3.20, unpacked from a mirror because the
  VM cannot reach github.com. MSYS2's `/` is `C:\msys64`, so the checkout sits
  under it (`/ezc/repo2` = `C:\msys64\ezc\repo2`) with a junction
  `C:\ezc\repo2` → that path; that is what makes the POSIX-style absolute paths
  `scripts/e2e.sh` embeds in the generated crate resolve for native `rustc`.
  No file in the repo was edited for the leg.

### What was run

```bash
bash scripts/e2e.sh                                    # media + documents
bash scripts/e2e.sh --media-only --inject=drop-output  # the gate must go red
```

with `EZC_VIPS_DIR`, `EZC_FFMPEG_DIR` and `EZC_SOFFICE_DIR` pointing at the three
engine locations above. The driver around it only exports those and `cd`s into the
checkout; it does not touch argv, the table or the counts.

### The six `→ avif` directions, as the suite printed them

| direction | verdict | what the row says |
|---|---|---|
| `jpeg->avif` | **FAIL** | encode verified (899 B, 64×48, mean 124.9); `strip run failed: output failed verification: …/meta-jpeg-to-avif-strip.avif is missing` |
| `png->avif` | **FAIL** | same shape — 875 B encoded, strip output missing |
| `webp->avif` | **FAIL** | same shape — 971 B encoded, strip output missing |
| `tiff->avif` | **FAIL** | same shape — 875 B encoded, strip output missing |
| `heic->avif` | PASS | 990 B, 200×400, `meta=n/a: no heic fixture carrying EXIF/ICC can be produced on this dist` |
| `gif->avif` | PASS | 964 B, 64×48, `meta=n/a: no gif fixture carrying EXIF/ICC…` |

So: the AVIF **encoder** works on this dist (all six encode, geometry and mean
verify, the two rows whose metadata policy is `n/a` pass end to end). What fails
is the `strip` half of the metadata policy, and only where a metadata-bearing
source of the same input type exists — that is exactly the four rows above.

### Finding: the AVIF `strip` path is broken, in two independent ways

Both were measured on the VM, with the shipped dist, outside the suite, and then
confirmed in the suite's own output directory.

1. **A bracket on a `heifsave` filename argument is a literal filename, not
   options.** `model`→`plan` renders `strip` into the saver path as
   `out.avif[strip]`. Ops whose output is a `VipsImage` (`copy`, `autorot`,
   `thumbnail`, `resize`) parse that bracket; ops whose output is a plain
   filename string do not. Measured:

   ```
   vips heifsave in.jpg "out.avif[strip]" --compression av1 … --strip
     → exit 0, no stderr, nothing at out.avif
     → a file named  out.avif[strip]  appears (1740 B)
   vips pngsave  in.jpg "out.png[strip]"     → same: out.png[strip] (3681 B)
   vips copy     in.jpg "out.png[strip]"     → out.png (2885 B), metadata gone
   ```

   The suite's own work dir shows the consequence for all four red rows:
   `meta-jpeg-to-avif-strip.avif[strip]`, `meta-png-…`, `meta-webp-…`,
   `meta-tiff-…` — 1740 / 1715 / 1566 / 1739 B. The requested path never exists,
   so `convert::convert` reports `Verify`, which is the row text above.

2. **`--keep all` beats `--strip`.** The AVIF argv passes both. Measured on the
   same source, `vipsheader -a` for EXIF presence and file size:

   | argv | bytes | EXIF after save |
   |---|---|---|
   | `--keep all` (the keep run) | 1740 | present |
   | `--keep all --strip` (what the plan sends) | 1740 | **present** |
   | `--strip` alone | 760 | absent |

   So even once (1) is fixed, the strip run would still carry the source's EXIF
   and the metadata policy would still fail — the fix has to drop `--keep all`
   when stripping, not just the bracket.

Impact: a user who asks for AVIF output **with metadata stripped** does not get
a file at the requested path; they get a stray `*.avif[strip]` name in the
destination folder. The keep path is unaffected. Nothing was changed here: this
card runs the suite, the fix belongs to the engine owner and is filed as card
`t_ade3f5a7`. The four red rows are reported to the matrix owner as-is — no cell
was flipped, and the matrix still says `Proven` only where it already said so.

### The gate is not Linux-only

`bash scripts/e2e.sh --media-only --inject=drop-output` on the same VM:

```
FAIL  image     jpeg->jpg   output missing or empty: C:/ezc/q3work\out\jpeg-to-jpg.jpg is missing
…
SUMMARY RED  (5 direction failure(s), 0 control(s) not caught) — see report.tsv
EXIT=1
```

Five, not one: the injected fault plus the four real AVIF `strip` failures above.
Exit code 1, `SUMMARY RED` — the same red the Linux leg produces
(`windows-fault-injection.txt`).

### The leg is deterministic

The full suite was run twice at the same head. After removing timings and byte
counts the two transcripts are identical except for the report-directory line;
both report `154/150/4/0`, `13/13` controls. The committed transcript is the
second run.

Artifacts: `windows.txt` (full transcript), `windows-report.tsv`,
`windows-report.json`, `windows-matrix.md` (the 154 rows),
`windows-fault-injection.txt` (the injected run).

## The CI leg — why `e2e` was cancelled on every build run (card `t_0819575f`)

`build.yml`'s `e2e` job had been ending `cancelled` at its 30-minute cap on every recent push,
on commits older than v1.0.1 as well, and the verdict publisher recorded `e2e: cancelled`.
Measured from the artifact, not the job page: run **41443566030** (`13b9a43`), job
125271468844, artifact `q3-e2e-report` id 13143680014, file `e2e.log` (27,263 B, 350 lines).
The upload step runs `if: always()`, so a cancelled run does publish — it published this:

| leg | rows | result |
|---|---|---|
| preflight | 6 | PASS |
| image | 32 | 26 PASS / 6 SKIP (avif) |
| audio | 49 | 49 PASS |
| video | 20 | 8 PASS / **12 FAIL** — `Unknown encoder 'libopenh264'`, 137 ms each |
| documents | 42 | **0 rows emitted** — the log ends at `soffice warm-up: bootstrapped=true` |

`End-to-end suite` step: started 12:19:30Z, killed 12:49:20Z. Three separate defects, all
measured on the runner image (`ubuntu-26.04`, LibreOffice 26.2.6.3, apt vips 8.18.0):

1. **The document leg waited out the engine's 120 s cap on every direction.** The suite's own
   `--docs-only` leg, run alone on the same image, produced no row for 900 s. With the engine's
   kill path fixed (below) it spoke: `FAIL document csv->pdf 120024 ms soffice failed: engine
   killed (Timeout)`, then the same for `csv->txt`, `csv->xlsx`, `doc->docx`. Arithmetic: 42
   directions × 120 s = 84 minutes of floor, which is why raising the job cap could never have
   produced a table.
2. **The engine could not end a batch whose work was done.** `/proc` while stalled: the export
   was already on disk (`out/sample.pdf`), `soffice.bin` alive (`utime` growing) with its
   `PipeIPC` thread parked in `accept(2)` and its main thread in a futex loop — LibreOffice
   finished the conversion and stayed alive in its own event loop. The same argv, same env,
   same seeded profile, from a plain shell on the same runner exits in 0.34-0.84 s (four cases
   measured, `ci/e2e-bisect-probe`). Waiting for the process was therefore a false failure;
   two more engine defects turned it into a silent one — `proc::run` killed only the direct
   child (`oosplash`) at the cap, and then joined its capture threads **without a bound**, and
   `oosplash`'s child kept the pipes open. That is the 29-minute silence with no output.
3. **CI never had an H.264 encoder.** The job installed the runner's `ffmpeg` (libx264 present,
   `libopenh264` absent) while the shipped plan names `libopenh264` — every H.264 output
   direction failed at `engine exited 8`, independent of the cap. The shipping Windows/macOS
   packs convert all 20 video directions.

### The fixes

- `src-tauri/src/engine/proc.rs` — the child leads its own process group (`process_group(0)`)
  and a timeout kills the **group** (`kill(-pgid, SIGKILL)`); the capture wait is bounded at
  2 s with the partial capture kept; a new `Settle` rule ends a document batch once every input
  has a non-empty output that has not changed for 1 s, and `Kill::Settled` is **not** fatal —
  the outputs are then verified by the existing attribute/verify rules. Two new tests
  (`a_settled_batch_ends_without_waiting_for_the_engine_to_exit`,
  `a_timed_out_engine_returns_a_timeout_outcome`, plus the escaped-pipe capture test).
  `docs/ARCHITECTURE.md` §6.0.3 states the rule; the POSIX group kill it already claimed is now
  what the code does.
- `.github/workflows/build.yml` — the `e2e` job fetches the **shipped** linux64 ffmpeg pin
  (BtbN `ffmpeg-N-128273-g836436cfb9-linux64-lgpl`, archive `b4a4236c…`, `ffmpeg`
  `cf745359…`, `ffprobe` `f9af0178…` — the `docs/SHIPPED-INVENTORY.md` §3 rows) into
  `$RUNNER_TEMP`, re-hashes all three, asserts `--enable-libopenh264` and no `--enable-gpl`,
  and points `EZC_FFMPEG_DIR` at it. Nothing is bundled; the licence gate never sees it.
  `stdbuf -o0 -e0 tee` so a run that is cut off ends its log at the last row, not at a 4 KiB
  stdio boundary. Cap: 30 → **20 minutes** (measured job ≈ 3.5 min).
- `e2e/q3/suite.rs` — every row is flushed, the document queue is printed before the first
  spawn (so a stall names the directions still owed), and each row is appended to
  `<report>/rows.tsv` as it happens: a cancelled run now uploads the table it reached.

### Measured after the fix (`ci/e2e-verify`, same runner image)

| leg | wall | rows |
|---|---|---|
| `--docs-only` | **74 s** | 42/42 documents PASS, `SUMMARY GREEN` |
| full suite | **90 s** | `advertised=154 pass=143 fail=0 skip=11 controls=13/13 caught` |
| `--media-only --inject=drop-output` | **27 s** | `SUMMARY RED`, exit 1 |

The 11 SKIPs are engine availability on the apt dist, and each row says so: 6 `→ avif`
(no AV1 encoder in apt vips) and 5 `heic → …` (no libheif decode plugin in apt vips). They are
`Proven` on the shipping dists; `--strict` turns any SKIP into a red run.

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
On Windows the same two commands run under MSYS2 bash against the shipping dist
(card `t_4b6ee179`; environment, tree hash and the driver's env vars in *The
Windows leg* above). The media matrix measured by hand on that dist is
`spikes/c2/EVIDENCE-WINDOWS.md` (card `t_44676171`).

## Gaps, stated

- **AVIF output** (6 directions): this box has no AV1 encoder, so the Linux leg is
  all SKIP. The Windows leg (`windows.txt`) runs all six. On the `t_4b6ee179` head four
  of them were red on the `strip` half of the metadata policy — a real engine defect
  (saver bracket taken as a literal file name, plus `--keep all` over `--strip`), filed
  as card `t_ade3f5a7`. **That card fixed it**: the saver slot is bare and the plan sends
  `--strip` without `--keep all`, and the `t_ade3f5a7` leg re-ran this suite on the same
  dist — `advertised=154 pass=154 fail=0 skip=0 controls=13/13 caught`, all four rows
  `meta=icc=kept strip=clean`. Transcript, reports and the raw engine probe:
  `evidence/avif-strip.md` and `evidence/avif-strip/`.
- **macOS** is unmeasured for every direction: nothing here ran on a Mac.
- The **CI verdict for this head** is not observable from this container (Contents
  scope only; Actions API 403). The suite is wired into `.github/workflows/build.yml`
  and the local transcript is what is claimed here.
- Animated GIF input stays `Planned` (`ARCHITECTURE.md` §17.12); the suite tests
  only what the matrix advertises, so it neither hides nor claims that call.
