# C2-Win — the media pipeline on the shipping Windows dist + LGPL ffmpeg

Card `t_44676171`, follow-up to `t_dc07a72e` (Linux leg, `spikes/c2/EVIDENCE.md`,
`SUMMARY pass=103 fail=0 skip=6`). Same Rust code, same planner, same spawn contract,
same verifiers — a different leg: the **self-built `spec-aom` libvips 8.18.7 dist the app
actually ships** and a **LGPL-only Windows ffmpeg**, both exercised through
`engine::convert::convert` on Windows 11 (VM `win11-dev`, `ryo@10.0.0.191`).

**Result: `SUMMARY pass=120 fail=0 skip=0`, exit code 0.** The six AVIF rows that were
`SKIP` on Linux (that Debian libvips cannot load its HEIF module) are `PASS` here, and the
HEIC-decode directions — no row at all in the Linux matrix — are `PASS` in the added heif
section below.

## 1. What was run, and how

| item | value |
|---|---|
| host | `win11-dev`, Windows 11 Pro 10.0.26200 build 26200, 6 vCPU, 7.9 GB |
| vips | **8.18.7**, `spec-aom` dist, `C:\ezc\dist` |
| ffmpeg | **N-128273-g836436cfb9-20261101**, BtbN `win64-lgpl`, `C:\ezc\ffmpeg` |
| fixtures | `C:\ezc\fixtures` (47 files = the repo `fixtures/` at this commit) |
| driver | `spikes/c2/e2e.rs` |
| command | `powershell -NoProfile -ExecutionPolicy Bypass -File C:\ezc\run-c2win.ps1` |
| env | `EZC_VIPS_DIR=C:\ezc\dist`, `EZC_FFMPEG_DIR=C:\ezc\ffmpeg` |

**Which path: the same Rust binary, cross-compiled — not argv replay.** `spikes/c2/run-e2e.sh`
needs cargo on the host it runs on, and this VM has no Rust toolchain (and no internet to get
one). Rather than replaying argv from PowerShell (the S3 pattern, `spikes/s3/run.ps1`), the
card took the stronger option already used by C3: the *identical* `spikes/c2/e2e.rs`, wrapped
in the same throwaway crate (`lofty` 0.22.4), cross-compiled to `x86_64-pc-windows-gnu` with
`rustc`/`cargo` and the Linux-hosted mingw toolchain, then copied to the VM and run there:

```
cargo build --offline --target x86_64-pc-windows-gnu   # Cargo.toml: lofty = "0.22", [workspace]
scp target/x86_64-pc-windows-gnu/debug/ezc-c2-e2e.exe  ryo@10.0.0.191:C:/ezc/
```

So the plan, the spawn contract, the stderr rules, `verify::*` and the `lofty` tag merge are
the ones that ship — not a transcription of them. `EnginePaths::discover` picks `vips.exe` /
`vipsheader.exe` / `ffmpeg.exe` / `ffprobe.exe` on Windows (`cfg!(windows)`), which this run
exercised for the first time.

## 2. The dist is the shipping dist, by hash

The 43-file tree was pulled from the build host (`C:\build\dist` on Ryo-PC), pushed to the VM,
and re-hashed **on the VM** with `Get-FileHash -Algorithm SHA256`:

```
43 files  →  diff against spikes/s4/artifacts/FINAL-dist-spec-aom-sha256.txt  →  identical
```

All 43 sha256 values and byte sizes match the manifest line for line, so the tree exercised
here is byte-identical to the one S2c signed off and S4 re-proved AVIF on.

## 3. LGPL assertion (not assumed — printed)

`ffmpeg -version` on the VM (full text in the transcript below):

```
ffmpeg version N-128273-g836436cfb9-20261101
enable-gpl occurrences in `ffmpeg -version`: 0
--enable-gpl token occurrences: 0
--enable-version3 present (LGPLv3): True
--disable-libx264 present: True
--disable-libx265 present: True
encoder rows for libx264/libx265: 0
libx26[45] substrings in `ffmpeg -encoders`: 0
```

