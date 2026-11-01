//! The target matrix: what a detected file can become, and how sure we are.
//!
//! `ARCHITECTURE.md` §8 is the contract. §0 is the rule that shapes the data:
//! **a capability exists when evidence exists, not when code exists.**
//!
//! The wheel reads [`offered_targets`] and shows only `Proven` routes; `Planned`
//! routes are invisible (absent, not greyed — `SPEC.md` §2) until their owner
//! card flips them with a fixture; `Cut` routes are measured dead and stay dead.
//!
//! Three tables are generated from this module and asserted in tests, so
//! documentation cannot drift from code:
//!
//! - `docs/DETECTION.md` — the type → target matrix (the documented table the
//!   acceptance criteria ask for);
//! - `evidence/ROUTES.md` — every `Proven` route's evidence, claim and source;
//! - the wheel-visible summary embedded in `docs/DETECTION.md`.

use super::types::{
    CanonicalType, CardId, Engine, EvidenceId, Provenance, Route, RouteStatus, TargetFormat,
    ALL_CANONICAL_TYPES, ALL_TARGETS,
};

// ---------------------------------------------------------------------------
// Owner cards (§0: a Planned route names the card that will move it)
// ---------------------------------------------------------------------------

/// C2 — media pipeline (vips images, ffmpeg audio/video).
pub const OWNER_C2: CardId = CardId("t_dc07a72e");
/// C3b — the remaining document directions. `C3` (`t_ea95c826`) shipped the pipeline and
/// flipped 22 cells; this card decided the 33 that were left. **It owns no `Planned` route
/// now** — every document cell is `Proven` or `Cut` (`evidence/c3-document.md` §10) — so the
/// constant stays for provenance, not as an owner.
pub const OWNER_C3: CardId = CardId("t_f8d19f60");
/// C4 — output rules. No route is owned here today; kept for symmetry.
pub const OWNER_C4: CardId = CardId("t_b2723c61");

// ---------------------------------------------------------------------------
// Evidence (§0; markers are greped by the tests)
// ---------------------------------------------------------------------------

const E_S2: &str = "evidence/s2-libvips.md";
const E_S2B: &str = "evidence/s2b-avif.md";
const E_S3: &str = "spikes/s3/EVIDENCE.md";
const E_C2: &str = "spikes/c2/EVIDENCE.md";
const E_C3: &str = "evidence/c3-document.md";

const EV_V2: EvidenceId = EvidenceId {
    label: "V2",
    claim: "jpg -> png OK (exit 0, PNG written)",
    source: E_S2,
    marker: "jpg->png OK",
    provenance: Provenance::Transcript,
};
const EV_V3: EvidenceId = EvidenceId {
    label: "V3",
    claim: "png -> webp OK (exit 0)",
    source: E_S2,
    marker: "png->webp OK",
    provenance: Provenance::Transcript,
};
const EV_V4: EvidenceId = EvidenceId {
    label: "V4",
    claim: "webp -> tiff OK (exit 0)",
    source: E_S2,
    marker: "webp->tiff OK",
    provenance: Provenance::Transcript,
};
const EV_V5: EvidenceId = EvidenceId {
    label: "V5",
    claim: "tiff -> png OK (exit 0)",
    source: E_S2,
    marker: "tiff->png OK",
    provenance: Provenance::Transcript,
};
const EV_V6: EvidenceId = EvidenceId {
    label: "V6",
    claim: "autorot JPEG 400x200 -> 200x400, Orientation rewritten to 1",
    source: E_S2,
    marker: "out: jpg_autorot.jpg 200x400",
    provenance: Provenance::Transcript,
};
const EV_V7: EvidenceId = EvidenceId {
    label: "V7",
    claim: "heic -> jpg, 200x400, libheif already applied irot (autorot is a no-op)",
    source: E_S2,
    marker: "spec_out.jpg: 200x400",
    provenance: Provenance::Transcript,
};
const EV_V8: EvidenceId = EvidenceId {
    label: "V8",
    claim: "heifsave av1: png -> avif 3,863 B; 2 MP jpg -> avif 302,068 B",
    source: E_S2B,
    marker: "PASS  png -> avif (400x200)",
    provenance: Provenance::Transcript,
};
const EV_V8B: EvidenceId = EvidenceId {
    label: "V8b",
    claim: "avif -> jpg kept EXIF IFD0 and the 3,144 B ICC profile",
    source: E_S2B,
    marker: "PASS  avif->jpg kept EXIF IFD0",
    provenance: Provenance::Transcript,
};
const EV_D1: EvidenceId = EvidenceId {
    label: "D1",
    claim: "docx -> pdf: 26,460 B (Linux) / 39,946 B (Windows), 1 page 612x792 pt",
    source: E_S3,
    marker: "--convert-to pdf:writer_pdf_Export",
    provenance: Provenance::Transcript,
};
const EV_D2: EvidenceId = EvidenceId {
    label: "D2",
    claim: "docx -> md: 389 B Linux / 404 B Windows, real Markdown",
    source: E_S3,
    marker: "--convert-to md:Markdown",
    provenance: Provenance::Transcript,
};
const EV_D3: EvidenceId = EvidenceId {
    label: "D3",
    claim: "docx -> html: 4,064 B, h1 + table + 13 p",
    source: E_S3,
    marker: "--convert-to html:HTML (StarWriter)",
    provenance: Provenance::Transcript,
};
const EV_D4: EvidenceId = EvidenceId {
    label: "D4",
    claim: "csv -> xlsx: 5,802 B, numeric cells, UTF-8 via filter options",
    source: E_S3,
    marker: "--convert-to xlsx:Calc MS Excel 2007 XML",
    provenance: Provenance::Transcript,
};
const EV_D5: EvidenceId = EvidenceId {
    label: "D5",
    claim: "xlsx -> csv: 147 B, named <stem>-<sheet>.csv",
    source: E_S3,
    marker:
        "--convert-to csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true,false,false,1",
    provenance: Provenance::Transcript,
};
const EV_D6: EvidenceId = EvidenceId {
    label: "D6",
    claim: "html -> docx: 6,863 B, real 2x2 Word table",
    source: E_S3,
    marker: "--convert-to docx:MS Word 2007 XML",
    provenance: Provenance::Transcript,
};
const EV_D7: EvidenceId = EvidenceId {
    label: "D7",
    claim: "pptx -> pdf: 33,957 B, 2 pages 720x540 pt",
    source: E_S3,
    marker: "--convert-to pdf:impress_pdf_Export",
    provenance: Provenance::Transcript,
};

// ---------------------------------------------------------------------------
// C3 — the legacy and rtf document rows, measured on Linux *and* Windows
// (evidence/c3-document.md §6). One EvidenceId per direction; the marker is the
// verbatim PASS line in that file, so a stale claim fails the matrix test.
// ---------------------------------------------------------------------------

const fn c3(label: &'static str, claim: &'static str, marker: &'static str) -> EvidenceId {
    EvidenceId {
        label,
        claim,
        source: E_C3,
        marker,
        provenance: Provenance::Transcript,
    }
}

const EV_C3_DOC_PDF: EvidenceId = c3(
    "C3-DOC-PDF",
    "doc -> pdf PASS both legs (26,460 B Linux / 46,489 B Windows, 1 page)",
    "doc -> pdf PASS",
);
const EV_C3_DOC_MD: EvidenceId = c3(
    "C3-DOC-MD",
    "doc -> md PASS both legs (385 B Linux / 400 B Windows)",
    "doc -> md PASS",
);
const EV_C3_DOC_HTML: EvidenceId = c3(
    "C3-DOC-HTML",
    "doc -> html PASS both legs (4,352 B Linux / 4,427 B Windows)",
    "doc -> html PASS",
);
const EV_C3_DOC_DOCX: EvidenceId = c3(
    "C3-DOC-DOCX",
    "doc -> docx PASS both legs (8,771 B Linux / 8,791 B Windows)",
    "doc -> docx PASS",
);
const EV_C3_DOC_TXT: EvidenceId = c3(
    "C3-DOC-TXT",
    "doc -> txt PASS both legs (341 B Linux / 349 B Windows via txt:Text (encoded):UTF8)",
    "doc -> txt PASS",
);
const EV_C3_XLS_PDF: EvidenceId = c3(
    "C3-XLS-PDF",
    "xls -> pdf PASS both legs with pdf:calc_pdf_Export (26,212 B / 41,142 B)",
    "xls -> pdf PASS",
);
const EV_C3_XLS_CSV: EvidenceId = c3(
    "C3-XLS-CSV",
    "xls -> csv PASS both legs, named <stem>-<sheet>.csv (147 B / 151 B)",
    "xls -> csv PASS",
);
const EV_C3_XLS_XLSX: EvidenceId = c3(
    "C3-XLS-XLSX",
    "xls -> xlsx PASS both legs (7,327 B / 7,330 B, workbooks re-opened)",
    "xls -> xlsx PASS",
);
const EV_C3_XLS_HTML: EvidenceId = c3(
    "C3-XLS-HTML",
    "xls -> html PASS both legs with html:HTML (StarCalc) (5,235 B / 5,330 B)",
    "xls -> html PASS",
);
const EV_C3_PPT_PDF: EvidenceId = c3(
    "C3-PPT-PDF",
    "ppt -> pdf PASS both legs with pdf:impress_pdf_Export (33,915 B / 79,102 B)",
    "ppt -> pdf PASS",
);
const EV_C3_PPT_PPTX: EvidenceId = c3(
    "C3-PPT-PPTX",
    "ppt -> pptx PASS both legs (pptx:Impress MS PowerPoint 2007 XML)",
    "ppt -> pptx PASS",
);
const EV_C3_RTF_PDF: EvidenceId = c3(
    "C3-RTF-PDF",
    "rtf -> pdf PASS both legs (13,100 B / 13,335 B)",
    "rtf -> pdf PASS",
);
const EV_C3_RTF_DOCX: EvidenceId = c3(
    "C3-RTF-DOCX",
    "rtf -> docx PASS both legs (5,015 B / 5,041 B)",
    "rtf -> docx PASS",
);
const EV_C3_RTF_HTML: EvidenceId = c3(
    "C3-RTF-HTML",
    "rtf -> html PASS both legs (668 B / 687 B)",
    "rtf -> html PASS",
);
const EV_C3_RTF_TXT: EvidenceId = c3(
    "C3-RTF-TXT",
    "rtf -> txt PASS both legs (33 B / 34 B)",
    "rtf -> txt PASS",
);

// ---------------------------------------------------------------------------
// C3b — the remaining document cells, decided and measured 2026-11-01
// (evidence/c3-document.md §10). Same rule as C3: one EvidenceId per direction,
// the marker is the verbatim PASS line in that file.
// ---------------------------------------------------------------------------

