//! Pure data types for detection, routing and the window.
//!
//! No I/O, no Tauri, no external crates (`ARCHITECTURE.md` §2.2). Everything in
//! this module is copyable data plus total functions over it, so the wheel, the
//! window and the tests can share one vocabulary.
//!
//! Sections: (1) canonical types, targets, routes and evidence — card `C1`;
//! (2) queue shapes, the target-name list and size formatting — card `U2`. The
//! two live here because both are pure data; the target-name list is asserted
//! against [`ALL_TARGETS`] so the window cannot drift from the matrix.

use std::fmt;
use std::path::PathBuf;

/// What the bytes are, as decided by `detect` (content first, name second).
///
/// `Unknown` is a first-class answer, not an error: `ARCHITECTURE.md` §7 rule 6
/// forbids guessing, so bytes we cannot place get no targets at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CanonicalType {
    // ---- images (SPEC.md §3, §9.2)
    Jpeg,
    Png,
    Heic,
    Avif,
    WebP,
    Gif,
    Tiff,
    Bmp,
    Svg,
    Pdf,
    // ---- audio (SPEC.md §3)
    Mp3,
    Wav,
    Flac,
    M4a,
    Aac,
    Ogg,
    Opus,
    // ---- video (SPEC.md §3)
    Mp4,
    Mkv,
    Mov,
    WebM,
    // ---- documents (SPEC.md §3, §9.2 for raster-in pdf)
    Docx,
    Xlsx,
    Pptx,
    Doc,
    Xls,
    Ppt,
    Rtf,
    Csv,
    Markdown,
    Html,
    Txt,
    // ---- the honest answer
    Unknown,
}

/// Every canonical type except [`CanonicalType::Unknown`], in declaration order.
///
/// One list, used by the matrix completeness tests. Adding a variant without
/// listing it here fails `every_canonical_type_is_covered_by_the_matrix`.
pub const ALL_CANONICAL_TYPES: &[CanonicalType] = &[
    CanonicalType::Jpeg,
    CanonicalType::Png,
    CanonicalType::Heic,
    CanonicalType::Avif,
    CanonicalType::WebP,
    CanonicalType::Gif,
    CanonicalType::Tiff,
    CanonicalType::Bmp,
    CanonicalType::Svg,
    CanonicalType::Pdf,
    CanonicalType::Mp3,
    CanonicalType::Wav,
    CanonicalType::Flac,
    CanonicalType::M4a,
    CanonicalType::Aac,
    CanonicalType::Ogg,
    CanonicalType::Opus,
    CanonicalType::Mp4,
    CanonicalType::Mkv,
    CanonicalType::Mov,
    CanonicalType::WebM,
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
];

/// Broad family, used for wheel grouping, batch partitioning and the
/// "never route a photo through ffmpeg" rule (`SPEC.md` §6 rule 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    Image,
    Audio,
    Video,
    Document,
    Unknown,
}

