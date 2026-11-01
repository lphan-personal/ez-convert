# C2 — media pipeline evidence (vips images, LGPL ffmpeg audio/video)

Card `t_dc07a72e`. Everything below is tool output from this host; nothing is
estimated. Two engines were measured:

| engine | what ran | why it is the right engine to measure |
|---|---|---|
| `libvips 8.16.1` | Debian `libvips-tools`/`libvips42t64` binaries, executed with `LD_LIBRARY_PATH` pointing at their own prefix | same upstream major as the shipping dist (8.18.7) and the same CLI; the shipping dist lives on Windows and its argv contract was already proven in `§6.1` (V1-V11) |
| `ffmpeg N-128273 (LGPL)` | BtbN `linux64-lgpl` build, `gcc 16.2.0` | the shipped engine must be LGPL-only, so measuring the GPL build that happens to sit on this box would prove nothing |

```
ffmpeg -version | grep -o 'enable-gpl'   ->  0 occurrences
ffmpeg -version | grep -oE 'enable-libx26[45]'  ->  0 occurrences
ffmpeg encoders: libopenh264, libvpx-vp9, libmp3lame, aac, libopus, libvorbis, flac, pcm_*
vips --version -> vips-8.16.1   (EXIF with libexif: true, ICC with lcms2: true)
```

**Known engine gap on this host, stated up front:** the Debian vips package loads
HEIF/AVIF through a *dynamic module* (`vips-heif.so`) that this packaging does not
register (`vips heifsave` → `unknown action "heifsave"`), and its dist has no
`librsvg`, no `cgif` and no BMP loader. So AVIF output and HEIC decode are
**skipped on this leg** and remain `Proven` only on the Windows
`spec-aom` dist, where they were measured (`spikes/s4/EVIDENCE.md`, §6.1 V8/V8b,
34 assertions). Nothing below claims AVIF/HEIC from this run.

---

## 1. libvips: how the CLI actually behaves (`evidence` for `vips.rs`)

The contract is one process per conversion, with bracket filename options:

```
vips <op> '<input>[fail-on=error]' '<output>[Q=90,strip]'
```

Measured, in order (`spikes/c2/discovery/` holds the scripts that produced these):

| # | command | result |
|---|---|---|
| D1 | `vips copy exif_icc.jpg out.png` | rc=0, PNG written, `exif-ifd0-Orientation` and all IFD0/IFD2 tags present in the output |
| D2 | `vips jpegload in out.png --Q 80` | **rc=1 `Unknown option --Q`** — a *loader* op has no saver options |
| D3 | `vips copy in.jpg out.jpg --strip` | **rc=1 `Unknown option --strip`** — same reason |
| D4 | `vips copy in.jpg --fail-on error` (flag form on `copy`) | **rc=1 `Unknown option --fail-on`** — confirms §6.1's note |
| D5 | `vips copy 'in.jpg[fail-on=error]' 'out.jpg[Q=80]'` | rc=0, 3,300 B; `[Q=20]` → 2,535 B; `[Q=90]` → 5,320 B — the bracket form is the one that carries *both* loader guard and saver options in one process |
| D6 | `vips copy 'in.jpg[fail-on=error]' 'out.jpg[strip]'` | rc=0, and **no** `exif-*`/`icc-profile-data` in the output (jpg) |
| D7 | same with `.png`, `.webp`, `.tif` | rc=0, 0 EXIF lines in all three — `strip` works on every saver this card uses |
| D8 | `vips copy 'truncated.jpg[fail-on=error]' out.png` | **rc=1**, `VipsJpeg: premature end of JPEG image`, **and a 0-byte `out.png` is left behind** |
| D9 | `vips copy truncated.jpg out.png` (no guard) | **rc=0** with `VIPS-WARNING … premature end of JPEG image`, 1,878 B PNG — the silent-garbage control |
| D10 | `vips copy 'truncated.png[fail-on=error]' out.png` | rc=1 `pngload: end of stream`, 0-byte output |
| D11 | `vips autorot 'exif_icc.jpg[fail-on=error]' 'out.jpg[Q=95]'` | rc=0, **400x200 → 200x400**, `exif-ifd0-Orientation: 1` |
| D12 | `vips resize 'exif_icc.jpg[fail-on=error]' 'out.png[compression=9]' 0.25` | rc=0, 100x50 — resize takes the scale as its third positional argument |
| D13 | `vips copy sample.svg out.png` | rc=0 on this dist (librsvg linked) — **excluded from the product anyway**: the shipping dist has no librsvg DLL, so SVG stays on the Resvg route |
| D14 | `vips copy sample.bmp out.png` | **rc=1 `not a known file format`** — no BMP loader in this build (matches `§8.2`: BMP goes through the `image` crate) |
| D15 | `vips copy sample.gif out.png` | rc=0, first page only (`n-pages: 1` on the single-frame fixture) |
| D16 | `vips copy 'nope.jpg[fail-on=error]' out.png` | rc=1 `file "nope.jpg" does not exist`, no output |