const EV_C3B_DOCX_TXT: EvidenceId = c3(
    "C3B-DOCX-TXT",
    "docx -> txt PASS both legs (writer text filter, UTF-8)",
    "docx -> txt PASS",
);
const EV_C3B_DOCX_RTF: EvidenceId = c3(
    "C3B-DOCX-RTF",
    "docx -> rtf PASS both legs (rtf:Rich Text Format, RTF header read back)",
    "docx -> rtf PASS",
);
const EV_C3B_DOC_RTF: EvidenceId = c3(
    "C3B-DOC-RTF",
    "doc -> rtf PASS both legs (legacy Word -> RTF)",
    "doc -> rtf PASS",
);
const EV_C3B_RTF_MD: EvidenceId = c3(
    "C3B-RTF-MD",
    "rtf -> md PASS both legs (md:Markdown, source text preserved)",
    "rtf -> md PASS",
);
const EV_C3B_MD_PDF: EvidenceId = c3(
    "C3B-MD-PDF",
    "md -> pdf PASS both legs (1 page)",
    "md -> pdf PASS",
);
const EV_C3B_MD_DOCX: EvidenceId = c3(
    "C3B-MD-DOCX",
    "md -> docx PASS both legs (word/document.xml present)",
    "md -> docx PASS",
);
const EV_C3B_MD_RTF: EvidenceId = c3(
    "C3B-MD-RTF",
    "md -> rtf PASS both legs (RTF header read back)",
    "md -> rtf PASS",
);
const EV_C3B_MD_TXT: EvidenceId = c3(
    "C3B-MD-TXT",
    "md -> txt PASS both legs (writer text filter, UTF-8)",
    "md -> txt PASS",
);
const EV_C3B_MD_HTML: EvidenceId = c3(
    "C3B-MD-HTML",
    "md -> html PASS both legs (the `<html` element is present in the output)",
    "md -> html PASS",
);
const EV_C3B_HTML_PDF: EvidenceId = c3(
    "C3B-HTML-PDF",
    "html -> pdf PASS both legs (1 page, imported with HTML (StarWriter))",
    "html -> pdf PASS",
);
const EV_C3B_HTML_MD: EvidenceId = c3(
    "C3B-HTML-MD",
    "html -> md PASS both legs (heading kept as # Fixture; this replaces the unmeasured crate path)",
    "html -> md PASS",
);
const EV_C3B_HTML_RTF: EvidenceId = c3(
    "C3B-HTML-RTF",
    "html -> rtf PASS both legs (RTF header read back)",
    "html -> rtf PASS",
);
const EV_C3B_XLSX_PDF: EvidenceId = c3(
    "C3B-XLSX-PDF",
    "xlsx -> pdf PASS both legs with pdf:calc_pdf_Export (2 pages: two sheets)",
    "xlsx -> pdf PASS",
);
const EV_C3B_XLSX_HTML: EvidenceId = c3(
    "C3B-XLSX-HTML",
    "xlsx -> html PASS both legs with html:HTML (StarCalc)",
    "xlsx -> html PASS",
);
const EV_C3B_XLSX_TXT: EvidenceId = c3(
    "C3B-XLSX-TXT",
    "xlsx -> txt PASS both legs via the Calc text filter, TAB-separated, named <stem>-<sheet>.txt",
    "xlsx -> txt PASS",
);
const EV_C3B_XLS_TXT: EvidenceId = c3(
    "C3B-XLS-TXT",
    "xls -> txt PASS both legs (legacy Calc, same TAB-separated text filter)",
    "xls -> txt PASS",
);
const EV_C3B_CSV_PDF: EvidenceId = c3(
    "C3B-CSV-PDF",
    "csv -> pdf PASS both legs with pdf:calc_pdf_Export (1 page)",
    "csv -> pdf PASS",
);
const EV_C3B_CSV_TXT: EvidenceId = c3(
    "C3B-CSV-TXT",
    "csv -> txt PASS both legs (Calc text filter, TAB-separated, cells intact)",
    "csv -> txt PASS",
);
const EV_C3B_PPTX_HTML: EvidenceId = c3(
    "C3B-PPTX-HTML",
    "pptx -> html PASS both legs with html:impress_html_Export (single HTML file)",
    "pptx -> html PASS",
);
const EV_C3B_PPT_HTML: EvidenceId = c3(
    "C3B-PPT-HTML",
    "ppt -> html PASS both legs with html:impress_html_Export (single HTML file)",
    "ppt -> html PASS",
);

// ---------------------------------------------------------------------------
// Status shorthands
// ---------------------------------------------------------------------------

// C2 — the media matrix. One marker per direction family; the source file
// carries the transcript and the engine identity (vips 8.16.1 CLI + LGPL
// ffmpeg N-128273) plus the notes about what was NOT measured on that leg.

const EV_C2_JPG_WEBP: EvidenceId = EvidenceId {
    label: "C2-JPEG-WEBP",
    claim: "jpg -> webp OK (1,188 B, magic webp)",
    source: E_C2,
    marker: "Jpeg -> webp PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_JPG_TIFF: EvidenceId = EvidenceId {
    label: "C2-JPEG-TIFF",
    claim: "jpg -> tiff OK (3,508 B, ICC carried)",
    source: E_C2,
    marker: "Jpeg -> tiff PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_PNG_JPG: EvidenceId = EvidenceId {
    label: "C2-PNG-JPEG",
    claim: "png -> jpg OK (2,781 B)",
    source: E_C2,
    marker: "Png -> jpg PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_PNG_TIFF: EvidenceId = EvidenceId {
    label: "C2-PNG-TIFF",
    claim: "png -> tiff OK (522 B)",
    source: E_C2,
    marker: "Png -> tiff PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_WEBP_JPG: EvidenceId = EvidenceId {
    label: "C2-WEBP-JPEG",
    claim: "webp -> jpg OK (2,904 B)",
    source: E_C2,
    marker: "Webp -> jpg PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_WEBP_PNG: EvidenceId = EvidenceId {
    label: "C2-WEBP-PNG",
    claim: "webp -> png OK (4,346 B)",
    source: E_C2,
    marker: "Webp -> png PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_TIFF_JPG: EvidenceId = EvidenceId {
    label: "C2-TIFF-JPEG",
    claim: "tiff -> jpg OK (2,781 B)",
    source: E_C2,
    marker: "Tiff -> jpg PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_TIFF_WEBP: EvidenceId = EvidenceId {
    label: "C2-TIFF-WEBP",
    claim: "tiff -> webp OK (1,130 B)",
    source: E_C2,
    marker: "Tiff -> webp PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_GIF_STILL: EvidenceId = EvidenceId {
    label: "C2-GIF-STILL",
    claim: "still image -> gif via vips normalise + ffmpeg palette (1 frame, 1,680 B)",
    source: E_C2,
    marker: "Jpeg -> gif PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_META: EvidenceId = EvidenceId {
    label: "C2-METADATA",
    claim: "EXIF orientation and the 736 B sRGB ICC survive jpg -> png/webp/tiff; strip removes both",
    source: E_C2,
    marker: "EXIF+ICC kept -> png PASS",
    provenance: Provenance::Transcript,
};

const EV_C2_AUDIO_MP3: EvidenceId = EvidenceId {
    label: "C2-AUDIO-MP3",
    claim: "every audio source -> mp3 PASS (6 of 6, explicit -ar/-sample_fmt)",
    source: E_C2,
    marker: "target mp3: 6 of 6 sources PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_AUDIO_WAV: EvidenceId = EvidenceId {
    label: "C2-AUDIO-WAV",
    claim: "every audio source -> wav PASS (6 of 6)",
    source: E_C2,
    marker: "target wav: 6 of 6 sources PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_AUDIO_FLAC: EvidenceId = EvidenceId {
    label: "C2-AUDIO-FLAC",
    claim: "every audio source -> flac PASS (6 of 6)",
    source: E_C2,
    marker: "target flac: 6 of 6 sources PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_AUDIO_M4A: EvidenceId = EvidenceId {
    label: "C2-AUDIO-M4A",
    claim: "every audio source -> m4a PASS (6 of 6)",
    source: E_C2,
    marker: "target m4a: 6 of 6 sources PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_AUDIO_AAC: EvidenceId = EvidenceId {
    label: "C2-AUDIO-AAC",
    claim: "every audio source -> aac PASS (6 of 6)",
    source: E_C2,
    marker: "target aac: 6 of 6 sources PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_AUDIO_OGG: EvidenceId = EvidenceId {
    label: "C2-AUDIO-OGG",
    claim: "every audio source -> ogg PASS (6 of 6)",
    source: E_C2,
    marker: "target ogg: 6 of 6 sources PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_AUDIO_OPUS: EvidenceId = EvidenceId {
    label: "C2-AUDIO-OPUS",
    claim: "every audio source -> opus PASS (6 of 6, 48 kHz)",
    source: E_C2,
    marker: "target opus: 6 of 6 sources PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_TAGS: EvidenceId = EvidenceId {
    label: "C2-TAGS",
    claim: "title/artist/album/date + cover art survive mp3 -> wav/flac/m4a/ogg/opus/aac via lofty",
    source: E_C2,
    marker: "tags survive mp3 -> ogg   5 items, 1 pictures, readback has title",
    provenance: Provenance::Transcript,
};

const EV_C2_VIDEO_MP4: EvidenceId = EvidenceId {
    label: "C2-VIDEO-MP4",
    claim: "every video source -> mp4 PASS (3 of 3), CFR, libopenh264 + aac",
    source: E_C2,
    marker: "target mp4: 3 of 3 sources PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_VIDEO_MKV: EvidenceId = EvidenceId {
    label: "C2-VIDEO-MKV",
    claim: "every video source -> mkv PASS (3 of 3)",
    source: E_C2,
    marker: "target mkv: 3 of 3 sources PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_VIDEO_MOV: EvidenceId = EvidenceId {
    label: "C2-VIDEO-MOV",
    claim: "every video source -> mov PASS (3 of 3)",
    source: E_C2,
    marker: "target mov: 3 of 3 sources PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_VIDEO_WEBM: EvidenceId = EvidenceId {
    label: "C2-VIDEO-WEBM",
    claim: "every video source -> webm PASS (3 of 3), VP9 + Opus",
    source: E_C2,
    marker: "target webm: 3 of 3 sources PASS",
    provenance: Provenance::Transcript,
};
const EV_C2_VIDEO_GIF: EvidenceId = EvidenceId {
    label: "C2-VIDEO-GIF",
    claim: "every video source -> gif PASS (4 of 4), fps + long-edge cap + palette",
    source: E_C2,
    marker: "target gif (from video): 4 of 4 sources PASS",
    provenance: Provenance::Transcript,
};