impl CanonicalType {
    /// Stable lower-case identifier (also the fixture-file stem and doc key).
    pub fn as_str(self) -> &'static str {
        match self {
            CanonicalType::Jpeg => "jpeg",
            CanonicalType::Png => "png",
            CanonicalType::Heic => "heic",
            CanonicalType::Avif => "avif",
            CanonicalType::WebP => "webp",
            CanonicalType::Gif => "gif",
            CanonicalType::Tiff => "tiff",
            CanonicalType::Bmp => "bmp",
            CanonicalType::Svg => "svg",
            CanonicalType::Pdf => "pdf",
            CanonicalType::Mp3 => "mp3",
            CanonicalType::Wav => "wav",
            CanonicalType::Flac => "flac",
            CanonicalType::M4a => "m4a",
            CanonicalType::Aac => "aac",
            CanonicalType::Ogg => "ogg",
            CanonicalType::Opus => "opus",
            CanonicalType::Mp4 => "mp4",
            CanonicalType::Mkv => "mkv",
            CanonicalType::Mov => "mov",
            CanonicalType::WebM => "webm",
            CanonicalType::Docx => "docx",
            CanonicalType::Xlsx => "xlsx",
            CanonicalType::Pptx => "pptx",
            CanonicalType::Doc => "doc",
            CanonicalType::Xls => "xls",
            CanonicalType::Ppt => "ppt",
            CanonicalType::Rtf => "rtf",
            CanonicalType::Csv => "csv",
            CanonicalType::Markdown => "md",
            CanonicalType::Html => "html",
            CanonicalType::Txt => "txt",
            CanonicalType::Unknown => "unknown",
        }
    }

    pub fn family(self) -> Family {
        match self {
            CanonicalType::Jpeg
            | CanonicalType::Png
            | CanonicalType::Heic
            | CanonicalType::Avif
            | CanonicalType::WebP
            | CanonicalType::Gif
            | CanonicalType::Tiff
            | CanonicalType::Bmp
            | CanonicalType::Svg => Family::Image,
            // pdf is raster-in for images (§9.2) *and* a document; its document
            // role is the primary one, and its routes are merged in the matrix.
            CanonicalType::Pdf => Family::Document,
            CanonicalType::Mp3
            | CanonicalType::Wav
            | CanonicalType::Flac
            | CanonicalType::M4a
            | CanonicalType::Aac
            | CanonicalType::Ogg
            | CanonicalType::Opus => Family::Audio,
            CanonicalType::Mp4 | CanonicalType::Mkv | CanonicalType::Mov | CanonicalType::WebM => {
                Family::Video
            }
            CanonicalType::Docx
            | CanonicalType::Xlsx
            | CanonicalType::Pptx
            | CanonicalType::Doc
            | CanonicalType::Xls
            | CanonicalType::Ppt
            | CanonicalType::Rtf
            | CanonicalType::Csv
            | CanonicalType::Markdown
            | CanonicalType::Html
            | CanonicalType::Txt => Family::Document,
            CanonicalType::Unknown => Family::Unknown,
        }
    }

    /// True when the type can be admitted as an *image* input: everything in
    /// `SPEC.md` §9.2's "in" column, where `pdf` is raster-in only.
    pub fn is_image_input(self) -> bool {
        matches!(
            self,
            CanonicalType::Jpeg
                | CanonicalType::Png
                | CanonicalType::Heic
                | CanonicalType::Avif
                | CanonicalType::WebP
                | CanonicalType::Gif
                | CanonicalType::Tiff
                | CanonicalType::Bmp
                | CanonicalType::Svg
                | CanonicalType::Pdf
        )
    }
}

impl fmt::Display for CanonicalType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What the *name* claimed, for diagnostics only (`ARCHITECTURE.md` §7 rule 1).
///
/// Never an input to routing: `claimed_matches_content` may log a mismatch, and
/// that is the whole of its authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claimed(pub String);

