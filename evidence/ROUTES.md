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

The generated table below is the contract. Regenerate it with
`/tmp/model-tables --write` after changing `matrix.rs`; the test fails on drift.

<!-- routes:begin -->
| evidence | provenance | source | claim | routes it proves |
|---|---|---|---|---|
| V6 | transcript | `evidence/s2-libvips.md` | autorot JPEG 400x200 -> 200x400, Orientation rewritten to 1 | jpeg → jpg |
| V2 | transcript | `evidence/s2-libvips.md` | jpg -> png OK (exit 0, PNG written) | jpeg → png |
| V8 | transcript | `evidence/s2b-avif.md` | heifsave av1: png -> avif 3,863 B; 2 MP jpg -> avif 302,068 B | jpeg → avif, png → avif |
| V3 | transcript | `evidence/s2-libvips.md` | png -> webp OK (exit 0) | png → webp |
| V7 | transcript | `evidence/s2-libvips.md` | heic -> jpg, 200x400, libheif already applied irot (autorot is a no-op) | heic → jpg |
| V8b | transcript | `evidence/s2b-avif.md` | avif -> jpg kept EXIF IFD0 and the 3,144 B ICC profile | avif → jpg |
| V4 | transcript | `evidence/s2-libvips.md` | webp -> tiff OK (exit 0) | webp → tiff |
| V5 | transcript | `evidence/s2-libvips.md` | tiff -> png OK (exit 0) | tiff → png |
| D1 | transcript | `spikes/s3/EVIDENCE.md` | docx -> pdf: 26,460 B (Linux) / 39,946 B (Windows), 1 page 612x792 pt | docx → pdf |
| D2 | transcript | `spikes/s3/EVIDENCE.md` | docx -> md: 389 B Linux / 404 B Windows, real Markdown | docx → md |
| D3 | transcript | `spikes/s3/EVIDENCE.md` | docx -> html: 4,064 B, h1 + table + 13 p | docx → html |
| D5 | transcript | `spikes/s3/EVIDENCE.md` | xlsx -> csv: 147 B, named <stem>-<sheet>.csv | xlsx → csv |
| D7 | transcript | `spikes/s3/EVIDENCE.md` | pptx -> pdf: 33,957 B, 2 pages 720x540 pt | pptx → pdf |
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