const E_C2W: &str = "spikes/c2/EVIDENCE-WINDOWS.md";

/// Card `t_44676171`: the same pipeline, run on the dist the app actually ships
/// (`spec-aom` libvips 8.18.7) with a Windows LGPL ffmpeg. The Linux leg could not load
/// its HEIF module, so AVIF output had no transcript of its own and HEIC/AVIF *decode*
/// had no matrix row at all. These constants point at that run
/// (`SUMMARY pass=120 fail=0 skip=0`).
const EV_C2W_AVIF: EvidenceId = EvidenceId {
    label: "C2W-AVIF",
    claim: "avif out on the shipping dist: jpg/png/webp/tiff/gif/heic -> avif PASS (875-1740 B), magic-checked",
    source: E_C2W,
    marker: "PASS  Jpeg->\"avif\" sample.jpg",
    provenance: Provenance::Transcript,
};
const EV_C2W_HEIC_DECODE: EvidenceId = EvidenceId {
    label: "C2W-HEIC-DECODE",
    claim: "heic decode: heic -> png/webp/tiff PASS, 200x400 (libheif already applied irot)",
    source: E_C2W,
    marker: "PASS  Heic->\"png\" sample.heic",
    provenance: Provenance::Transcript,
};
const EV_C2W_AVIF_DECODE: EvidenceId = EvidenceId {
    label: "C2W-AVIF-DECODE",
    claim: "avif decode: avif -> png/webp/tiff PASS, 64x48, codec read back by ffprobe",
    source: E_C2W,
    marker: "PASS  Avif->\"png\" sample.avif",
    provenance: Provenance::Transcript,
};
const EV_C2W_HEIF_GIF: EvidenceId = EvidenceId {
    label: "C2W-HEIF-GIF",
    claim: "heic/avif -> gif PASS via vips normalise -> ffmpeg palette, 200x400 / 64x48",
    source: E_C2W,
    marker: "PASS  Heic->\"gif\" sample.heic",
    provenance: Provenance::Transcript,
};
/// v1 decision (Option C): a GIF source is offered for image targets and converts
/// **frame 1 only**; the route must say so in the UI (`ARCHITECTURE.md` §8.2.1).
const EV_C2W_GIF_IN: EvidenceId = EvidenceId {
    label: "C2W-GIF-IN",
    claim: "gif source -> jpg/png/webp/tiff/avif PASS; frame 1 only, labelled in the UI and the route",
    source: E_C2W,
    marker: "PASS  Gif->\"jpg\" sample.gif",
    provenance: Provenance::Transcript,
};

const PLANNED_GIF_IN: RouteStatus = RouteStatus::Planned {
    owner: OWNER_C2,
    hypothesis: "gif -> gif (re-encode of an animation) is not measured and not offered yet; \
                  gif *input* for other image targets is offered and converts frame 1 only \
                  (Option C, ARCHITECTURE.md §8.2.1)",
};

const PLANNED_IMG: RouteStatus = RouteStatus::Planned {
    owner: OWNER_C2,
    hypothesis: "same vips mechanism, different saver; this exact pair has no fixture yet",
};
const PLANNED_IMG_GIF: RouteStatus = RouteStatus::Planned {
    owner: OWNER_C2,
    hypothesis: "vips normalise -> ffmpeg palettegen/paletteuse (SPEC 9.3 D2), unmeasured",
};
const PLANNED_IMG_BMP: RouteStatus = RouteStatus::Planned {
    owner: OWNER_C2,
    hypothesis: "image crate <-> temp PNG <-> vips (SPEC 9.3 D5), unmeasured",
};
const PLANNED_SVG_IN: RouteStatus = RouteStatus::Planned {
    owner: OWNER_C2,
    hypothesis: "resvg raster hop -> lossless PNG -> vips (SPEC 9.3 D3); resvg never built or run",
};
const PLANNED_PDF_IN: RouteStatus = RouteStatus::Planned {
    owner: OWNER_C2,
    hypothesis: "PDFium raster -> vips (ARCHITECTURE 6.4); PDFium binary never built or run",
};
const CUT_HEIC: RouteStatus = RouteStatus::Cut {
    reason: "HEIC encode cut: x265 is GPL-2 and Media Foundation HEVC needs a Store codec pack (SPEC 9.3 D4)",
};
const CUT_SVG: RouteStatus = RouteStatus::Cut {
    reason: "svg is input-only in v1: no svg->svg and no svg->pdf (SPEC 9.3 D3)",
};
const CUT_IMG_PDF: RouteStatus = RouteStatus::Cut {
    reason: "no image writer to PDF in v1 (ARCHITECTURE 8.2)",
};
const CUT_DOC: RouteStatus = RouteStatus::Cut {
    reason: "not offered in v1: no measured filter for this pair (ARCHITECTURE 8.3)",
};
const CUT_PDF_WRITER: RouteStatus = RouteStatus::Cut {
    reason: "no raster-to-writer path for PDF input (ARCHITECTURE 8.3)",
};
const CUT_CROSS_FAMILY: RouteStatus = RouteStatus::Cut {
    reason: "no measured Writer/Calc/Impress cross-family filter for this pair (ARCHITECTURE 8.3)",
};

// -- C3b cuts: every one of these was measured (evidence/c3-document.md §10) ---

/// Writer document -> spreadsheet: not unsupported, *aborting*.
const CUT_DOC_CROSS_ABORT: RouteStatus = RouteStatus::Cut {
    reason: "measured: a Writer document exported with the Calc xlsx filter is not a conversion on \
             either leg — Linux aborts (rc=134, `Unspecified Application Error`), Windows answers \
             rc=0 and leaves only a zero-byte `*.tmp`. No workbook is written on either leg \
             (evidence/c3-document.md §10.2)",
};
/// Calc/Impress -> markdown: only Writer registers the markdown export filter.
const CUT_DOC_NO_MARKDOWN: RouteStatus = RouteStatus::Cut {
    reason: "measured: only Writer registers the Markdown export filter. A Calc or Impress \
             document answers rc=1 `Class:Write Code:16` for `md:Markdown`, so a spreadsheet or \
             a slide deck cannot become markdown through the sidecar (§10)",
};
/// Impress -> plain text: there is no text export filter in the Impress family.
const CUT_DOC_IMPRESS_NO_TEXT: RouteStatus = RouteStatus::Cut {
    reason: "measured: Impress registers no plain-text export filter; both `txt:Text \
             (encoded):UTF8` and the Calc text filter answer rc=1 `Class:Write Code:16` on a \
             .ppt/.pptx (§10)",
};
/// html -> txt converts, but md from the same source keeps the structure.
const CUT_DOC_HTML_TXT: RouteStatus = RouteStatus::Cut {
    reason: "measured PASS (27 B) but not offered: html -> txt is the same text with the heading \
             stripped, while html -> md from the identical source keeps it (`# Fixture`). v1 \
             offers one text target per source and it is md (§10)",
};
/// A `.txt` source has no offer in v1 - and that is the N3 refusal, not an oversight.
const CUT_DOC_TXT_SOURCE: RouteStatus = RouteStatus::Cut {
    reason: "measured PASS (pdf 14,522 B / docx 5,117 B / rtf 3,423 B) but not offered in v1: \
             offering it turns the N3 trap (plain text renamed .docx is detected as Txt) from a \
             refusal into a conversion, and that refusal is a shipped C3 acceptance. Flipping it \
             is one route entry plus a stakeholder decision (§10)",
};
/// txt -> md wraps the whole file in one fenced code block.
const CUT_DOC_TXT_TO_MD: RouteStatus = RouteStatus::Cut {
    reason: "measured PASS but not offered: LibreOffice's markdown export of a .txt wraps the \
             entire file in a fenced code block (174 B from a 164 B source), which is not what \
             txt -> md means; cut on the measurement (§10)",
};
/// PDF input: the text extractor is a different product.
const CUT_PDF_TEXT: RouteStatus = RouteStatus::Cut {
    reason: "measured: a PDF imports as a Draw document, so the writer text filter answers rc=1 \
             `Class:Write Code:16`; extracting text from a PDF is a different product \
             (ARCHITECTURE 8.3, §10)",
};

macro_rules! route {
    ($target:ident, $engine:ident, $status:expr) => {
        Route {
            target: TargetFormat::$target,
            engine: Engine::$engine,
            status: $status,
        }
    };
}

// ---------------------------------------------------------------------------
// Image routes (ARCHITECTURE §8.2; SPEC §9.2 is the in/out contract)
// ---------------------------------------------------------------------------

static R_JPEG: &[Route] = &[
    route!(Jpg, Vips, RouteStatus::Proven { evidence: EV_V6 }),
    route!(Png, Vips, RouteStatus::Proven { evidence: EV_V2 }),
    route!(Webp, Vips, RouteStatus::Proven { evidence: EV_C2_JPG_WEBP }),
    route!(Tiff, Vips, RouteStatus::Proven { evidence: EV_C2_JPG_TIFF }),
    route!(Avif, Vips, RouteStatus::Proven { evidence: EV_C2W_AVIF }),
    route!(Gif, Ffmpeg, RouteStatus::Proven { evidence: EV_C2_GIF_STILL }),
    route!(Bmp, RustCrate, PLANNED_IMG_BMP),
    route!(Heic, Vips, CUT_HEIC),
    route!(Svg, Vips, CUT_SVG),
    route!(Pdf, Vips, CUT_IMG_PDF),
];

static R_PNG: &[Route] = &[
    route!(Jpg, Vips, RouteStatus::Proven { evidence: EV_C2_PNG_JPG }),
    route!(Webp, Vips, RouteStatus::Proven { evidence: EV_V3 }),
    route!(Tiff, Vips, RouteStatus::Proven { evidence: EV_C2_PNG_TIFF }),
    route!(Avif, Vips, RouteStatus::Proven { evidence: EV_C2W_AVIF }),
    route!(Gif, Ffmpeg, RouteStatus::Proven { evidence: EV_C2_GIF_STILL }),
    route!(Bmp, RustCrate, PLANNED_IMG_BMP),
    route!(Heic, Vips, CUT_HEIC),
    route!(Svg, Vips, CUT_SVG),
    route!(Pdf, Vips, CUT_IMG_PDF),
];

static R_WEBP: &[Route] = &[
    route!(Jpg, Vips, RouteStatus::Proven { evidence: EV_C2_WEBP_JPG }),
    route!(Png, Vips, RouteStatus::Proven { evidence: EV_C2_WEBP_PNG }),
    route!(Tiff, Vips, RouteStatus::Proven { evidence: EV_V4 }),
    route!(Avif, Vips, RouteStatus::Proven { evidence: EV_C2W_AVIF }),
    route!(Gif, Ffmpeg, RouteStatus::Proven { evidence: EV_C2_GIF_STILL }),
    route!(Bmp, RustCrate, PLANNED_IMG_BMP),
    route!(Heic, Vips, CUT_HEIC),
    route!(Svg, Vips, CUT_SVG),
    route!(Pdf, Vips, CUT_IMG_PDF),
];