impl Claimed {
    /// Lower-cased, dot-free extension of a path's file name.
    pub fn from_file_name(name: &str) -> Option<Claimed> {
        let (stem, ext) = name.rsplit_once('.')?;
        // `.bashrc` is a name, not an extension; `foo.` has an empty extension.
        if stem.is_empty() || ext.is_empty() {
            return None;
        }
        Some(Claimed(ext.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Claimed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The type an extension *claims*. Diagnostics and fixtures only — detection
/// never consults this to decide anything.
pub fn type_for_extension(ext: &str) -> Option<CanonicalType> {
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    Some(match ext.as_str() {
        "jpg" | "jpeg" | "jpe" => CanonicalType::Jpeg,
        "png" => CanonicalType::Png,
        "heic" | "heif" => CanonicalType::Heic,
        "avif" => CanonicalType::Avif,
        "webp" => CanonicalType::WebP,
        "gif" => CanonicalType::Gif,
        "tif" | "tiff" => CanonicalType::Tiff,
        "bmp" => CanonicalType::Bmp,
        "svg" => CanonicalType::Svg,
        "pdf" => CanonicalType::Pdf,
        "mp3" => CanonicalType::Mp3,
        "wav" | "wave" => CanonicalType::Wav,
        "flac" => CanonicalType::Flac,
        "m4a" | "m4b" => CanonicalType::M4a,
        "aac" => CanonicalType::Aac,
        "ogg" | "oga" => CanonicalType::Ogg,
        "opus" => CanonicalType::Opus,
        "mp4" | "m4v" => CanonicalType::Mp4,
        "mkv" => CanonicalType::Mkv,
        "mov" => CanonicalType::Mov,
        "webm" => CanonicalType::WebM,
        "docx" => CanonicalType::Docx,
        "xlsx" => CanonicalType::Xlsx,
        "pptx" => CanonicalType::Pptx,
        "doc" => CanonicalType::Doc,
        "xls" => CanonicalType::Xls,
        "ppt" => CanonicalType::Ppt,
        "rtf" => CanonicalType::Rtf,
        "csv" => CanonicalType::Csv,
        "md" | "markdown" => CanonicalType::Markdown,
        "html" | "htm" => CanonicalType::Html,
        "txt" | "text" | "log" => CanonicalType::Txt,
        _ => return None,
    })
}

/// Whether a parsed or complete container was found.
///
/// `Unreadable` means we could not read the bytes at all (I/O, denied, over the
/// per-input cap); it is *not* a verdict about the format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Integrity {
    /// The file ends the way this format is required to end.
    Complete,
    /// Content is recognisable but the end marker / directory record is absent.
    /// Conversion may still work (a truncated JPEG often decodes); the UI warns.
    Truncated,
    /// The bytes could not be read (I/O error, permission, size cap).
    Unreadable,
}

impl Integrity {
    pub fn as_str(self) -> &'static str {
        match self {
            Integrity::Complete => "complete",
            Integrity::Truncated => "truncated",
            Integrity::Unreadable => "unreadable",
        }
    }
}

/// Which text heuristic won. Kept structured so the UI can explain itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextShape {
    Svg,
    Html,
    Csv,
    Markdown,
    Plain,
}

/// The evidence for a detection verdict — why we believe `ty`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Detail {
    /// No extra detail (should not occur for a non-Unknown verdict).
    None,
    /// Zero-byte file.
    Empty,
    /// A fixed byte signature matched, named as in the signature table.
    Signature(&'static str),
    /// An ISOBMFF / EBML brand or doc-type decided it.
    Brand(&'static str),
    /// A ZIP container carried exactly these required parts.
    ZipParts(&'static [&'static str]),
    /// A CFB container carried exactly these directory entries.
    CfbEntries(&'static [&'static str]),
    /// Valid ZIP, but none of the OOXML parts.
    ZipWithoutOfficePart,
    /// Valid CFB, but none of the Office streams.
    CfbWithoutOfficeEntry,
    /// Container recognised; its end record is missing.
    Truncated(&'static str),
    /// A text-shape heuristic (last resort, never for binary magic).
    Text(TextShape),
    /// Bigger than the per-input cap.
    TooLarge { seen: u64, limit: u64 },
    /// The header declares more pixels than we are willing to decode.
    TooManyPixels { width: u64, height: u64, limit: u64 },
    /// Recognised but deliberately unsupported (e.g. ZIP64, EBML without a
    /// doctype). Recorded as a named gap rather than a guess.
    Unsupported(&'static str),
    /// The read failed; the kind is mapped to a static string (no allocation,
    /// and `io::ErrorKind` display is stable enough for a log line).
    Io(&'static str),
}

impl Detail {
    pub fn as_str(&self) -> &'static str {
        match self {
            Detail::None => "none",
            Detail::Empty => "empty file",
            Detail::Signature(s) => s,
            Detail::Brand(b) => b,
            Detail::ZipParts(parts) => parts.first().copied().unwrap_or("zip parts"),
            Detail::CfbEntries(entries) => entries.first().copied().unwrap_or("cfb entries"),
            Detail::ZipWithoutOfficePart => "zip without an OOXML part",
            Detail::CfbWithoutOfficeEntry => "cfb without an Office stream",
            Detail::Truncated(why) => why,
            Detail::Text(shape) => match shape {
                TextShape::Svg => "text: svg root element",
                TextShape::Html => "text: html doctype/root",
                TextShape::Csv => "text: stable delimiter counts",
                TextShape::Markdown => "text: markdown markers",
                TextShape::Plain => "text: plain",
            },
            Detail::TooLarge { .. } => "over the per-input size cap",
            Detail::TooManyPixels { .. } => "over the declared-pixel cap",
            Detail::Unsupported(why) => why,
            Detail::Io(kind) => kind,
        }
    }
}

/// The full detection verdict for one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detected {
    pub ty: CanonicalType,
    pub integrity: Integrity,
    pub claimed: Option<Claimed>,
    pub detail: Detail,
}

impl Detected {
    pub fn unknown(integrity: Integrity, claimed: Option<Claimed>, detail: Detail) -> Detected {
        Detected {
            ty: CanonicalType::Unknown,
            integrity,
            claimed,
            detail,
        }
    }

