# Evidence index — every `PROVEN` route

`ARCHITECTURE.md` §0: a route is `PROVEN` only when a measurement exists. This
file is the machine-checked index: `cargo test` asserts that each evidence file
still exists **and** still contains the literal quoted in `matrix.rs`.

Two provenance levels, stated so nobody confuses them:

- **transcript** — a raw measurement transcript is committed in this repo
  (`evidence/`, `spikes/s3/`) and the marker is grepped in it.
- **recorded** — measured on a named host and recorded in
  `docs/ARCHITECTURE.md`, but no raw transcript was committed for that exact
  invocation. None of today's routes are in this class; the level exists so an
  honest downgrade is expressible instead of a silent one.

Sources:

| file | what it is |
|---|---|
| `evidence/s2-libvips.md` | S2 spike, self-built GPL-free libvips 8.18.7 on Windows (V1-V11, HEIC decode, autorot, the four copy hops) |
| `evidence/s2b-avif.md` | S2b spike, libaom AVIF encode on the `spec-aom` dist (V8/V8b round trip, EXIF + ICC survival, 33 assertions) |
| `spikes/s3/EVIDENCE.md` | S3 spike, LibreOffice 26.8.2.2, the seven document directions on Linux and Windows (D1-D7) plus negative controls N1-N7 |
| `evidence/c3-document.md` | C3 pipeline, the same seven re-run through `engine::doc` **plus** the 15 legacy/rtf directions, batching, warm-up and the negative controls, on Linux **and** the Windows VM (C3-*) |

The generated table below is the contract. Regenerate it with
`/tmp/model-tables --write` after changing `matrix.rs`; the test fails on drift.