static R_TIFF: &[Route] = &[
    route!(Jpg, Vips, RouteStatus::Proven { evidence: EV_C2_TIFF_JPG }),
    route!(Png, Vips, RouteStatus::Proven { evidence: EV_V5 }),
    route!(Webp, Vips, RouteStatus::Proven { evidence: EV_C2_TIFF_WEBP }),
    route!(Avif, Vips, RouteStatus::Proven { evidence: EV_C2W_AVIF }),
    route!(Gif, Ffmpeg, RouteStatus::Proven { evidence: EV_C2_GIF_STILL }),
    route!(Bmp, RustCrate, PLANNED_IMG_BMP),
    route!(Heic, Vips, CUT_HEIC),
    route!(Svg, Vips, CUT_SVG),
    route!(Pdf, Vips, CUT_IMG_PDF),
];

static R_HEIC: &[Route] = &[
    route!(Jpg, Vips, RouteStatus::Proven { evidence: EV_V7 }),
    route!(Png, Vips, RouteStatus::Proven { evidence: EV_C2W_HEIC_DECODE }),
    route!(Webp, Vips, RouteStatus::Proven { evidence: EV_C2W_HEIC_DECODE }),
    route!(Tiff, Vips, RouteStatus::Proven { evidence: EV_C2W_HEIC_DECODE }),
    route!(Avif, Vips, RouteStatus::Proven { evidence: EV_C2W_AVIF }),
    route!(Gif, Ffmpeg, RouteStatus::Proven { evidence: EV_C2W_HEIF_GIF }),
    route!(Bmp, RustCrate, PLANNED_IMG_BMP),
    route!(Heic, Vips, CUT_HEIC),
    route!(Svg, Vips, CUT_SVG),
    route!(Pdf, Vips, CUT_IMG_PDF),
];

static R_AVIF: &[Route] = &[
    route!(Jpg, Vips, RouteStatus::Proven { evidence: EV_V8B }),
    route!(Png, Vips, RouteStatus::Proven { evidence: EV_C2W_AVIF_DECODE }),
    route!(Webp, Vips, RouteStatus::Proven { evidence: EV_C2W_AVIF_DECODE }),
    route!(Tiff, Vips, RouteStatus::Proven { evidence: EV_C2W_AVIF_DECODE }),
    route!(Gif, Ffmpeg, RouteStatus::Proven { evidence: EV_C2W_HEIF_GIF }),
    route!(Bmp, RustCrate, PLANNED_IMG_BMP),
    route!(Heic, Vips, CUT_HEIC),
    route!(Svg, Vips, CUT_SVG),
    route!(Pdf, Vips, CUT_IMG_PDF),
];

static R_GIF: &[Route] = &[
    // v1 (Option C): gif input is offered for image targets and converts frame 1 only.
    // The UI must label it; see ARCHITECTURE.md §8.2.1.
    route!(Jpg, Vips, RouteStatus::Proven { evidence: EV_C2W_GIF_IN }),
    route!(Png, Vips, RouteStatus::Proven { evidence: EV_C2W_GIF_IN }),
    route!(Webp, Vips, RouteStatus::Proven { evidence: EV_C2W_GIF_IN }),
    route!(Tiff, Vips, RouteStatus::Proven { evidence: EV_C2W_GIF_IN }),
    route!(Avif, Vips, RouteStatus::Proven { evidence: EV_C2W_AVIF }),
    route!(Gif, Ffmpeg, PLANNED_GIF_IN),
    route!(Bmp, RustCrate, PLANNED_IMG_BMP),
    route!(Heic, Vips, CUT_HEIC),
    route!(Svg, Vips, CUT_SVG),
    route!(Pdf, Vips, CUT_IMG_PDF),
];

static R_BMP: &[Route] = &[
    route!(Jpg, Vips, PLANNED_IMG),
    route!(Png, Vips, PLANNED_IMG),
    route!(Webp, Vips, PLANNED_IMG),
    route!(Tiff, Vips, PLANNED_IMG),
    route!(Avif, Vips, PLANNED_IMG),
    route!(Gif, Ffmpeg, PLANNED_IMG_GIF),
    route!(Bmp, RustCrate, PLANNED_IMG_BMP),
    route!(Heic, Vips, CUT_HEIC),
    route!(Svg, Vips, CUT_SVG),
    route!(Pdf, Vips, CUT_IMG_PDF),
];

static R_SVG: &[Route] = &[
    route!(Jpg, Resvg, PLANNED_SVG_IN),
    route!(Png, Resvg, PLANNED_SVG_IN),
    route!(Webp, Resvg, PLANNED_SVG_IN),
    route!(Tiff, Resvg, PLANNED_SVG_IN),
    route!(Avif, Resvg, PLANNED_SVG_IN),
    route!(Gif, Ffmpeg, PLANNED_IMG_GIF),
    route!(Bmp, RustCrate, PLANNED_IMG_BMP),
    route!(Heic, Resvg, CUT_HEIC),
    route!(Svg, Resvg, CUT_SVG),
    route!(Pdf, Resvg, CUT_SVG),
];

static R_PDF: &[Route] = &[
    // pdf as a *raster* input (SPEC §9.2: "pdf — yes, raster in")
    route!(Jpg, Pdfium, PLANNED_PDF_IN),
    route!(Png, Pdfium, PLANNED_PDF_IN),
    route!(Webp, Pdfium, PLANNED_PDF_IN),
    route!(Tiff, Pdfium, PLANNED_PDF_IN),
    route!(Avif, Pdfium, PLANNED_PDF_IN),
    route!(Gif, Pdfium, PLANNED_PDF_IN),
    route!(Bmp, Pdfium, PLANNED_PDF_IN),
    route!(Heic, Pdfium, CUT_HEIC),
    route!(Svg, Pdfium, CUT_SVG),
    route!(Pdf, Pdfium, CUT_IMG_PDF),
    // pdf as a *document* input (ARCHITECTURE §8.3): no raster-to-writer path,
    // and text extraction is a different product.
    route!(Md, Soffice, CUT_PDF_WRITER),
    route!(Html, Soffice, CUT_PDF_WRITER),
    route!(Docx, Soffice, CUT_PDF_WRITER),
    route!(Pptx, Soffice, CUT_PDF_WRITER),
    route!(Rtf, Soffice, CUT_PDF_WRITER),
    route!(Xlsx, Soffice, CUT_PDF_WRITER),
    route!(Csv, Soffice, CUT_PDF_WRITER),
    route!(Txt, Soffice, CUT_PDF_TEXT),
];

// ---------------------------------------------------------------------------
// Document routes (ARCHITECTURE §8.3)
//
// Row order *is* wheel order (`offered_targets` preserves it): each row lists its
// `Proven` targets in the family's intent rank — see `wheel_rank` — and the cuts
// follow. `document_wheel_order_follows_the_intent_rank` asserts both the order
// and the 6-segment cap (ARCHITECTURE §10.2).
// ---------------------------------------------------------------------------

static R_DOCX: &[Route] = &[
    route!(Pdf, Soffice, RouteStatus::Proven { evidence: EV_D1 }),
    route!(Md, Soffice, RouteStatus::Proven { evidence: EV_D2 }),
    route!(Txt, Soffice, RouteStatus::Proven { evidence: EV_C3B_DOCX_TXT }),
    route!(Html, Soffice, RouteStatus::Proven { evidence: EV_D3 }),
    route!(Rtf, Soffice, RouteStatus::Proven { evidence: EV_C3B_DOCX_RTF }),
    route!(Xlsx, Soffice, CUT_DOC_CROSS_ABORT),
    route!(Pptx, Soffice, CUT_CROSS_FAMILY),
    route!(Csv, Soffice, CUT_DOC),
];

static R_PPTX: &[Route] = &[
    route!(Pdf, Soffice, RouteStatus::Proven { evidence: EV_D7 }),
    route!(Html, Soffice, RouteStatus::Proven { evidence: EV_C3B_PPTX_HTML }),
    route!(Md, Soffice, CUT_DOC_NO_MARKDOWN),
    route!(Txt, Soffice, CUT_DOC_IMPRESS_NO_TEXT),
    route!(Docx, Soffice, CUT_CROSS_FAMILY),
    route!(Xlsx, Soffice, CUT_DOC),
    route!(Rtf, Soffice, CUT_CROSS_FAMILY),
    route!(Csv, Soffice, CUT_DOC),
];

static R_XLSX: &[Route] = &[
    route!(Pdf, Soffice, RouteStatus::Proven { evidence: EV_C3B_XLSX_PDF }),
    route!(Csv, Soffice, RouteStatus::Proven { evidence: EV_D5 }),
    route!(Txt, Soffice, RouteStatus::Proven { evidence: EV_C3B_XLSX_TXT }),
    route!(Html, Soffice, RouteStatus::Proven { evidence: EV_C3B_XLSX_HTML }),
    route!(Md, Soffice, CUT_DOC_NO_MARKDOWN),
    route!(Docx, Soffice, CUT_DOC),
    route!(Pptx, Soffice, CUT_CROSS_FAMILY),
    route!(Rtf, Soffice, CUT_CROSS_FAMILY),
];

static R_CSV: &[Route] = &[
    route!(Pdf, Soffice, RouteStatus::Proven { evidence: EV_C3B_CSV_PDF }),
    route!(Xlsx, Soffice, RouteStatus::Proven { evidence: EV_D4 }),
    route!(Txt, Soffice, RouteStatus::Proven { evidence: EV_C3B_CSV_TXT }),
    route!(Md, Soffice, CUT_DOC_NO_MARKDOWN),
    route!(Html, Soffice, CUT_DOC),
    route!(Docx, Soffice, CUT_DOC),
    route!(Pptx, Soffice, CUT_CROSS_FAMILY),
    route!(Rtf, Soffice, CUT_CROSS_FAMILY),
];

static R_HTML: &[Route] = &[
    route!(Pdf, Soffice, RouteStatus::Proven { evidence: EV_C3B_HTML_PDF }),
    route!(Md, Soffice, RouteStatus::Proven { evidence: EV_C3B_HTML_MD }),
    route!(Docx, Soffice, RouteStatus::Proven { evidence: EV_D6 }),
    route!(Rtf, Soffice, RouteStatus::Proven { evidence: EV_C3B_HTML_RTF }),
    route!(Txt, Soffice, CUT_DOC_HTML_TXT),
    route!(Pptx, Soffice, CUT_CROSS_FAMILY),
    route!(Xlsx, Soffice, CUT_DOC),
    route!(Csv, Soffice, CUT_DOC),
];