Two consequences baked into the code:

1. `engine/verify.rs` treats a **0-byte output as a failure** (D8/D10). A missing
   or empty file after a "successful" run is exactly the trap §6.0.4 describes.
2. `engine/plan.rs` writes the GIF intermediate with a **`.png` extension**
   (`Plan::temp_exts`), because vips picks the saver by file name: a temp named
   `step0.tmp` fails with `not a known file format` (measured while building the
   GIF path).

---

## 2. Still-image directions (vips only)

Every row is `engine::convert::convert` → real `vips` → re-opened output.
`PASS` means the output exists, is non-empty, its **magic bytes match the target**
(`model::detect`) and `vipsheader` reports a positive width.

| direction | result |
|---|---|
| Jpeg -> png PASS | 3,537 B, magic png |
| Jpeg -> webp PASS | 1,188 B |
| Jpeg -> tiff PASS | 3,508 B |
| Png -> jpg PASS | 2,781 B |
| Png -> webp PASS | 1,130 B |
| Png -> tiff PASS | 522 B |
| Webp -> jpg PASS | 2,904 B |
| Webp -> png PASS | 4,346 B |
| Webp -> tiff PASS | 4,268 B |
| Tiff -> jpg PASS | 2,781 B |
| Tiff -> png PASS | 626 B |
| Tiff -> webp PASS | 1,130 B |
| Jpeg -> gif PASS | 64x48, 1 frame, 1,680 B (vips normalise → ffmpeg palette) |
| Png -> gif PASS | 1 frame |
| Webp -> gif PASS | 1 frame |
| Tiff -> gif PASS | 1 frame |

Metadata, measured on `fixtures/exif_icc.jpg` (400x200, EXIF `Orientation=6`,
736 B sRGB ICC):

```
PASS  EXIF+ICC kept -> png          exif=True icc=True
PASS  EXIF+ICC kept -> webp         exif=True icc=True
PASS  EXIF+ICC kept -> tiff         exif=False icc=True   (TIFF has no EXIF blob: the
                                     orientation is baked into the pixels by autorot,
                                     and vips writes ICC as a native TIFF tag)
PASS  strip removes EXIF/ICC -> png  no exif/icc tags in the output
PASS  strip removes EXIF/ICC -> webp no exif/icc tags in the output
PASS  strip removes EXIF/ICC -> tiff no exif/icc tags in the output
PASS  orientation baked (autorot)    400x200 + Orientation=6 -> 200x400
PASS  image resize 50%               width 400 -> 200
```

**"ffmpeg is never used for still images"** is a unit test, not a promise:
`engine::plan::tests::ffmpeg_never_reads_or_writes_a_still_image` walks every
still type × every target, and for any step whose engine is ffmpeg asserts
(a) it does not read `Source::Input` — the user's file — and (b) the target is
`gif`. `the_only_ffmpeg_step_in_a_still_plan_is_the_gif_palette_encoder` pins the
one tolerated use: ffmpeg reads `<tmp0>.png`, a file vips wrote.

