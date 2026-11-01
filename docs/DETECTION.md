# Detection and the target matrix

Implementation of `ARCHITECTURE.md` §7 (detection) and §8 (the matrix), plus the
`SPEC.md` §3 scope they answer to. The detection core lives in
`src-tauri/src/model/` — `types.rs`, `detect.rs`, `matrix.rs` — and is **std-only
Rust**: no Tauri, no engine, no third-party crate (`scripts/check-model-purity.sh`
enforces it; `scripts/test-model.sh` compiles and tests it with plain rustc). The
other `model/` modules — `settings.rs`, `settings_schema.rs`, `naming.rs` — belong
to cards `U2`/`C4` and use `serde` for our own JSON document, which is why the
purity rule is per-file rather than blanket.

**The rule that shapes this document** (`ARCHITECTURE.md` §0): a capability
exists when evidence exists, not when code exists. The wheel shows only
`PROVEN` targets. `planned` rows are invisible until their owner card lands a
fixture, and `cut` rows are measured dead.

Every table below except the signature table is **generated from the code** by
`scripts/model-tables.rs`, and `cargo test` compares them byte for byte. If you
change `matrix.rs`, regenerate:

```bash
rustc --edition 2021 -o /tmp/model-tables scripts/model-tables.rs
/tmp/model-tables --write
```

---

## 1. What detection does

`detect_path()` / `detect_bytes()` read a bounded prefix (64 KiB), a bounded
tail (64 KiB), and — only when a container signature matched — the ZIP central
directory or the CFB FAT + directory sectors. Nothing decodes content. The
verdict is `Detected { ty, integrity, claimed, detail }`.

Rules, all unit-tested against `fixtures/`:

1. **Content decides.** The extension is recorded (`claimed`) and compared; a
   mismatch is reported and never changes the outcome. The acceptance fixture
   `renamed_jpeg_as.png` is byte-identical to `sample.jpg` and resolves to
   `jpeg`, offering JPEG's targets.
2. **Containers are validated structurally.** `docx` = ZIP with
   `[Content_Types].xml` **and** `word/document.xml`; `xlsx` =
   `xl/workbook.xml`; `pptx` = `ppt/presentation.xml`; `doc` = CFB with a
   `WordDocument` entry; `xls` = `Workbook`/`Book`; `ppt` =
   `PowerPoint Document`. A ZIP with none of those is **not** a document, and a
   CFB with none of those is **not** an Office file. This is what stops the
   measured silent-garbage trap (`ARCHITECTURE.md` §6.2 N3): a text file named
   `.docx` becomes a valid-looking 12,388 B PDF at exit 0, so detection must
   never route it to LibreOffice. Fixture: `text_renamed.docx` → `txt`.
3. **Truncation is cheap to see**:

   | type | end marker | truncated verdict |
   |---|---|---|
   | jpeg | EOI `FF D9` | `Truncated`, still `jpeg` (may decode) |
   | png | `IEND` | `Truncated`, still `png` |
   | gif | `0x3B` trailer | `Truncated`, still `gif` |
   | pdf | `%%EOF` | `Truncated`, still `pdf` |
   | rtf | closing `}` | `Truncated`, still `rtf` |
   | zip/ooxml | end-of-central-directory | `Unknown` + `Truncated`: the container cannot be validated, so the type is not guessed |
   | webp, tiff, bmp, heic, avif, mp4, mov, m4a, mkv, webm, mp3, wav, flac, ogg, opus, aac | none cheap | `Complete` (a truncated one fails at decode) |

4. **Text shapes** resolve on validated UTF-8 (or UTF-16 with a BOM), no NULs,
   at most 2% control characters, in this fixed order:
   `svg` (first element is `<svg`, or the SVG namespace appears) → `html`
   (`<!doctype html` or first element `<html`) → `csv` (≥ 2 non-empty lines,
   equal delimiter counts *outside quotes*, ≥ 2 delimiters per line or ≥ 3
   lines) → `md` (ATX heading, fence, list marker, link syntax) → `txt`.
   A UTF-16 BOM is checked **before** the MPEG frame-sync heuristic, because
   `FF FE` is both a BOM and a legal-looking frame header.
5. **Limits before any parse**: inputs > 2 GiB are refused (`Unreadable` +
   `TooLarge`); a ZIP whose declared uncompressed total exceeds 8 GiB is
   refused; more than 10,000 container directory entries is refused.
6. **Unknown is a valid answer.** No signature, no guess, no targets.
7. **MPEG audio is only detected at the start of the file** (ID3 tag or frame
   sync at offset 0). Scanning arbitrary bytes for a sync pattern classified a
   NUL-laced binary blob as MP3 during review.