<!-- routes:begin -->
| evidence | provenance | source | claim | routes it proves |
|---|---|---|---|---|
| V6 | transcript | `evidence/s2-libvips.md` | autorot JPEG 400x200 -> 200x400, Orientation rewritten to 1 | jpeg → jpg |
| V2 | transcript | `evidence/s2-libvips.md` | jpg -> png OK (exit 0, PNG written) | jpeg → png |
| C2-JPEG-WEBP | transcript | `spikes/c2/EVIDENCE.md` | jpg -> webp OK (1,188 B, magic webp) | jpeg → webp |
| C2-JPEG-TIFF | transcript | `spikes/c2/EVIDENCE.md` | jpg -> tiff OK (3,508 B, ICC carried) | jpeg → tiff |
| V8 | transcript | `evidence/s2b-avif.md` | heifsave av1: png -> avif 3,863 B; 2 MP jpg -> avif 302,068 B | jpeg → avif, png → avif |
| C2-GIF-STILL | transcript | `spikes/c2/EVIDENCE.md` | still image -> gif via vips normalise + ffmpeg palette (1 frame, 1,680 B) | jpeg → gif, png → gif, webp → gif, tiff → gif |
| C2-PNG-JPEG | transcript | `spikes/c2/EVIDENCE.md` | png -> jpg OK (2,781 B) | png → jpg |
| V3 | transcript | `evidence/s2-libvips.md` | png -> webp OK (exit 0) | png → webp |
| C2-PNG-TIFF | transcript | `spikes/c2/EVIDENCE.md` | png -> tiff OK (522 B) | png → tiff |
| V7 | transcript | `evidence/s2-libvips.md` | heic -> jpg, 200x400, libheif already applied irot (autorot is a no-op) | heic → jpg |
| V8b | transcript | `evidence/s2b-avif.md` | avif -> jpg kept EXIF IFD0 and the 3,144 B ICC profile | avif → jpg |
| C2-WEBP-JPEG | transcript | `spikes/c2/EVIDENCE.md` | webp -> jpg OK (2,904 B) | webp → jpg |
| C2-WEBP-PNG | transcript | `spikes/c2/EVIDENCE.md` | webp -> png OK (4,346 B) | webp → png |
| V4 | transcript | `evidence/s2-libvips.md` | webp -> tiff OK (exit 0) | webp → tiff |
| C2-TIFF-JPEG | transcript | `spikes/c2/EVIDENCE.md` | tiff -> jpg OK (2,781 B) | tiff → jpg |
| V5 | transcript | `evidence/s2-libvips.md` | tiff -> png OK (exit 0) | tiff → png |
| C2-TIFF-WEBP | transcript | `spikes/c2/EVIDENCE.md` | tiff -> webp OK (1,130 B) | tiff → webp |
| C2-AUDIO-MP3 | transcript | `spikes/c2/EVIDENCE.md` | every audio source -> mp3 PASS (6 of 6, explicit -ar/-sample_fmt) | mp3 → mp3, wav → mp3, flac → mp3, m4a → mp3, aac → mp3, ogg → mp3, opus → mp3 |
| C2-AUDIO-WAV | transcript | `spikes/c2/EVIDENCE.md` | every audio source -> wav PASS (6 of 6) | mp3 → wav, wav → wav, flac → wav, m4a → wav, aac → wav, ogg → wav, opus → wav |
| C2-AUDIO-FLAC | transcript | `spikes/c2/EVIDENCE.md` | every audio source -> flac PASS (6 of 6) | mp3 → flac, wav → flac, flac → flac, m4a → flac, aac → flac, ogg → flac, opus → flac |
| C2-AUDIO-M4A | transcript | `spikes/c2/EVIDENCE.md` | every audio source -> m4a PASS (6 of 6) | mp3 → m4a, wav → m4a, flac → m4a, m4a → m4a, aac → m4a, ogg → m4a, opus → m4a |
| C2-AUDIO-AAC | transcript | `spikes/c2/EVIDENCE.md` | every audio source -> aac PASS (6 of 6) | mp3 → aac, wav → aac, flac → aac, m4a → aac, aac → aac, ogg → aac, opus → aac |
| C2-AUDIO-OGG | transcript | `spikes/c2/EVIDENCE.md` | every audio source -> ogg PASS (6 of 6) | mp3 → ogg, wav → ogg, flac → ogg, m4a → ogg, aac → ogg, ogg → ogg, opus → ogg |
| C2-AUDIO-OPUS | transcript | `spikes/c2/EVIDENCE.md` | every audio source -> opus PASS (6 of 6, 48 kHz) | mp3 → opus, wav → opus, flac → opus, m4a → opus, aac → opus, ogg → opus, opus → opus |
| C2-VIDEO-MP4 | transcript | `spikes/c2/EVIDENCE.md` | every video source -> mp4 PASS (3 of 3), CFR, libopenh264 + aac | mp4 → mp4, mkv → mp4, mov → mp4, webm → mp4 |
| C2-VIDEO-MKV | transcript | `spikes/c2/EVIDENCE.md` | every video source -> mkv PASS (3 of 3) | mp4 → mkv, mkv → mkv, mov → mkv, webm → mkv |
| C2-VIDEO-MOV | transcript | `spikes/c2/EVIDENCE.md` | every video source -> mov PASS (3 of 3) | mp4 → mov, mkv → mov, mov → mov, webm → mov |
| C2-VIDEO-WEBM | transcript | `spikes/c2/EVIDENCE.md` | every video source -> webm PASS (3 of 3), VP9 + Opus | mp4 → webm, mkv → webm, mov → webm, webm → webm |
| C2-VIDEO-GIF | transcript | `spikes/c2/EVIDENCE.md` | every video source -> gif PASS (4 of 4), fps + long-edge cap + palette | mp4 → gif, mkv → gif, mov → gif, webm → gif |
| D1 | transcript | `spikes/s3/EVIDENCE.md` | docx -> pdf: 26,460 B (Linux) / 39,946 B (Windows), 1 page 612x792 pt | docx → pdf |
| D2 | transcript | `spikes/s3/EVIDENCE.md` | docx -> md: 389 B Linux / 404 B Windows, real Markdown | docx → md |
| D3 | transcript | `spikes/s3/EVIDENCE.md` | docx -> html: 4,064 B, h1 + table + 13 p | docx → html |
| D5 | transcript | `spikes/s3/EVIDENCE.md` | xlsx -> csv: 147 B, named <stem>-<sheet>.csv | xlsx → csv |
| D7 | transcript | `spikes/s3/EVIDENCE.md` | pptx -> pdf: 33,957 B, 2 pages 720x540 pt | pptx → pdf |
| C3-DOC-PDF | transcript | `evidence/c3-document.md` | doc -> pdf PASS both legs (26,460 B Linux / 46,489 B Windows, 1 page) | doc → pdf |
| C3-DOC-MD | transcript | `evidence/c3-document.md` | doc -> md PASS both legs (385 B Linux / 400 B Windows) | doc → md |
| C3-DOC-HTML | transcript | `evidence/c3-document.md` | doc -> html PASS both legs (4,352 B Linux / 4,427 B Windows) | doc → html |
| C3-DOC-DOCX | transcript | `evidence/c3-document.md` | doc -> docx PASS both legs (8,771 B Linux / 8,791 B Windows) | doc → docx |
| C3-DOC-TXT | transcript | `evidence/c3-document.md` | doc -> txt PASS both legs (341 B Linux / 349 B Windows via txt:Text (encoded):UTF8) | doc → txt |
| C3-XLS-PDF | transcript | `evidence/c3-document.md` | xls -> pdf PASS both legs with pdf:calc_pdf_Export (26,212 B / 41,142 B) | xls → pdf |
| C3-XLS-HTML | transcript | `evidence/c3-document.md` | xls -> html PASS both legs with html:HTML (StarCalc) (5,235 B / 5,330 B) | xls → html |
| C3-XLS-XLSX | transcript | `evidence/c3-document.md` | xls -> xlsx PASS both legs (7,327 B / 7,330 B, workbooks re-opened) | xls → xlsx |
| C3-XLS-CSV | transcript | `evidence/c3-document.md` | xls -> csv PASS both legs, named <stem>-<sheet>.csv (147 B / 151 B) | xls → csv |
| C3-PPT-PDF | transcript | `evidence/c3-document.md` | ppt -> pdf PASS both legs with pdf:impress_pdf_Export (33,915 B / 79,102 B) | ppt → pdf |
| C3-PPT-PPTX | transcript | `evidence/c3-document.md` | ppt -> pptx PASS both legs (pptx:Impress MS PowerPoint 2007 XML) | ppt → pptx |
| C3-RTF-PDF | transcript | `evidence/c3-document.md` | rtf -> pdf PASS both legs (13,100 B / 13,335 B) | rtf → pdf |
| C3-RTF-HTML | transcript | `evidence/c3-document.md` | rtf -> html PASS both legs (668 B / 687 B) | rtf → html |
| C3-RTF-DOCX | transcript | `evidence/c3-document.md` | rtf -> docx PASS both legs (5,015 B / 5,041 B) | rtf → docx |
| C3-RTF-TXT | transcript | `evidence/c3-document.md` | rtf -> txt PASS both legs (33 B / 34 B) | rtf → txt |
| D4 | transcript | `spikes/s3/EVIDENCE.md` | csv -> xlsx: 5,802 B, numeric cells, UTF-8 via filter options | csv → xlsx |
| D6 | transcript | `spikes/s3/EVIDENCE.md` | html -> docx: 6,863 B, real 2x2 Word table | html → docx |
<!-- routes:end -->

## Notes that the table cannot carry

- **V8 backs two cells** (`jpeg → avif` and `png → avif`): the S2b suite measured
  both inputs. Nine image cells therefore rest on eight evidence items.
- **V2-V5 are a single summary line** in the S2 transcript
  (`jpg->png OK | png->webp OK | webp->tiff OK | tiff->png OK`), not four
  separate command transcripts. That is the weakest link in the image evidence.
- **AVIF was proven on the S2b dist (37 files), not the final 43-file dist.**
  Re-running the suite on the shipped dist is `t_61e55863` (forge) and a release
  gate (`ARCHITECTURE.md` §19). The matrix keeps `PROVEN` because the
  architecture does; the caveat is real and belongs to that card.
- **TIFF input is `planned`, not proven, for every direction except
  `tiff → png`** — and even that pair was measured with plain `copy`, so the
  production route must use `tiffload … --fail-on error` (V9/V10: JBIG TIFF
  gave a silent black frame at exit 0).