// Legacy Office split into three rows by C3: .doc is Writer, .xls is Calc and
// .ppt is Impress, so the measured filters (and the supported directions)
// differ. One shared row would have advertised `xls -> csv` to a .doc.
static R_DOC: &[Route] = &[
    route!(Pdf, Soffice, RouteStatus::Proven { evidence: EV_C3_DOC_PDF }),
    route!(Md, Soffice, RouteStatus::Proven { evidence: EV_C3_DOC_MD }),
    route!(Txt, Soffice, RouteStatus::Proven { evidence: EV_C3_DOC_TXT }),
    route!(Docx, Soffice, RouteStatus::Proven { evidence: EV_C3_DOC_DOCX }),
    route!(Html, Soffice, RouteStatus::Proven { evidence: EV_C3_DOC_HTML }),
    route!(Rtf, Soffice, RouteStatus::Proven { evidence: EV_C3B_DOC_RTF }),
    route!(Pptx, Soffice, CUT_CROSS_FAMILY),
    route!(Xlsx, Soffice, CUT_DOC),
    route!(Csv, Soffice, CUT_DOC),
];

static R_XLS: &[Route] = &[
    route!(Pdf, Soffice, RouteStatus::Proven { evidence: EV_C3_XLS_PDF }),
    route!(Csv, Soffice, RouteStatus::Proven { evidence: EV_C3_XLS_CSV }),
    route!(Xlsx, Soffice, RouteStatus::Proven { evidence: EV_C3_XLS_XLSX }),
    route!(Txt, Soffice, RouteStatus::Proven { evidence: EV_C3B_XLS_TXT }),
    route!(Html, Soffice, RouteStatus::Proven { evidence: EV_C3_XLS_HTML }),
    route!(Md, Soffice, CUT_DOC_NO_MARKDOWN),
    route!(Docx, Soffice, CUT_CROSS_FAMILY),
    route!(Pptx, Soffice, CUT_CROSS_FAMILY),
    route!(Rtf, Soffice, CUT_CROSS_FAMILY),
];

static R_PPT: &[Route] = &[
    route!(Pdf, Soffice, RouteStatus::Proven { evidence: EV_C3_PPT_PDF }),
    route!(Pptx, Soffice, RouteStatus::Proven { evidence: EV_C3_PPT_PPTX }),
    route!(Html, Soffice, RouteStatus::Proven { evidence: EV_C3B_PPT_HTML }),
    route!(Md, Soffice, CUT_DOC_NO_MARKDOWN),
    route!(Txt, Soffice, CUT_DOC_IMPRESS_NO_TEXT),
    route!(Docx, Soffice, CUT_CROSS_FAMILY),
    route!(Rtf, Soffice, CUT_CROSS_FAMILY),
    route!(Xlsx, Soffice, CUT_DOC),
    route!(Csv, Soffice, CUT_DOC),
];

static R_RTF: &[Route] = &[
    route!(Pdf, Soffice, RouteStatus::Proven { evidence: EV_C3_RTF_PDF }),
    route!(Md, Soffice, RouteStatus::Proven { evidence: EV_C3B_RTF_MD }),
    route!(Txt, Soffice, RouteStatus::Proven { evidence: EV_C3_RTF_TXT }),
    route!(Docx, Soffice, RouteStatus::Proven { evidence: EV_C3_RTF_DOCX }),
    route!(Html, Soffice, RouteStatus::Proven { evidence: EV_C3_RTF_HTML }),
    route!(Pptx, Soffice, CUT_CROSS_FAMILY),
    route!(Xlsx, Soffice, CUT_DOC),
    route!(Csv, Soffice, CUT_DOC),
];

static R_MD: &[Route] = &[
    route!(Pdf, Soffice, RouteStatus::Proven { evidence: EV_C3B_MD_PDF }),
    route!(Txt, Soffice, RouteStatus::Proven { evidence: EV_C3B_MD_TXT }),
    route!(Docx, Soffice, RouteStatus::Proven { evidence: EV_C3B_MD_DOCX }),
    // Measured and flipped: the sidecar writes markdown directly, so the
    // in-process comrak/htmd path this row used to plan is retired.
    route!(Html, Soffice, RouteStatus::Proven { evidence: EV_C3B_MD_HTML }),
    route!(Rtf, Soffice, RouteStatus::Proven { evidence: EV_C3B_MD_RTF }),
    route!(Pptx, Soffice, CUT_CROSS_FAMILY),
    route!(Xlsx, Soffice, CUT_DOC),
    route!(Csv, Soffice, CUT_DOC),
];

// A .txt source offers nothing in v1 by decision, not by omission: every target
// below measured PASS (evidence/c3-document.md §10) and is held back so the N3
// refusal keeps working. See `CUT_DOC_TXT_SOURCE`.
static R_TXT: &[Route] = &[
    route!(Pdf, Soffice, CUT_DOC_TXT_SOURCE),
    route!(Docx, Soffice, CUT_DOC_TXT_SOURCE),
    route!(Rtf, Soffice, CUT_DOC_TXT_SOURCE),
    route!(Md, Soffice, CUT_DOC_TXT_TO_MD),
    route!(Html, Soffice, CUT_DOC),
    route!(Pptx, Soffice, CUT_CROSS_FAMILY),
    route!(Xlsx, Soffice, CUT_DOC),
    route!(Csv, Soffice, CUT_DOC),
];

// ---------------------------------------------------------------------------
// Audio routes (SPEC §3: mp3, wav, flac, m4a/aac, ogg, opus + bitrate)
//
// The cross product is the v1 scope and every cell is now Proven by measurement
// (`spikes/c2/EVIDENCE.md` §3): 7 sources x 7 targets, each opened again with
// ffprobe and checked for codec, sample rate and duration. Identity is a real
// route: it is the re-encode that applies a bitrate change.
// ---------------------------------------------------------------------------

macro_rules! audio_row {
    () => {
        &[
            route!(Mp3, Ffmpeg, RouteStatus::Proven { evidence: EV_C2_AUDIO_MP3 }),
            route!(Wav, Ffmpeg, RouteStatus::Proven { evidence: EV_C2_AUDIO_WAV }),
            route!(Flac, Ffmpeg, RouteStatus::Proven { evidence: EV_C2_AUDIO_FLAC }),
            route!(M4a, Ffmpeg, RouteStatus::Proven { evidence: EV_C2_AUDIO_M4A }),
            route!(Aac, Ffmpeg, RouteStatus::Proven { evidence: EV_C2_AUDIO_AAC }),
            route!(Ogg, Ffmpeg, RouteStatus::Proven { evidence: EV_C2_AUDIO_OGG }),
            route!(Opus, Ffmpeg, RouteStatus::Proven { evidence: EV_C2_AUDIO_OPUS }),
        ]
    };
}

/// One table for every audio input: the v1 scope is the full cross product
/// (SPEC §3) and every cell is measured, so the row is identical.
static R_AUDIO: &[Route] = audio_row!();

// ---------------------------------------------------------------------------
// Video routes (SPEC §3: mp4, mkv, mov, webm, gif + trim & resize)
// ---------------------------------------------------------------------------

macro_rules! video_row {
    () => {
        &[
            route!(Mp4, Ffmpeg, RouteStatus::Proven { evidence: EV_C2_VIDEO_MP4 }),
            route!(Mkv, Ffmpeg, RouteStatus::Proven { evidence: EV_C2_VIDEO_MKV }),
            route!(Mov, Ffmpeg, RouteStatus::Proven { evidence: EV_C2_VIDEO_MOV }),
            route!(Webm, Ffmpeg, RouteStatus::Proven { evidence: EV_C2_VIDEO_WEBM }),
            route!(Gif, Ffmpeg, RouteStatus::Proven { evidence: EV_C2_VIDEO_GIF }),
        ]
    };
}

static R_VIDEO: &[Route] = video_row!();

// ---------------------------------------------------------------------------
// Lookup
// ---------------------------------------------------------------------------

/// Every route for a detected type, in matrix column order.
///
/// `Unknown` deliberately has no routes: no bytes, no offers (`ARCHITECTURE.md`
/// §7 rule 6).
pub fn routes_for(ty: CanonicalType) -> &'static [Route] {
    match ty {
        CanonicalType::Jpeg => R_JPEG,
        CanonicalType::Png => R_PNG,
        CanonicalType::WebP => R_WEBP,
        CanonicalType::Tiff => R_TIFF,
        CanonicalType::Heic => R_HEIC,
        CanonicalType::Avif => R_AVIF,
        CanonicalType::Gif => R_GIF,
        CanonicalType::Bmp => R_BMP,
        CanonicalType::Svg => R_SVG,
        CanonicalType::Pdf => R_PDF,
        CanonicalType::Docx => R_DOCX,
        CanonicalType::Pptx => R_PPTX,
        CanonicalType::Xlsx => R_XLSX,
        CanonicalType::Csv => R_CSV,
        CanonicalType::Html => R_HTML,
        CanonicalType::Doc => R_DOC,
        CanonicalType::Xls => R_XLS,
        CanonicalType::Ppt => R_PPT,
        CanonicalType::Rtf => R_RTF,
        CanonicalType::Markdown => R_MD,
        CanonicalType::Txt => R_TXT,
        CanonicalType::Mp3
        | CanonicalType::Wav
        | CanonicalType::Flac
        | CanonicalType::M4a
        | CanonicalType::Aac
        | CanonicalType::Ogg
        | CanonicalType::Opus => R_AUDIO,
        CanonicalType::Mp4 | CanonicalType::Mkv | CanonicalType::Mov | CanonicalType::WebM => {
            R_VIDEO
        }
        CanonicalType::Unknown => &[],
    }
}

/// The wheel's list: only `Proven` targets, in matrix order. Empty for
/// unknown bytes and for types whose routes are all planned/cut.
pub fn offered_targets(ty: CanonicalType) -> Vec<TargetFormat> {
    routes_for(ty)
        .iter()
        .filter(|r| r.status.is_proven())
        .map(|r| r.target)
        .collect()
}

/// True when the wheel would show nothing for this type at all.
pub fn has_no_offer(ty: CanonicalType) -> bool {
    offered_targets(ty).is_empty()
}

// ---------------------------------------------------------------------------
// Wheel ordering for documents (C3b)
//
// `offered_targets` returns matrix order, so the row order *is* the wheel order.
// Document sources are ranked by intent inside their family, because "the same
// list for every source" is wrong: a spreadsheet's second segment is `csv`, not
// `md`. The rank is data, not prose, so a row that drifts fails a test.
// ---------------------------------------------------------------------------