    /// `None` when the name claimed nothing; `Some(false)` is a mislabelled file
    /// and must be logged, never acted on.
    pub fn claimed_matches_content(&self) -> Option<bool> {
        let claimed = self.claimed.as_ref()?;
        let claimed_ty = type_for_extension(claimed.as_str())?;
        Some(claimed_ty == self.ty)
    }

    /// Routing gate: unknown bytes and unreadable files get no targets.
    pub fn is_routable(&self) -> bool {
        self.ty != CanonicalType::Unknown && self.integrity != Integrity::Unreadable
    }
}

/// An output format the wheel can offer.
///
/// Deliberately a separate enum from [`CanonicalType`]: `m4a`/`aac` and
/// `md`/`txt` are distinct *targets* even where detection folds them, and the
/// matrix must be able to say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TargetFormat {
    Jpg,
    Png,
    Webp,
    Tiff,
    Avif,
    Gif,
    Bmp,
    Heic,
    Svg,
    Pdf,
    Mp3,
    Wav,
    Flac,
    M4a,
    Aac,
    Ogg,
    Opus,
    Mp4,
    Mkv,
    Mov,
    Webm,
    Docx,
    Xlsx,
    Pptx,
    Rtf,
    Csv,
    Md,
    Html,
    Txt,
}

pub const ALL_TARGETS: &[TargetFormat] = &[
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
    TargetFormat::Mp3,
    TargetFormat::Wav,
    TargetFormat::Flac,
    TargetFormat::M4a,
    TargetFormat::Aac,
    TargetFormat::Ogg,
    TargetFormat::Opus,
    TargetFormat::Mp4,
    TargetFormat::Mkv,
    TargetFormat::Mov,
    TargetFormat::Webm,
    TargetFormat::Docx,
    TargetFormat::Xlsx,
    TargetFormat::Pptx,
    TargetFormat::Rtf,
    TargetFormat::Csv,
    TargetFormat::Md,
    TargetFormat::Html,
    TargetFormat::Txt,
];

impl TargetFormat {
    /// Lower-case id, as it appears in docs and settings.
    pub fn as_str(self) -> &'static str {
        match self {
            TargetFormat::Jpg => "jpg",
            TargetFormat::Png => "png",
            TargetFormat::Webp => "webp",
            TargetFormat::Tiff => "tiff",
            TargetFormat::Avif => "avif",
            TargetFormat::Gif => "gif",
            TargetFormat::Bmp => "bmp",
            TargetFormat::Heic => "heic",
            TargetFormat::Svg => "svg",
            TargetFormat::Pdf => "pdf",
            TargetFormat::Mp3 => "mp3",
            TargetFormat::Wav => "wav",
            TargetFormat::Flac => "flac",
            TargetFormat::M4a => "m4a",
            TargetFormat::Aac => "aac",
            TargetFormat::Ogg => "ogg",
            TargetFormat::Opus => "opus",
            TargetFormat::Mp4 => "mp4",
            TargetFormat::Mkv => "mkv",
            TargetFormat::Mov => "mov",
            TargetFormat::Webm => "webm",
            TargetFormat::Docx => "docx",
            TargetFormat::Xlsx => "xlsx",
            TargetFormat::Pptx => "pptx",
            TargetFormat::Rtf => "rtf",
            TargetFormat::Csv => "csv",
            TargetFormat::Md => "md",
            TargetFormat::Html => "html",
            TargetFormat::Txt => "txt",
        }
    }

    /// Extension to write. Same as the id except where the conventional
    /// extension differs (`jpeg` files are `.jpg`, markdown is `.md`).
    pub fn extension(self) -> &'static str {
        self.as_str()
    }

    pub fn family(self) -> Family {
        match self {
            TargetFormat::Jpg
            | TargetFormat::Png
            | TargetFormat::Webp
            | TargetFormat::Tiff
            | TargetFormat::Avif
            | TargetFormat::Gif
            | TargetFormat::Bmp
            | TargetFormat::Heic
            | TargetFormat::Svg => Family::Image,
            TargetFormat::Pdf => Family::Document,
            TargetFormat::Mp3
            | TargetFormat::Wav
            | TargetFormat::Flac
            | TargetFormat::M4a
            | TargetFormat::Aac
            | TargetFormat::Ogg
            | TargetFormat::Opus => Family::Audio,
            TargetFormat::Mp4 | TargetFormat::Mkv | TargetFormat::Mov | TargetFormat::Webm => {
                Family::Video
            }
            TargetFormat::Docx
            | TargetFormat::Xlsx
            | TargetFormat::Pptx
            | TargetFormat::Rtf
            | TargetFormat::Csv
            | TargetFormat::Md
            | TargetFormat::Html
            | TargetFormat::Txt => Family::Document,
        }
    }
}

