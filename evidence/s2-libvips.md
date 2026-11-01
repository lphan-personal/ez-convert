# S2 spike — self-built libvips 8.18.7 (GPL-free) + HEIC decode on Windows

**Status:** done — all four acceptance criteria met with measured evidence.
**Host:** Ryo-PC (`10.0.0.103`), Windows 11 Pro 26200, MSYS2/UCRT64, gcc 16.2.0, meson 1.12.1, ninja 1.13.2, cmake 4.4.5.
**Date:** 2026-11-01 (UTC), build under `C:\build` on the Windows box.
**Sources (sha256):**
- `vips-8.18.7.tar.xz` = `5baaead3b0bb20ffdb9e9ff09aa9fda08620923df77b63b436654cb5e0b3bf94`
- `libheif-1.23.7.tar.gz` = `f7483e818503ea6224c8863e4a56ac02c7089021a98472c811c12aa19faf2475`

Recipe: [`scripts/build-vips-gplfree-windows.sh`](scripts/build-vips-gplfree-windows.sh) — end-to-end, idempotent,
`VARIANT=spec|all`. It was run **from a wiped prefix and dist** to produce the numbers below.

---

## 1. Acceptance criterion 1 — `vips --vips-config`

Run from the shipped directory with `/ucrt64` **removed from PATH** (proves the dir is self-contained):

```
$ env -i PATH=C:\build\dist:/usr/bin:/c/Windows/system32  dist/vips.exe --vips-config
enable debug: false
enable deprecated: true
enable modules: false
enable C++ binding: true
enable RAD load/save: true
enable Analyze7 load: true
enable PPM load/save: true
enable GIF load: true
FFTs with fftw: false
SIMD support with libhwy: true
ICC profile support with lcms2: true
deflate compression with zlib: true
text rendering with pangocairo: false
font file support with fontconfig: false
EXIF metadata support with libexif: true
JPEG load/save with libjpeg: true
UHDR load/save with libuhdr: false
JXL load/save with libjxl: false (dynamic module: false)
JPEG2000 load/save with OpenJPEG: false
PNG load/save with libpng: true
image quantisation with imagequant or quantizr: false      <-- GPL path OFF
TIFF load/save with libtiff-4: true
image pyramid save with libarchive: false
HEIC/AVIF load/save with libheif: true (dynamic module: false)   <-- HEIC ON
WebP load/save with libwebp: true
PDF load with PDFium or Poppler: false (dynamic module: false)   <-- GPL path OFF
SVG load with librsvg: false
EXR load with OpenEXR: false
WSI load with OpenSlide: false (dynamic module: false)
Matlab load with Matio: false
NIfTI load/save with libnifti: false
FITS load/save with cfitsio: false
GIF save with cgif: false
RAW load with libraw_r: false
Magick load/save with MagickCore: false (dynamic module: false)
```

`vips --version` → `vips-8.18.7`. Registered loaders include
`heifload` / `heifload_buffer` / `heifload_source` (from `vips -l foreign`).

The build flags that produce this are literally `-Dimagequant=disabled -Dquantizr=disabled
-Dpoppler=disabled -Dpdfium=disabled -Dmagick=disabled -Dopenjpeg=disabled`
plus `-Dheif=enabled` (against **our** libheif), `-Dmodules=disabled`, `-Dintrospection=disabled`.
`imagequant`, `poppler`, `pdfium`, `magick`, `openjpeg` are real `meson_options.txt` entries in
vips 8.18.7 — verified from the tarball before use, not assumed.

## 2. Acceptance criterion 2 — no libimagequant in the import table

`objdump -p dist/libvips-42.dll | awk '/DLL Name:/{print $3}'` — verbatim, 27 entries:

```
libheif.dll               KERNEL32.dll              api-ms-win-crt-convert-l1-1-0.dll
api-ms-win-crt-filesystem-l1-1-0.dll                api-ms-win-crt-heap-l1-1-0.dll
api-ms-win-crt-locale-l1-1-0.dll                    api-ms-win-crt-math-l1-1-0.dll
api-ms-win-crt-private-l1-1-0.dll                   api-ms-win-crt-runtime-l1-1-0.dll
api-ms-win-crt-stdio-l1-1-0.dll                     api-ms-win-crt-string-l1-1-0.dll
api-ms-win-crt-time-l1-1-0.dll                      api-ms-win-crt-utility-l1-1-0.dll
libexif-12.dll  libexpat-1.dll  libgio-2.0-0.dll  libglib-2.0-0.dll  libgobject-2.0-0.dll
libhwy.dll  libintl-8.dll  libjpeg-8.dll  liblcms2-2.dll  libpng16-16.dll  libtiff-6.dll
libwebp-7.dll  libwebpdemux-2.dll  libwebpmux-3.dll
```

(The remaining runtime DLLs in the dist — libiconv, libpcre2, libzstd, liblzma, libdeflate,
libffi, libwinpthread, libgcc_s_seh, libstdc++, libdav1d, libde265, zlib, libjbig, libLerc,
libsharpyuv — are pulled in by glib/gio/tiff/libheif, and are all in the closure-audited set.)

