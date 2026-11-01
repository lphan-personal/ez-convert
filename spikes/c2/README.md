# C2 spike — media pipeline argv discovery + end-to-end matrix

Card `t_dc07a72e`. Two things live here:

| path | what it is |
|---|---|
| `EVIDENCE.md` | the findings and the full acceptance transcript (`SUMMARY pass=103 fail=0 skip=6`) |
| `e2e.rs` | the end-to-end matrix driver: runs `engine::convert::convert` against real engines and asserts on the outputs |
| `run-e2e.sh` | builds `e2e.rs` in a throwaway cargo crate (so `lofty` is available) and runs it |
| `discovery/*.sh` | the throwaway scripts that produced the argv facts in `EVIDENCE.md` §1-§5 |

## Running the end-to-end matrix

```bash
export EZC_VIPS_DIR=/path/to/dir-with-vips-and-vipsheader
export EZC_FFMPEG_DIR=/path/to/dir-with-ffmpeg-and-ffprobe
export EZC_ENGINE_LIB_DIR=/path/to/shared-libs   # Linux only, if vips is not in a system prefix
spikes/c2/run-e2e.sh                             # fixtures/ by default, temp work dir
```

Exit code 0 means every direction passed. Each line is `PASS`/`FAIL`/`SKIP` plus the property
that was checked (bytes, dimensions, codec, sample rate, frame-rate constancy, tag readback).

Needs a network-free cargo only for the first `cargo build` of `lofty`; a `rustc`-only host can
still run `scripts/test-engine.sh`, which covers everything except the `lofty` merge (stubbed
there, proven here).

## Running the unit/contract tests

```bash
bash scripts/test-engine.sh
```

That is the same set CI runs: the argv builders, the planner, the spawn contract, the stderr
rules and the verifiers — 136 tests including
`ffmpeg_never_reads_or_writes_a_still_image`.

## The discovery scripts

They are kept verbatim, including the absolute paths of the host that ran them
(`/opt/data/profiles/wright/cache/scratch/eng/...`), because they are the transcripts behind
`EVIDENCE.md`. Point the `V=`/`FF=` variables at your own engines to re-run them; nothing in
`src-tauri/` depends on them.

| script | what it established |
|---|---|
| `d1-vips.sh` | the flag forms are rejected; metadata survival on `copy`; truncated-input behaviour |
| `d2-vips.sh` | the **bracket** form works for loader guard + saver options; `strip`; autorot; resize; bmp/gif/svg availability |
| `d3-av.sh` | the audio matrix, sample-rate limits, tag behaviour, CFR/VFR control, rotation, trim, GIF palettes |
| `d5.sh` | trim forms on a 2 s clip, 96 kHz clamp behaviour, channels, image-as-audio failure |

The AVIF/HEIC rows in `d2` and the e2e are `SKIP` on the Linux distribution's libvips: it ships
the HEIF codec as a dynamic module this packaging does not register. That gap is stated at the
top of `EVIDENCE.md` and is why AVIF/HEIC remain Proven on the Windows dist only.