---

## 3. Audio directions (LGPL ffmpeg, explicit `-ar` / `-sample_fmt`)

42 directions (7 source families × 6 other targets), all PASS: the output
re-opens with `ffprobe`, reports the expected codec, a sample rate and a duration
> 20 ms.

| source | -> mp3 | -> wav | -> flac | -> m4a | -> aac | -> ogg | -> opus |
|---|---|---|---|---|---|---|---|
| mp3 | — | PASS | PASS | PASS | PASS | PASS | PASS |
| wav | PASS | — | PASS | PASS | PASS | PASS | PASS |
| flac | PASS | PASS | — | PASS | PASS | PASS | PASS |
| m4a | PASS | PASS | PASS | — | PASS | PASS | PASS |
| aac | PASS | PASS | PASS | PASS | — | PASS | PASS |
| ogg | PASS | PASS | PASS | PASS | PASS | — | PASS |
| opus | PASS | PASS | PASS | PASS | PASS | PASS | — |

Per-target markers used by `model/matrix.rs`:

```
target mp3: 6 of 6 sources PASS
target wav: 6 of 6 sources PASS
target flac: 6 of 6 sources PASS
target m4a: 6 of 6 sources PASS
target aac: 6 of 6 sources PASS
target ogg: 6 of 6 sources PASS
target opus: 6 of 6 sources PASS
```

### 3.1 Sampled-rate discipline (the silent-wrongness class)

| finding | measurement |
|---|---|
| libmp3lame has a fixed rate list | `-ar 96000` → **rc=234**, encoder prints `Supported sample rates: 44100 48000 32000 22050 24000 16000 11025 12000 8000`; `-ar 8000/22050/44100/48000` all rc=0 with the rate preserved |
| libopus accepts only 48/24 kHz | `-ar 44100` → **rc=234** `Specified sample rate 44100 is not supported by the libopus encoder … Supported sample rates: 48000, 24000` |
| FLAC and WAV take any rate | 96 kHz source → 96 kHz FLAC (`sample_fmt=s32`), 96 kHz WAV (`pcm_s16le` when the source is 16-bit, `pcm_s24le` when it is 24-bit) |
| Vorbis and AAC accept 96 kHz | `-ar 96000` rc=0, output reports 96000 |

So `ffmpeg::clamp_rate` picks the highest measured-supported rate ≤ the source,
Opus is pinned to 48 kHz, and `plan.rs` refuses to plan without source facts:

```
PASS  96 kHz source -> mp3    sample_rate=48000 Hz (clamped as measured)
PASS  96 kHz source -> flac   sample_rate=96000 Hz
PASS  96 kHz source -> opus   sample_rate=48000 Hz
PASS  96 kHz source -> aac    sample_rate=96000 Hz
```

### 3.2 The `-b:a` trap (found by this card, would have hung the app)

```
ffmpeg -i sample.mp3 ... -c:a aac -b:a 192 ... out.m4a
[aac] Bitrate 192 is extremely low, maybe you mean 192k
[aac] The bitrate parameter is set too low. It takes bits/s as argument, not kbits/s
... size=0KiB time=00:00:00.09 speed=0.007x   (still running after 20 s)
```

A bare number is **bits per second**. The conversion does not fail — it crawls,
which on a 3-minute song means a hang. `Arg::Kbps` renders `192k`; a unit test
fails if any `-b:v`/`-b:a` value is a bare number other than the documented VP9
`-b:v 0`. The stall detector (`proc.rs`, 120 s without output) is what surfaced
it: the run was reported `engine killed (Stalled)` rather than hanging forever.

### 3.3 Tags (lofty, not ffmpeg's mapping)