The *encoder* check is the load-bearing one: the configure string necessarily contains the
words `--disable-libx264` / `--disable-libx265`, so a grep of `-version` alone would be
ambiguous. `ffmpeg -encoders` lists neither, which is what the licence boundary actually
depends on. Encoders the pipeline used in this run: `libaom-av1`, `libvpx-vp9`, `libopenh264`,
`mpeg4`, `gif`, `libmp3lame`, `aac`, `flac`, `libopus`, `pcm_s16le`, `pcm_s24le`, `vorbis`.

## 4. The matrix extension (read this before comparing counts)

The Linux run's 109 rows cover six legacy still sources; the committed matrix had **no
HEIC-source row at all**, so a dist with a working libheif had nothing to prove on decode,
and `heic -> jpg` rested on the S2 manual spike (`EV_V7`). This card added one section to
`spikes/c2/e2e.rs` — `## heif sources (decode; avif + heic input)`, 11 rows through the same
`engine::convert::convert` + `verify::magic_matches` + ffprobe path. Nothing above that
section was changed, so the first 109 rows are the committed matrix unchanged.

Arithmetic: 109 (committed: 103 PASS + 6 AVIF SKIP on Linux) + 11 (added heif rows) = **120**.

## 5. Transcript (verbatim, `C:\ezc\run-c2win.ps1`)

```
== host ==
Microsoft Windows 11 Pro 10.0.26200 build 26200, 6 vCPU, 7.9 GB
== engines ==
vips-8.18.7

        VipsForeignLoadHeif (heifload_base), load a HEIF image, priority=0
          VipsForeignLoadHeifFile (heifload), load a HEIF image (.heic, .heif, .avif), priority=0, is_a, get_flags, 
header, load
          VipsForeignLoadHeifBuffer (heifload_buffer), load a HEIF image, priority=0, is_a_buffer, get_flags, header, 
load
          VipsForeignLoadHeifSource (heifload_source), load a HEIF image, nocache, priority=0, is_a_source, get_flags, 
header, load
ffmpeg version N-128273-g836436cfb9-20261101 Copyright (c) 2000-2026 the FFmpeg developers
== matrix ==
engines: vips="C:\\ezc\\dist\\vips.exe" ffmpeg="C:\\ezc\\ffmpeg\\ffmpeg.exe"
fixtures: C:\ezc\fixtures

## still images (vips only)
PASS  Jpeg->"png" sample.jpg       3537B png 3537B
PASS  Jpeg->"webp" sample.jpg      1188B webp 1188B
PASS  Jpeg->"tiff" sample.jpg      3508B tiff 3508B
PASS  Jpeg->"avif" sample.jpg      899B avif 899B
PASS  Jpeg->"gif" sample.jpg       64x48 frames=Some((100, 4)) 2904B
PASS  Png->"jpg" sample.png        2781B jpg 2781B
PASS  Png->"webp" sample.png       1130B webp 1130B
PASS  Png->"tiff" sample.png       522B tiff 522B
PASS  Png->"avif" sample.png       875B avif 875B
PASS  Png->"gif" sample.png        64x48 frames=Some((100, 4)) 1680B
PASS  WebP->"jpg" sample.webp      2904B jpg 2904B
PASS  WebP->"png" sample.webp      4346B png 4346B
PASS  WebP->"tiff" sample.webp     4268B tiff 4268B
PASS  WebP->"avif" sample.webp     971B avif 971B
PASS  WebP->"gif" sample.webp      64x48 frames=Some((100, 4)) 2812B
PASS  Tiff->"jpg" sample.tif       2781B jpg 2781B
PASS  Tiff->"png" sample.tif       626B png 626B
PASS  Tiff->"webp" sample.tif      1130B webp 1130B
PASS  Tiff->"avif" sample.tif      875B avif 875B
PASS  Tiff->"gif" sample.tif       64x48 frames=Some((100, 4)) 1680B
PASS  Gif->"jpg" sample.gif        3010B jpg 3010B
PASS  Gif->"png" sample.gif        770B png 770B
PASS  Gif->"webp" sample.gif       1332B webp 1332B
PASS  Gif->"tiff" sample.gif       774B tiff 774B
PASS  Gif->"avif" sample.gif       964B avif 964B
PASS  Jpeg->"png" exif_icc.jpg     3468B png 3468B
PASS  Jpeg->"webp" exif_icc.jpg    1490B webp 1490B
PASS  Jpeg->"tiff" exif_icc.jpg    4078B tiff 4078B
PASS  Jpeg->"avif" exif_icc.jpg    1740B avif 1740B
PASS  Jpeg->"gif" exif_icc.jpg     200x400 frames=Some((100, 4)) 8263B

## heif sources (decode; avif + heic input)
PASS  Heic->"jpg" sample.heic      5747B jpg 200x400 codec=Some("mjpeg") 5747B
PASS  Heic->"png" sample.heic      1940B png 200x400 codec=Some("png") 1940B
PASS  Heic->"webp" sample.heic     740B webp 200x400 codec=Some("webp") 740B
PASS  Heic->"tiff" sample.heic     2788B tiff 200x400 codec=Some("tiff") 2788B
PASS  Heic->"avif" sample.heic     990B avif 200x400 codec=Some("av1") 990B
PASS  Heic->"gif" sample.heic      7756B gif 200x400 codec=Some("gif") 7756B
PASS  Avif->"jpg" sample.avif      2594B jpg 64x48 codec=Some("mjpeg") 2594B
PASS  Avif->"png" sample.avif      2350B png 64x48 codec=Some("png") 2350B
PASS  Avif->"webp" sample.avif     1162B webp 64x48 codec=Some("webp") 1162B
PASS  Avif->"tiff" sample.avif     2412B tiff 64x48 codec=Some("tiff") 2412B
PASS  Avif->"gif" sample.avif      2454B gif 64x48 codec=Some("gif") 2454B

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
      source: r_frame_rate=Some((30, 1)) avg_frame_rate=Some((30, 1)) is_vfr=false
PASS  VFR -> CFR                   Some((30, 1)) == Some((30, 1)) (constant)
PASS  trim 0.5 s + 1.0 s           duration=1000 ms, frames=Some((30, 1))
PASS  video resize to 160          160x120

SUMMARY pass=120 fail=0 skip=0
EXITCODE=0


```

