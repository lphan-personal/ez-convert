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
/// C3 — document pipeline (soffice, pandoc, rust crates).
pub const OWNER_C3: CardId = CardId("t_ea95c826");
/// C4 — output rules. No route is owned here today; kept for symmetry.
pub const OWNER_C4: CardId = CardId("t_b2723c61");

// ---------------------------------------------------------------------------
// Evidence (§0; markers are greped by the tests)
// ---------------------------------------------------------------------------

const E_S2: &str = "evidence/s2-libvips.md";
const E_S2B: &str = "evidence/s2b-avif.md";
const E_S3: &str = "spikes/s3/EVIDENCE.md";

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
// Status shorthands
// ---------------------------------------------------------------------------

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
const PLANNED_DOC: RouteStatus = RouteStatus::Planned {
    owner: OWNER_C3,
    hypothesis: "LibreOffice filter exists but this direction has no fixture yet",
};
const PLANNED_DOC_CRATE: RouteStatus = RouteStatus::Planned {
    owner: OWNER_C3,
    hypothesis: "in-process comrak/htmd conversion; unmeasured",
};
const PLANNED_LEGACY: RouteStatus = RouteStatus::Planned {
    owner: OWNER_C3,
    hypothesis: "same LibreOffice filters as the OOXML rows, no legacy fixture on either platform",
};
const PLANNED_AV: RouteStatus = RouteStatus::Planned {
    owner: OWNER_C2,
    hypothesis: "ffmpeg sidecar surface designed (ARCHITECTURE 6.3) but unmeasured end to end",
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
    route!(Webp, Vips, PLANNED_IMG),
    route!(Tiff, Vips, PLANNED_IMG),
    route!(Avif, Vips, RouteStatus::Proven { evidence: EV_V8 }),
    route!(Gif, Ffmpeg, PLANNED_IMG_GIF),
    route!(Bmp, RustCrate, PLANNED_IMG_BMP),
    route!(Heic, Vips, CUT_HEIC),
    route!(Svg, Vips, CUT_SVG),
    route!(Pdf, Vips, CUT_IMG_PDF),
];

static R_PNG: &[Route] = &[
    route!(Jpg, Vips, PLANNED_IMG),
    route!(Webp, Vips, RouteStatus::Proven { evidence: EV_V3 }),
    route!(Tiff, Vips, PLANNED_IMG),
    route!(Avif, Vips, RouteStatus::Proven { evidence: EV_V8 }),
    route!(Gif, Ffmpeg, PLANNED_IMG_GIF),
    route!(Bmp, RustCrate, PLANNED_IMG_BMP),
    route!(Heic, Vips, CUT_HEIC),
    route!(Svg, Vips, CUT_SVG),
    route!(Pdf, Vips, CUT_IMG_PDF),
];

static R_WEBP: &[Route] = &[
    route!(Jpg, Vips, PLANNED_IMG),
    route!(Png, Vips, PLANNED_IMG),
    route!(Tiff, Vips, RouteStatus::Proven { evidence: EV_V4 }),
    route!(Avif, Vips, PLANNED_IMG),
    route!(Gif, Ffmpeg, PLANNED_IMG_GIF),
    route!(Bmp, RustCrate, PLANNED_IMG_BMP),
    route!(Heic, Vips, CUT_HEIC),
    route!(Svg, Vips, CUT_SVG),
    route!(Pdf, Vips, CUT_IMG_PDF),
];

static R_TIFF: &[Route] = &[
    route!(Jpg, Vips, PLANNED_IMG),
    route!(Png, Vips, RouteStatus::Proven { evidence: EV_V5 }),
    route!(Webp, Vips, PLANNED_IMG),
    route!(Avif, Vips, PLANNED_IMG),
    route!(Gif, Ffmpeg, PLANNED_IMG_GIF),
    route!(Bmp, RustCrate, PLANNED_IMG_BMP),
    route!(Heic, Vips, CUT_HEIC),
    route!(Svg, Vips, CUT_SVG),
    route!(Pdf, Vips, CUT_IMG_PDF),
];