ffmpeg is told `-map_metadata -1`; `engine/tags.rs` then copies the source's tags
and cover art with `lofty` 0.22. Measured with `fixtures/tagged.mp3`
(title/artist/album/date + attached cover):

```
PASS  tags survive mp3 -> wav   5 items, 1 pictures, readback has title
PASS  tags survive mp3 -> flac  5 items, 1 pictures, readback has title
PASS  tags survive mp3 -> m4a   5 items, 1 pictures, readback has title
PASS  tags survive mp3 -> ogg   5 items, 1 pictures, readback has title
PASS  tags survive mp3 -> opus  5 items, 1 pictures, readback has title
PASS  tags survive mp3 -> aac   5 items, 1 pictures, readback has title
```

Two measured corrections went into the code:

- **The tag family must come from the target container, not from the output
  file's current tags.** ffmpeg writes an Ogg with no tag block, so "use the
  target's existing type" fell back to ID3v2 and wrote ID3 into an Ogg file:
  lofty read it back happily, `ffprobe` and every player saw nothing. `tag_type_for`
  now maps `.mp3`→Id3v2, `.flac/.ogg/.opus`→VorbisComments, `.m4a`→Mp4Ilst,
  `.wav`→RiffInfo.
- **Ogg/Opus tags are stream-level in ffprobe**, not format-level; the verifier
  asks for both or it reports perfectly good tags as missing.

---

## 4. Video directions (CFR forced, LGPL encoders only)

Codecs: H.264 through **`libopenh264`** (the LGPL build has no libx264/libx265),
VP9 through `libvpx-vp9`, audio AAC or Opus. Every row PASSes with
`r_frame_rate == avg_frame_rate` (i.e. the output is not variable).

| source | -> mp4 | -> mkv | -> mov | -> webm | -> gif |
|---|---|---|---|---|---|
| mp4 | — | PASS | PASS | PASS | PASS |
| mkv | PASS | — | PASS | PASS | PASS |
| mov | PASS | PASS | — | PASS | PASS |
| webm | PASS | PASS | PASS | — | PASS |

```
target mp4: 3 of 3 sources PASS
target mkv: 3 of 3 sources PASS
target mov: 3 of 3 sources PASS
target webm: 3 of 3 sources PASS
target gif (from video): 4 of 4 sources PASS
```

### 4.1 The VFR trap, with its control

A genuinely variable-timestamp source (`fixtures/sample_vfr.mkv`, 89 frames over
2.966 s) transcoded **with** the flag and **without** it:

| command | frames | r_frame_rate | avg_frame_rate |
|---|---|---|---|
| `-fps_mode cfr -r 30` | **89** | 30/1 | 30/1 |
| no `-fps_mode` | **68** | 30/1 | **2040/89** (≈22.9) |

The second output looks fine in `ffprobe`'s "fps" field and stutters in every
player: frames were dropped to fit a nominal rate. `plan.rs` always emits
`-fps_mode cfr -r <source rate>`, and `verify::is_vfr` is the check that catches a
regression (`PASS  VFR -> CFR`).

### 4.2 Rotation, trim, resize

| finding | measurement |
|---|---|
| display-matrix rotation is applied on re-encode | source 320x240 with `rotation=90` → output **240x320**, rotation side data gone |
| `-c:v copy` keeps the rotation side data instead | `PASS` control: `stream_side_data=rotation=90` survives a stream copy, which is why we re-encode rather than copy |
| `-ss` before `-i` + `-t` after it = fast seek, exact length | 2 s / 25 fps clip, `-ss 0.5 … -t 1.0` → **25 frames, 1.000 s** |
| `-to` before `-i` is relative to the seek point | `-ss 0.5 -to 1.5` → 26 frames, **1.040 s** (wrong if you meant 1.5 s absolute) |
| output-side `-ss`/`-to` is exact but decodes from zero | 25 frames / 1.000 s |
| resize | `scale=160:-2:flags=lanczos` → 160x120 |