/// The LibreOffice family a document source is converted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocWheelFamily {
    /// docx, doc, rtf, md, html: imported as a text document.
    Writer,
    /// xlsx, xls, csv: imported as a spreadsheet.
    Calc,
    /// pptx, ppt: imported as a presentation.
    Impress,
}

/// The wheel family for a document source, or `None` for non-documents.
pub fn document_wheel_family(ty: CanonicalType) -> Option<DocWheelFamily> {
    Some(match ty {
        CanonicalType::Docx
        | CanonicalType::Doc
        | CanonicalType::Rtf
        | CanonicalType::Markdown
        | CanonicalType::Html => DocWheelFamily::Writer,
        CanonicalType::Xlsx | CanonicalType::Xls | CanonicalType::Csv => DocWheelFamily::Calc,
        CanonicalType::Pptx | CanonicalType::Ppt => DocWheelFamily::Impress,
        _ => return None,
    })
}

/// The wheel segment order for a document direction: 0 is the first segment.
///
/// Writer sources: hand-off, editable notes, plain text, editable Word, web,
/// legacy interchange. Calc sources: hand-off, data, modern workbook, plain
/// text, web. Impress sources: hand-off, modern deck, web.
pub fn wheel_rank(from: CanonicalType, to: TargetFormat) -> Option<usize> {
    let order: &[TargetFormat] = match document_wheel_family(from)? {
        DocWheelFamily::Writer => &[
            TargetFormat::Pdf,
            TargetFormat::Md,
            TargetFormat::Txt,
            TargetFormat::Docx,
            TargetFormat::Html,
            TargetFormat::Rtf,
        ],
        DocWheelFamily::Calc => &[
            TargetFormat::Pdf,
            TargetFormat::Csv,
            TargetFormat::Xlsx,
            TargetFormat::Txt,
            TargetFormat::Html,
        ],
        DocWheelFamily::Impress => &[TargetFormat::Pdf, TargetFormat::Pptx, TargetFormat::Html],
    };
    order.iter().position(|t| *t == to)
}

/// The SPEC §3 scope for a type: every target the matrix knows about,
/// regardless of status. This is what the documented table must match.
pub fn spec_scope_targets(ty: CanonicalType) -> Vec<TargetFormat> {
    routes_for(ty).iter().map(|r| r.target).collect()
}

// ---------------------------------------------------------------------------
// Multi-file drops (ARCHITECTURE §8.5)
// ---------------------------------------------------------------------------

/// The wheel's answer for a multi-file drop: the intersection of the files'
/// proven targets, or — when that is empty — the union with a warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiFileTargets {
    pub targets: Vec<TargetFormat>,
    /// True when `targets` is the union fallback rather than the intersection.
    pub used_union_fallback: bool,
}

/// `ARCHITECTURE.md` §8.5: offer the intersection; if it is empty, fall back to
/// the union so the user is not left with an empty wheel, and let the per-file
/// results say which files cannot do it.
pub fn multi_file_targets(types: &[CanonicalType]) -> MultiFileTargets {
    if types.is_empty() {
        return MultiFileTargets {
            targets: Vec::new(),
            used_union_fallback: false,
        };
    }
    let mut intersection = offered_targets(types[0]);
    for ty in &types[1..] {
        let next = offered_targets(*ty);
        intersection.retain(|t| next.contains(t));
    }
    if !intersection.is_empty() {
        return MultiFileTargets {
            targets: intersection,
            used_union_fallback: false,
        };
    }
    let mut union: Vec<TargetFormat> = Vec::new();
    for ty in types {
        for t in offered_targets(*ty) {
            if !union.contains(&t) {
                union.push(t);
            }
        }
    }
    MultiFileTargets {
        targets: union,
        used_union_fallback: true,
    }
}

// ---------------------------------------------------------------------------
// Generated documentation
// ---------------------------------------------------------------------------

/// `docs/DETECTION.md` block: one row per route.
pub fn render_matrix_table() -> String {
    let mut out =
        String::from("| input | target | engine | status | evidence / owner / reason |\n");
    out.push_str("|---|---|---|---|---|\n");
    for ty in ALL_CANONICAL_TYPES {
        for route in routes_for(*ty) {
            let (status, detail) = match route.status {
                RouteStatus::Proven { evidence } => (
                    "PROVEN".to_string(),
                    format!("{} — {}", evidence.label, evidence.claim),
                ),
                RouteStatus::Planned { owner, hypothesis } => {
                    ("planned".to_string(), format!("{owner} — {hypothesis}"))
                }
                RouteStatus::Cut { reason } => ("cut".to_string(), reason.to_string()),
            };
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                ty.as_str(),
                route.target.as_str(),
                route.engine.as_str(),
                status,
                detail
            ));
        }
    }
    out
}

/// `docs/DETECTION.md` block: what the wheel shows per type today.
pub fn render_offered_table() -> String {
    let mut out = String::from("| input | family | wheel-visible targets (Proven only) |\n");
    out.push_str("|---|---|---|\n");
    for ty in ALL_CANONICAL_TYPES {
        let offered: Vec<&str> = offered_targets(*ty)
            .into_iter()
            .map(|t| t.as_str())
            .collect();
        let cell = if offered.is_empty() {
            "—".to_string()
        } else {
            offered.join(", ")
        };
        out.push_str(&format!(
            "| {} | {:?} | {} |\n",
            ty.as_str(),
            ty.family(),
            cell
        ));
    }
    out
}

/// `evidence/ROUTES.md` block: every Proven route, its claim and its source.
pub fn render_routes_table() -> String {
    // Collect evidence in first-seen order, then the routes it backs.
    let mut labels: Vec<EvidenceId> = Vec::new();
    let mut backed: Vec<Vec<String>> = Vec::new();
    for ty in ALL_CANONICAL_TYPES {
        for route in routes_for(*ty) {
            if let RouteStatus::Proven { evidence } = route.status {
                let idx = match labels.iter().position(|e| e.label == evidence.label) {
                    Some(i) => i,
                    None => {
                        labels.push(evidence);
                        backed.push(Vec::new());
                        labels.len() - 1
                    }
                };
                let cell = format!("{} → {}", ty.as_str(), route.target.as_str());
                if !backed[idx].contains(&cell) {
                    backed[idx].push(cell);
                }
            }
        }
    }
    let mut out = String::from("| evidence | provenance | source | claim | routes it proves |\n");
    out.push_str("|---|---|---|---|---|\n");
    for (i, ev) in labels.iter().enumerate() {
        out.push_str(&format!(
            "| {} | {} | `{}` | {} | {} |\n",
            ev.label,
            ev.provenance.as_str(),
            ev.source,
            ev.claim,
            backed[i].join(", ")
        ));
    }
    out
}

/// `docs/DETECTION.md` block: SPEC §3 scope versus what is offered today.
pub fn render_spec_scope_table() -> String {
    let mut out =
        String::from("| input | SPEC §3 scope (all modelled targets) | offered today |\n");
    out.push_str("|---|---|---|\n");
    for ty in ALL_CANONICAL_TYPES {
        let scope: Vec<&str> = spec_scope_targets(*ty)
            .into_iter()
            .map(|t| t.as_str())
            .collect();
        let offered: Vec<&str> = offered_targets(*ty)
            .into_iter()
            .map(|t| t.as_str())
            .collect();
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            ty.as_str(),
            if scope.is_empty() {
                "—".to_string()
            } else {
                scope.join(", ")
            },
            if offered.is_empty() {
                "— (nothing proven yet)".to_string()
            } else {
                offered.join(", ")
            }
        ));
    }
    out
}

/// Every target id that appears anywhere in the matrix (for coverage tests).
pub fn all_modelled_targets() -> Vec<TargetFormat> {
    let mut out: Vec<TargetFormat> = Vec::new();
    for ty in ALL_CANONICAL_TYPES {
        for route in routes_for(*ty) {
            if !out.contains(&route.target) {
                out.push(route.target);
            }
        }
    }
    out
}

