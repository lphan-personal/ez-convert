# S2b — libaom AVIF encoder enabled, avif round-trip proved (EXIF + ICC)

**Status:** done — every acceptance item below is backed by command output, not by a plan.
**Host:** Ryo-PC (`10.0.0.103`), Windows 11 Pro 26200, MSYS2/UCRT64, gcc 16.2.0, meson 1.12.1,
ninja 1.13.2, cmake 4.4.5, 24 threads.
**Date:** 2026-11-01 (UTC). Build tree `C:\build`.
**Card:** `t_803b8add` (SPEC.md §9.1 decision **D1**: AVIF output IN).
**Predecessor:** `EVIDENCE-S2-libvips.md` (spec dist, AVIF decode only, gap **G2** = "avif save
unsupported"). This report closes G2 and G5.

**Sources (sha256, re-verified this run):**

```
5baaead3b0bb20ffdb9e9ff09aa9fda08620923df77b63b436654cb5e0b3bf94 *vips-8.18.7.tar.xz
f7483e818503ea6224c8863e4a56ac02c7089021a98472c811c12aa19faf2475 *libheif-1.23.7.tar.gz
```

**Artifacts:**

| artifact | sha256 |
|---|---|
| `scripts/build-vips-gplfree-windows.sh` (updated, `spec-aom` variant) | `768a4b646c27ab5111266be57d23df2af5c598e8352f830c8fcc9200dfdc0c1e` |
| `scripts/13-finalize.sh` (variant-aware audit + isolated acceptance) | `f22c5048d3d138f770718358c747cecaa0915b9fa4af8639a2260edb8c558607` |
| `scripts/18-avif-tests.sh` (new, the S2b proof suite — 33 assertions) | `d5314fbfa29e9b21a826a2835a35417eab9dccf6e7b90a8dfdd5a63ccbb74224` |
| `fixtures/mkjpeg_exif2.py` (new, EXIF IFD0 injector) | `f62add5c21c0efef3137c866f5ec1cbc98db0878c0b06811c500150c4990ad08` |
| `docs/LICENSES.md` (new, libaom + full engine inventory) | see §7 |

---

## 1. What changed in the build

`VARIANT=spec-aom` is a **third** variant and is `spec` plus exactly one flag:

```diff
-AOM_DECODER=OFF; [ "$VARIANT" = "all" ] && AOM_DECODER=ON
+AOM_DECODER=OFF; AOM_ENCODER=OFF
+case "$VARIANT" in
+  all)      AOM_DECODER=ON ;;
+  spec)     : ;;
+  spec-aom) AOM_ENCODER=ON ;;
+esac
 ...
-  -DWITH_AOM_DECODER=$AOM_DECODER -DWITH_AOM_ENCODER=OFF \
+  -DWITH_AOM_DECODER=$AOM_DECODER -DWITH_AOM_ENCODER=$AOM_ENCODER \
```

`spec` was **not** repurposed — it still builds AVIF-decode-only and the recipe now *asserts* that
(`PASS: spec — no libaom in the closure`). `all` is untouched. The vips feature set for `spec-aom`
is the `spec` set verbatim (single `case` arm shared by both, with a comment forbidding a fork).

New knobs: `BUILD_JOBS` (default 8) and `STRICT_LICENCE` (see §6).

**Prerequisite correction:** the card says `mingw-w64-ucrt-x86_64-libaom`. **That package does not
exist**; the MSYS2 package is `mingw-w64-ucrt-x86_64-aom` (3.15.3-1, provides `libaom.dll`,
`aom.pc`, `aom/aom_encoder.h`). The recipe now checks `pkg-config --exists aom` and fails loudly
instead of silently building an encoder-less libheif:

```
=== libheif 1.23.7 (variant=spec-aom) ===
aom encoder library: 3.15.3 at C:/msys64/ucrt64/bin/..
libheif codec flags: AOM_DECODER=OFF AOM_ENCODER=ON
-- Found AOM: C:/msys64/ucrt64/lib/cmake/AOM
-- Looking for AOM_USAGE_GOOD_QUALITY
-- Looking for AOM_USAGE_GOOD_QUALITY - found
libde265 HEVC decoder                 : + built-in
x265 HEVC encoder                     : - disabled
Kvazaar HEVC encoder                  : - disabled
AOM AV1 decoder                       : - disabled      <-- decode stays dav1d
AOM AV1 encoder                       : + built-in      <-- the whole point of the card
Dav1d AV1 decoder                     : + built-in
x264 AVC encoder                      : - disabled
```

Build was run **from a wiped `prefix/`, `dist/`, `heif-build/` and `vips-build/`** and completed in a
single pass (531 ninja targets for libvips). Log: `logs/spec-aom-build.log`.

---

## 2. Acceptance — `vips --vips-config` from the dist, `/ucrt64` off PATH

```
$ env -i PATH=C:\build\dist:/usr/bin:/c/Windows/system32:/c/Windows  dist/vips.exe --vips-config
enable modules: false
ICC profile support with lcms2: true
EXIF metadata support with libexif: true
JPEG load/save with libjpeg: true
PNG load/save with libpng: true
image quantisation with imagequant or quantizr: false
TIFF load/save with libtiff-4: true
HEIC/AVIF load/save with libheif: true (dynamic module: false)
WebP load/save with libwebp: true
PDF load with PDFium or Poppler: false (dynamic module: false)
SVG load with librsvg: false
EXR load with OpenEXR: false
GIF save with cgif: false
Magick load/save with MagickCore: false (dynamic module: false)
```

**This line alone is not proof.** Measured on the `spec` build in S2 *and* re-measured here, the
config report says `HEIC/AVIF load/save with libheif: true` even when no AV1 encoder is linked —
`heifsave` then fails with `heifsave: Unsupported compression`. The capability must be probed by an
actual save. That is why the audit asserts the import table instead:

```
$ objdump -p dist/libheif.dll | grep "DLL Name"
  libgcc_s_seh-1.dll
  KERNEL32.dll
  api-ms-win-crt-*.dll (12 entries)
  libwinpthread-1.dll
  libstdc++-6.dll
  libaom.dll            <-- AV1 encoder, present
  libdav1d-7.dll        <-- AV1 decoder
  libde265-0.dll        <-- HEVC decoder
  libsharpyuv-0.dll
```

`libaom.dll` drags in **nothing** new — its own import table is CRT + `libwinpthread` only:

```
$ objdump -p dist/libaom.dll | grep "DLL Name"
  KERNEL32.dll
  api-ms-win-crt-*.dll (11 entries)
  libwinpthread-1.dll
```

---

## 3. Acceptance — size delta vs the S2 spec baseline

Baseline captured before the change (`baseline/spec-dist-baseline.tar.gz`,
sha256 `cb9c4551a621bdcf04378b5feda0915793e1798e7d001be33c43a3ff06ebb5c0`) and diffed file by file.

```
spec baseline : 36 files, 25935876 bytes   (35 modules + vips-config.txt)
spec-aom      : 37 files, 35649889 bytes   (36 modules + vips-config.txt)
                du: 34896 KiB = 34.08 MiB
```

| | files | bytes | MiB |
|---|---|---|---|
| `VARIANT=spec` (baseline) | 36 | 25,935,876 | 24.73 |
| `VARIANT=spec-aom` | 37 | 35,649,889 | 34.00 |
| **delta** | +1 module | **+9,714,013** | **+9.26** |

The delta is fully accounted for, to the byte: `libaom.dll` **+9,682,038** plus `libheif.dll`
**+31,975** (3,526,374 → 3,558,349). Nothing else grew.

> Card note: the card's baseline figure reads `36-file / 24,934,578 B`. Measured, the spec dist is
> **25,935,876 B** including `vips-config.txt` (25,934,578 B for the 35 modules alone). The card's
> `24,934,578` is a digit transposition; the "≈34 MiB / +9.7 MB" planning figure is confirmed
> (+9.71 MB decimal, 34.00 MiB).

Per-file comparison of all 35 baseline modules (shas + byte-level diff where hashes differ):

```
identical: 30
libheif.dll        3,526,374 -> 3,558,349   DIFFERS  size delta +31,975
libvips-42.dll     4,124,559 -> 4,124,559   DIFFERS  6 byte(s) differ at [136,137, 216,217, 3366404,3366405]
vips.exe              61,943 ->    61,943   DIFFERS  5 byte(s) differ at [136,137, 216,217,218]
vipsheader.exe        50,596 ->    50,596   DIFFERS  4 byte(s) differ at [136,137, 216,217]
vipsthumbnail.exe     57,126 ->    57,126   DIFFERS  4 byte(s) differ at [136,137, 216,217]
libaom.dll                 - -> 9,682,038   NEW
```

Offsets 136–137 are the PE `TimeDateStamp` (`e_lfanew = 0x80`, stamp at `0x88`):
`spec` = 1793500770 (2026-11-01T02:39:30Z), `spec-aom` = 1793502532 (2026-11-01T03:08:52Z). The
4-byte stamp differs only in its low two bytes, which is why 2 bytes show. **The remaining 2 bytes
at offset 3,366,404 in `libvips-42.dll` were not identified** — they are the only unexplained byte
difference in the whole comparison, and they are inside the image, not in the import table.

`libaom.dll` sha256 = `9b2bf9122f924b9ad7f9c2be31133758331fd7a7fd33261a63a6d620b31f1883`
`libheif.dll` sha256 = `54f9e58e132ed36ee6f4d22639950ebf6e3e9f95acc9ee706cde7eab20306102`
`libvips-42.dll` sha256 = `618c9aa0400d201f440e897e44652ca6aaeda11550c60b833c1b2a347a8ec2f3`

---

## 4. Acceptance — avif save/round-trip with EXIF **and** ICC surviving

### Fixtures (built on the box, `scripts/18-avif-tests.sh` §0)

A 2 MP and an 8 MP photo-like source derived from the real-world `sample1.heic`, ICC-embedded by
libvips itself (`jpegsave --profile sRGB Color Space Profile.icm`, 3,144 B, a genuine Windows sRGB
ICC v2 profile — no hand-rolled blob) and given a 5-tag EXIF IFD0 by `fixtures/mkjpeg_exif2.py`:

```
wrote src2mp.jpg: 1390559 bytes, IFD0 tags=['0x10f','0x110','0x112','0x131','0x132'] orientation=1
wrote src8mp.jpg: 3550197 bytes, IFD0 tags=['0x10f','0x110','0x112','0x131','0x132'] orientation=1
--- src2mp.jpg metadata ---
width: 1728      height: 1152
exif-ifd0-Make: EZConvertTest (ASCII, 14 bytes)
exif-ifd0-Model: AVIF Proving Rig (ASCII, 17 bytes)
exif-ifd0-Orientation: 1 (Top-left, Short, 1 component, 2 bytes)
exif-ifd0-Software: libvips 8.18.7 spec-aom (ASCII, 24 bytes)
exif-ifd0-DateTime: 2026:11:01 03:00:00 (ASCII, 20 bytes)
icc-profile-data: 3144 bytes of binary data
```

### png → avif and jpg → avif (the operation that used to fail)

```
$ dist/vips.exe heifsave small.png small.avif --compression av1 --Q 60 --effort 6 --bitdepth 8 --keep all
  PASS  png -> avif (400x200)            -> small.avif 3,863 bytes
$ dist/vips.exe heifsave src2mp.jpg big2mp.avif --compression av1 --Q 60 --effort 6 --bitdepth 8 --keep all
  PASS  jpg -> avif (2 MP)               -> big2mp.avif 302,068 bytes

--- container brand (bytes 4..12) ---
small.avif    \0 \0 \0 034 f t y p a v i f      <-- ISO-BMFF brand 'avif', not 'mif1'/'heic'
big2mp.avif   \0 \0 \0 034 f t y p a v i f

--- avif headers ---
small.avif: 400x200 uchar, 3 bands, srgb, heifload
big2mp.avif: 1728x1152 uchar, 3 bands, srgb, heifload
```

### EXIF + ICC inside the avif (the assertion the card asks for)

```
--- big2mp.avif, vipsheader -a ---
width: 1728
height: 1152
bands: 3
format: uchar
exif-ifd0-Make: EZConvertTest (ASCII, 14 bytes)
exif-ifd0-Model: AVIF Proving Rig (ASCII, 17 bytes)
exif-ifd0-XResolution: 72/1
exif-ifd0-YResolution: 72/1
exif-ifd0-ResolutionUnit: 2 (Inch)
exif-ifd0-Software: libvips 8.18.7 spec-aom (ASCII, 24 bytes)
exif-ifd0-DateTime: 2026:11:01 03:00:00 (ASCII, 20 bytes)
icc-profile-data: 3144 bytes of binary data      <-- same 3144 B profile, byte count preserved

PASS  small.avif kept icc-profile-data
PASS  big2mp.avif kept icc-profile-data
PASS  big2mp.avif kept exif-ifd0-Make
PASS  big2mp.avif kept exif-ifd0-Model
PASS  big2mp.avif kept exif-ifd0-Software
PASS  big2mp.avif kept exif-ifd0-DateTime
```

`exif-ifd0-Orientation` is deliberately **not** asserted for this fixture: value `1` is the identity
orientation and libvips normalises it away on save. The non-default case is measured separately
below, because that is where the double-rotation trap lives.

### avif → jpg (decode through dav1d)

```
$ dist/vips.exe jpegsave small.avif small_back.jpg --Q 95        rc=0
$ dist/vips.exe jpegsave big2mp.avif big2mp_back.jpg --Q 95      rc=0
--- big2mp_back.jpg ---
width: 1728      height: 1152
exif-ifd0-Make: EZConvertTest (ASCII, 14 bytes)
exif-ifd0-Model: AVIF Proving Rig (ASCII, 17 bytes)
exif-ifd0-Orientation: 1 (Top-left, Short, 1 component, 2 bytes)
icc-profile-data: 3144 bytes of binary data

PASS  avif->jpg kept the ICC profile
PASS  avif->jpg kept EXIF IFD0
PASS  avif geometry 400x200

--- quadrant pixels, source vs avif-decoded (lossy, 1/255 drift is the expectation) ---
source small.png : 220 30 30   30 200 30
avif  small.avif : 220 30 30   30 200 31
```

### Non-default EXIF orientation (Orientation = 6) — the double-rotation trap

```
--- source: 1728x1152, Orientation=6 (correct display = 1152x1728) ---
width: 1728   height: 1152   exif-ifd0-Orientation: 6 (Right-top)
$ dist/vips.exe heifsave ori6_src.jpg ori6.avif --compression av1 --Q 60 --effort 6 --bitdepth 8 --keep all   rc=0

--- reading the avif back (libheif has already applied irot at load) ---
width: 1152   height: 1728
exif-ifd0-Make: EZConvertTest
icc-profile-data: 3144 bytes of binary data

--- after decode + re-save as jpg ---
width: 1152   height: 1728     exif-ifd0-Orientation: 1 (Top-left)

PASS  avif round trip lands correctly oriented at 1152x1728
PASS  no stale EXIF Orientation in the avif (libheif rewrote it; irot carries the rotation)
PASS  orientation-6 avif still carries the rest of the EXIF IFD0
PASS  decoded avif presents Orientation=1 (irot already applied -> autorot is a no-op)
```

So the geometry is right and the file is **not** double-rotate bait: libheif converts the EXIF
orientation into an `irot` property and the EXIF block no longer carries a contradicting value.
This is the same behaviour S2 found for HEIC (finding **F2**), now confirmed for AVIF output too —
`vips autorot` after loading an avif/HEIC is a safe no-op and must **not** be the thing that
corrects orientation.

---

## 5. Acceptance — avif decode and HEIC decode unchanged

```
PASS  avif -> jpg (dav1d decode of the avif we just wrote)

--- HEIC orientation test, unchanged from S2 ---
heic in : orient6.heic  width: 200  height: 400
heic out: avif_out_heic.jpg  width: 200  height: 400  exif-ifd0-Orientation: 1
corners : 30 59 221 / 220 30 32 / 246 220 22 / 30 200 31
PASS  HEIC orientation test: 200x400 (unchanged from S2)
PASS  top-left still blue (30 59 221)
```

Real-world HEIC (not synthesised) still converts: `sample1.heic` 1440x960 → 266,357-byte avif.

### Gap G5 — the `bad seek` warning: **not reproduced**

```
--- orient6.heic (2345 bytes) -> avif, stderr captured ---
  exit=0  out=990 bytes      stderr: (empty)
  PASS  no 'bad seek' warning on orient6.heic
--- sample1.heic (293608 bytes) -> avif, stderr captured ---
  exit=0  out=266357 bytes   stderr: (empty)
  PASS  no 'bad seek' warning on sample1.heic
```

Honest reading: the S2 sighting happened on the heic→avif path **at the moment that path failed**
(`heifsave: Unsupported compression`), i.e. inside a failing encoder hand-off, not on a healthy
decode. With a real encoder linked, neither the 2,345 B nor the 293,608 B HEIC emits the warning and
both produce correct output. G5 is therefore **closed as not reproducible on a working build**, but
the fuzz corpus should still keep a byte-truncated HEIC fixture — a `bad seek` on damaged input is a
parser concern, not an encoder concern.

---

## 6. Measured encode latency (wall clock, 3 runs each, same box)

All numbers are `vips heifsave` process wall time measured with `time`, 8-bit, `keep=all`,
`--compression av1`. No estimates.

| input | setting | run 1 | run 2 | run 3 | output |
|---|---|---|---|---|---|
| 400×200 (0.08 MP) | Q60 effort6 | 0.069 s | 0.068 s | 0.070 s | 3,863 B |
| 1728×1152 (2.0 MP) | Q60 effort6 | 0.923 s | 0.916 s | 0.953 s | 296,722 B |
| 1728×1152 (2.0 MP) | Q80 effort6 | 1.084 s | 1.082 s | 1.090 s | 527,245 B |
| 3456×2304 (8.0 MP) | Q60 effort6 | 2.078 s | 2.096 s | 2.114 s | 637,278 B |
| 3456×2304 (8.0 MP) | Q60 **effort0** | 0.217 s | 0.199 s | 0.197 s | 785,574 B |
| 3456×2304 (8.0 MP) | Q60 **effort9** | 6.061 s | 6.034 s | 6.093 s | 633,409 B |

Same 8 MP image through the encoders we already ship, for scale:

| encoder | time | output |
|---|---|---|
| jpeg Q80 | 0.145 s | 1,376,541 B |
| webp Q80 | 0.610 s | 974,066 B |
| png (lossless) | 0.504 s | 19,389,662 B |
| **avif Q60 effort6** | **2.10 s** | **637,278 B** |

Decode (dav1d), avif → jpg:

| input | time |
|---|---|
| 3456×2304 avif → jpg | 0.129 s |
| 1728×1152 avif → jpg | 0.079 s |

**What this means for the UI (D1's "no latency promise until measured"):**

- avif at the recommended Q60/effort6 costs **≈1 s per 2 MP** and **≈2.1 s per 8 MP** on a
  24-thread desktop — ~14× slower to encode than JPEG, but the same order as WebP.
- Decode is *faster* than JPEG-class work at 0.13 s for 8 MP, so previewing an avif needs no special
  handling.
- Consequence: a single avif row does **not** need a modal progress bar, but a batch of 20 phone
  photos (12 MP+) will spend ~30–40 s, so the queue needs a per-file progress entry and the avif row
  should be labelled as the slow path. **effort 9 is not a UI default** (3× the time for 0.6 % of
  the bytes at Q60).
- `effort` is the knob, not "speed": see §8.

---

## 7. Acceptance — 0 banned imports, and the licence work this card triggered

```
############ AUDIT: banned imports ############
PASS: no imagequant / poppler / magick / x265 / x264 anywhere in dist

--- AV1 encoder assertion (VARIANT=spec-aom) ---
PASS: libheif.dll imports libaom.dll (AV1 encoder linked)
  dist/libaom.dll present: 9682038 bytes
```

Adding libaom pulled **nothing else** in (its import table is CRT-only, §2). But widening the audit
found a **pre-existing** GPL module that the banned list never covered:

```
--- copyleft beyond the banned list (libjbig via libtiff) ---
  WARN  /c/build/dist/libtiff-6.dll -> libjbig-0.dll
  WARN  libjbig-0.dll = mingw-w64-ucrt-x86_64-jbigkit 2.1-6, spdx:GPL-2.0, pulled in by libtiff
        OPEN exposure, pre-existing since S2, NOT introduced by libaom — docs/LICENSES.md section 4
```

`libjbig` is GPL-2.0 and arrives via libvips → libtiff, not via libaom. It is **not** declared
clean here: `docs/LICENSES.md` §4 records it as an open issue with the remediation options, the
recipe prints the WARN on every build, and `STRICT_LICENCE=1` turns it into a hard FAIL for CI.
Remediation card: **`t_57c7ad7b`** (forge — rebuild libtiff with JBIG disabled).

Licence records landed with this change: `docs/LICENSES.md` (new) + verbatim
`docs/licenses/libaom-LICENSE.txt` and `docs/licenses/libaom-PATENTS.txt` (BSD-2-Clause **and** AOM
Patent License 1.0 — both obligations, both texts retained, hashes recorded).

---

## 8. Contract corrections for A1 / SPEC §9.3

These are measured behaviours of the shipped binary; A1's engine contract should use them as-is.

1. **The option is `effort`, not `speed`.** `heifsave` exposes `effort` (gint, 0–9, default **4**);
   there is no `speed` option in vips 8.18.7. SPEC §9.3's `heifsave[compression=av1, Q, speed]`
   should read `heifsave[compression=av1, Q, effort]`.
2. **`keep=all` is already the default** (`exif:xmp:iptc:icc:other:gainmap:all`) — passing it is
   documentation, not a behaviour change.
3. **`bitdepth` defaults to 12 but does not up-convert.** An 8-bit source through
   `heifsave --compression av1` with no `--bitdepth` produced `400x200 uchar` (8-bit). Pass
   `--bitdepth 8` explicitly anyway so SDR output is deterministic rather than dependent on input
   depth inference.
4. **A missing encoder is a WARNING with exit code 0.** `--encoder rav1e` prints
   `VIPS-WARNING: heifsave: could not find rav1e`, silently falls back to libaom and **returns 0**
   with a valid 3,853-byte file. So:
   - do not use `--encoder` for capability detection;
   - do not trust exit code alone for avif — assert the output file exists and is non-empty
     (the standing "no silent failures" rule).
5. **`--vips-config` is not a capability probe.** It reported `HEIC/AVIF load/save: true` on both
   builds, including the one where avif save failed outright. Probe by doing.
6. **avif output geometry after re-read is already rotated** (§4): never call `autorot` to "fix"
   orientation of an avif that libheif produced; the transform is already applied and the EXIF
   orientation has been normalised.
7. Recommended avif preset from the measurements: `--compression av1 --Q 60 --effort 6
   --bitdepth 8 --keep all` (≈1 s / 2 MP, 0.09 bytes/px at 8 MP). Offer `effort 0` (0.20 s) as the
   "fast" preset and do not expose effort 9 in v1.

---

## 9. Build-host findings (operational, for G6)

- **Detached builds die.** Two attempts launched with `nohup … &` from an exiting ssh session
  failed with `Exit code 0xc0000142` (`STATUS_DLL_INIT_FAILED`) mid-`libheif`, at `-j 8` and at
  `-j 4`. The identical command run in a **session that stays attached** completed in one pass.
  Practical rule for this host: keep the ssh channel open for the whole build.
- `BUILD_JOBS` was added because of the above (default 8 preserved for reproducibility).
- `pacman -Qo` has **no file database** on this box (`error: No package owns …` even for files that
  demonstrably belong to an installed package), so package attribution was done with
  `pacman -Qi <name>` / `pacman -Qs` instead. `cmp` is also absent (diffutils not installed) — the
  byte comparison above was done with Python.
- Build cost: full libheif + libvips rebuild from wiped prefix/dist completed well inside one
  session; the release path stays a CI job (SPEC G6) — this box is Ryo's daily driver.

---

## 10. Reproduce

```bash
# on the Windows box, MSYS2 shell, with an ssh session held open
cd /c/build
rm -rf prefix dist src/heif-build vips-build
VARIANT=spec-aom BUILD_JOBS=4 bash /c/build/build-vips-gplfree-windows.sh
VARIANT=spec-aom bash /c/build/13-finalize.sh          # dist closure, audit, size, isolated config
D=/c/build/dist   bash /c/build/18-avif-tests.sh       # 33 assertions incl. EXIF+ICC round trip
```

Expected tail:

```
######## SUMMARY ########
  PASS=33  FAIL=0
```

## 11. Artifact identity at hand-off (concurrent change on the same files)

The recipe is a **hotspot**: follow-up card **`t_57c7ad7b`** (forge, libtiff/JBIG removal) started
while this card was finishing and extended the same two scripts — and by the time the evidence bundle
was assembled it had also redeployed them to `C:\build` and wiped `dist/`. Nothing below is inferred:
the frozen artifacts are hashed and included in the bundle.

| artifact | state |
|---|---|
| **dist, as tested** | frozen in `spec-aom-dist.tar.gz`, sha256 `ea24b4c64fb0097ad2dacec31acbca25d8ef69274eb4f214adeb058bd7eacc81`. Re-verified after freeze by extracting it: **37 files, 35,649,889 B**, `libaom.dll` `9b2bf912…1883`, `libheif.dll` `54f9e58e…6102`, `libvips-42.dll` `618c9aa0…c2f3` — all three match the values in §3. `libjbig-0.dll` present (the open issue in §7). |
| `scripts/build-vips-gplfree-windows.sh` as tested | `768a4b64…0c1e`. Preserved **verbatim** in the bundle as `build-vips-gplfree-windows.TESTED-768a4b64.sh`. |
| `scripts/13-finalize.sh` as tested | `f22c5048…8607`. **Bytes are not recoverable**: the concurrent card overwrote this file in the repo *and* on the build host after it ran. Its complete output — the audit, size table and isolated acceptance run quoted in §2/§3/§7 — is preserved in `logs/spec-aom-finalize.log`, and the file only orchestrates (assemble, audit, print); it computes nothing that is quoted. |
| `scripts/18-avif-tests.sh` as tested | `d5314fbf…4224`, unchanged by anyone else, in the bundle as `18-avif-tests.TESTED-d5314fbf.sh`. |
| `fixtures/mkjpeg_exif2.py` | `f62add5c…ad08`, unchanged. |
| `docs/LICENSES.md` | `53cfce16…6c1e` at hand-off, unchanged by the other card. |

Consequence for a re-run: use the TESTED copies to reproduce these exact bytes; use the repo tip to
get the JBIG fix, which will change the dist (expect ~−61 KB and no `libjbig-0.dll`).

---

## Policy cross-check

The mandatory **CROSS-CHECK (policy v1)** block for this work unit is posted verbatim in the card
comment thread (`t_803b8add`), which is where `docs/REFERENCE_POLICY.md` §1 requires it. It is
deliberately **not** reproduced inside this file: the block's field names are themselves
deny-listed tokens, and `docs/PRIOR_ART.md` scopes every occurrence of those tokens to
`docs/PRIOR_ART.md` and `docs/REFERENCE_POLICY.md`. Keeping the block in the comment keeps this
repo tree at zero hits.

**Open, not hidden:** `libjbig-0.dll` (GPL-2.0, via libtiff) is still in the dist — pre-existing,
tracked as `t_57c7ad7b`, enforced by `STRICT_LICENCE=1`, documented in `docs/LICENSES.md` §4.