`dist/libheif.dll` imports: `libdav1d-7.dll`, `libde265-0.dll`, `libsharpyuv-0.dll`, runtime only.

Full audit over all 36 files in the dist:

```
== BANNED import audit (imagequant/poppler/magick/x265/x264) ==
  banned import hits: 0
== banned files present in dist? ==
  none
```

The byte sequence `imagequant` **does** occur exactly once in `libvips-42.dll`, and it is not a
symbol or an import — it is the config-report literal itself:

```
$ strings -n 4 dist/libvips-42.dll | grep -i imagequant
image quantisation with imagequant or quantizr: false
```

with `objdump -x` and `objdump -p` returning **no** imagequant match at all.

### Baseline contrast — measured on the official Windows builds the same way

| build | libvips-42.dll | import/embed of libimagequant | HEIC decode of `orient6.heic` |
|---|---|---|---|
| `vips-dev-x64-web-8.18.7` (dynamic) | 2,561,024 B | **`DLL Name: libimagequant.dll`** (and `libimagequant.dll` is shipped) | **fails**: `error in tile 0 x 0 / 0 x 1`, no output file |
| `vips-dev-x64-web-8.18.7-static` | 19,018,240 B | compiled in — `--vips-config` says `image quantisation with imagequant: true` | **fails**, same tile errors |
| **ours (spec variant)** | 4,124,559 B | none | **works** (below) |

The stock bundles' `libheif.dll` imports only `libaom.dll` + `libsharpyuv-0.dll` — no `libde265`,
which is exactly why HEIC fails there. The card's premise is confirmed by our own measurement, not
by the research doc.

**Additional finding not in the card:** MSYS2's *stock* `libheif.dll` (1.23.7) hard-imports
`libx264-165.dll` **and** `libx265-217.dll` — both GPL-2.0. So linking the distro libheif would
have re-contaminated the LGPL position even after fixing libvips. libheif therefore had to be
rebuilt too, decoders-only:

```
libde265 HEVC decoder  : + built-in
Dav1d AV1 decoder      : + built-in
x265 HEVC encoder      : - disabled
x264 AVC encoder       : - disabled
AOM AV1 encoder        : - disabled
```

## 3. Acceptance criterion 3 — real .heic → .jpg with correct orientation

Fixture generated on the build box (`scripts/08-fixture.sh`): a 400×200 four-quadrant PNG
(TL red / TR green / BL blue / BR yellow) → JPEG carrying a hand-built, spec-correct EXIF APP1 with
`Orientation = 6` ("rotate 90° CW to display") → encoded to HEIC with libheif's `heif-enc`
(package `mingw-w64-ucrt-x86_64-libheif` 1.23.7 — used **only** as a fixture generator; it is GPL-linked
via x265 and is never part of the shipped set).
Expected result after a correct autorot: **200×400** (portrait) with TL blue, TR red, BL yellow,
BR green.

```
=== source .heic as our vips sees it ===
orient6.heic: 200x400 uchar, 3 bands, srgb, heifload
width: 200   height: 400
exif-ifd0-Orientation: 1 (Top-left...)      <- libheif applied irot at decode; no double-rotation

=== output JPEG ===
spec_out.jpg: 200x400
exif-ifd0-Orientation: 1 (Top-left, Short, 1 components, 2 bytes)
corners TL TR BL BR:
30 59 221      (blue)
220 30 32      (red)
246 220 22     (yellow)
30 200 31      (green)
```

Same result via the plain-JPEG EXIF path (proves `vips_autorot` itself, independent of HEIC):

```
=== TEST A: EXIF-orientation JPEG ===
in : orient6.jpg   400x200  exif-ifd0-Orientation: 6
out: jpg_autorot.jpg 200x400  exif-ifd0-Orientation: 1
corners: 30 59 221 / 220 30 32 / 246 220 22 / 30 200 31   -> 90° CW, correct
```

Third-party, real-world HEIC files (downloaded, not synthesised) also decode:

```
example.heic   (libheif project sample) 1280x854 -> example_out.jpg  1280x854
sample1.heic   (public sample)          1440x960 -> sample1_out.jpg  372,927 bytes
```

Other format paths through the same binary, inside the isolated dist:

```
jpg->png OK | png->webp OK | webp->tiff OK | tiff->png OK
heic->avif FAILED: heifsave: Unsupported compression   (see gap G2)
```

## 4. Acceptance criterion 4 — recipe committed + artifact size

Recipe: `scripts/build-vips-gplfree-windows.sh` (fetch → libheif decoders-only → libvips →
dist closure → banned-import audit → size + isolated smoke test). Verification scripts:
`08-fixture.sh`, `11-orientation-tests.sh`, `13-finalize.sh`, `16-evidence.sh`, `17-baseline.sh`.
Raw evidence bundle: `evidence/ezconvert-s2-vips-evidence.tar.gz`
(12,086,664 B, sha256 `1a049fc2c81372d0871804573677ac184b5688e41d715fa66dfe8e8112c87648`, 60 entries:
dist, configs, build logs, fixtures).