```
PASS  trim 0.5 s + 1.0 s        duration=1000 ms
PASS  video resize to 160       160x120
PASS  VFR -> CFR                30/1 == 30/1 (constant)
```

### 4.3 GIF output

- **Still image → GIF:** vips decodes, autorots, strips and writes a PNG; ffmpeg
  only palette-encodes that PNG (`split[a][b];[a]palettegen=stats_mode=single[p];[b][p]paletteuse=…`).
  Verified by the unit test above: ffmpeg's input is `Temp(0)`, never the user's file.
- **Video → GIF:** `fps=<n>,scale=min(<w>\,iw):-1:flags=lanczos,…` with
  `palettegen=stats_mode=diff`. Measured output: 12 frames at 160x120 for a 1 s
  clip at `fps=12`; 4 frames for the 4 s/fps-4 animated fixture.

---

## 5. Inputs that must fail loudly (and do)

| input | behaviour | test |
|---|---|---|
| truncated jpg with the loader guard | rc=1, 0-byte file removed by `convert` | `verify::exists_nonempty` |
| missing file | `path does not exist` before any spawn | `proc::admit` |
| image as an audio source | `Output file does not contain any stream`, rc=234, no file | e2e discovery `A9` |
| zero-byte mp4 | rc=183, no output | discovery `V17` |
| same format (`jpg -> jpg`) | `PlanError::SameFormat`, the queue skips instead of re-encoding | `plan::tests::same_format_is_a_skip_not_a_reencode` |
| unrouted direction (`jpg -> bmp`) | `PlanError::NotRouted` | `plan::tests::unrouted_directions_are_refused_not_guessed` |

---

## 6. The run

```
spikes/c2/run-e2e.sh
EZC_VIPS_DIR=<dist with vips+vipsheader>
EZC_FFMPEG_DIR=<dir with ffmpeg+ffprobe>
EZC_ENGINE_LIB_DIR=<shared libs, Linux only>
```

Full transcript of the acceptance run (`SUMMARY pass=103 fail=0 skip=6`; the six
skips are the AVIF rows explained at the top):