/// Total number of routes in the matrix (all statuses).
pub fn route_count() -> usize {
    ALL_CANONICAL_TYPES
        .iter()
        .map(|t| routes_for(*t).len())
        .sum()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::test_support::{read_repo_file, repo_root};
    use super::super::types::{type_for_extension, Family};
    use super::*;

    fn all_routes() -> Vec<(CanonicalType, Route)> {
        let mut out = Vec::new();
        for ty in ALL_CANONICAL_TYPES {
            for route in routes_for(*ty) {
                out.push((*ty, *route));
            }
        }
        out
    }

    // -- §0: evidence actually exists and still says what we claim ---------

    #[test]
    fn every_proven_route_points_at_evidence_that_still_says_so() {
        let root = repo_root();
        let mut checked = 0usize;
        for (ty, route) in all_routes() {
            let RouteStatus::Proven { evidence } = route.status else {
                continue;
            };
            let path = root.join(evidence.source);
            assert!(
                path.is_file(),
                "{ty}→{}: evidence file missing: {}",
                route.target,
                path.display()
            );
            let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!("{ty}→{}: cannot read {}: {e}", route.target, path.display())
            });
            assert!(
                text.contains(evidence.marker),
                "{ty}→{}: marker {:?} not found in {}",
                route.target,
                evidence.marker,
                path.display()
            );
            assert!(
                !evidence.claim.is_empty() && !evidence.label.is_empty(),
                "{ty}→{}: evidence must name a label and a claim",
                route.target
            );
            checked += 1;
        }
        assert!(
            checked >= 16,
            "expected at least 16 proven routes, saw {checked}"
        );
    }

    #[test]
    fn proven_counts_match_the_architecture_tables() {
        let mut image = 0;
        let mut audio = 0;
        let mut video = 0;
        let mut document = 0;
        for (ty, route) in all_routes() {
            if !route.status.is_proven() {
                continue;
            }
            match ty.family() {
                Family::Image => image += 1,
                Family::Audio => audio += 1,
                Family::Video => video += 1,
                Family::Document => document += 1,
                other => panic!(
                    "unexpected proven route in family {other:?}: {ty}→{}",
                    route.target
                ),
            }
        }
        // ARCHITECTURE §8.2: C2 added 12 image cells (incl. 4 gif outputs) to C1's 9 = 21;
        // C2-Win added 16 more (the HEIF family, which the Linux leg could not load:
        // 6 avif outputs that were SKIP, 3 heic decodes + heic->gif, 3 avif decodes +
        // avif->gif, and 4 gif inputs) = 37.
        // §8.4 audio: 7 sources × 7 targets = 49, every one measured.
        // §8.4 video: 4 sources × 5 targets (mp4/mkv/mov/webm/gif) = 20.
        // §8.3 documents: 7 from S3 + 15 legacy/rtf cells measured by C3 = 22;
        // C3b flipped 20 more (§10, the remaining Planned cells) = 42.
        assert_eq!(
            image, 37,
            "proven image cells drifted from ARCHITECTURE §8.2"
        );
        assert_eq!(
            audio, 49,
            "proven audio cells drifted from ARCHITECTURE §8.4"
        );
        assert_eq!(
            video, 20,
            "proven video cells drifted from ARCHITECTURE §8.4"
        );
        assert_eq!(
            document, 42,
            "proven document cells drifted from ARCHITECTURE §8.3"
        );
    }

    #[test]
    fn evidence_index_documents_every_proven_route() {
        let doc = read_repo_file("evidence/ROUTES.md");
        let expected = render_routes_table();
        let block = between_markers(&doc, "routes:begin", "routes:end");
        assert_eq!(
            block.trim(),
            expected.trim(),
            "evidence/ROUTES.md is stale — regenerate it from matrix::render_routes_table()"
        );
    }

    // -- §0: planned routes name live-shaped cards -------------------------

    #[test]
    fn every_planned_route_names_a_well_formed_card_and_a_hypothesis() {
        let doc = read_repo_file("docs/DETECTION.md");
        let mut seen: Vec<CardId> = Vec::new();
        for (ty, route) in all_routes() {
            if let RouteStatus::Planned { owner, hypothesis } = route.status {
                assert!(
                    owner.is_well_formed(),
                    "{ty}→{}: owner {:?} is not a board card id",
                    route.target,
                    owner.0
                );
                assert!(
                    !hypothesis.is_empty(),
                    "{ty}→{}: planned route needs a hypothesis",
                    route.target
                );
                if !seen.contains(&owner) {
                    seen.push(owner);
                }
            }
        }
        assert!(!seen.is_empty(), "the matrix has no planned routes at all");
        for owner in seen {
            assert!(
                doc.contains(owner.0),
                "docs/DETECTION.md does not name owner card {owner}; either document it \
                 or fix the matrix"
            );
        }
    }

    #[test]
    fn every_cut_route_states_a_reason() {
        for (ty, route) in all_routes() {
            if let RouteStatus::Cut { reason } = route.status {
                assert!(
                    reason.len() > 20,
                    "{ty}→{}: cut reason is not explanatory: {:?}",
                    route.target,
                    reason
                );
            }
        }
    }

    // -- the wheel ----------------------------------------------------------

    #[test]
    fn wheel_offers_only_proven_targets() {
        for ty in ALL_CANONICAL_TYPES {
            let offered = offered_targets(*ty);
            for target in &offered {
                let route = routes_for(*ty)
                    .iter()
                    .find(|r| r.target == *target)
                    .expect("offered target has no route");
                assert!(
                    route.status.is_proven(),
                    "{ty}→{target} is offered but is not proven"
                );
            }
            // And the converse for image/doc types: planned is not offered.
            for route in routes_for(*ty) {
                if route.status.is_planned() || route.status.is_cut() {
                    assert!(
                        !offered.contains(&route.target),
                        "{ty}→{} is not proven but appears in the wheel list",
                        route.target
                    );
                }
            }
        }
    }

    /// The wheel shows a document source's targets in intent order, never more
    /// than 6 segments (ARCHITECTURE §10.2), and every offered pair has a rank.
    #[test]
    fn document_wheel_order_follows_the_intent_rank() {
        for ty in ALL_CANONICAL_TYPES {
            let Some(family) = document_wheel_family(*ty) else {
                continue;
            };
            assert_eq!(
                ty.family(),
                Family::Document,
                "{ty} has a wheel family but is not a document"
            );
            let offered = offered_targets(*ty);
            assert!(
                offered.len() <= 6,
                "{ty} offers {} segments ({offered:?}) — the wheel caps at 6",
                offered.len()
            );
            let mut last: Option<usize> = None;
            for target in &offered {
                let rank = wheel_rank(*ty, *target).unwrap_or_else(|| {
                    panic!("{ty}→{target} is offered but has no wheel rank in {family:?}")
                });
                if let Some(previous) = last {
                    assert!(
                        rank > previous,
                        "{ty}: {target} (rank {rank}) is listed after rank {previous} — \
                         row order is wheel order"
                    );
                }
                last = Some(rank);
            }
        }
        // The one row pinned explicitly: a .docx is the most common drop, so its
        // wheel is checked literally, not only by the ordering invariant.
        assert_eq!(
            offered_targets(CanonicalType::Docx),
            vec![
                TargetFormat::Pdf,
                TargetFormat::Md,
                TargetFormat::Txt,
                TargetFormat::Html,
                TargetFormat::Rtf,
            ],
            "docx wheel order changed"
        );
        // A .doc is the one source that fills all six segments.
        assert_eq!(offered_targets(CanonicalType::Doc).len(), 6);
        // A .txt offers nothing in v1 by decision (`CUT_DOC_TXT_SOURCE`).
        assert!(has_no_offer(CanonicalType::Txt));
        assert_eq!(document_wheel_family(CanonicalType::Jpeg), None);
    }

    /// The document grid is closed: every cell is `Proven` or `Cut` with a reason.
    /// C3b's whole point — a `Planned` cell here would need an owner card, and
    /// there is none because nothing in the grid is undecided.
    #[test]
    fn no_document_cell_is_left_planned() {
        let mut planned: Vec<String> = Vec::new();
        for ty in ALL_CANONICAL_TYPES {
            for route in routes_for(*ty) {
                // Only the LibreOffice document engine: `pdf → png` is a PDF
                // *image* source (PDFium, C2) that happens to sit on the pdf row.
                if route.engine != Engine::Soffice {
                    continue;
                }
                if route.status.is_planned() {
                    planned.push(format!("{ty}→{}", route.target));
                }
            }
        }
        assert!(
            planned.is_empty(),
            "document cells left Planned without a measurement: {planned:?} \
             (evidence/c3-document.md §10 decides every one of them)"
        );
    }

    #[test]
    fn unknown_bytes_offer_nothing() {
        assert!(routes_for(CanonicalType::Unknown).is_empty());
        assert!(has_no_offer(CanonicalType::Unknown));
        assert!(multi_file_targets(&[CanonicalType::Unknown])
            .targets
            .is_empty());
    }

    #[test]
    fn every_type_except_unknown_has_at_least_one_route() {
        for ty in ALL_CANONICAL_TYPES {
            assert!(
                !routes_for(*ty).is_empty(),
                "{ty} has no routes at all, not even cut ones"
            );
        }
        assert!(routes_for(CanonicalType::Unknown).is_empty());
    }

    #[test]
    fn no_type_lists_the_same_target_twice() {
        for ty in ALL_CANONICAL_TYPES {
            let mut seen: Vec<TargetFormat> = Vec::new();
            for route in routes_for(*ty) {
                assert!(
                    !seen.contains(&route.target),
                    "{ty} lists target {} twice",
                    route.target
                );
                seen.push(route.target);
            }
        }
    }

    #[test]
    fn every_target_format_is_modelled_somewhere() {
        let modelled = all_modelled_targets();
        for target in ALL_TARGETS {
            assert!(
                modelled.contains(target),
                "target {target} is declared but no route produces it"
            );
        }
    }

    // -- family invariants --------------------------------------------------

    #[test]
    fn ffmpeg_only_appears_for_gif_from_images_or_for_audio_video() {
        for (ty, route) in all_routes() {
            if route.engine != Engine::Ffmpeg {
                continue;
            }
            match ty.family() {
                Family::Image => assert_eq!(
                    route.target,
                    TargetFormat::Gif,
                    "{ty}→{}: SPEC §6 rule 1 forbids ffmpeg for image work except the \
                     GIF palette encode (SPEC §9.3 D2)",
                    route.target
                ),
                Family::Audio | Family::Video => {}
                other => panic!("ffmpeg route in family {other:?}: {ty}→{}", route.target),
            }
        }
    }

    #[test]
    fn vips_is_never_used_for_documents_or_audio() {
        for (ty, route) in all_routes() {
            if route.engine != Engine::Vips {
                continue;
            }
            assert!(
                matches!(ty.family(), Family::Image),
                "vips route outside images: {ty}→{}",
                route.target
            );
        }
    }

    #[test]
    fn soffice_is_never_used_for_images_or_audio() {
        for (ty, route) in all_routes() {
            if route.engine != Engine::Soffice {
                continue;
            }
            assert_eq!(
                ty.family(),
                Family::Document,
                "soffice route outside documents: {ty}→{}",
                route.target
            );
        }
    }

    #[test]
    fn identity_routes_are_absent_or_explicitly_justified() {
        // Identity is only modelled where it is a real operation: jpg→jpg
        // (measured re-encode/compress, V6) and the audio/video rows (a bitrate
        // or trim change). Everywhere else it must be absent.
        for ty in [
            CanonicalType::Png,
            CanonicalType::WebP,
            CanonicalType::Tiff,
            CanonicalType::Docx,
            CanonicalType::Xlsx,
            CanonicalType::Markdown,
            CanonicalType::Txt,
        ] {
            let same = match ty {
                CanonicalType::Png => TargetFormat::Png,
                CanonicalType::WebP => TargetFormat::Webp,
                CanonicalType::Tiff => TargetFormat::Tiff,
                CanonicalType::Docx => TargetFormat::Docx,
                CanonicalType::Xlsx => TargetFormat::Xlsx,
                CanonicalType::Markdown => TargetFormat::Md,
                CanonicalType::Txt => TargetFormat::Txt,
                _ => unreachable!(),
            };
            assert!(
                !spec_scope_targets(ty).contains(&same),
                "{ty} must not offer an identity route"
            );
        }
        // and the two that are real operations are present
        assert!(spec_scope_targets(CanonicalType::Jpeg).contains(&TargetFormat::Jpg));
        assert!(spec_scope_targets(CanonicalType::Mp3).contains(&TargetFormat::Mp3));
    }

    #[test]
    fn heic_and_svg_output_are_cut_for_every_image_input() {
        for ty in ALL_CANONICAL_TYPES.iter().filter(|t| t.is_image_input()) {
            for (target, what) in [(TargetFormat::Heic, "heic"), (TargetFormat::Svg, "svg")] {
                let route = routes_for(*ty)
                    .iter()
                    .find(|r| r.target == target)
                    .unwrap_or_else(|| panic!("{ty} has no →{what} route to mark cut"));
                assert!(
                    route.status.is_cut(),
                    "{ty}→{what} must be cut (SPEC §9.3 D3/D4)"
                );
            }
        }
    }

    // -- SPEC §3 scope ------------------------------------------------------

    #[test]
    fn spec_section_3_image_scope_is_fully_modelled() {
        // SPEC §3 line 1 + §9.2 in/out table: these ten are the image targets.
        let image_targets = [
            TargetFormat::Jpg,
            TargetFormat::Png,
            TargetFormat::Webp,
            TargetFormat::Tiff,
            TargetFormat::Avif,
            TargetFormat::Gif,
            TargetFormat::Bmp,
            TargetFormat::Heic,
            TargetFormat::Svg,
            TargetFormat::Pdf,
        ];
        for ty in ALL_CANONICAL_TYPES.iter().filter(|t| t.is_image_input()) {
            let scope = spec_scope_targets(*ty);
            let identity = identity_target(*ty);
            for target in image_targets {
                if Some(target) == identity {
                    continue; // identity is modelled only where it is a real operation
                }
                assert!(
                    scope.contains(&target),
                    "{ty}: SPEC §3 image target {target} is missing from the matrix"
                );
            }
        }
    }

    /// The target that would be a no-op for this type, if any.
    fn identity_target(ty: CanonicalType) -> Option<TargetFormat> {
        Some(match ty {
            CanonicalType::Jpeg => TargetFormat::Jpg,
            CanonicalType::Png => TargetFormat::Png,
            CanonicalType::WebP => TargetFormat::Webp,
            CanonicalType::Tiff => TargetFormat::Tiff,
            CanonicalType::Heic => TargetFormat::Heic,
            CanonicalType::Avif => TargetFormat::Avif,
            CanonicalType::Gif => TargetFormat::Gif,
            CanonicalType::Bmp => TargetFormat::Bmp,
            CanonicalType::Svg => TargetFormat::Svg,
            CanonicalType::Pdf => TargetFormat::Pdf,
            CanonicalType::Docx => TargetFormat::Docx,
            CanonicalType::Xlsx => TargetFormat::Xlsx,
            CanonicalType::Pptx => TargetFormat::Pptx,
            CanonicalType::Doc => TargetFormat::Docx,
            CanonicalType::Xls => TargetFormat::Xlsx,
            CanonicalType::Ppt => TargetFormat::Pptx,
            CanonicalType::Rtf => TargetFormat::Rtf,
            CanonicalType::Csv => TargetFormat::Csv,
            CanonicalType::Markdown => TargetFormat::Md,
            CanonicalType::Html => TargetFormat::Html,
            CanonicalType::Txt => TargetFormat::Txt,
            CanonicalType::Mp3 => TargetFormat::Mp3,
            CanonicalType::Wav => TargetFormat::Wav,
            CanonicalType::Flac => TargetFormat::Flac,
            CanonicalType::M4a => TargetFormat::M4a,
            CanonicalType::Aac => TargetFormat::Aac,
            CanonicalType::Ogg => TargetFormat::Ogg,
            CanonicalType::Opus => TargetFormat::Opus,
            CanonicalType::Mp4 => TargetFormat::Mp4,
            CanonicalType::Mkv => TargetFormat::Mkv,
            CanonicalType::Mov => TargetFormat::Mov,
            CanonicalType::WebM => TargetFormat::Webm,
            CanonicalType::Unknown => return None,
        })
    }

    #[test]
    fn spec_section_3_audio_and_video_scope_is_fully_modelled() {
        let audio = [
            TargetFormat::Mp3,
            TargetFormat::Wav,
            TargetFormat::Flac,
            TargetFormat::M4a,
            TargetFormat::Aac,
            TargetFormat::Ogg,
            TargetFormat::Opus,
        ];
        for ty in [
            CanonicalType::Mp3,
            CanonicalType::Wav,
            CanonicalType::Flac,
            CanonicalType::M4a,
            CanonicalType::Aac,
            CanonicalType::Ogg,
            CanonicalType::Opus,
        ] {
            let scope = spec_scope_targets(ty);
            for target in audio {
                assert!(
                    scope.contains(&target),
                    "{ty}: audio target {target} missing"
                );
            }
        }
        let video = [
            TargetFormat::Mp4,
            TargetFormat::Mkv,
            TargetFormat::Mov,
            TargetFormat::Webm,
            TargetFormat::Gif,
        ];
        for ty in [
            CanonicalType::Mp4,
            CanonicalType::Mkv,
            CanonicalType::Mov,
            CanonicalType::WebM,
        ] {
            let scope = spec_scope_targets(ty);
            for target in video {
                assert!(
                    scope.contains(&target),
                    "{ty}: video target {target} missing"
                );
            }
        }
    }

    #[test]
    fn spec_section_3_document_scope_is_fully_modelled() {
        // The 9 document targets modelled: ARCHITECTURE §8.3's seven columns,
        // plus pptx and rtf, which SPEC §3 names as document formats.
        let doc = [
            TargetFormat::Pdf,
            TargetFormat::Md,
            TargetFormat::Html,
            TargetFormat::Docx,
            TargetFormat::Xlsx,
            TargetFormat::Pptx,
            TargetFormat::Rtf,
            TargetFormat::Csv,
            TargetFormat::Txt,
        ];
        let doc_types = [
            CanonicalType::Docx,
            CanonicalType::Xlsx,
            CanonicalType::Pptx,
            CanonicalType::Doc,
            CanonicalType::Xls,
            CanonicalType::Ppt,
            CanonicalType::Rtf,
            CanonicalType::Csv,
            CanonicalType::Markdown,
            CanonicalType::Html,
            CanonicalType::Txt,
            CanonicalType::Pdf,
        ];
        for ty in doc_types {
            let scope = spec_scope_targets(ty);
            let identity = identity_target(ty);
            for target in doc {
                if Some(target) == identity {
                    continue;
                }
                assert!(
                    scope.contains(&target),
                    "{ty}: document target {target} missing from the matrix"
                );
            }
        }
    }

    #[test]
    fn out_of_scope_formats_never_enter_the_matrix() {
        for ext in ["msg", "eml", "one", "pub", "vsdx"] {
            assert_eq!(
                type_for_extension(ext),
                None,
                "SPEC §3 parks .{ext} as out of scope"
            );
        }
    }

    // -- multi-file drops ---------------------------------------------------

    #[test]
    fn multi_file_drop_offers_the_intersection() {
        let plan = multi_file_targets(&[CanonicalType::Jpeg, CanonicalType::Avif]);
        assert!(!plan.used_union_fallback);
        // jpeg proves jpg, png, webp, tiff, avif, gif; avif proves those five except
        // avif itself (no avif->avif) → intersection = jpg, png, webp, tiff, gif
        let mut targets = plan.targets.clone();
        targets.sort();
        let mut expected = vec![
            TargetFormat::Jpg,
            TargetFormat::Png,
            TargetFormat::Webp,
            TargetFormat::Tiff,
            TargetFormat::Gif,
        ];
        expected.sort();
        assert_eq!(targets, expected);
    }

    #[test]
    fn multi_file_drop_falls_back_to_the_union_when_the_intersection_is_empty() {
        // docx proves pdf/md/html; avif proves jpg only → nothing shared.
        let plan = multi_file_targets(&[CanonicalType::Docx, CanonicalType::Avif]);
        assert!(plan.used_union_fallback, "docx+avif share no proven target");
        let mut targets = plan.targets;
        targets.sort();
        let mut expected = offered_targets(CanonicalType::Docx);
        for t in offered_targets(CanonicalType::Avif) {
            if !expected.contains(&t) {
                expected.push(t);
            }
        }
        expected.sort();
        assert_eq!(targets, expected);
        assert!(targets.contains(&TargetFormat::Pdf));
        assert!(targets.contains(&TargetFormat::Jpg));
    }

    #[test]
    fn a_drop_of_only_unproven_types_offers_nothing() {
        // pdf and txt have routes but nothing proven: the wheel is empty and the
        // UI shows the "nothing to do yet" state rather than a fake target.
        let plan = multi_file_targets(&[CanonicalType::Pdf, CanonicalType::Txt]);
        assert!(plan.targets.is_empty());
        assert!(plan.used_union_fallback);
    }

    #[test]
    fn single_file_drop_is_just_its_offered_targets() {
        let plan = multi_file_targets(&[CanonicalType::Docx]);
        assert!(!plan.used_union_fallback);
        assert_eq!(plan.targets, offered_targets(CanonicalType::Docx));
        assert_eq!(
            plan.targets,
            vec![
                TargetFormat::Pdf,
                TargetFormat::Md,
                TargetFormat::Txt,
                TargetFormat::Html,
                TargetFormat::Rtf
            ]
        );
    }

    // -- documentation stays in sync ---------------------------------------

    #[test]
    fn detection_doc_matrix_matches_the_code() {
        let doc = read_repo_file("docs/DETECTION.md");
        let block = between_markers(&doc, "matrix:begin", "matrix:end");
        assert_eq!(
            block.trim(),
            render_matrix_table().trim(),
            "docs/DETECTION.md matrix block is stale — regenerate it"
        );
        let offered = between_markers(&doc, "offered:begin", "offered:end");
        assert_eq!(
            offered.trim(),
            render_offered_table().trim(),
            "docs/DETECTION.md offered block is stale — regenerate it"
        );
        let scope = between_markers(&doc, "scope:begin", "scope:end");
        assert_eq!(
            scope.trim(),
            render_spec_scope_table().trim(),
            "docs/DETECTION.md scope block is stale — regenerate it"
        );
    }

    #[test]
    fn architecture_doc_still_carries_the_labels_the_evidence_points_at() {
        let root = repo_root();
        let arch = std::fs::read_to_string(root.join("docs/ARCHITECTURE.md"))
            .expect("docs/ARCHITECTURE.md must be in the repo");
        for ev in [EV_V2, EV_V6, EV_V8, EV_V8B, EV_D1, EV_D7] {
            assert!(
                arch.contains(ev.label),
                "docs/ARCHITECTURE.md no longer mentions evidence label {}",
                ev.label
            );
        }
    }

    #[test]
    fn matrix_is_wide_enough_to_be_useful() {
        // A canary against accidental table truncation during a refactor.
        assert!(
            route_count() >= 150,
            "matrix has only {} routes; the SPEC §3 scope is larger than that",
            route_count()
        );
    }

    /// Extract the text between two HTML comment markers, exclusive.
    fn between_markers<'a>(doc: &'a str, begin: &str, end: &str) -> &'a str {
        let begin_tag = format!("<!-- {begin} -->");
        let end_tag = format!("<!-- {end} -->");
        let start = doc
            .find(&begin_tag)
            .unwrap_or_else(|| panic!("missing marker {begin_tag}"))
            + begin_tag.len();
        let stop = doc[start..]
            .find(&end_tag)
            .unwrap_or_else(|| panic!("missing marker {end_tag}"))
            + start;
        &doc[start..stop]
    }
}