## 6. Parity with the Linux leg

Same harness, same fixtures, and where the engines agree the byte counts agree too — e.g.
`Jpeg->"png" sample.jpg 3537B`, `WebP->"png" sample.webp 4346B`,
`Wav->"wav" 22128B`-class PCM sizes, `tags=5 items, 1 pictures`, `VFR -> CFR Some((30, 1))`
in both transcripts. The Windows-only deltas are the HEIF rows and the file sizes of the
container formats that differ between builds.

The property set is the one the Linux leg checked: bytes, magic family, dimensions,
video codec, frame-rate constancy (`r_frame_rate == avg_frame_rate`), audio codec + sample
rate + channel count, EXIF/ICC survival and stripping, and `lofty` tag readback
(5 items + 1 picture into wav/flac/m4a/ogg/opus/aac).

## 7. Matrix cells this transcript makes Proven

See `src-tauri/src/model/matrix.rs` — evidence id `C2W-AVIF`, `C2W-HEIC-DECODE`,
`C2W-AVIF-DECODE`, `C2W-HEIF-GIF`, all sourced from this file. Before this run the AVIF
output cells cited `V8` (the S2b manual spike) or were `Planned`; HEIC decode had no
matrix-wide evidence at all.

## 8. What this does NOT prove

- **macOS.** Unmeasured, as before.
- **Performance.** No wall-clock or throughput claim is made from this run; the numbers in
  the transcript are output sizes, not timings.
- **HEIC encode.** Still `Cut`: x265 is GPL-2 (licence) and Media Foundation HEVC needs a
  Store codec pack. This run does not change that.
- **Linux parity for heif.** The Debian libvips used by the Linux leg still cannot load its
  HEIF module; nothing here says otherwise. AVIF/HEIC are proven **on the dist the app
  ships**, which is the point of the card.
- **Animated GIF input.** `vips gifload` returns page 0, so a GIF source converts frame 1
  only (v1 decision: offer it, label it). `sample.gif` in this run is a single-frame-paged
  fixture; the multi-frame semantics are the product decision recorded in
  `docs/ARCHITECTURE.md`, not something this transcript measures.