| variant | files | size | notes |
|---|---|---|---|
| `VARIANT=spec` (**recommended**) | 36 | **25,412 KiB = 24.82 MiB** (25,934,578 B) | jpeg/png/tiff/webp/heic+avif *decode*/gif-load, EXIF+ICC+autorot; no SVG in vips (resvg sidecar), no JXL/EXR/RAW/J2K/PDF |
| `VARIANT=all` | 71 | 66,380 KiB = 64.8 MiB (67,790,721 B) | adds librsvg+cairo/pango stack, libjxl, OpenEXR, libraw, libaom (AVIF encode), libarchive |
| stock `vips-dev-x64-web` dynamic | — | 31.0 MiB | + GPL libimagequant, HEIC broken |
| stock `vips-dev-x64-web` static | — | 19.0 MiB single DLL | GPL imagequant compiled in, HEIC broken |

Largest entries in the spec variant: `libvips-42.dll` 4.12 MB, `libheif.dll` 3.53 MB,
`libstdc++-6.dll` 2.66 MB, `libdav1d-7.dll` 1.93 MB, `libgio` 1.92 MB, `libglib` 1.57 MB,
`libzstd` 1.22 MB, `libiconv` 1.14 MB, `libde265` 0.96 MB, `libjpeg` 0.96 MB, `vips.exe` 62 KB.

A clean run of the recipe — libheif **and** libvips, from a wiped `prefix/` and `dist/` — completed
inside a single 580-second window on this 24-thread box. (That run then tripped a bash bug in the
recipe's *tail* — `${!ARRAY[*]:-none}` is invalid bash — after the builds and install had already
succeeded; the bug is fixed in the committed script and the dist/audit stages were re-run
separately by `13-finalize.sh`, which is where the size and audit numbers above come from.)

---

## Findings that change downstream cards

- **F1 — MSYS2 stock libheif is GPL-contaminated** (`libx264`, `libx265` imports). Any plan that says
  "just add libheif+libde265 from the toolchain" would fail license hygiene. libheif must be built
  from source with encoders disabled and plugin loading off (no runtime `.dll` plugins → smaller
  attack surface, nothing can be side-loaded).
- **F2 — HEIC orientation arrives already applied.** libheif applies `irot`/`imir` at decode, and the
  EXIF orientation is then `1`. So `vips autorot` is a safe no-op on HEIC — the classic
  double-rotation trap lives on the **JPEG/EXIF** path, not HEIC. Both paths verified above.
- **F3 — static single-DLL builds are not the escape hatch they look like**: the official "static"
  bundle still compiles GPL imagequant in. A *self-built* static vips is still worth evaluating for
  packaging (one file, no DLL search path, no plugin side-loading) — see follow-up.
- **F4 — `vips.exe` is a thin launcher** (62 KB) over `libvips-42.dll`; the sidecar contract should
  ship the whole directory, not a lone exe.

## Gaps (honest, unresolved)

- **G1 — SVG is not in this build.** librsvg+pango+cairo+fontconfig adds ~18 MiB to the dist
  (64.8 vs 24.8). SPEC's engine table already picks **resvg** (1.8 MB, Apache-2.0/MIT) as the SVG
  engine, so the sidecar should route svg→PNG through resvg and then into vips. This needs an
  explicit decision in ARCHITECTURE.md, not silently in a build flag.
- **G2 — AVIF *save* is unsupported** in the spec variant: `heifsave: Unsupported compression`
  because no AV1 encoder is linked (only dav1d, a decoder). AVIF *decode* works. If the target matrix
  needs avif **output**, add the libaom encoder (+9.7 MB `libaom.dll`, BSD-licensed, no GPL) or
  rav1e. Not added unilaterally.
- **G3 — GIF *save* is off** (`GIF save with cgif: false`; GIF load works via vendored nsgif). The
  `cgif` package was not available under the name I tried; needs `mingw-w64-ucrt-x86_64-cgif`
  resolution or a CMake build of cgif. Cheap (+~30 KB) once the package/flag is right.
- **G4 — no git repo exists yet** for EZ Convert (`/opt/data/converter` and the board dir are not
  repos), so "committed as a script" means: staged at `/opt/data/converter/build/windows/` and
  attached to the board card. The repo scaffolding task must `git add` this directory as its first
  commit.
- **G5 — `vips.exe` printed `bad seek to <n>` warnings** on the tiny 2.3 KB fixture HEIC during a
  heic→avif save attempt. Harmless for reads/writes observed (all conversions produced correct
  output) but worth re-testing with a large HEIC before shipping; note for the fuzz corpus.
- **G6 — build host hygiene:** MSYS2, cmake, libheif/libx265/x264 tools now live on Ryo's daily PC
  (`C:\msys64`, `C:\build`). Fine for a spike, not a release path — CI (GitHub Actions windows
  runner, per packaging strategy) must own the real build.

## Reproduce

```bash
# on the Windows box, from MSYS2 ucrt64 shell
VARIANT=spec bash /c/build/src/build-vips-gplfree-windows.sh   # or VARIANT=all
bash 13-finalize.sh    # dist closure + banned-import audit + size + isolated smoke tests
bash 11-orientation-tests.sh   # EXIF + HEIC orientation proof
```
