# THIRD-PARTY NOTICES — EZ Convert media sidecar (Windows x64)

This file, together with the licence texts in this directory, is the notices pack that ships
inside the app (`dist/licenses/` and the installer's `licenses/` folder). It exists to satisfy
the obligations that come with redistributing third-party binaries — in particular the
**LGPL corresponding-notice and source-availability duties** (LGPL-3.0 §4(d)(1), LGPL-2.1 §6(b))
that apply because libvips, libheif, libde265, glib2, libexif and libiconv are shipped as
separate DLLs next to the app.

Every version, licence string and file size below is a transcription of tool output from the
build box (**Ryo-PC**, Windows 11 26200, MSYS2/UCRT64, 2026-11-01): `pacman -Qi` for package
metadata, `sha256sum` for digests, `objdump -p` for the import graph. Nothing here is inferred.
The machine-readable form of the licence gate that enforces this inventory is
`build/windows/scripts/audit-licenses.sh` (+ `elections.tsv`, §4).

---

## 1. What actually ships

Two sidecar variants are built from the same recipe; only the AV1 encoder differs.

| variant | components | binaries | dist size | note |
|---|---|---|---|---|
| `spec` | 25 | 34 | 26,019,042 B (24.81 MiB) incl. `vips-config.txt` | AVIF **decode** only (dav1d) |
| `spec-aom` | 26 (adds `libaom.dll`) | 35 | 35,731,757 B (34.08 MiB) | AVIF **save** (SPEC §9.1 D1) |

`vips-config.txt` in the dist is a text report produced by `vips --vips-config` at finalize time,
not part of the binary set. In both variants a `licenses/` directory with the texts in §4 is added
by the finalize step, so the on-disk dist holds 43 entries for `spec-aom` (35 binaries + the report
+ 7 notice files).

---

## 2. Corresponding source for the components we build ourselves

Three components are **not** taken from MSYS2. They are built from pinned upstream tarballs
into `/c/build/prefix` and copied into the dist; the dist copy is asserted byte-identical to the
prefix build at finalize time (`PASS: dist libtiff-6.dll == the source-built prefix build`).

| component | version | upstream source (corresponding source) | sha256 of the tarball |
|---|---|---|---|
| libvips | 8.18.7 | <https://github.com/libvips/libvips/releases/download/v8.18.7/vips-8.18.7.tar.xz> | `5baaead3b0bb20ffdb9e9ff09aa9fda08620923df77b63b436654cb5e0b3bf94` |
| libheif | 1.23.7 | <https://github.com/strukturag/libheif/releases/download/v1.23.7/libheif-1.23.7.tar.gz> | `f7483e818503ea6224c8863e4a56ac02c7089021a98472c811c12aa19faf2475` |
| libtiff | 4.7.2 | <https://download.osgeo.org/libtiff/tiff-4.7.2.tar.gz> | `672bd7d10aee4606171afb864f3570b83340f6a33e2c186dc0512f7145ffdf6a` |

**Our delta** — the only change we make to these upstream sources is *configuration*, not code:

- `libheif`: decoders libde265 + dav1d (+ libaom encoder in `spec-aom`), every other codec
  `OFF`, plugin loading `OFF` (`-DENABLE_PLUGIN_LOADING=OFF`).
- `libtiff`: standard codecs on, **JBIG `OFF`** (`-Djbig=OFF`) — see §5.
- `libvips`: imagequant/quantizr/poppler/pdfium/magick/openjpeg/openslide `disabled`, modules
  `disabled`, HEIF enabled, everything else per variant.

The build script is the complete, reproducible record of that delta and is part of the source
we make available alongside this notice:
`build/windows/scripts/build-vips-gplfree-windows.sh` (in the repository, attached to the card
that produced this dist).

Everything else in the table below is an **unmodified MSYS2/UCRT64 package**. For those, the
corresponding source is the package's own upstream release, and the exact build recipe is the
MSYS2 package definition:

- package index: `https://packages.msys2.org/package/<pkgname>`
- package recipes: `https://github.com/msys2/MINGW-packages/tree/master/mingw-w64-<name>`

Any of this source can be supplied in full on request, as LGPL-3.0 §4(d)(0)/§6 require.

---

## 3. Complete component inventory of the shipped dist

Legend: **self** = built by us from source (§2); everything else is an MSYS2/UCRT64 package,
shipped unmodified. Licence strings are verbatim `Licenses` fields from `pacman -Qi`.

| shipped file(s) | component | version | licence | how obtained |
|---|---|---|---|---|
| `libvips-42.dll`, `vips.exe`, `vipsheader.exe`, `vipsthumbnail.exe` | libvips | 8.18.7 | **LGPL-2.1-or-later** | **self** |
| `libheif.dll` | libheif (library only; sample apps and C++/Go wrappers are MIT and are not shipped) | 1.23.7 | **LGPL-3.0** | **self** |
| `libtiff-6.dll` | libtiff | 4.7.2 | **MIT** (libtiff licence) | **self**, JBIG disabled |
| `libaom.dll` | aom (AV1 encoder, `spec-aom` only) | 3.15.3-1 | BSD-2-Clause **+ AOM Patent License 1.0** | MSYS2 `aom` |
| `libde265-0.dll` | libde265 (HEVC decoder) | 1.1.3-1 | LGPL-3.0-or-later (+ MIT for the sample apps, not shipped) | MSYS2 |
| `libdav1d-7.dll` | dav1d (AV1 decoder) | 1.5.4-1 | BSD-2-Clause | MSYS2 |
| `libwebp-7.dll`, `libwebpdemux-2.dll`, `libwebpmux-3.dll`, `libsharpyuv-0.dll` | libwebp | 1.6.0-1 | BSD-3-Clause | MSYS2 |
| `libjpeg-8.dll` | libjpeg-turbo | 3.2.1-1 | BSD-like (IJG/zlib-style) | MSYS2 |
| `libpng16-16.dll` | libpng | 1.6.59-2 | libpng-2.0 | MSYS2 |
| `libLerc.dll` | LERC | 4.1.1-1 | Apache-2.0 | MSYS2 |
| `liblcms2-2.dll` | Little CMS | 2.19.1-1 | `MIT AND GPL-3.0-or-later` → **MIT arm** (§4) | MSYS2 |
| `libexif-12.dll` | libexif | 0.6.26-1 | **LGPL-2.1** | MSYS2 |
| `libexpat-1.dll` | Expat | 2.9.0-1 | MIT | MSYS2 |
| `libglib-2.0-0.dll`, `libgio-2.0-0.dll`, `libgobject-2.0-0.dll`, `libgmodule-2.0-0.dll` | GLib | 2.90.1-1 | **LGPL-2.1-or-later** | MSYS2 |
| `libintl-8.dll` | gettext-runtime (`libintl`) | 1.0-1 | `GPL-3.0-or-later AND LGPL-2.1-or-later` → **LGPL-2.1-or-later arm** (§4) | MSYS2 |
| `libiconv-2.dll` | libiconv | 1.19-1 | **LGPL-2.1-or-later** (package also carries GPL docs, not shipped) (§4) | MSYS2 |
| `libhwy.dll` | Highway | 1.4.0-3 | Apache-2.0 | MSYS2 |
| `liblzma-5.dll` | XZ Utils (`liblzma`) | 5.8.4-1 | `0BSD AND LGPL-2.1-or-later AND GPL-2.0-or-later` → **0BSD arm** (§4) | MSYS2 |
| `libzstd.dll` | Zstandard | 1.5.7-2 | `BSD-3-Clause OR GPL-2.0-or-later` → **BSD-3-Clause** (§4) | MSYS2 |
| `zlib1.dll` | zlib | 1.3.2-3 | Zlib | MSYS2 |
| `libdeflate.dll` | libdeflate | 1.26-1 | MIT | MSYS2 |
| `libffi-8.dll` | libffi | 3.8.0-1 | MIT | MSYS2 |
| `libpcre2-8-0.dll` | PCRE2 | 10.49-1 | BSD-3-Clause | MSYS2 |
| `libwinpthread-1.dll` | mingw-w64 winpthreads | 14.0.0.r569.g34eeafb7d-1 | MIT AND BSD-3-Clause | MSYS2 |
| `libstdc++-6.dll` | GCC libstdc++ | 16.2.0-6 | `GPL-3.0-or-later WITH GCC-exception-3.1` → **runtime exception applies** | MSYS2 |
| `libgcc_s_seh-1.dll` | GCC runtime | 16.2.0-6 | `GPL-3.0-or-later WITH GCC-exception-3.1` → **runtime exception applies** | MSYS2 |

### Deliberately not present

No GPL or AGPL component ships. In particular: `libimagequant` (GPL-3.0), `libpoppler` /
PDFium, ImageMagick, Ghostscript, MuPDF, `libx265`, `libx264`, `cgif`, and — removed by this
change — **`libjbig-0.dll` (JBIG-KIT, GPL-2.0)**.

`vips-poppler`, `vips-magick` and dynamic modules are compiled out, so there is no loadable
plugin path that could pull a side-loaded library in at runtime.

---

## 4. Licence texts in this directory

| file | text | sha256 | covers |
|---|---|---|---|
| `GPL-3.0.txt` | GNU GPL v3, 29 June 2007 (35,149 B) | `3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986` | referenced by LGPL-3.0; GCC runtime exception context |
| `LGPL-3.0.txt` | GNU LGPL v3, 29 June 2007 (7,652 B) | `e3a994d82e644b03a792a930f574002658412f62407f5fee083f2555c5f23118` | libheif, libde265 |
| `LGPL-2.1.txt` | GNU LGPL v2.1, February 1999 (26,419 B) | `20e50fe7aae3e56378ebf0417d9de904f55a0e61e4df315333e632a4d3555d95` | libvips, GLib, libexif, libiconv, libintl, liblzma |
| `libtiff-LICENSE.md` | libtiff licence (MIT-style, 2,416 B) | `0e27c2382d7b8147972bbb746e04059a1152c8d0fda9d03ef1399d1a433c4ade` | self-built libtiff 4.7.2 |
| `libaom-LICENSE.txt` | BSD-2-Clause (1,316 B) | (see docs/LICENSES.md §1) | `libaom.dll`, `spec-aom` only |
| `libaom-PATENTS.txt` | AOM Patent License 1.0 (5,701 B) | (see docs/LICENSES.md §1) | `libaom.dll`, `spec-aom` only |

The GPL-3.0 and LGPL-2.1 texts were cross-checked byte-for-byte against the copies MSYS2 ships
in `ucrt64/share/licenses/xz/` on the build box (identical sha256, `3972dc97…` and `20e50fe7…`).
LGPL-3.0 is the FSF text as distributed verbatim by Debian (`/usr/share/common-licenses/LGPL-3`).

`GPL-3.0.txt` is included even though no GPL code ships: LGPL-3.0 is written as a set of
additional permissions on top of GPL-3.0 and is not intelligible without it.

---

## 5. Elections register (multi-licence packages)

Five packages publish a licence expression that *contains* a GPL term while the file we ship is
covered by a permissive arm. Each is an explicit, reviewable election — recorded here, mirrored
machine-readably in `elections.tsv`, and enforced by `audit-licenses.sh`. **A package with a GPL
term and no entry here is a hard build failure**, which is precisely how JBIG-KIT was caught.

| package | published expression | arm we ship | why (evidence) |
|---|---|---|---|
| `gettext-runtime` | `GPL-3.0-or-later AND LGPL-2.1-or-later` | **LGPL-2.1-or-later** | `share/licenses/gettext-runtime/COPYING`: *"The following parts are under the LGPL … the libintl and libasprintf libraries … The following parts are under the GPL … the **programs** gettext, ngettext, envsubst, and the documentation."* We ship `libintl-8.dll` only; no gettext program is in the dist. |
| `lcms2` | `MIT AND GPL-3.0-or-later` | **MIT** | `share/licenses/lcms2/LICENSE` is the MIT text ("MIT License / Copyright (c) 2023 Marti Maria Saguer"). `liblcms2-2.dll` is the MIT-licensed library; no lcms2 utility is shipped. |
| `xz` | `0BSD AND LGPL-2.1-or-later AND GPL-2.0-or-later` | **0BSD** (liblzma) | `share/licenses/xz/COPYING.0BSD` covers liblzma; the GPL terms cover the `xz`/`lzmainfo` command-line tools, which are not shipped. |
| `zstd` | `BSD-3-Clause OR GPL-2.0-or-later` | **BSD-3-Clause** | Disjunctive licence — we elect BSD-3-Clause (`share/licenses/zstd/LICENSE`). |
| `libiconv` | `LGPL-2.1-or-later`, `documentation: GPL-3.0-or-later` | **LGPL-2.1-or-later** | Only the library is shipped; the GPL term is attached to documentation, which is not distributed. |

Runtime-exception components need no election: `libstdc++-6.dll` and `libgcc_s_seh-1.dll` are
`GPL-3.0-or-later WITH GCC-exception-3.1`, and that exception is exactly what permits shipping
the GCC runtime DLLs with a non-GPL program.

**Outstanding for warden:** the five elections above are asserted from the licence files shipped
in the MSYS2 packages, not from a per-object-file analysis. If warden does not accept any of
them, the fix is a build change (build the component from source), not a footnote here.

---

## 6. Capability loss recorded with this change

`libtiff` is built with `-Djbig=OFF`, so **TIFF files using JBIG compression can no longer be
decoded**. Everything else in TIFF (none, LZW, Deflate, PackBits, JPEG, ZSTD, WebP, CCITT fax 4) is
unchanged and covered by the finalize acceptance run.

Measured on the build box with a JBIG-*tagged* TIFF (compression tag 34661; libtiff 4.x has no JBIG
encoder, so the tag is patched rather than the data encoded):

- `vips tiffload … --fail-on error` → **exit 1, no output**, `tiff2vips: ISO JBIG compression
  support is not configured`. This is the correct behaviour.
- `vips copy …` (default) → **exit 0 and a 400x200 all-black image**. A silent wrong result — the
  pre-existing vips behaviour for any decode error, not new, but now reachable by every JBIG TIFF.
  **The application must therefore call the loader explicitly with `--fail-on error`** (there is no
  global equivalent: `--vips-fail-on`, and `--fail-on` on `vips copy`, are rejected as unknown
  options). See `docs/LICENSES.md` §4.4.

JBIG-compressed TIFF is a rare archival profile; a file hitting it must fail visibly, never be
written out as a black frame.

---

## 7. LGPL compliance statement (why a DLL set is compliant)

We ship the LGPL components as **separate DLLs** loaded at run time next to `vips.exe`. They are
not statically linked into, and not combined with, our application binary. A user can therefore
replace any of these DLLs with a modified version and the app will load it — that satisfies the
relinking freedom of LGPL-2.1 §6(b) and LGPL-3.0 §4(d)(1) without us having to supply object
files. What was missing was the other half of the bargain, and that is what this pack supplies:
**the licences, the notices, and an offer of the corresponding source** (§2, §4). We make no
modification to the LGPL libraries beyond build configuration, so the upstream tarball plus our
build script is the complete corresponding source.

---

## 8. LibreOffice engine pack (separate process, not linked)

The document engine is a **separate OS process** (`soffice`, spawned through `proc::run`; no
FFI, nothing linked into the app — `engine/soffice.rs`). It is redistributed under **MPL-2.0**
(the pack's own `LICENSE`: "This product is made available subject to the terms of the Mozilla
Public License, v. 2.0"), and the pack ships its own `LICENSE`, `LICENSE.html` (MPL text plus
the third-party notices for its bundled libraries, extensions and fonts) and `NOTICE`. Those
documents travel with the pack, so the pack carries its own notices.

**No GPL content ships in the engine pack.** Three groups were stripped by decision card
t_20fae747 (`scripts/strip-engine-pack.py`, enforced by `scripts/dist-license-gate.sh --scan`
against `docs/shipped-inventory.tsv`) and are recorded there as `not-shipped` rows:

| stripped | why |
|---|---|
| five Culmus families (`DavidCLM`, `FrankRuehlCLM`, `MiriamCLM`, `MiriamMonoCLM`, `NachlieliCLM`) — 18 files | **GPL-2.0 with no font-embedding exception.** LibreOffice's PDF export embeds the font program, so keeping them would put GPL font software (a subset is a modified version) into every PDF the product writes, with no permission to do so. The OFL-1.1 twins (`DavidLibre`, `FrankRuhlHofshi`, `MiriamLibre`) remain in the pack. |
| `share/gallery/**` — 175 files | clipart with no licence statement in the pack's own licence documents; unused by headless conversion |
| `help/**` — 5,334 files | help-viewer content; terms verifiable (MPL-2.0 plus permissive `help3xsl` assets) but unused by headless conversion |

The "no GPL component ships" statement in §3 ("Deliberately not present") is about **code**
shipped next to the app; with the Culmus font files gone the engine pack no longer adds an
exception to it either.

Hebrew documents that name the stripped families keep their fidelity through fonts that *do*
ship: the app seeds LibreOffice's font-replacement table in its own profile
(`engine/soffice.rs::Profile::seed_font_substitutions`, card t_76ca844f) so that `David CLM` →
David Libre, `Frank Ruehl CLM` → Frank Ruhl Hofshi, `Miriam CLM` → Miriam Libre, `Miriam Mono
CLM` → Liberation Mono and `Nachlieli CLM` → Alef. All five targets are OFL-1.1 rows already
listed in the engine pack's own notices; no font is added and none of the stripped files is
needed.

---

## 9. Microsoft Visual C++ runtime in the Windows install root

The Windows NSIS installer sets `bundle.windows.bundleVCRuntime: true`
(`docs/PACKAGING-WINDOWS.md`) and therefore copies eight Microsoft Visual C++ runtime DLLs
into the install root (`%LOCALAPPDATA%\EZ Convert\`) next to `ez-convert.exe`. They are
**unmodified** copies from the Visual Studio 2022 redistributable tree — at bundle time on
the build VM (win11-dev) the redist root was `C:\p2_offdev\vcredist\`, so the payload is
`C:\p2_offdev\vcredist\14.44.35211\x64\Microsoft.VC143.CRT\` (MSVC toolset 14.44.35211,
`Microsoft.VC143.CRT` family). The documented purpose of bundling them is that "the WebView2
loader and the sidecars must run without a separately installed VC++ redistributable"
(`docs/PACKAGING-WINDOWS.md`). Nothing of ours is combined into the DLLs themselves, and no
shipped executable of ours names them in its PE import table (measured on `ez-convert.exe`
and `uninstall.exe`).

| shipped file | size (B) | sha256 |
|---|---|---|
| `concrt140.dll` | 324,208 | `2405355f0a58067b258f8df33c327e3a3d716eaac5a3a5aebb757842d85bd376` |
| `msvcp140.dll` | 557,728 | `0f885b509a685d2bbfa652fed26b5fb31d88fbdab0a978c641d1c7b8aa460aa9` |
| `msvcp140_1.dll` | 35,952 | `bfad5aef4c63a669e3c140655cdfdf395b6c979b400a447bd5dcb65ed8826c3d` |
| `msvcp140_2.dll` | 280,200 | `3ea06f0ee098b4823cb79599df3780e7f23cce52c19aac31d2a0d47efe33a5e9` |
| `msvcp140_atomic_wait.dll` | 50,304 | `640b2aefced484d0368eea5bdd06addd0658a3a70a49256e560d6923b404a479` |
| `msvcp140_codecvt_ids.dll` | 31,872 | `f2069a52880ec885ee7f0511186100eb7fada0411a2b4948fafea7735b878a18` |
| `vcruntime140.dll` | 124,544 | `d5e4d9a3e835fa679450145d6a7d94e36573a509317111904d9b3712c30d9066` |
| `vcruntime140_1.dll` | 49,792 | `1f2d41c4aa5db0bc33ebf7b66d72943a817d7ce6cbe880502a9403823633093f` |

Sizes and digests are tool output measured on win11-dev (2026-11-02): `certutil -hashfile
<file> SHA256` over the installed tree, cross-checked with `sha256sum` after copying the
files out. The installed bytes are byte-identical to the copies in the redist directory,
which is what demonstrates the "unmodified" condition below.

**Redistribution terms.** These DLLs are Distributable Code of Visual Studio 2022. The
"Distributable Code Files for Visual Studio 2022" — the `REDIST.txt` list referenced by the
"Distributable Code" section of the Microsoft Software License Terms for Visual Studio
Enterprise 2022, Visual Studio Professional 2022, Visual Studio Community 2022 — says
(Microsoft Learn, *Visual Studio 2022 Redistribution*):

> "If you have a validly licensed copy of such software, you may copy and distribute with
> your program the unmodified form of the files listed below, subject to the License Terms
> for the software."

> "Subject to the License Terms for the software, you may copy and distribute with your
> program any of the files within the following folder and its subfolders except as noted
> below. You may not modify these files. \[VisualStudioFolder\]\VC\redist"

> "Distribution of the Visual C++ Runtime Redistributable package, merge modules, and
> individual binaries obtained from those pages is limited to licensed Visual Studio users
> and is subject to its license terms."

We distribute the DLLs exactly in that position: unmodified (the digests above pin the
bytes), with our program, as part of its installation package. The `debug_nonredist`
folders the same list excludes are not redistributed. The full terms are the Microsoft
Software License Terms for Visual Studio 2022 (Visual Studio License Directory).
Authority: <https://learn.microsoft.com/en-us/visualstudio/releases/2022/redistribution>
("Visual C++ Runtime Files") and
<https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files>.