Precedence (a polyglot is classified by this order, never by "whichever parse
succeeds"): ZIP → CFB → PDF → JPEG → PNG → GIF → TIFF → RIFF (WebP/WAV) → BMP →
ISOBMFF (HEIC/AVIF/MP4/MOV/M4A) → EBML (MKV/WebM) → Ogg → FLAC → UTF-16 BOM →
MPEG audio → RTF → text shapes → unknown.

Documented non-answers (deliberate):

- **ZIP64** directories are `Unsupported` (`zip: zip64 directory (not parsed)`),
  not guessed. Real Office documents do not need ZIP64 (2 GiB input cap).
- **A bare `mif1` HEIF brand with no codec hint** is `Unknown` — it could be
  HEVC or AV1 payload, and picking one would be a guess. `avif`/`avis` and the
  `heic`/`heix`/`hevc`/`hevx`/`heim`/`heis` brands all resolve.
- Files that are a valid header followed by garbage are classified by the
  header and fail at decode (`ARCHITECTURE.md` §7.1); the stderr rule (§6.0.4)
  catches the silent case.

---

## 2. Canonical types and their fixtures

Every row is exercised by `detect::tests::every_corpus_fixture_detects_its_expected_type_completely`;
the fixture corpus and how each file was produced is `fixtures/PROVENANCE.md`.

| canonical type | family | signature / structural rule | fixture |
|---|---|---|---|
| jpeg | image | `FF D8 FF` | `fixtures/sample.jpg` |
| png | image | 8-byte PNG signature | `fixtures/sample.png` |
| webp | image | `RIFF` + `WEBP` | `fixtures/sample.webp` |
| tiff | image | `II*\0` / `MM\0*` | `fixtures/sample.tif` |
| bmp | image | `BM` + plausible DIB header size | `fixtures/sample.bmp` |
| gif | image | `GIF87a` / `GIF89a` | `fixtures/sample.gif` |
| heic | image | ISOBMFF brand `heic`/`heix`/`hevc`/`hevx`/`heim`/`heis` | `fixtures/sample.heic` |
| avif | image | ISOBMFF brand `avif`/`avis` (wins over a HEIF compatible brand) | `fixtures/sample.avif` |
| svg | image | text, first element `<svg` | `fixtures/sample.svg` |
| pdf | document | `%PDF-` + `%%EOF` | `fixtures/sample.pdf` |
| mp3 | audio | `ID3` or MPEG frame sync at offset 0, layer ≠ 0 | `fixtures/sample.mp3` |
| aac | audio | ADTS sync at offset 0 (layer bits 00) | `fixtures/sample.aac` |
| wav | audio | `RIFF` + `WAVE` | `fixtures/sample.wav` |
| flac | audio | `fLaC` | `fixtures/sample.flac` |
| ogg | audio | `OggS` without `OpusHead` | `fixtures/sample.ogg` |
| opus | audio | `OggS` + `OpusHead` | `fixtures/sample.opus` |
| m4a | audio | ISOBMFF brand `M4A ` / `M4B ` / `M4P ` | `fixtures/sample.m4a` |
| mp4 | video | ISOBMFF brand `isom`/`iso2`/`mp41`/`mp42`/`avc1`/`dash`/`MSNV`/`M4V `/`F4V ` | `fixtures/sample.mp4` |
| mov | video | ISOBMFF brand `qt  ` | `fixtures/sample.mov` |
| mkv | video | EBML header + doctype `matroska` | `fixtures/sample.mkv` |
| webm | video | EBML header + doctype `webm` | `fixtures/sample.webm` |
| docx | document | ZIP with `[Content_Types].xml` + `word/document.xml` | `fixtures/sample.docx` |
| xlsx | document | ZIP with `xl/workbook.xml` | `fixtures/sample.xlsx` |
| pptx | document | ZIP with `ppt/presentation.xml` | `fixtures/sample.pptx` |
| doc | document | CFB with a `WordDocument` entry | `fixtures/sample.doc` |
| xls | document | CFB with `Workbook`/`Book` | `fixtures/sample.xls` |
| ppt | document | CFB with `PowerPoint Document` | `fixtures/sample.ppt` |
| rtf | document | text starting `{\rtf` | `fixtures/sample.rtf` |
| csv | document | text, quoted-aware stable delimiter counts | `fixtures/sample.csv` |
| md | document | text with markdown markers | `fixtures/sample.md` |
| html | document | text with `<!doctype html` / `<html` | `fixtures/sample.html` |
| txt | document | text with no other shape | `fixtures/sample.txt` |

Broken and hostile fixtures: `renamed_jpeg_as.png` (acceptance), `truncated.jpg`,
`truncated.png`, `truncated.docx`, `empty.bin`, `plain.zip`, `plain.cfb`,
`text_renamed.docx`.

---

## 3. The target matrix (type → target, with status)

Generated from `matrix::routes_for()`. `PROVEN` = measured, evidence named;
`planned` = invisible in the wheel, owner card named; `cut` = measured or
specified dead.

<!-- matrix:begin -->
| input | target | engine | status | evidence / owner / reason |
|---|---|---|---|---|
| jpeg | jpg | vips | PROVEN | V6 — autorot JPEG 400x200 -> 200x400, Orientation rewritten to 1 |
| jpeg | png | vips | PROVEN | V2 — jpg -> png OK (exit 0, PNG written) |
| jpeg | webp | vips | PROVEN | C2-JPEG-WEBP — jpg -> webp OK (1,188 B, magic webp) |
| jpeg | tiff | vips | PROVEN | C2-JPEG-TIFF — jpg -> tiff OK (3,508 B, ICC carried) |
| jpeg | avif | vips | PROVEN | C2W-AVIF — avif out on the shipping dist: jpg/png/webp/tiff/gif/heic -> avif PASS (875-1740 B), magic-checked |
| jpeg | gif | ffmpeg | PROVEN | C2-GIF-STILL — still image -> gif via vips normalise + ffmpeg palette (1 frame, 1,680 B) |
| jpeg | bmp | rust-crate | planned | t_dc07a72e — image crate <-> temp PNG <-> vips (SPEC 9.3 D5), unmeasured |
| jpeg | heic | vips | cut | HEIC encode cut: x265 is GPL-2 and Media Foundation HEVC needs a Store codec pack (SPEC 9.3 D4) |
| jpeg | svg | vips | cut | svg is input-only in v1: no svg->svg and no svg->pdf (SPEC 9.3 D3) |
| jpeg | pdf | vips | cut | no image writer to PDF in v1 (ARCHITECTURE 8.2) |
| png | jpg | vips | PROVEN | C2-PNG-JPEG — png -> jpg OK (2,781 B) |
| png | webp | vips | PROVEN | V3 — png -> webp OK (exit 0) |
| png | tiff | vips | PROVEN | C2-PNG-TIFF — png -> tiff OK (522 B) |
| png | avif | vips | PROVEN | C2W-AVIF — avif out on the shipping dist: jpg/png/webp/tiff/gif/heic -> avif PASS (875-1740 B), magic-checked |
| png | gif | ffmpeg | PROVEN | C2-GIF-STILL — still image -> gif via vips normalise + ffmpeg palette (1 frame, 1,680 B) |
| png | bmp | rust-crate | planned | t_dc07a72e — image crate <-> temp PNG <-> vips (SPEC 9.3 D5), unmeasured |
| png | heic | vips | cut | HEIC encode cut: x265 is GPL-2 and Media Foundation HEVC needs a Store codec pack (SPEC 9.3 D4) |
| png | svg | vips | cut | svg is input-only in v1: no svg->svg and no svg->pdf (SPEC 9.3 D3) |
| png | pdf | vips | cut | no image writer to PDF in v1 (ARCHITECTURE 8.2) |
| heic | jpg | vips | PROVEN | V7 — heic -> jpg, 200x400, libheif already applied irot (autorot is a no-op) |
| heic | png | vips | PROVEN | C2W-HEIC-DECODE — heic decode: heic -> png/webp/tiff PASS, 200x400 (libheif already applied irot) |
| heic | webp | vips | PROVEN | C2W-HEIC-DECODE — heic decode: heic -> png/webp/tiff PASS, 200x400 (libheif already applied irot) |
| heic | tiff | vips | PROVEN | C2W-HEIC-DECODE — heic decode: heic -> png/webp/tiff PASS, 200x400 (libheif already applied irot) |
| heic | avif | vips | PROVEN | C2W-AVIF — avif out on the shipping dist: jpg/png/webp/tiff/gif/heic -> avif PASS (875-1740 B), magic-checked |
| heic | gif | ffmpeg | PROVEN | C2W-HEIF-GIF — heic/avif -> gif PASS via vips normalise -> ffmpeg palette, 200x400 / 64x48 |
| heic | bmp | rust-crate | planned | t_dc07a72e — image crate <-> temp PNG <-> vips (SPEC 9.3 D5), unmeasured |
| heic | heic | vips | cut | HEIC encode cut: x265 is GPL-2 and Media Foundation HEVC needs a Store codec pack (SPEC 9.3 D4) |
| heic | svg | vips | cut | svg is input-only in v1: no svg->svg and no svg->pdf (SPEC 9.3 D3) |
| heic | pdf | vips | cut | no image writer to PDF in v1 (ARCHITECTURE 8.2) |
| avif | jpg | vips | PROVEN | V8b — avif -> jpg kept EXIF IFD0 and the 3,144 B ICC profile |
| avif | png | vips | PROVEN | C2W-AVIF-DECODE — avif decode: avif -> png/webp/tiff PASS, 64x48, codec read back by ffprobe |
| avif | webp | vips | PROVEN | C2W-AVIF-DECODE — avif decode: avif -> png/webp/tiff PASS, 64x48, codec read back by ffprobe |
| avif | tiff | vips | PROVEN | C2W-AVIF-DECODE — avif decode: avif -> png/webp/tiff PASS, 64x48, codec read back by ffprobe |
| avif | gif | ffmpeg | PROVEN | C2W-HEIF-GIF — heic/avif -> gif PASS via vips normalise -> ffmpeg palette, 200x400 / 64x48 |
| avif | bmp | rust-crate | planned | t_dc07a72e — image crate <-> temp PNG <-> vips (SPEC 9.3 D5), unmeasured |
| avif | heic | vips | cut | HEIC encode cut: x265 is GPL-2 and Media Foundation HEVC needs a Store codec pack (SPEC 9.3 D4) |
| avif | svg | vips | cut | svg is input-only in v1: no svg->svg and no svg->pdf (SPEC 9.3 D3) |
| avif | pdf | vips | cut | no image writer to PDF in v1 (ARCHITECTURE 8.2) |
| webp | jpg | vips | PROVEN | C2-WEBP-JPEG — webp -> jpg OK (2,904 B) |
| webp | png | vips | PROVEN | C2-WEBP-PNG — webp -> png OK (4,346 B) |
| webp | tiff | vips | PROVEN | V4 — webp -> tiff OK (exit 0) |
| webp | avif | vips | PROVEN | C2W-AVIF — avif out on the shipping dist: jpg/png/webp/tiff/gif/heic -> avif PASS (875-1740 B), magic-checked |
| webp | gif | ffmpeg | PROVEN | C2-GIF-STILL — still image -> gif via vips normalise + ffmpeg palette (1 frame, 1,680 B) |
| webp | bmp | rust-crate | planned | t_dc07a72e — image crate <-> temp PNG <-> vips (SPEC 9.3 D5), unmeasured |
| webp | heic | vips | cut | HEIC encode cut: x265 is GPL-2 and Media Foundation HEVC needs a Store codec pack (SPEC 9.3 D4) |
| webp | svg | vips | cut | svg is input-only in v1: no svg->svg and no svg->pdf (SPEC 9.3 D3) |
| webp | pdf | vips | cut | no image writer to PDF in v1 (ARCHITECTURE 8.2) |
| gif | jpg | vips | PROVEN | C2W-GIF-IN — gif source -> jpg/png/webp/tiff/avif PASS; frame 1 only, labelled in the UI and the route |
| gif | png | vips | PROVEN | C2W-GIF-IN — gif source -> jpg/png/webp/tiff/avif PASS; frame 1 only, labelled in the UI and the route |
| gif | webp | vips | PROVEN | C2W-GIF-IN — gif source -> jpg/png/webp/tiff/avif PASS; frame 1 only, labelled in the UI and the route |
| gif | tiff | vips | PROVEN | C2W-GIF-IN — gif source -> jpg/png/webp/tiff/avif PASS; frame 1 only, labelled in the UI and the route |
| gif | avif | vips | PROVEN | C2W-AVIF — avif out on the shipping dist: jpg/png/webp/tiff/gif/heic -> avif PASS (875-1740 B), magic-checked |
| gif | gif | ffmpeg | planned | t_dc07a72e — gif -> gif (re-encode of an animation) is not measured and not offered yet; gif *input* for other image targets is offered and converts frame 1 only (Option C, ARCHITECTURE.md §8.2.1) |
| gif | bmp | rust-crate | planned | t_dc07a72e — image crate <-> temp PNG <-> vips (SPEC 9.3 D5), unmeasured |
| gif | heic | vips | cut | HEIC encode cut: x265 is GPL-2 and Media Foundation HEVC needs a Store codec pack (SPEC 9.3 D4) |
| gif | svg | vips | cut | svg is input-only in v1: no svg->svg and no svg->pdf (SPEC 9.3 D3) |
| gif | pdf | vips | cut | no image writer to PDF in v1 (ARCHITECTURE 8.2) |
| tiff | jpg | vips | PROVEN | C2-TIFF-JPEG — tiff -> jpg OK (2,781 B) |
| tiff | png | vips | PROVEN | V5 — tiff -> png OK (exit 0) |
| tiff | webp | vips | PROVEN | C2-TIFF-WEBP — tiff -> webp OK (1,130 B) |
| tiff | avif | vips | PROVEN | C2W-AVIF — avif out on the shipping dist: jpg/png/webp/tiff/gif/heic -> avif PASS (875-1740 B), magic-checked |
| tiff | gif | ffmpeg | PROVEN | C2-GIF-STILL — still image -> gif via vips normalise + ffmpeg palette (1 frame, 1,680 B) |
| tiff | bmp | rust-crate | planned | t_dc07a72e — image crate <-> temp PNG <-> vips (SPEC 9.3 D5), unmeasured |
| tiff | heic | vips | cut | HEIC encode cut: x265 is GPL-2 and Media Foundation HEVC needs a Store codec pack (SPEC 9.3 D4) |
| tiff | svg | vips | cut | svg is input-only in v1: no svg->svg and no svg->pdf (SPEC 9.3 D3) |
| tiff | pdf | vips | cut | no image writer to PDF in v1 (ARCHITECTURE 8.2) |
| bmp | jpg | vips | planned | t_dc07a72e — same vips mechanism, different saver; this exact pair has no fixture yet |
| bmp | png | vips | planned | t_dc07a72e — same vips mechanism, different saver; this exact pair has no fixture yet |
| bmp | webp | vips | planned | t_dc07a72e — same vips mechanism, different saver; this exact pair has no fixture yet |
| bmp | tiff | vips | planned | t_dc07a72e — same vips mechanism, different saver; this exact pair has no fixture yet |
| bmp | avif | vips | planned | t_dc07a72e — same vips mechanism, different saver; this exact pair has no fixture yet |
| bmp | gif | ffmpeg | planned | t_dc07a72e — vips normalise -> ffmpeg palettegen/paletteuse (SPEC 9.3 D2), unmeasured |
| bmp | bmp | rust-crate | planned | t_dc07a72e — image crate <-> temp PNG <-> vips (SPEC 9.3 D5), unmeasured |
| bmp | heic | vips | cut | HEIC encode cut: x265 is GPL-2 and Media Foundation HEVC needs a Store codec pack (SPEC 9.3 D4) |
| bmp | svg | vips | cut | svg is input-only in v1: no svg->svg and no svg->pdf (SPEC 9.3 D3) |
| bmp | pdf | vips | cut | no image writer to PDF in v1 (ARCHITECTURE 8.2) |
| svg | jpg | resvg | planned | t_dc07a72e — resvg raster hop -> lossless PNG -> vips (SPEC 9.3 D3); resvg never built or run |
| svg | png | resvg | planned | t_dc07a72e — resvg raster hop -> lossless PNG -> vips (SPEC 9.3 D3); resvg never built or run |
| svg | webp | resvg | planned | t_dc07a72e — resvg raster hop -> lossless PNG -> vips (SPEC 9.3 D3); resvg never built or run |
| svg | tiff | resvg | planned | t_dc07a72e — resvg raster hop -> lossless PNG -> vips (SPEC 9.3 D3); resvg never built or run |
| svg | avif | resvg | planned | t_dc07a72e — resvg raster hop -> lossless PNG -> vips (SPEC 9.3 D3); resvg never built or run |
| svg | gif | ffmpeg | planned | t_dc07a72e — vips normalise -> ffmpeg palettegen/paletteuse (SPEC 9.3 D2), unmeasured |
| svg | bmp | rust-crate | planned | t_dc07a72e — image crate <-> temp PNG <-> vips (SPEC 9.3 D5), unmeasured |
| svg | heic | resvg | cut | HEIC encode cut: x265 is GPL-2 and Media Foundation HEVC needs a Store codec pack (SPEC 9.3 D4) |
| svg | svg | resvg | cut | svg is input-only in v1: no svg->svg and no svg->pdf (SPEC 9.3 D3) |
| svg | pdf | resvg | cut | svg is input-only in v1: no svg->svg and no svg->pdf (SPEC 9.3 D3) |
| pdf | jpg | pdfium | planned | t_dc07a72e — PDFium raster -> vips (ARCHITECTURE 6.4); PDFium binary never built or run |
| pdf | png | pdfium | planned | t_dc07a72e — PDFium raster -> vips (ARCHITECTURE 6.4); PDFium binary never built or run |
| pdf | webp | pdfium | planned | t_dc07a72e — PDFium raster -> vips (ARCHITECTURE 6.4); PDFium binary never built or run |
| pdf | tiff | pdfium | planned | t_dc07a72e — PDFium raster -> vips (ARCHITECTURE 6.4); PDFium binary never built or run |
| pdf | avif | pdfium | planned | t_dc07a72e — PDFium raster -> vips (ARCHITECTURE 6.4); PDFium binary never built or run |
| pdf | gif | pdfium | planned | t_dc07a72e — PDFium raster -> vips (ARCHITECTURE 6.4); PDFium binary never built or run |
| pdf | bmp | pdfium | planned | t_dc07a72e — PDFium raster -> vips (ARCHITECTURE 6.4); PDFium binary never built or run |
| pdf | heic | pdfium | cut | HEIC encode cut: x265 is GPL-2 and Media Foundation HEVC needs a Store codec pack (SPEC 9.3 D4) |
| pdf | svg | pdfium | cut | svg is input-only in v1: no svg->svg and no svg->pdf (SPEC 9.3 D3) |
| pdf | pdf | pdfium | cut | no image writer to PDF in v1 (ARCHITECTURE 8.2) |
| pdf | md | soffice | cut | no raster-to-writer path for PDF input (ARCHITECTURE 8.3) |
| pdf | html | soffice | cut | no raster-to-writer path for PDF input (ARCHITECTURE 8.3) |
| pdf | docx | soffice | cut | no raster-to-writer path for PDF input (ARCHITECTURE 8.3) |
| pdf | pptx | soffice | cut | no raster-to-writer path for PDF input (ARCHITECTURE 8.3) |
| pdf | rtf | soffice | cut | no raster-to-writer path for PDF input (ARCHITECTURE 8.3) |
| pdf | xlsx | soffice | cut | no raster-to-writer path for PDF input (ARCHITECTURE 8.3) |
| pdf | csv | soffice | cut | no raster-to-writer path for PDF input (ARCHITECTURE 8.3) |
| pdf | txt | soffice | cut | measured: a PDF imports as a Draw document, so the writer text filter answers rc=1 `Class:Write Code:16`; extracting text from a PDF is a different product (ARCHITECTURE 8.3, §10) |
| mp3 | mp3 | ffmpeg | PROVEN | C2-AUDIO-MP3 — every audio source -> mp3 PASS (6 of 6, explicit -ar/-sample_fmt) |
| mp3 | wav | ffmpeg | PROVEN | C2-AUDIO-WAV — every audio source -> wav PASS (6 of 6) |
| mp3 | flac | ffmpeg | PROVEN | C2-AUDIO-FLAC — every audio source -> flac PASS (6 of 6) |
| mp3 | m4a | ffmpeg | PROVEN | C2-AUDIO-M4A — every audio source -> m4a PASS (6 of 6) |
| mp3 | aac | ffmpeg | PROVEN | C2-AUDIO-AAC — every audio source -> aac PASS (6 of 6) |
| mp3 | ogg | ffmpeg | PROVEN | C2-AUDIO-OGG — every audio source -> ogg PASS (6 of 6) |
| mp3 | opus | ffmpeg | PROVEN | C2-AUDIO-OPUS — every audio source -> opus PASS (6 of 6, 48 kHz) |
| wav | mp3 | ffmpeg | PROVEN | C2-AUDIO-MP3 — every audio source -> mp3 PASS (6 of 6, explicit -ar/-sample_fmt) |
| wav | wav | ffmpeg | PROVEN | C2-AUDIO-WAV — every audio source -> wav PASS (6 of 6) |
| wav | flac | ffmpeg | PROVEN | C2-AUDIO-FLAC — every audio source -> flac PASS (6 of 6) |
| wav | m4a | ffmpeg | PROVEN | C2-AUDIO-M4A — every audio source -> m4a PASS (6 of 6) |
| wav | aac | ffmpeg | PROVEN | C2-AUDIO-AAC — every audio source -> aac PASS (6 of 6) |
| wav | ogg | ffmpeg | PROVEN | C2-AUDIO-OGG — every audio source -> ogg PASS (6 of 6) |
| wav | opus | ffmpeg | PROVEN | C2-AUDIO-OPUS — every audio source -> opus PASS (6 of 6, 48 kHz) |
| flac | mp3 | ffmpeg | PROVEN | C2-AUDIO-MP3 — every audio source -> mp3 PASS (6 of 6, explicit -ar/-sample_fmt) |
| flac | wav | ffmpeg | PROVEN | C2-AUDIO-WAV — every audio source -> wav PASS (6 of 6) |
| flac | flac | ffmpeg | PROVEN | C2-AUDIO-FLAC — every audio source -> flac PASS (6 of 6) |
| flac | m4a | ffmpeg | PROVEN | C2-AUDIO-M4A — every audio source -> m4a PASS (6 of 6) |
| flac | aac | ffmpeg | PROVEN | C2-AUDIO-AAC — every audio source -> aac PASS (6 of 6) |
| flac | ogg | ffmpeg | PROVEN | C2-AUDIO-OGG — every audio source -> ogg PASS (6 of 6) |
| flac | opus | ffmpeg | PROVEN | C2-AUDIO-OPUS — every audio source -> opus PASS (6 of 6, 48 kHz) |
| m4a | mp3 | ffmpeg | PROVEN | C2-AUDIO-MP3 — every audio source -> mp3 PASS (6 of 6, explicit -ar/-sample_fmt) |
| m4a | wav | ffmpeg | PROVEN | C2-AUDIO-WAV — every audio source -> wav PASS (6 of 6) |
| m4a | flac | ffmpeg | PROVEN | C2-AUDIO-FLAC — every audio source -> flac PASS (6 of 6) |
| m4a | m4a | ffmpeg | PROVEN | C2-AUDIO-M4A — every audio source -> m4a PASS (6 of 6) |
| m4a | aac | ffmpeg | PROVEN | C2-AUDIO-AAC — every audio source -> aac PASS (6 of 6) |
| m4a | ogg | ffmpeg | PROVEN | C2-AUDIO-OGG — every audio source -> ogg PASS (6 of 6) |
| m4a | opus | ffmpeg | PROVEN | C2-AUDIO-OPUS — every audio source -> opus PASS (6 of 6, 48 kHz) |
| aac | mp3 | ffmpeg | PROVEN | C2-AUDIO-MP3 — every audio source -> mp3 PASS (6 of 6, explicit -ar/-sample_fmt) |
| aac | wav | ffmpeg | PROVEN | C2-AUDIO-WAV — every audio source -> wav PASS (6 of 6) |
| aac | flac | ffmpeg | PROVEN | C2-AUDIO-FLAC — every audio source -> flac PASS (6 of 6) |
| aac | m4a | ffmpeg | PROVEN | C2-AUDIO-M4A — every audio source -> m4a PASS (6 of 6) |
| aac | aac | ffmpeg | PROVEN | C2-AUDIO-AAC — every audio source -> aac PASS (6 of 6) |
| aac | ogg | ffmpeg | PROVEN | C2-AUDIO-OGG — every audio source -> ogg PASS (6 of 6) |
| aac | opus | ffmpeg | PROVEN | C2-AUDIO-OPUS — every audio source -> opus PASS (6 of 6, 48 kHz) |
| ogg | mp3 | ffmpeg | PROVEN | C2-AUDIO-MP3 — every audio source -> mp3 PASS (6 of 6, explicit -ar/-sample_fmt) |
| ogg | wav | ffmpeg | PROVEN | C2-AUDIO-WAV — every audio source -> wav PASS (6 of 6) |
| ogg | flac | ffmpeg | PROVEN | C2-AUDIO-FLAC — every audio source -> flac PASS (6 of 6) |
| ogg | m4a | ffmpeg | PROVEN | C2-AUDIO-M4A — every audio source -> m4a PASS (6 of 6) |
| ogg | aac | ffmpeg | PROVEN | C2-AUDIO-AAC — every audio source -> aac PASS (6 of 6) |
| ogg | ogg | ffmpeg | PROVEN | C2-AUDIO-OGG — every audio source -> ogg PASS (6 of 6) |
| ogg | opus | ffmpeg | PROVEN | C2-AUDIO-OPUS — every audio source -> opus PASS (6 of 6, 48 kHz) |
| opus | mp3 | ffmpeg | PROVEN | C2-AUDIO-MP3 — every audio source -> mp3 PASS (6 of 6, explicit -ar/-sample_fmt) |
| opus | wav | ffmpeg | PROVEN | C2-AUDIO-WAV — every audio source -> wav PASS (6 of 6) |
| opus | flac | ffmpeg | PROVEN | C2-AUDIO-FLAC — every audio source -> flac PASS (6 of 6) |
| opus | m4a | ffmpeg | PROVEN | C2-AUDIO-M4A — every audio source -> m4a PASS (6 of 6) |
| opus | aac | ffmpeg | PROVEN | C2-AUDIO-AAC — every audio source -> aac PASS (6 of 6) |
| opus | ogg | ffmpeg | PROVEN | C2-AUDIO-OGG — every audio source -> ogg PASS (6 of 6) |
| opus | opus | ffmpeg | PROVEN | C2-AUDIO-OPUS — every audio source -> opus PASS (6 of 6, 48 kHz) |
| mp4 | mp4 | ffmpeg | PROVEN | C2-VIDEO-MP4 — every video source -> mp4 PASS (3 of 3), CFR, libopenh264 + aac |
| mp4 | mkv | ffmpeg | PROVEN | C2-VIDEO-MKV — every video source -> mkv PASS (3 of 3) |
| mp4 | mov | ffmpeg | PROVEN | C2-VIDEO-MOV — every video source -> mov PASS (3 of 3) |
| mp4 | webm | ffmpeg | PROVEN | C2-VIDEO-WEBM — every video source -> webm PASS (3 of 3), VP9 + Opus |
| mp4 | gif | ffmpeg | PROVEN | C2-VIDEO-GIF — every video source -> gif PASS (4 of 4), fps + long-edge cap + palette |
| mkv | mp4 | ffmpeg | PROVEN | C2-VIDEO-MP4 — every video source -> mp4 PASS (3 of 3), CFR, libopenh264 + aac |
| mkv | mkv | ffmpeg | PROVEN | C2-VIDEO-MKV — every video source -> mkv PASS (3 of 3) |
| mkv | mov | ffmpeg | PROVEN | C2-VIDEO-MOV — every video source -> mov PASS (3 of 3) |
| mkv | webm | ffmpeg | PROVEN | C2-VIDEO-WEBM — every video source -> webm PASS (3 of 3), VP9 + Opus |
| mkv | gif | ffmpeg | PROVEN | C2-VIDEO-GIF — every video source -> gif PASS (4 of 4), fps + long-edge cap + palette |
| mov | mp4 | ffmpeg | PROVEN | C2-VIDEO-MP4 — every video source -> mp4 PASS (3 of 3), CFR, libopenh264 + aac |
| mov | mkv | ffmpeg | PROVEN | C2-VIDEO-MKV — every video source -> mkv PASS (3 of 3) |
| mov | mov | ffmpeg | PROVEN | C2-VIDEO-MOV — every video source -> mov PASS (3 of 3) |
| mov | webm | ffmpeg | PROVEN | C2-VIDEO-WEBM — every video source -> webm PASS (3 of 3), VP9 + Opus |
| mov | gif | ffmpeg | PROVEN | C2-VIDEO-GIF — every video source -> gif PASS (4 of 4), fps + long-edge cap + palette |
| webm | mp4 | ffmpeg | PROVEN | C2-VIDEO-MP4 — every video source -> mp4 PASS (3 of 3), CFR, libopenh264 + aac |
| webm | mkv | ffmpeg | PROVEN | C2-VIDEO-MKV — every video source -> mkv PASS (3 of 3) |
| webm | mov | ffmpeg | PROVEN | C2-VIDEO-MOV — every video source -> mov PASS (3 of 3) |
| webm | webm | ffmpeg | PROVEN | C2-VIDEO-WEBM — every video source -> webm PASS (3 of 3), VP9 + Opus |
| webm | gif | ffmpeg | PROVEN | C2-VIDEO-GIF — every video source -> gif PASS (4 of 4), fps + long-edge cap + palette |
| docx | pdf | soffice | PROVEN | D1 — docx -> pdf: 26,460 B (Linux) / 39,946 B (Windows), 1 page 612x792 pt |
| docx | md | soffice | PROVEN | D2 — docx -> md: 389 B Linux / 404 B Windows, real Markdown |
| docx | txt | soffice | PROVEN | C3B-DOCX-TXT — docx -> txt PASS both legs (writer text filter, UTF-8) |
| docx | html | soffice | PROVEN | D3 — docx -> html: 4,064 B, h1 + table + 13 p |
| docx | rtf | soffice | PROVEN | C3B-DOCX-RTF — docx -> rtf PASS both legs (rtf:Rich Text Format, RTF header read back) |
| docx | xlsx | soffice | cut | measured: a Writer document exported with the Calc xlsx filter is not a conversion on either leg — Linux aborts (rc=134, `Unspecified Application Error`), Windows answers rc=0 and leaves only a zero-byte `*.tmp`. No workbook is written on either leg (evidence/c3-document.md §10.2) |
| docx | pptx | soffice | cut | no measured Writer/Calc/Impress cross-family filter for this pair (ARCHITECTURE 8.3) |
| docx | csv | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
| xlsx | pdf | soffice | PROVEN | C3B-XLSX-PDF — xlsx -> pdf PASS both legs with pdf:calc_pdf_Export (2 pages: two sheets) |
| xlsx | csv | soffice | PROVEN | D5 — xlsx -> csv: 147 B, named <stem>-<sheet>.csv |
| xlsx | txt | soffice | PROVEN | C3B-XLSX-TXT — xlsx -> txt PASS both legs via the Calc text filter, TAB-separated, named <stem>-<sheet>.txt |
| xlsx | html | soffice | PROVEN | C3B-XLSX-HTML — xlsx -> html PASS both legs with html:HTML (StarCalc) |
| xlsx | md | soffice | cut | measured: only Writer registers the Markdown export filter. A Calc or Impress document answers rc=1 `Class:Write Code:16` for `md:Markdown`, so a spreadsheet or a slide deck cannot become markdown through the sidecar (§10) |
| xlsx | docx | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
| xlsx | pptx | soffice | cut | no measured Writer/Calc/Impress cross-family filter for this pair (ARCHITECTURE 8.3) |
| xlsx | rtf | soffice | cut | no measured Writer/Calc/Impress cross-family filter for this pair (ARCHITECTURE 8.3) |
| pptx | pdf | soffice | PROVEN | D7 — pptx -> pdf: 33,957 B, 2 pages 720x540 pt |
| pptx | html | soffice | PROVEN | C3B-PPTX-HTML — pptx -> html PASS both legs with html:impress_html_Export (single HTML file) |
| pptx | md | soffice | cut | measured: only Writer registers the Markdown export filter. A Calc or Impress document answers rc=1 `Class:Write Code:16` for `md:Markdown`, so a spreadsheet or a slide deck cannot become markdown through the sidecar (§10) |
| pptx | txt | soffice | cut | measured: Impress registers no plain-text export filter; both `txt:Text (encoded):UTF8` and the Calc text filter answer rc=1 `Class:Write Code:16` on a .ppt/.pptx (§10) |
| pptx | docx | soffice | cut | no measured Writer/Calc/Impress cross-family filter for this pair (ARCHITECTURE 8.3) |
| pptx | xlsx | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
| pptx | rtf | soffice | cut | no measured Writer/Calc/Impress cross-family filter for this pair (ARCHITECTURE 8.3) |
| pptx | csv | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
| doc | pdf | soffice | PROVEN | C3-DOC-PDF — doc -> pdf PASS both legs (26,460 B Linux / 46,489 B Windows, 1 page) |
| doc | md | soffice | PROVEN | C3-DOC-MD — doc -> md PASS both legs (385 B Linux / 400 B Windows) |
| doc | txt | soffice | PROVEN | C3-DOC-TXT — doc -> txt PASS both legs (341 B Linux / 349 B Windows via txt:Text (encoded):UTF8) |
| doc | docx | soffice | PROVEN | C3-DOC-DOCX — doc -> docx PASS both legs (8,771 B Linux / 8,791 B Windows) |
| doc | html | soffice | PROVEN | C3-DOC-HTML — doc -> html PASS both legs (4,352 B Linux / 4,427 B Windows) |
| doc | rtf | soffice | PROVEN | C3B-DOC-RTF — doc -> rtf PASS both legs (legacy Word -> RTF) |
| doc | pptx | soffice | cut | no measured Writer/Calc/Impress cross-family filter for this pair (ARCHITECTURE 8.3) |
| doc | xlsx | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
| doc | csv | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
| xls | pdf | soffice | PROVEN | C3-XLS-PDF — xls -> pdf PASS both legs with pdf:calc_pdf_Export (26,212 B / 41,142 B) |
| xls | csv | soffice | PROVEN | C3-XLS-CSV — xls -> csv PASS both legs, named <stem>-<sheet>.csv (147 B / 151 B) |
| xls | xlsx | soffice | PROVEN | C3-XLS-XLSX — xls -> xlsx PASS both legs (7,327 B / 7,330 B, workbooks re-opened) |
| xls | txt | soffice | PROVEN | C3B-XLS-TXT — xls -> txt PASS both legs (legacy Calc, same TAB-separated text filter) |
| xls | html | soffice | PROVEN | C3-XLS-HTML — xls -> html PASS both legs with html:HTML (StarCalc) (5,235 B / 5,330 B) |
| xls | md | soffice | cut | measured: only Writer registers the Markdown export filter. A Calc or Impress document answers rc=1 `Class:Write Code:16` for `md:Markdown`, so a spreadsheet or a slide deck cannot become markdown through the sidecar (§10) |
| xls | docx | soffice | cut | no measured Writer/Calc/Impress cross-family filter for this pair (ARCHITECTURE 8.3) |
| xls | pptx | soffice | cut | no measured Writer/Calc/Impress cross-family filter for this pair (ARCHITECTURE 8.3) |
| xls | rtf | soffice | cut | no measured Writer/Calc/Impress cross-family filter for this pair (ARCHITECTURE 8.3) |
| ppt | pdf | soffice | PROVEN | C3-PPT-PDF — ppt -> pdf PASS both legs with pdf:impress_pdf_Export (33,915 B / 79,102 B) |
| ppt | pptx | soffice | PROVEN | C3-PPT-PPTX — ppt -> pptx PASS both legs (pptx:Impress MS PowerPoint 2007 XML) |
| ppt | html | soffice | PROVEN | C3B-PPT-HTML — ppt -> html PASS both legs with html:impress_html_Export (single HTML file) |
| ppt | md | soffice | cut | measured: only Writer registers the Markdown export filter. A Calc or Impress document answers rc=1 `Class:Write Code:16` for `md:Markdown`, so a spreadsheet or a slide deck cannot become markdown through the sidecar (§10) |
| ppt | txt | soffice | cut | measured: Impress registers no plain-text export filter; both `txt:Text (encoded):UTF8` and the Calc text filter answer rc=1 `Class:Write Code:16` on a .ppt/.pptx (§10) |
| ppt | docx | soffice | cut | no measured Writer/Calc/Impress cross-family filter for this pair (ARCHITECTURE 8.3) |
| ppt | rtf | soffice | cut | no measured Writer/Calc/Impress cross-family filter for this pair (ARCHITECTURE 8.3) |
| ppt | xlsx | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
| ppt | csv | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
| rtf | pdf | soffice | PROVEN | C3-RTF-PDF — rtf -> pdf PASS both legs (13,100 B / 13,335 B) |
| rtf | md | soffice | PROVEN | C3B-RTF-MD — rtf -> md PASS both legs (md:Markdown, source text preserved) |
| rtf | txt | soffice | PROVEN | C3-RTF-TXT — rtf -> txt PASS both legs (33 B / 34 B) |
| rtf | docx | soffice | PROVEN | C3-RTF-DOCX — rtf -> docx PASS both legs (5,015 B / 5,041 B) |
| rtf | html | soffice | PROVEN | C3-RTF-HTML — rtf -> html PASS both legs (668 B / 687 B) |
| rtf | pptx | soffice | cut | no measured Writer/Calc/Impress cross-family filter for this pair (ARCHITECTURE 8.3) |
| rtf | xlsx | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
| rtf | csv | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
| csv | pdf | soffice | PROVEN | C3B-CSV-PDF — csv -> pdf PASS both legs with pdf:calc_pdf_Export (1 page) |
| csv | xlsx | soffice | PROVEN | D4 — csv -> xlsx: 5,802 B, numeric cells, UTF-8 via filter options |
| csv | txt | soffice | PROVEN | C3B-CSV-TXT — csv -> txt PASS both legs (Calc text filter, TAB-separated, cells intact) |
| csv | md | soffice | cut | measured: only Writer registers the Markdown export filter. A Calc or Impress document answers rc=1 `Class:Write Code:16` for `md:Markdown`, so a spreadsheet or a slide deck cannot become markdown through the sidecar (§10) |
| csv | html | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
| csv | docx | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
| csv | pptx | soffice | cut | no measured Writer/Calc/Impress cross-family filter for this pair (ARCHITECTURE 8.3) |
| csv | rtf | soffice | cut | no measured Writer/Calc/Impress cross-family filter for this pair (ARCHITECTURE 8.3) |
| md | pdf | soffice | PROVEN | C3B-MD-PDF — md -> pdf PASS both legs (1 page) |
| md | txt | soffice | PROVEN | C3B-MD-TXT — md -> txt PASS both legs (writer text filter, UTF-8) |
| md | docx | soffice | PROVEN | C3B-MD-DOCX — md -> docx PASS both legs (word/document.xml present) |
| md | html | soffice | PROVEN | C3B-MD-HTML — md -> html PASS both legs (the `<html` element is present in the output) |
| md | rtf | soffice | PROVEN | C3B-MD-RTF — md -> rtf PASS both legs (RTF header read back) |
| md | pptx | soffice | cut | no measured Writer/Calc/Impress cross-family filter for this pair (ARCHITECTURE 8.3) |
| md | xlsx | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
| md | csv | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
| html | pdf | soffice | PROVEN | C3B-HTML-PDF — html -> pdf PASS both legs (1 page, imported with HTML (StarWriter)) |
| html | md | soffice | PROVEN | C3B-HTML-MD — html -> md PASS both legs (heading kept as # Fixture; this replaces the unmeasured crate path) |
| html | docx | soffice | PROVEN | D6 — html -> docx: 6,863 B, real 2x2 Word table |
| html | rtf | soffice | PROVEN | C3B-HTML-RTF — html -> rtf PASS both legs (RTF header read back) |
| html | txt | soffice | cut | measured PASS (27 B) but not offered: html -> txt is the same text with the heading stripped, while html -> md from the identical source keeps it (`# Fixture`). v1 offers one text target per source and it is md (§10) |
| html | pptx | soffice | cut | no measured Writer/Calc/Impress cross-family filter for this pair (ARCHITECTURE 8.3) |
| html | xlsx | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
| html | csv | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
| txt | pdf | soffice | cut | measured PASS (pdf 14,522 B / docx 5,117 B / rtf 3,423 B) but not offered in v1: offering it turns the N3 trap (plain text renamed .docx is detected as Txt) from a refusal into a conversion, and that refusal is a shipped C3 acceptance. Flipping it is one route entry plus a stakeholder decision (§10) |
| txt | docx | soffice | cut | measured PASS (pdf 14,522 B / docx 5,117 B / rtf 3,423 B) but not offered in v1: offering it turns the N3 trap (plain text renamed .docx is detected as Txt) from a refusal into a conversion, and that refusal is a shipped C3 acceptance. Flipping it is one route entry plus a stakeholder decision (§10) |
| txt | rtf | soffice | cut | measured PASS (pdf 14,522 B / docx 5,117 B / rtf 3,423 B) but not offered in v1: offering it turns the N3 trap (plain text renamed .docx is detected as Txt) from a refusal into a conversion, and that refusal is a shipped C3 acceptance. Flipping it is one route entry plus a stakeholder decision (§10) |
| txt | md | soffice | cut | measured PASS but not offered: LibreOffice's markdown export of a .txt wraps the entire file in a fenced code block (174 B from a 164 B source), which is not what txt -> md means; cut on the measurement (§10) |
| txt | html | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
| txt | pptx | soffice | cut | no measured Writer/Calc/Impress cross-family filter for this pair (ARCHITECTURE 8.3) |
| txt | xlsx | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
| txt | csv | soffice | cut | not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3) |
<!-- matrix:end -->

Counts today: **148 `PROVEN` routes** — 37 image cells, 49 audio, 20 video, and **42
document directions** (7 from S3, 15 legacy/rtf from C3, 20 from C3b). Everything else is
`planned` or `cut`; the document grid has no `planned` cell left.

---

## 4. What the wheel shows today

`matrix::offered_targets()` — the only list the wheel may render
(`SPEC.md` §2: invalid targets are absent, not greyed).

<!-- offered:begin -->
| input | family | wheel-visible targets (Proven only) |
|---|---|---|
| jpeg | Image | jpg, png, webp, tiff, avif, gif |
| png | Image | jpg, webp, tiff, avif, gif |
| heic | Image | jpg, png, webp, tiff, avif, gif |
| avif | Image | jpg, png, webp, tiff, gif |
| webp | Image | jpg, png, tiff, avif, gif |
| gif | Image | jpg, png, webp, tiff, avif |
| tiff | Image | jpg, png, webp, avif, gif |
| bmp | Image | — |
| svg | Image | — |
| pdf | Document | — |
| mp3 | Audio | mp3, wav, flac, m4a, aac, ogg, opus |
| wav | Audio | mp3, wav, flac, m4a, aac, ogg, opus |
| flac | Audio | mp3, wav, flac, m4a, aac, ogg, opus |
| m4a | Audio | mp3, wav, flac, m4a, aac, ogg, opus |
| aac | Audio | mp3, wav, flac, m4a, aac, ogg, opus |
| ogg | Audio | mp3, wav, flac, m4a, aac, ogg, opus |
| opus | Audio | mp3, wav, flac, m4a, aac, ogg, opus |
| mp4 | Video | mp4, mkv, mov, webm, gif |
| mkv | Video | mp4, mkv, mov, webm, gif |
| mov | Video | mp4, mkv, mov, webm, gif |
| webm | Video | mp4, mkv, mov, webm, gif |
| docx | Document | pdf, md, txt, html, rtf |
| xlsx | Document | pdf, csv, txt, html |
| pptx | Document | pdf, html |
| doc | Document | pdf, md, txt, docx, html, rtf |
| xls | Document | pdf, csv, xlsx, txt, html |
| ppt | Document | pdf, pptx, html |
| rtf | Document | pdf, md, txt, docx, html |
| csv | Document | pdf, xlsx, txt |
| md | Document | pdf, txt, docx, html, rtf |
| html | Document | pdf, md, docx, rtf |
| txt | Document | — |
<!-- offered:end -->

---

## 5. SPEC §3 scope versus what is offered

`SPEC.md` §3 says which formats are in v1 scope. Scope is not capability: the
matrix models every in-scope target and labels the ones that are not proven yet.

<!-- scope:begin -->
| input | SPEC §3 scope (all modelled targets) | offered today |
|---|---|---|
| jpeg | jpg, png, webp, tiff, avif, gif, bmp, heic, svg, pdf | jpg, png, webp, tiff, avif, gif |
| png | jpg, webp, tiff, avif, gif, bmp, heic, svg, pdf | jpg, webp, tiff, avif, gif |
| heic | jpg, png, webp, tiff, avif, gif, bmp, heic, svg, pdf | jpg, png, webp, tiff, avif, gif |
| avif | jpg, png, webp, tiff, gif, bmp, heic, svg, pdf | jpg, png, webp, tiff, gif |
| webp | jpg, png, tiff, avif, gif, bmp, heic, svg, pdf | jpg, png, tiff, avif, gif |
| gif | jpg, png, webp, tiff, avif, gif, bmp, heic, svg, pdf | jpg, png, webp, tiff, avif |
| tiff | jpg, png, webp, avif, gif, bmp, heic, svg, pdf | jpg, png, webp, avif, gif |
| bmp | jpg, png, webp, tiff, avif, gif, bmp, heic, svg, pdf | — (nothing proven yet) |
| svg | jpg, png, webp, tiff, avif, gif, bmp, heic, svg, pdf | — (nothing proven yet) |
| pdf | jpg, png, webp, tiff, avif, gif, bmp, heic, svg, pdf, md, html, docx, pptx, rtf, xlsx, csv, txt | — (nothing proven yet) |
| mp3 | mp3, wav, flac, m4a, aac, ogg, opus | mp3, wav, flac, m4a, aac, ogg, opus |
| wav | mp3, wav, flac, m4a, aac, ogg, opus | mp3, wav, flac, m4a, aac, ogg, opus |
| flac | mp3, wav, flac, m4a, aac, ogg, opus | mp3, wav, flac, m4a, aac, ogg, opus |
| m4a | mp3, wav, flac, m4a, aac, ogg, opus | mp3, wav, flac, m4a, aac, ogg, opus |
| aac | mp3, wav, flac, m4a, aac, ogg, opus | mp3, wav, flac, m4a, aac, ogg, opus |
| ogg | mp3, wav, flac, m4a, aac, ogg, opus | mp3, wav, flac, m4a, aac, ogg, opus |
| opus | mp3, wav, flac, m4a, aac, ogg, opus | mp3, wav, flac, m4a, aac, ogg, opus |
| mp4 | mp4, mkv, mov, webm, gif | mp4, mkv, mov, webm, gif |
| mkv | mp4, mkv, mov, webm, gif | mp4, mkv, mov, webm, gif |
| mov | mp4, mkv, mov, webm, gif | mp4, mkv, mov, webm, gif |
| webm | mp4, mkv, mov, webm, gif | mp4, mkv, mov, webm, gif |
| docx | pdf, md, txt, html, rtf, xlsx, pptx, csv | pdf, md, txt, html, rtf |
| xlsx | pdf, csv, txt, html, md, docx, pptx, rtf | pdf, csv, txt, html |
| pptx | pdf, html, md, txt, docx, xlsx, rtf, csv | pdf, html |
| doc | pdf, md, txt, docx, html, rtf, pptx, xlsx, csv | pdf, md, txt, docx, html, rtf |
| xls | pdf, csv, xlsx, txt, html, md, docx, pptx, rtf | pdf, csv, xlsx, txt, html |
| ppt | pdf, pptx, html, md, txt, docx, rtf, xlsx, csv | pdf, pptx, html |
| rtf | pdf, md, txt, docx, html, pptx, xlsx, csv | pdf, md, txt, docx, html |
| csv | pdf, xlsx, txt, md, html, docx, pptx, rtf | pdf, xlsx, txt |
| md | pdf, txt, docx, html, rtf, pptx, xlsx, csv | pdf, txt, docx, html, rtf |
| html | pdf, md, docx, rtf, txt, pptx, xlsx, csv | pdf, md, docx, rtf |
| txt | pdf, docx, rtf, md, html, pptx, xlsx, csv | — (nothing proven yet) |
<!-- scope:end -->

Owner cards for `planned` rows (all on the `ezconvert` board):

| owner | card | what it owns |
|---|---|---|
| C2 | `t_dc07a72e` | media pipeline — vips images, ffmpeg audio/video, resvg, PDFium |
| C3 | `t_ea95c826` | document pipeline — soffice filters, markdown/html crates |
| C4 | `t_b2723c61` | output rules (naming, location, metadata, undo) — no route today |

---

## 6. Deliberate deltas from `ARCHITECTURE.md`

Recorded here rather than silently diverging:

1. **`Route` has no `plan` function yet.** §8.1 shows
   `plan: fn(&JobCtx) -> ExecPlan`; `engine::ExecPlan` and `JobCtx` do not exist
   (they arrive with `C2`/`C3`). A fn pointer to a non-existent type would be
   decoration, so `Route` is `{ target, engine, status }` for now and the plan
   field is added with the first real argv builder.
2. **Document targets are §8.3's seven columns plus `pptx` and `rtf`.** `SPEC.md`
   §3 names both as document formats, so they are modelled as targets
   (`planned`, owner C3). Legacy `.doc/.xls/.ppt` remain **input-only**: no
   filter contract or measurement exists for them as outputs. If the stakeholder
   wants document targets to be the §8.3 columns exactly, the two extra rows
   disappear and this paragraph is the record of why.
3. **`Integrity` keeps §7's three variants.** A zero-byte file is
   `Unknown` + `Detail::Empty` with `Integrity::Complete` (we read all of it); a
   refused input is `Unreadable` + `TooLarge`; an I/O failure is `Unreadable` +
   `Detail::Io`. No fourth variant was invented.
4. **`Engine::Ffmpeg` appears on image rows only for the GIF target** (SPEC §9.3
   D2: vips normalises, ffmpeg palette-encodes). §19's shorthand "no image route
   resolves to ffmpeg" predates D2; the test encodes the precise invariant:
   ffmpeg may touch an image only to encode GIF, never to decode.
5. **Identity routes** are modelled only where they are a real operation:
   `jpeg → jpg` (V6 autorot/compress) and every audio/video row (bitrate or trim
   change). `png → png`, `docx → docx`, `svg → svg` etc. are absent, and
   `heic → heic` is `cut` (D4).
6. **`types.rs` also carries the window's queue shapes** (`InputFile`,
   `ItemState`, `TARGET_FORMATS`, `human_size`) because `U2` merged into the same
   file after this card started. `TARGET_FORMATS` is a second list of target
   names, so a test asserts it equals `ALL_TARGETS` exactly — a drift guard, not
   an endorsement. The window should eventually ask `matrix::offered_targets`
   instead; until then, equality keeps the two honest.

---

## 7. Honest gaps

- **ZIP64 central directories are not parsed** (`Unsupported`). No Office
  document in the 2 GiB input cap needs them; large-archive support would be a
  new parser with its own fuzz corpus.
- **The signature table has no fuzz corpus yet.** `ARCHITECTURE.md` §19 puts
  detection-parser fuzzing in the QA gate (`t_1dd5b122`, `warden`); this card
  ships the fixture corpus, not the fuzzer.
- **Two image cell groups are proven on a relinked build.** V8/V8b were measured
  on the S2b dist (37 files), not the final 43-file dist; re-running that suite is
  `t_61e55863` (forge). The matrix says `PROVEN` because the architecture says so;
  the evidence column points at the measurement that exists.
- **macOS is unmeasured** for detection too (it is pure Rust, so the risk is
  low, but it is unmeasured).
- **A truncated ZIP cannot be typed**, only flagged truncated. That is
  deliberate: without a central directory there is no structural proof it was a
  docx rather than any other ZIP.
