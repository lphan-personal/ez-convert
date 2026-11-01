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