static R_HEIC: &[Route] = &[
    route!(Jpg, Vips, RouteStatus::Proven { evidence: EV_V7 }),
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

static R_AVIF: &[Route] = &[
    route!(Jpg, Vips, RouteStatus::Proven { evidence: EV_V8B }),
    route!(Png, Vips, PLANNED_IMG),
    route!(Webp, Vips, PLANNED_IMG),
    route!(Tiff, Vips, PLANNED_IMG),
    route!(Gif, Ffmpeg, PLANNED_IMG_GIF),
    route!(Bmp, RustCrate, PLANNED_IMG_BMP),
    route!(Heic, Vips, CUT_HEIC),
    route!(Svg, Vips, CUT_SVG),
    route!(Pdf, Vips, CUT_IMG_PDF),
];

static R_GIF: &[Route] = &[
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
    route!(Txt, Soffice, PLANNED_DOC),
];

// ---------------------------------------------------------------------------
// Document routes (ARCHITECTURE §8.3)
// ---------------------------------------------------------------------------

static R_DOCX: &[Route] = &[
    route!(Pdf, Soffice, RouteStatus::Proven { evidence: EV_D1 }),
    route!(Md, Soffice, RouteStatus::Proven { evidence: EV_D2 }),
    route!(Html, Soffice, RouteStatus::Proven { evidence: EV_D3 }),
    route!(Xlsx, Soffice, PLANNED_DOC),
    route!(Pptx, Soffice, CUT_CROSS_FAMILY),
    route!(Rtf, Soffice, PLANNED_DOC),
    route!(Csv, Soffice, CUT_DOC),
    route!(Txt, Soffice, PLANNED_DOC),
];

static R_PPTX: &[Route] = &[
    route!(Pdf, Soffice, RouteStatus::Proven { evidence: EV_D7 }),
    route!(Md, Soffice, PLANNED_DOC),
    route!(Html, Soffice, PLANNED_DOC),
    route!(Docx, Soffice, CUT_CROSS_FAMILY),
    route!(Xlsx, Soffice, CUT_DOC),
    route!(Rtf, Soffice, CUT_CROSS_FAMILY),
    route!(Csv, Soffice, CUT_DOC),
    route!(Txt, Soffice, CUT_DOC),
];

static R_XLSX: &[Route] = &[
    route!(Pdf, Soffice, PLANNED_DOC),
    route!(Md, Soffice, CUT_DOC),
    route!(Html, Soffice, PLANNED_DOC),
    route!(Docx, Soffice, CUT_DOC),
    route!(Pptx, Soffice, CUT_CROSS_FAMILY),
    route!(Rtf, Soffice, CUT_CROSS_FAMILY),
    route!(Csv, Soffice, RouteStatus::Proven { evidence: EV_D5 }),
    route!(Txt, Soffice, PLANNED_DOC),
];

static R_CSV: &[Route] = &[
    route!(Pdf, Soffice, PLANNED_DOC),
    route!(Md, Soffice, CUT_DOC),
    route!(Html, Soffice, CUT_DOC),
    route!(Docx, Soffice, CUT_DOC),
    route!(Pptx, Soffice, CUT_CROSS_FAMILY),
    route!(Rtf, Soffice, CUT_CROSS_FAMILY),
    route!(Xlsx, Soffice, RouteStatus::Proven { evidence: EV_D4 }),
    route!(Txt, Soffice, PLANNED_DOC),
];

static R_HTML: &[Route] = &[
    route!(Pdf, Soffice, PLANNED_DOC),
    route!(Md, RustCrate, PLANNED_DOC_CRATE),
    route!(Docx, Soffice, RouteStatus::Proven { evidence: EV_D6 }),
    route!(Pptx, Soffice, CUT_CROSS_FAMILY),
    route!(Rtf, Soffice, PLANNED_DOC),
    route!(Xlsx, Soffice, CUT_DOC),
    route!(Csv, Soffice, CUT_DOC),
    route!(Txt, Soffice, PLANNED_DOC),
];

static R_DOC_LEGACY: &[Route] = &[
    route!(Pdf, Soffice, PLANNED_LEGACY),
    route!(Md, Soffice, PLANNED_LEGACY),
    route!(Html, Soffice, PLANNED_LEGACY),
    route!(Docx, Soffice, PLANNED_LEGACY),
    route!(Pptx, Soffice, PLANNED_LEGACY),
    route!(Rtf, Soffice, PLANNED_LEGACY),
    route!(Xlsx, Soffice, PLANNED_LEGACY),
    route!(Csv, Soffice, PLANNED_LEGACY),
    route!(Txt, Soffice, PLANNED_LEGACY),
];

static R_RTF: &[Route] = &[
    route!(Pdf, Soffice, PLANNED_DOC),
    route!(Md, Soffice, PLANNED_DOC),
    route!(Html, Soffice, PLANNED_DOC),
    route!(Docx, Soffice, PLANNED_DOC),
    route!(Pptx, Soffice, CUT_CROSS_FAMILY),
    route!(Xlsx, Soffice, CUT_DOC),
    route!(Csv, Soffice, CUT_DOC),
    route!(Txt, Soffice, PLANNED_DOC),
];

static R_MD: &[Route] = &[
    route!(Pdf, Soffice, PLANNED_DOC),
    route!(Html, RustCrate, PLANNED_DOC_CRATE),
    route!(Docx, Soffice, PLANNED_DOC),
    route!(Pptx, Soffice, CUT_CROSS_FAMILY),
    route!(Rtf, Soffice, PLANNED_DOC),
    route!(Xlsx, Soffice, CUT_DOC),
    route!(Csv, Soffice, CUT_DOC),
    route!(Txt, Soffice, PLANNED_DOC),
];

static R_TXT: &[Route] = &[
    route!(Pdf, Soffice, PLANNED_DOC),
    route!(Md, Soffice, CUT_DOC),
    route!(Html, Soffice, CUT_DOC),
    route!(Docx, Soffice, PLANNED_DOC),
    route!(Pptx, Soffice, CUT_CROSS_FAMILY),
    route!(Rtf, Soffice, PLANNED_DOC),
    route!(Xlsx, Soffice, CUT_DOC),
    route!(Csv, Soffice, CUT_DOC),
];

// ---------------------------------------------------------------------------
// Audio routes (SPEC §3: mp3, wav, flac, m4a/aac, ogg, opus + bitrate)
//
// The cross product is the v1 scope; every pair is Planned because no ffmpeg
// contract has been measured (ARCHITECTURE §6.3, §8.4). Identity is a real
// route: it is the re-encode that applies a bitrate change.
// ---------------------------------------------------------------------------

macro_rules! audio_row {
    () => {
        &[
            route!(Mp3, Ffmpeg, PLANNED_AV),
            route!(Wav, Ffmpeg, PLANNED_AV),
            route!(Flac, Ffmpeg, PLANNED_AV),
            route!(M4a, Ffmpeg, PLANNED_AV),
            route!(Aac, Ffmpeg, PLANNED_AV),
            route!(Ogg, Ffmpeg, PLANNED_AV),
            route!(Opus, Ffmpeg, PLANNED_AV),
        ]
    };
}

/// One table for every audio input: the v1 scope is the full cross product
/// (SPEC §3), and no ffmpeg pair has been measured, so the row is identical.
static R_AUDIO: &[Route] = audio_row!();

// ---------------------------------------------------------------------------
// Video routes (SPEC §3: mp4, mkv, mov, webm, gif + trim & resize)
// ---------------------------------------------------------------------------

macro_rules! video_row {
    () => {
        &[
            route!(Mp4, Ffmpeg, PLANNED_AV),
            route!(Mkv, Ffmpeg, PLANNED_AV),
            route!(Mov, Ffmpeg, PLANNED_AV),
            route!(Webm, Ffmpeg, PLANNED_AV),
            route!(Gif, Ffmpeg, PLANNED_AV),
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
        CanonicalType::Doc => R_DOC_LEGACY,
        CanonicalType::Xls => R_DOC_LEGACY,
        CanonicalType::Ppt => R_DOC_LEGACY,
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
        let mut document = 0;
        for (ty, route) in all_routes() {
            if !route.status.is_proven() {
                continue;
            }
            match ty.family() {
                Family::Image => image += 1,
                Family::Document => document += 1,
                other => panic!(
                    "unexpected proven route in family {other:?}: {ty}→{}",
                    route.target
                ),
            }
        }
        // ARCHITECTURE §8.2 has 9 PROVEN cells (8 measured invocations; V8 backs
        // both jpg→avif and png→avif). §8.3 has exactly 7.
        assert_eq!(
            image, 9,
            "proven image cells drifted from ARCHITECTURE §8.2"
        );
        assert_eq!(
            document, 7,
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
        // jpeg proves jpg, png, avif; avif proves jpg only → intersection = jpg
        assert_eq!(plan.targets, vec![TargetFormat::Jpg]);
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
            vec![TargetFormat::Pdf, TargetFormat::Md, TargetFormat::Html]
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