impl fmt::Display for TargetFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A board card id (`t_` + 8 hex chars), used to own a `Planned` route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardId(pub &'static str);

impl CardId {
    /// Structural check only — CI cannot reach the board, so the test asserts
    /// the shape and that `evidence/ROUTES.md` names the id; `warden` re-checks
    /// liveness at the gate (`docs/REFERENCE_POLICY.md` §1).
    pub fn is_well_formed(self) -> bool {
        let s = self.0;
        let Some(rest) = s.strip_prefix("t_") else {
            return false;
        };
        rest.len() == 8 && rest.bytes().all(|b| b.is_ascii_hexdigit())
    }
}

impl fmt::Display for CardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// How strong the recorded evidence is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    /// A raw measurement transcript is committed in the repo and `marker`
    /// occurs in it: exit code, bytes, dimensions, assertions.
    Transcript,
    /// Measured on a named host and recorded in the architecture document, but
    /// no raw transcript was committed for this specific invocation.
    Recorded,
}

impl Provenance {
    pub fn as_str(self) -> &'static str {
        match self {
            Provenance::Transcript => "transcript",
            Provenance::Recorded => "recorded",
        }
    }
}

/// A pointer at committed evidence: the label used in `ARCHITECTURE.md` §6.1 /
/// §6.2, the claim it supports, and a file + literal that must still be true.
///
/// The test opens `source` and greps for `marker`, so a "Proven" route whose
/// evidence was deleted or rewritten fails `cargo test` (`§0`, `§19`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvidenceId {
    /// Label in the architecture's measured-invocation table (`V2`, `D1`, ...).
    pub label: &'static str,
    /// The measured outcome, in the words of the evidence.
    pub claim: &'static str,
    /// Repo-relative path of the file the claim lives in.
    pub source: &'static str,
    /// Literal that must occur in `source`.
    pub marker: &'static str,
    pub provenance: Provenance,
}

impl fmt::Display for EvidenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({} {}, {})",
            self.label,
            self.provenance.as_str(),
            self.source,
            self.claim
        )
    }
}

/// `ARCHITECTURE.md` §0: a capability exists when evidence exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteStatus {
    Proven {
        evidence: EvidenceId,
    },
    Planned {
        owner: CardId,
        hypothesis: &'static str,
    },
    Cut {
        reason: &'static str,
    },
}

impl RouteStatus {
    pub fn is_proven(self) -> bool {
        matches!(self, RouteStatus::Proven { .. })
    }
    pub fn is_planned(self) -> bool {
        matches!(self, RouteStatus::Planned { .. })
    }
    pub fn is_cut(self) -> bool {
        matches!(self, RouteStatus::Cut { .. })
    }
}

/// The engine family a route goes through. Mirrors `SPEC.md` §6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// libvips sidecar (image encode/decode/rotate).
    Vips,
    /// ffmpeg sidecar. Images reach it only for GIF palette encoding (§9.3 D2).
    Ffmpeg,
    /// LibreOffice `soffice` (documents).
    Soffice,
    /// PDFium raster path (pdf → image). `C2`, planned.
    Pdfium,
    /// resvg sidecar (svg → png hop). `C2`, planned.
    Resvg,
    /// In-process Rust crate (bmp via `image`, calamine, rust_xlsxwriter, comrak).
    RustCrate,
}

impl Engine {
    pub fn as_str(self) -> &'static str {
        match self {
            Engine::Vips => "vips",
            Engine::Ffmpeg => "ffmpeg",
            Engine::Soffice => "soffice",
            Engine::Pdfium => "pdfium",
            Engine::Resvg => "resvg",
            Engine::RustCrate => "rust-crate",
        }
    }
}

/// One target for one input type.
///
/// `ARCHITECTURE.md` §8.1 also carries `plan: fn(&JobCtx) -> ExecPlan`. That
/// field is deliberately absent here: `engine::ExecPlan` does not exist yet
/// (it lands with `C2`/`C3`), and a fn pointer to a non-existent type would be
/// decoration pretending to be a design. See `docs/DETECTION.md` "Deltas".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Route {
    pub target: TargetFormat,
    pub engine: Engine,
    pub status: RouteStatus,
}