```
engines: vips="/opt/data/profiles/wright/cache/scratch/eng/prefix/usr/bin/vips" ffmpeg="/opt/data/profiles/wright/cache/scratch/eng/ffmpeg-master-latest-linux64-lgpl/bin/ffmpeg"
fixtures: /opt/data/kanban/boards/ezconvert/workspaces/t_dc07a72e/repo/fixtures

## still images (vips only)
PASS  Jpeg->"png" sample.jpg       3537B png 3537B
PASS  Jpeg->"webp" sample.jpg      1188B webp 1188B
PASS  Jpeg->"tiff" sample.jpg      3508B tiff 3508B
SKIP  Jpeg->"avif" sample.jpg      engine cannot do this on this dist: engine failed: step 1 (vips heifsave (av1)): engine exited 1; stderr: vips: unknown action "heifsave"
PASS  Jpeg->"gif" sample.jpg       64x48 frames=Some((100, 4)) 2904B
PASS  Png->"jpg" sample.png        2781B jpg 2781B
PASS  Png->"webp" sample.png       1130B webp 1130B
PASS  Png->"tiff" sample.png       522B tiff 522B
SKIP  Png->"avif" sample.png       engine cannot do this on this dist: engine failed: step 1 (vips heifsave (av1)): engine exited 1; stderr: vips: unknown action "heifsave"
PASS  Png->"gif" sample.png        64x48 frames=Some((100, 4)) 1680B
PASS  WebP->"jpg" sample.webp      2904B jpg 2904B
PASS  WebP->"png" sample.webp      4346B png 4346B
PASS  WebP->"tiff" sample.webp     4268B tiff 4268B
SKIP  WebP->"avif" sample.webp     engine cannot do this on this dist: engine failed: step 1 (vips heifsave (av1)): engine exited 1; stderr: vips: unknown action "heifsave"
PASS  WebP->"gif" sample.webp      64x48 frames=Some((100, 4)) 2812B
PASS  Tiff->"jpg" sample.tif       2781B jpg 2781B
PASS  Tiff->"png" sample.tif       626B png 626B
PASS  Tiff->"webp" sample.tif      1130B webp 1130B
SKIP  Tiff->"avif" sample.tif      engine cannot do this on this dist: engine failed: step 1 (vips heifsave (av1)): engine exited 1; stderr: vips: unknown action "heifsave"
PASS  Tiff->"gif" sample.tif       64x48 frames=Some((100, 4)) 1680B
PASS  Gif->"jpg" sample.gif        3010B jpg 3010B
PASS  Gif->"png" sample.gif        770B png 770B
PASS  Gif->"webp" sample.gif       1332B webp 1332B
PASS  Gif->"tiff" sample.gif       774B tiff 774B
SKIP  Gif->"avif" sample.gif       engine cannot do this on this dist: engine failed: step 1 (vips heifsave (av1)): engine exited 1; stderr: vips: unknown action "heifsave"
PASS  Jpeg->"png" exif_icc.jpg     3468B png 3468B
PASS  Jpeg->"webp" exif_icc.jpg    1490B webp 1490B
PASS  Jpeg->"tiff" exif_icc.jpg    4078B tiff 4078B
SKIP  Jpeg->"avif" exif_icc.jpg    engine cannot do this on this dist: engine failed: step 1 (vips heifsave (av1)): engine exited 1; stderr: vips: unknown action "heifsave"
PASS  Jpeg->"gif" exif_icc.jpg     200x400 frames=Some((100, 4)) 8263B

## metadata (kept by default, stripped on request)
PASS  EXIF+ICC kept -> png         exif=true icc=true (ICC always; EXIF blob where the container has one)
PASS  strip removes EXIF/ICC -> png no exif/icc tags in the output
PASS  EXIF+ICC kept -> webp        exif=true icc=true (ICC always; EXIF blob where the container has one)
PASS  strip removes EXIF/ICC -> webp no exif/icc tags in the output
PASS  EXIF+ICC kept -> tiff        exif=false icc=true (ICC always; EXIF blob where the container has one)
PASS  strip removes EXIF/ICC -> tiff no exif/icc tags in the output
PASS  orientation baked (autorot)  400x200 + Orientation=6 -> 200x400
PASS  image resize 50%             width 400 -> 200

## audio (LGPL ffmpeg, explicit -ar/-sample_fmt)
PASS  Mp3->"wav" sample.mp3        pcm_s16le 44100 Hz 1 ch tags=1pics=0 22128B
PASS  Mp3->"flac" sample.mp3       flac 44100 Hz 1 ch tags=1pics=0 29993B
PASS  Mp3->"m4a" sample.mp3        aac 44100 Hz 1 ch tags=1pics=0 6140B
PASS  Mp3->"aac" sample.mp3        aac 44100 Hz 1 ch tags=1pics=0 5377B
PASS  Mp3->"ogg" sample.mp3        vorbis 44100 Hz 1 ch tags=1pics=0 4926B
PASS  Mp3->"opus" sample.mp3       opus 48000 Hz 1 ch tags=1pics=0 5115B
PASS  Wav->"mp3" sample.wav        mp3 44100 Hz 1 ch tags=1pics=0 7567B
PASS  Wav->"flac" sample.wav       flac 44100 Hz 1 ch tags=1pics=0 12560B
PASS  Wav->"m4a" sample.wav        aac 44100 Hz 1 ch tags=1pics=0 6517B
PASS  Wav->"aac" sample.wav        aac 44100 Hz 1 ch tags=1pics=0 5754B
PASS  Wav->"ogg" sample.wav        vorbis 44100 Hz 1 ch tags=1pics=0 4742B
PASS  Wav->"opus" sample.wav       opus 48000 Hz 1 ch tags=1pics=0 5423B
PASS  Flac->"mp3" sample.flac      mp3 44100 Hz 1 ch tags=1pics=0 7567B
PASS  Flac->"wav" sample.flac      pcm_s16le 44100 Hz 1 ch tags=1pics=0 22128B
PASS  Flac->"m4a" sample.flac      aac 44100 Hz 1 ch tags=1pics=0 6517B
PASS  Flac->"aac" sample.flac      aac 44100 Hz 1 ch tags=1pics=0 5754B
PASS  Flac->"ogg" sample.flac      vorbis 44100 Hz 1 ch tags=1pics=0 4742B
PASS  Flac->"opus" sample.flac     opus 48000 Hz 1 ch tags=1pics=0 5423B
PASS  Aac->"mp3" sample.aac        mp3 44100 Hz 1 ch tags=0pics=0 8194B
PASS  Aac->"wav" sample.aac        pcm_s16le 44100 Hz 1 ch tags=0pics=0 24654B
PASS  Aac->"flac" sample.aac       flac 44100 Hz 1 ch tags=0pics=0 13233B
PASS  Aac->"m4a" sample.aac        aac 44100 Hz 1 ch tags=0pics=0 7461B
PASS  Aac->"ogg" sample.aac        vorbis 44100 Hz 1 ch tags=0pics=0 5608B
PASS  Aac->"opus" sample.aac       opus 48000 Hz 1 ch tags=0pics=0 5959B
PASS  M4a->"mp3" sample.m4a        mp3 44100 Hz 1 ch tags=1pics=0 7567B
PASS  M4a->"wav" sample.m4a        pcm_s16le 44100 Hz 1 ch tags=1pics=0 22128B
PASS  M4a->"flac" sample.m4a       flac 44100 Hz 1 ch tags=1pics=0 12911B
PASS  M4a->"aac" sample.m4a        aac 44100 Hz 1 ch tags=1pics=0 4972B
PASS  M4a->"ogg" sample.m4a        vorbis 44100 Hz 1 ch tags=1pics=0 5144B
PASS  M4a->"opus" sample.m4a       opus 48000 Hz 1 ch tags=1pics=0 5162B
PASS  Ogg->"mp3" sample.ogg        mp3 44100 Hz 1 ch tags=1pics=0 7567B
PASS  Ogg->"wav" sample.ogg        pcm_s16le 44100 Hz 1 ch tags=1pics=0 21872B
PASS  Ogg->"flac" sample.ogg       flac 44100 Hz 1 ch tags=1pics=0 13120B
PASS  Ogg->"m4a" sample.ogg        aac 44100 Hz 1 ch tags=1pics=0 5510B
PASS  Ogg->"aac" sample.ogg        aac 44100 Hz 1 ch tags=1pics=0 4747B
PASS  Ogg->"opus" sample.ogg       opus 48000 Hz 1 ch tags=1pics=0 5359B
PASS  Opus->"mp3" sample.opus      mp3 48000 Hz 1 ch tags=1pics=0 7533B
PASS  Opus->"wav" sample.opus      pcm_s16le 48000 Hz 1 ch tags=1pics=0 24078B
PASS  Opus->"flac" sample.opus     flac 48000 Hz 1 ch tags=1pics=0 13656B
PASS  Opus->"m4a" sample.opus      aac 48000 Hz 1 ch tags=1pics=0 6426B
PASS  Opus->"aac" sample.opus      aac 48000 Hz 1 ch tags=1pics=0 5666B
PASS  Opus->"ogg" sample.opus      vorbis 48000 Hz 1 ch tags=1pics=0 4970B

## audio rate discipline (measured encoder limits)
PASS  96 kHz source -> mp3         sample_rate=48000 Hz (clamped as measured)
PASS  96 kHz source -> flac        sample_rate=96000 Hz (clamped as measured)
PASS  96 kHz source -> opus        sample_rate=48000 Hz (clamped as measured)
PASS  96 kHz source -> aac         sample_rate=96000 Hz (clamped as measured)

## audio tags (lofty merge after the encode)
PASS  tags survive mp3 -> wav      5 items, 1 pictures, readback has title
PASS  tags survive mp3 -> flac     5 items, 1 pictures, readback has title
PASS  tags survive mp3 -> m4a      5 items, 1 pictures, readback has title
PASS  tags survive mp3 -> ogg      5 items, 1 pictures, readback has title
PASS  tags survive mp3 -> opus     5 items, 1 pictures, readback has title
PASS  tags survive mp3 -> aac      5 items, 1 pictures, readback has title

## video (CFR forced)
PASS  Mp4->"mkv" sample.mp4        h264 64x48 10fps dur=Some(1000)ms 3860B
PASS  Mp4->"mov" sample.mp4        h264 64x48 10fps dur=Some(1000)ms 3964B
PASS  Mp4->"webm" sample.mp4       vp9 64x48 10fps dur=Some(1000)ms 2206B
PASS  Mp4->"gif" sample.mp4        64x48 frames=Some((25, 2)) 10048B
PASS  Mkv->"mp4" sample.mkv        h264 64x48 10fps dur=Some(1000)ms 4017B
PASS  Mkv->"mov" sample.mkv        h264 64x48 10fps dur=Some(1000)ms 3964B
PASS  Mkv->"webm" sample.mkv       vp9 64x48 10fps dur=Some(1000)ms 2176B
PASS  Mkv->"gif" sample.mkv        64x48 frames=Some((25, 2)) 10048B
PASS  Mov->"mp4" sample.mov        h264 64x48 10fps dur=Some(1000)ms 4017B
PASS  Mov->"mkv" sample.mov        h264 64x48 10fps dur=Some(1000)ms 3860B
PASS  Mov->"webm" sample.mov       vp9 64x48 10fps dur=Some(1000)ms 2206B
PASS  Mov->"gif" sample.mov        64x48 frames=Some((25, 2)) 10048B
PASS  WebM->"mp4" sample.webm      h264 64x48 10fps dur=Some(1000)ms 3468B
PASS  WebM->"mkv" sample.webm      h264 64x48 10fps dur=Some(1000)ms 3273B
PASS  WebM->"mov" sample.webm      h264 64x48 10fps dur=Some(1000)ms 3415B
PASS  WebM->"gif" sample.webm      64x48 frames=Some((25, 2)) 6585B

## the VFR trap (measured control)
PASS  VFR -> CFR                   Some((30, 1)) == Some((30, 1)) (constant)
PASS  trim 0.5 s + 1.0 s           duration=1000 ms, frames=Some((30, 1))
PASS  video resize to 160          160x120

SUMMARY pass=103 fail=0 skip=6

```

## 7. What this does NOT prove

- **AVIF output and HEIC decode** on this leg: the Debian vips cannot load its
  HEIF module (see the top of this file). They stay `Proven` on the Windows
  `spec-aom` dist only (`spikes/s4/EVIDENCE.md`).
- **The shipping Windows dist is not re-run here.** The argv is the one §6.1
  measured (bracket loader guard + saver options) plus the AVIF flag set V8
  measured; a Windows re-run of this e2e is the honest next check and is filed as
  a follow-up on the card.
- **Animated GIF input** is not offered. `vips gifload` returns page 0, so an
  animated source would silently lose every other frame. The GIF-input routes stay
  `Planned` in the matrix with that reason; `fixtures/sample_anim.gif` exists so
  the decision is testable the day we handle animation.
- **Legacy/odd audio rates** below 8 kHz and above 96 kHz are clamped by
  measurement of the encoder's own list, not by round-trip listening.
- **Concurrency and timeouts** are safety limits (§6.0.3), not measurements. The
  `-b:a` trap above is the one time the 120 s stall detector fired, and it fired
  correctly.