// ---------------------------------------------------------------------------
// Queue-facing shapes (card U2)
/// A file accepted into the queue.
///
/// `detected` is `None` until the detection step (`C1`) has run; the window
/// renders it as "checking type" rather than inventing a guess. `SPEC.md` §3
/// makes detection content-first, so no extension-based classification exists
/// here on purpose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputFile {
    pub path: PathBuf,
    pub size: u64,
}

/// Where an item sits in the per-file state machine (`ARCHITECTURE.md` §5.1).
///
/// `U2` renders these; the worker (`C4`) drives them. The variants are the ones
/// the window can show today — `Verifying` is included because `Done` must be
/// unreachable without a validator (§5.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemState {
    Queued,
    Preparing,
    Running,
    Verifying,
    Done,
    Failed(String),
    Skipped(String),
    Cancelled,
}

impl ItemState {
    /// Stable identifier for the UI (never localised text).
    pub fn code(&self) -> &'static str {
        match self {
            ItemState::Queued => "queued",
            ItemState::Preparing => "preparing",
            ItemState::Running => "running",
            ItemState::Verifying => "verifying",
            ItemState::Done => "done",
            ItemState::Failed(_) => "failed",
            ItemState::Skipped(_) => "skipped",
            ItemState::Cancelled => "cancelled",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ItemState::Done | ItemState::Failed(_) | ItemState::Skipped(_) | ItemState::Cancelled
        )
    }

    pub fn detail(&self) -> Option<&str> {
        match self {
            ItemState::Failed(why) | ItemState::Skipped(why) => Some(why.as_str()),
            _ => None,
        }
    }
}

/// Target formats the window may offer.
///
/// This is the *union of names* from `SPEC.md` §3/§9 so the settings editor can
/// express a wheel layout and the queue can show a chosen target. Which of them
/// is actually offered for a given input is the route matrix's decision
/// (`ARCHITECTURE.md` §0/§8, card `C1`) — a name here is not a promise that the
/// route is `Proven`.
pub const TARGET_FORMATS: &[&str] = &[
    "jpg", "png", "webp", "tiff", "avif", "gif", "bmp", "heic", "svg", "pdf", "mp3", "wav", "flac",
    "m4a", "aac", "ogg", "opus", "mp4", "mkv", "mov", "webm", "docx", "xlsx", "pptx", "rtf", "csv",
    "md", "html", "txt",
];

/// True when `target` is a name from the spec's format list.
/// Case-insensitive; the queue stores the lowercase form.
pub fn is_known_target(target: &str) -> bool {
    TARGET_FORMATS.contains(&target.to_ascii_lowercase().as_str())
}

/// Human label for a size in bytes (binary prefixes, one decimal).
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod queue_shape_tests {
    use super::*;

    #[test]
    fn target_names_are_lowercase_and_unique() {
        let mut seen = TARGET_FORMATS.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), TARGET_FORMATS.len(), "duplicate target name");
        assert!(TARGET_FORMATS.iter().all(|t| *t == t.to_ascii_lowercase()));
    }

    #[test]
    fn known_target_is_case_insensitive() {
        assert!(is_known_target("PNG"));
        assert!(is_known_target("pdf"));
        assert!(!is_known_target("klingon"));
    }

    #[test]
    fn every_queue_state_has_a_stable_code() {
        let states = [
            ItemState::Queued,
            ItemState::Preparing,
            ItemState::Running,
            ItemState::Verifying,
            ItemState::Done,
            ItemState::Failed("x".into()),
            ItemState::Skipped("y".into()),
            ItemState::Cancelled,
        ];
        let codes: Vec<&str> = states.iter().map(ItemState::code).collect();
        let mut unique = codes.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), codes.len(), "state codes must be unique");
        assert_eq!(ItemState::Failed("why".into()).detail(), Some("why"));
        assert!(!ItemState::Verifying.is_terminal());
    }

    #[test]
    fn human_size_is_readable() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KiB");
        assert_eq!(human_size(3 * 1024 * 1024), "3.0 MiB");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_type_ids_are_unique_and_lowercase() {
        let mut seen: Vec<&str> = ALL_CANONICAL_TYPES.iter().map(|t| t.as_str()).collect();
        seen.push("unknown");
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(before, seen.len(), "duplicate canonical type id");
        assert!(
            seen.iter().all(|s| *s == s.to_ascii_lowercase()),
            "canonical ids must be lowercase"
        );
    }

    #[test]
    fn target_ids_are_unique_and_lowercase() {
        let mut seen: Vec<&str> = ALL_TARGETS.iter().map(|t| t.as_str()).collect();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(before, seen.len(), "duplicate target id");
        assert!(
            seen.iter().all(|s| *s == s.to_ascii_lowercase()),
            "target ids must be lowercase"
        );
    }

    #[test]
    fn every_family_has_a_member_and_unknown_is_its_own() {
        assert_eq!(CanonicalType::Unknown.family(), Family::Unknown);
        assert_eq!(CanonicalType::Jpeg.family(), Family::Image);
        assert_eq!(CanonicalType::Mp3.family(), Family::Audio);
        assert_eq!(CanonicalType::Mp4.family(), Family::Video);
        assert_eq!(CanonicalType::Docx.family(), Family::Document);
        for t in ALL_CANONICAL_TYPES {
            assert_ne!(t.family(), Family::Unknown, "{t} has no family");
        }
    }

    #[test]
    fn claimed_extension_is_parsed_from_names_not_paths() {
        assert_eq!(
            Claimed::from_file_name("photo.JPG"),
            Some(Claimed("jpg".into()))
        );
        assert_eq!(
            Claimed::from_file_name("archive.tar.gz"),
            Some(Claimed("gz".into()))
        );
        assert_eq!(Claimed::from_file_name(".bashrc"), None);
        assert_eq!(Claimed::from_file_name("no_extension"), None);
        assert_eq!(Claimed::from_file_name("trailing."), None);
    }

    #[test]
    fn extension_table_accepts_aliases() {
        assert_eq!(type_for_extension("jpg"), Some(CanonicalType::Jpeg));
        assert_eq!(type_for_extension(".JPEG"), Some(CanonicalType::Jpeg));
        assert_eq!(type_for_extension("tif"), Some(CanonicalType::Tiff));
        assert_eq!(
            type_for_extension("MARKDOWN"),
            Some(CanonicalType::Markdown)
        );
        assert_eq!(type_for_extension("klingon"), None);
    }

    #[test]
    fn claimed_mismatch_is_reported_not_resolved() {
        let d = Detected {
            ty: CanonicalType::Jpeg,
            integrity: Integrity::Complete,
            claimed: Some(Claimed("png".into())),
            detail: Detail::Signature("jpeg SOI"),
        };
        assert_eq!(d.claimed_matches_content(), Some(false));
        assert!(d.is_routable(), "a mislabelled JPEG is still routable");

        let matching = Detected {
            claimed: Some(Claimed("jpeg".into())),
            ..d.clone()
        };
        assert_eq!(matching.claimed_matches_content(), Some(true));

        let unclaimed = Detected { claimed: None, ..d };
        assert_eq!(unclaimed.claimed_matches_content(), None);
    }

    #[test]
    fn unknown_and_unreadable_are_not_routable() {
        let base = Detected {
            ty: CanonicalType::Png,
            integrity: Integrity::Complete,
            claimed: None,
            detail: Detail::Signature("png signature"),
        };
        assert!(base.is_routable());
        assert!(!Detected {
            ty: CanonicalType::Unknown,
            ..base.clone()
        }
        .is_routable());
        assert!(!Detected {
            integrity: Integrity::Unreadable,
            ..base
        }
        .is_routable());
    }

    #[test]
    fn card_ids_must_look_like_board_ids() {
        assert!(CardId("t_dc07a72e").is_well_formed());
        assert!(!CardId("C2").is_well_formed());
        assert!(!CardId("t_XYZ").is_well_formed());
        assert!(!CardId("t_dc07a72").is_well_formed());
    }

    #[test]
    fn the_windows_target_name_list_matches_the_matrix_targets() {
        // U2 keeps a `&[&str]` list for the settings editor and the queue; the
        // matrix owns `TargetFormat`. Two sources of truth for "what targets
        // exist" is one too many, so they are asserted equal here. When the
        // window starts asking the matrix (`offered_targets`) this test can go.
        let from_matrix: Vec<&str> = ALL_TARGETS.iter().map(|t| t.as_str()).collect();
        assert_eq!(
            TARGET_FORMATS,
            from_matrix.as_slice(),
            "TARGET_FORMATS (U2) and ALL_TARGETS (C1) have drifted; the matrix is authoritative"
        );
    }
}
