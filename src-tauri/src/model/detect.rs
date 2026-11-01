//! Content-first file-type detection.
//!
//! `ARCHITECTURE.md` §7 is the contract; this module is its implementation.
//! The rules, restated because they are the whole point:
//!
//! 1. **Content decides.** The name is recorded for diagnostics and never
//!    changes the verdict.
//! 2. **Containers are validated structurally** — a ZIP carrying
//!    `word/document.xml` is a docx, a ZIP carrying nothing familiar is not a
//!    document at all. This is the guard against the measured silent-garbage
//!    trap (`ARCHITECTURE.md` §6.2 N3: a text file renamed `.docx` produces a
//!    valid-looking 12,388 B PDF at exit 0).
//! 3. **Truncation is detected cheaply** from end markers / directory records.
//! 4. **Text types** resolve by shape in a fixed order: svg → html → csv → md → txt.
//! 5. **Limits before any parse**: 2 GiB per input, 250 Mpx declared by a still
//!    image's header, 8 GiB declared ZIP payload, 10,000 container directory
//!    entries.
//! 6. **Unknown is a valid answer.** We never guess, and unknown gets no targets.
//!
//! I/O is bounded: a 64 KiB prefix, a 64 KiB tail, and (only when a container
//! signature matched) the ZIP central directory or the CFB FAT + directory
//! sectors. Nothing here decodes pixel or text content of the whole file.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use super::types::{CanonicalType, Claimed, Detail, Detected, Integrity, TextShape};

/// Bytes read from the front of every file.
pub const PREFIX_LEN: usize = 64 * 1024;
/// Bytes read from the end of every file (end markers, ZIP EOCD).
pub const TAIL_LEN: usize = 64 * 1024;
/// Refuse inputs larger than this before parsing (`ARCHITECTURE.md` §7 rule 5).
pub const MAX_INPUT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Refuse a still image whose header *declares* more pixels than this
/// (`ARCHITECTURE.md` §7 rule 5), before any decoder sees it.
///
/// The byte cap above is not enough: a 1.55 MB PNG declaring 20,000x20,000
/// (400 Mpx) passed detection and converted all the way through vips — flat
/// memory, a 10.9 MB JPEG out, bounded only by the 120 s engine kill (Q1
/// finding F2). 250 Mpx is above every camera and stitched panorama we expect
/// and below the point where a decode stops being a UI action; the geometry is
/// read from the prefix, so the refusal costs one small read.
pub const MAX_DECLARED_PIXELS: u64 = 250_000_000;
/// Refuse a ZIP whose declared uncompressed total exceeds this (bomb guard).
pub const MAX_ZIP_TOTAL_UNCOMPRESSED: u64 = 8 * 1024 * 1024 * 1024;
/// Refuse a ZIP/CFB directory with more entries than this.
pub const MAX_CONTAINER_ENTRIES: usize = 10_000;
/// Cap on bytes read for a ZIP central directory or a CFB FAT chain.
const MAX_STRUCTURE_BYTES: usize = 8 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Byte sources
// ---------------------------------------------------------------------------

/// Random-access read surface. Implemented for a `File` and for in-memory
/// fixtures, so tests exercise exactly the production code path.
pub trait Source {
    fn len(&self) -> u64;
    fn read_at(&self, offset: u64, len: usize) -> io::Result<Vec<u8>>;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// In-memory source (tests, and the output validators of `ARCHITECTURE.md` §6.0.5).
pub struct Bytes<'a>(pub &'a [u8]);

impl Source for Bytes<'_> {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        let start = match usize::try_from(offset) {
            Ok(v) => v,
            Err(_) => return Ok(Vec::new()),
        };
        if start >= self.0.len() {
            return Ok(Vec::new());
        }
        let end = start.saturating_add(len).min(self.0.len());
        Ok(self.0[start..end].to_vec())
    }
}

/// File-backed source. Reads are positional and short-read safe.
pub struct FileSource {
    file: File,
    len: u64,
}

impl FileSource {
    pub fn open(path: &Path) -> io::Result<FileSource> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        Ok(FileSource { file, len })
    }
}

impl Source for FileSource {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        let mut f = &self.file;
        f.seek(SeekFrom::Start(offset))?;
        let mut read = 0;
        while read < buf.len() {
            match f.read(&mut buf[read..]) {
                Ok(0) => break,
                Ok(n) => read += n,
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        buf.truncate(read);
        Ok(buf)
    }
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Detect a file on disk. Never returns `Result`: an I/O failure is a verdict
/// (`Integrity::Unreadable`), because the wheel needs to render *something*.
pub fn detect_path(path: &Path) -> Detected {
    let claimed = path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(Claimed::from_file_name);
    match FileSource::open(path) {
        Ok(src) => detect_source(&src, claimed),
        Err(e) => Detected::unknown(Integrity::Unreadable, claimed, Detail::Io(io_kind_name(&e))),
    }
}

/// Detect from memory. `claimed` is the extension the name asserted, if any.
pub fn detect_bytes(data: &[u8], claimed: Option<Claimed>) -> Detected {
    detect_source(&Bytes(data), claimed)
}

/// Convenience for tests and the CLI: detect from memory given a file *name*.
pub fn detect_bytes_named(data: &[u8], file_name: Option<&str>) -> Detected {
    let claimed = file_name.and_then(Claimed::from_file_name);
    detect_bytes(data, claimed)
}

fn io_kind_name(e: &io::Error) -> &'static str {
    match e.kind() {
        io::ErrorKind::NotFound => "io: not found",
        io::ErrorKind::PermissionDenied => "io: permission denied",
        io::ErrorKind::UnexpectedEof => "io: unexpected end of file",
        _ => "io: read failed",
    }
}

// ---------------------------------------------------------------------------
// The one detection pipeline
// ---------------------------------------------------------------------------

/// Core pipeline: one bounded prefix + one bounded tail, then classification.
pub fn detect_source<S: Source + ?Sized>(src: &S, claimed: Option<Claimed>) -> Detected {
    let len = src.len();

    if len == 0 {
        return Detected::unknown(Integrity::Complete, claimed, Detail::Empty);
    }
    if len > MAX_INPUT_BYTES {
        return Detected::unknown(
            Integrity::Unreadable,
            claimed,
            Detail::TooLarge {
                seen: len,
                limit: MAX_INPUT_BYTES,
            },
        );
    }

    let prefix_len = usize::try_from(len.min(PREFIX_LEN as u64)).unwrap_or(PREFIX_LEN);
    let prefix = match src.read_at(0, prefix_len) {
        Ok(v) => v,
        Err(e) => {
            return Detected::unknown(Integrity::Unreadable, claimed, Detail::Io(io_kind_name(&e)))
        }
    };
    let tail_len = usize::try_from(len.min(TAIL_LEN as u64)).unwrap_or(TAIL_LEN);
    let tail_off = len - tail_len as u64;
    let tail = match src.read_at(tail_off, tail_len) {
        Ok(v) => v,
        Err(e) => {
            return Detected::unknown(Integrity::Unreadable, claimed, Detail::Io(io_kind_name(&e)))
        }
    };

    let (ty, integrity, detail) = classify(src, len, &prefix, &tail);
    // Rule 5, pixels: a header that *declares* more than the cap is refused
    // before a decoder sees the file. Prefix-only, so this is one small read.
    if integrity != Integrity::Unreadable {
        if let Some((width, height)) = declared_pixels(ty, &prefix) {
            if width.saturating_mul(height) > MAX_DECLARED_PIXELS {
                return Detected::unknown(
                    Integrity::Unreadable,
                    claimed,
                    Detail::TooManyPixels {
                        width,
                        height,
                        limit: MAX_DECLARED_PIXELS,
                    },
                );
            }
        }
    }
    Detected {
        ty,
        integrity,
        claimed,
        detail,
    }
}

/// Signature constants. One place, so the doc table and the code cannot drift.
mod sig {
    pub const ZIP_LOCAL: &[u8] = b"PK\x03\x04";
    pub const ZIP_EMPTY: &[u8] = b"PK\x05\x06";
    pub const ZIP_SPANNED: &[u8] = b"PK\x07\x08";
    pub const ZIP_EOCD: &[u8] = b"PK\x05\x06";
    pub const ZIP_CD: u32 = 0x0201_4b50;
    pub const CFB: &[u8] = &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    pub const PDF: &[u8] = b"%PDF-";
    pub const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF];
    pub const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    pub const GIF87: &[u8] = b"GIF87a";
    pub const GIF89: &[u8] = b"GIF89a";
    pub const TIFF_LE: &[u8] = b"II*\x00";
    pub const TIFF_BE: &[u8] = b"MM\x00*";
    pub const RIFF: &[u8] = b"RIFF";
    pub const BMP: &[u8] = b"BM";
    pub const FTYP: &[u8] = b"ftyp";
    pub const EBML: &[u8] = &[0x1A, 0x45, 0xDF, 0xA3];
    pub const OGG: &[u8] = b"OggS";
    pub const FLAC: &[u8] = b"fLaC";
    pub const ID3: &[u8] = b"ID3";
    pub const RTF: &[u8] = b"{\\rtf";
}

fn starts_with(hay: &[u8], needle: &[u8]) -> bool {
    hay.len() >= needle.len() && &hay[..needle.len()] == needle
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || hay.len() < needle.len() {
        return false;
    }
    hay.windows(needle.len()).any(|w| w == needle)
}

fn rfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    (0..=hay.len() - needle.len())
        .rev()
        .find(|&i| &hay[i..i + needle.len()] == needle)
}

/// Fixed precedence, documented in `docs/DETECTION.md` ("Precedence").
///
/// Containers first (they need structure reads), then formats with a strong
/// magic, then audio frames, then text. A polyglot is classified by this order
/// and never by "whichever parse succeeds".
fn classify<S: Source + ?Sized>(
    src: &S,
    len: u64,
    prefix: &[u8],
    tail: &[u8],
) -> (CanonicalType, Integrity, Detail) {
    // ---- 1. OOXML / ZIP containers ----------------------------------------
    if starts_with(prefix, sig::ZIP_LOCAL)
        || starts_with(prefix, sig::ZIP_EMPTY)
        || starts_with(prefix, sig::ZIP_SPANNED)
    {
        return classify_zip(src, len, tail);
    }

    // ---- 2. OLE2 / CFB (legacy Office) ------------------------------------
    if starts_with(prefix, sig::CFB) {
        return classify_cfb(src, prefix);
    }

    // ---- 3. Portable Document Format ---------------------------------------
    if starts_with(prefix, sig::PDF) {
        return (
            CanonicalType::Pdf,
            if rfind(tail, b"%%EOF").is_some() {
                Integrity::Complete
            } else {
                Integrity::Truncated
            },
            if rfind(tail, b"%%EOF").is_some() {
                Detail::Signature("%PDF- + %%EOF")
            } else {
                Detail::Truncated("pdf: no %%EOF trailer")
            },
        );
    }

    // ---- 4. JPEG -----------------------------------------------------------
    if starts_with(prefix, sig::JPEG) {
        let complete = rfind(tail, &[0xFF, 0xD9]).is_some();
        return (
            CanonicalType::Jpeg,
            if complete {
                Integrity::Complete
            } else {
                Integrity::Truncated
            },
            if complete {
                Detail::Signature("jpeg SOI/EOI")
            } else {
                Detail::Truncated("jpeg: no EOI marker")
            },
        );
    }

    // ---- 5. PNG ------------------------------------------------------------
    if starts_with(prefix, sig::PNG) {
        let complete = rfind(tail, b"IEND").is_some();
        return (
            CanonicalType::Png,
            if complete {
                Integrity::Complete
            } else {
                Integrity::Truncated
            },
            if complete {
                Detail::Signature("png signature/IEND")
            } else {
                Detail::Truncated("png: no IEND chunk")
            },
        );
    }

    // ---- 6. GIF ------------------------------------------------------------
    if starts_with(prefix, sig::GIF87) || starts_with(prefix, sig::GIF89) {
        let complete = tail.last() == Some(&0x3B);
        return (
            CanonicalType::Gif,
            if complete {
                Integrity::Complete
            } else {
                Integrity::Truncated
            },
            if complete {
                Detail::Signature("gif87a/89a + trailer")
            } else {
                Detail::Truncated("gif: no 0x3B trailer")
            },
        );
    }

    // ---- 7. TIFF -----------------------------------------------------------
    if starts_with(prefix, sig::TIFF_LE) || starts_with(prefix, sig::TIFF_BE) {
        // No cheap end marker; a truncated TIFF is caught at decode (V9/V10).
        return (
            CanonicalType::Tiff,
            Integrity::Complete,
            Detail::Signature("tiff II*/MM* header"),
        );
    }

    // ---- 8. RIFF containers: WebP, WAV ------------------------------------
    if starts_with(prefix, sig::RIFF) && prefix.len() >= 12 {
        if &prefix[8..12] == b"WEBP" {
            return (
                CanonicalType::WebP,
                Integrity::Complete,
                Detail::Signature("RIFF/WEBP"),
            );
        }
        if &prefix[8..12] == b"WAVE" {
            return (
                CanonicalType::Wav,
                Integrity::Complete,
                Detail::Signature("RIFF/WAVE"),
            );
        }
    }

    // ---- 9. BMP ------------------------------------------------------------
    if starts_with(prefix, sig::BMP) && prefix.len() >= 18 {
        let dib = u32::from_le_bytes([prefix[14], prefix[15], prefix[16], prefix[17]]);
        if matches!(dib, 12 | 16 | 40 | 52 | 56 | 64 | 108 | 124) {
            return (
                CanonicalType::Bmp,
                Integrity::Complete,
                Detail::Signature("bmp BM + DIB header"),
            );
        }
    }

    // ---- 10. ISOBMFF: HEIC, AVIF, MP4, MOV, M4A ---------------------------
    if prefix.len() >= 12 && &prefix[4..8] == sig::FTYP {
        return classify_isobmff(prefix);
    }

    // ---- 11. EBML: MKV, WebM ---------------------------------------------
    if starts_with(prefix, sig::EBML) {
        let head = &prefix[..prefix.len().min(1024)];
        if contains(head, b"webm") {
            return (
                CanonicalType::WebM,
                Integrity::Complete,
                Detail::Brand("ebml doctype=webm"),
            );
        }
        if contains(head, b"matroska") {
            return (
                CanonicalType::Mkv,
                Integrity::Complete,
                Detail::Brand("ebml doctype=matroska"),
            );
        }
        return (
            CanonicalType::Unknown,
            Integrity::Complete,
            Detail::Unsupported("ebml: doctype not found in prefix"),
        );
    }

    // ---- 12. Ogg (Opus, else Vorbis/other) -------------------------------
    if starts_with(prefix, sig::OGG) {
        let head = &prefix[..prefix.len().min(256)];
        if contains(head, b"OpusHead") {
            return (
                CanonicalType::Opus,
                Integrity::Complete,
                Detail::Brand("ogg/OpusHead"),
            );
        }
        return (
            CanonicalType::Ogg,
            Integrity::Complete,
            Detail::Signature("OggS"),
        );
    }

    // ---- 13. FLAC ----------------------------------------------------------
    if starts_with(prefix, sig::FLAC) {
        return (
            CanonicalType::Flac,
            Integrity::Complete,
            Detail::Signature("fLaC"),
        );
    }

    // ---- 14. BOM-bearing text before any frame-sync guess -------------------
    //
    // `FF FE` / `FE FF` is a valid-looking MPEG frame header, and a UTF-16 file
    // starts with exactly that. Text with an explicit BOM wins: it is an
    // unambiguous declaration, whereas a frame sync is a heuristic.
    if starts_with(prefix, &[0xFF, 0xFE]) || starts_with(prefix, &[0xFE, 0xFF]) {
        if let Some(text) = decode_text(prefix) {
            return classify_text(&text);
        }
    }

    // ---- 15. MPEG audio: MP3 and ADTS AAC ---------------------------------
    if let Some((ty, detail)) = classify_mpeg_audio(prefix) {
        return (ty, Integrity::Complete, detail);
    }

    // ---- 16. RTF (before text: it is ASCII) -------------------------------
    if starts_with(prefix, sig::RTF) {
        let ends = tail
            .iter()
            .rev()
            .find(|b| !b.is_ascii_whitespace())
            .map(|b| *b == b'}')
            .unwrap_or(false);
        return (
            CanonicalType::Rtf,
            if ends {
                Integrity::Complete
            } else {
                Integrity::Truncated
            },
            if ends {
                Detail::Signature("{\\rtf")
            } else {
                Detail::Truncated("rtf: no closing brace")
            },
        );
    }

    // ---- 16. Text shapes ---------------------------------------------------
    if let Some(text) = decode_text(prefix) {
        return classify_text(&text);
    }

    // ---- 17. The honest answer --------------------------------------------
    (
        CanonicalType::Unknown,
        Integrity::Complete,
        Detail::Unsupported("no known signature"),
    )
}

// ---------------------------------------------------------------------------
// ZIP / OOXML
// ---------------------------------------------------------------------------

/// Required-part rules from `ARCHITECTURE.md` §7 rule 2.
const DOCX_PARTS: &[&str] = &["[Content_Types].xml", "word/document.xml"];
const XLSX_PARTS: &[&str] = &["xl/workbook.xml"];
const PPTX_PARTS: &[&str] = &["ppt/presentation.xml"];

fn classify_zip<S: Source + ?Sized>(
    src: &S,
    len: u64,
    tail: &[u8],
) -> (CanonicalType, Integrity, Detail) {
    // End of central directory: 22 bytes + comment. Take the last signature in
    // the tail window (a file may legally contain the bytes inside a comment).
    let Some(eocd) = rfind(tail, sig::ZIP_EOCD) else {
        return (
            CanonicalType::Unknown,
            Integrity::Truncated,
            Detail::Truncated("zip: no end-of-central-directory"),
        );
    };
    if tail.len() < eocd + 22 {
        return (
            CanonicalType::Unknown,
            Integrity::Truncated,
            Detail::Truncated("zip: truncated end-of-central-directory"),
        );
    }
    let e = &tail[eocd..];
    let entries = u16::from_le_bytes([e[10], e[11]]);
    let cd_size = u32::from_le_bytes([e[12], e[13], e[14], e[15]]);
    let cd_off = u32::from_le_bytes([e[16], e[17], e[18], e[19]]);

    if entries == 0xFFFF || cd_size == 0xFFFF_FFFF || cd_off == 0xFFFF_FFFF {
        return (
            CanonicalType::Unknown,
            Integrity::Complete,
            Detail::Unsupported("zip: zip64 directory (not parsed)"),
        );
    }
    if usize::from(entries) > MAX_CONTAINER_ENTRIES {
        return (
            CanonicalType::Unknown,
            Integrity::Complete,
            Detail::Unsupported("zip: directory entry cap exceeded"),
        );
    }
    if u64::from(cd_off) + u64::from(cd_size) > len {
        return (
            CanonicalType::Unknown,
            Integrity::Truncated,
            Detail::Truncated("zip: central directory outside the file"),
        );
    }

    let want = usize::try_from(cd_size).unwrap_or(MAX_STRUCTURE_BYTES);
    let want = want.min(MAX_STRUCTURE_BYTES);
    let cd = match src.read_at(u64::from(cd_off), want) {
        Ok(v) => v,
        Err(e) => {
            return (
                CanonicalType::Unknown,
                Integrity::Unreadable,
                Detail::Io(io_kind_name(&e)),
            )
        }
    };
    if cd.len() < usize::try_from(cd_size).unwrap_or(0) {
        return (
            CanonicalType::Unknown,
            Integrity::Truncated,
            Detail::Truncated("zip: central directory short read"),
        );
    }

    let mut has_types = false;
    let mut has_word = false;
    let mut has_xl = false;
    let mut has_ppt = false;
    let mut declared: u64 = 0;
    let mut off = 0usize;
    let mut seen = 0usize;

    while off + 46 <= cd.len() && seen < MAX_CONTAINER_ENTRIES {
        let sig = u32::from_le_bytes([cd[off], cd[off + 1], cd[off + 2], cd[off + 3]]);
        if sig != sig::ZIP_CD {
            break;
        }
        let uncompressed =
            u32::from_le_bytes([cd[off + 24], cd[off + 25], cd[off + 26], cd[off + 27]]);
        let name_len = usize::from(u16::from_le_bytes([cd[off + 28], cd[off + 29]]));
        let extra_len = usize::from(u16::from_le_bytes([cd[off + 30], cd[off + 31]]));
        let comment_len = usize::from(u16::from_le_bytes([cd[off + 32], cd[off + 33]]));

        let name_end = off + 46 + name_len;
        if name_end > cd.len() {
            break;
        }
        let name = &cd[off + 46..name_end];
        declared = declared.saturating_add(u64::from(uncompressed));

        if name == DOCX_PARTS[0].as_bytes() {
            has_types = true;
        }
        if name == DOCX_PARTS[1].as_bytes() {
            has_word = true;
        }
        if name == XLSX_PARTS[0].as_bytes() {
            has_xl = true;
        }
        if name == PPTX_PARTS[0].as_bytes() {
            has_ppt = true;
        }

        let advance = 46usize
            .saturating_add(name_len)
            .saturating_add(extra_len)
            .saturating_add(comment_len);
        if advance == 0 {
            break;
        }
        off = off.saturating_add(advance);
        seen += 1;
    }

    if declared > MAX_ZIP_TOTAL_UNCOMPRESSED {
        return (
            CanonicalType::Unknown,
            Integrity::Complete,
            Detail::Unsupported("zip: declared payload over the bomb guard"),
        );
    }

    if has_types && has_word {
        return (
            CanonicalType::Docx,
            Integrity::Complete,
            Detail::ZipParts(DOCX_PARTS),
        );
    }
    if has_xl {
        return (
            CanonicalType::Xlsx,
            Integrity::Complete,
            Detail::ZipParts(XLSX_PARTS),
        );
    }
    if has_ppt {
        return (
            CanonicalType::Pptx,
            Integrity::Complete,
            Detail::ZipParts(PPTX_PARTS),
        );
    }
    (
        CanonicalType::Unknown,
        Integrity::Complete,
        Detail::ZipWithoutOfficePart,
    )
}

// ---------------------------------------------------------------------------
// Compound File Binary (legacy .doc/.xls/.ppt)
// ---------------------------------------------------------------------------

const CFB_ENTRY_CAP: usize = MAX_CONTAINER_ENTRIES;
const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
const FREE_SECTOR: u32 = 0xFFFF_FFFF;

fn classify_cfb<S: Source + ?Sized>(src: &S, prefix: &[u8]) -> (CanonicalType, Integrity, Detail) {
    if prefix.len() < 76 {
        return (
            CanonicalType::Unknown,
            Integrity::Truncated,
            Detail::Truncated("cfb: header short"),
        );
    }
    let sector_shift = u16::from_le_bytes([prefix[30], prefix[31]]);
    let sector_size: usize = match sector_shift {
        9 => 512,
        12 => 4096,
        _ => {
            return (
                CanonicalType::Unknown,
                Integrity::Complete,
                Detail::Unsupported("cfb: unsupported sector size"),
            )
        }
    };
    let first_dir = u32::from_le_bytes([prefix[48], prefix[49], prefix[50], prefix[51]]);
    let num_difat = u32::from_le_bytes([prefix[72], prefix[73], prefix[74], prefix[75]]);
    let first_difat = u32::from_le_bytes([prefix[68], prefix[69], prefix[70], prefix[71]]);

    // --- FAT sector list: 109 inline DIFAT entries, then the DIFAT chain.
    let mut fat_sectors: Vec<u32> = Vec::new();
    for i in 0..109 {
        let at = 76 + i * 4;
        if at + 4 > prefix.len() {
            break;
        }
        let s = u32::from_le_bytes([prefix[at], prefix[at + 1], prefix[at + 2], prefix[at + 3]]);
        if s != FREE_SECTOR {
            fat_sectors.push(s);
        }
    }
    let entries_per_difat = sector_size / 4 - 1;
    let mut difat = first_difat;
    let mut guard = 0u32;
    while difat != END_OF_CHAIN && difat != FREE_SECTOR && guard < num_difat.min(1024) {
        let off = u64::from(difat.saturating_add(1)) * sector_size as u64;
        let Ok(sec) = src.read_at(off, sector_size) else {
            break;
        };
        if sec.len() < sector_size {
            break;
        }
        for i in 0..entries_per_difat {
            let at = i * 4;
            let s = u32::from_le_bytes([sec[at], sec[at + 1], sec[at + 2], sec[at + 3]]);
            if s != FREE_SECTOR {
                fat_sectors.push(s);
            }
        }
        let at = entries_per_difat * 4;
        difat = u32::from_le_bytes([sec[at], sec[at + 1], sec[at + 2], sec[at + 3]]);
        guard += 1;
    }
    if fat_sectors.len() * sector_size > MAX_STRUCTURE_BYTES {
        return (
            CanonicalType::Unknown,
            Integrity::Complete,
            Detail::Unsupported("cfb: FAT too large"),
        );
    }

    let mut fat: Vec<u32> = Vec::new();
    for s in &fat_sectors {
        let off = u64::from(s.saturating_add(1)) * sector_size as u64;
        match src.read_at(off, sector_size) {
            Ok(sec) if sec.len() == sector_size => {
                for c in sec.chunks_exact(4) {
                    fat.push(u32::from_le_bytes([c[0], c[1], c[2], c[3]]));
                }
            }
            _ => {
                return (
                    CanonicalType::Unknown,
                    Integrity::Truncated,
                    Detail::Truncated("cfb: FAT sector unreadable"),
                )
            }
        }
    }
    if fat.is_empty() {
        return (
            CanonicalType::Unknown,
            Integrity::Truncated,
            Detail::Truncated("cfb: empty FAT"),
        );
    }

    // --- Directory chain.
    let mut found: Vec<&'static str> = Vec::new();
    let mut sector = first_dir;
    let mut visited: Vec<u32> = Vec::new();
    let mut dir_entries = 0usize;

    while sector != END_OF_CHAIN && sector != FREE_SECTOR {
        if visited.contains(&sector) || visited.len() > 4096 {
            return (
                CanonicalType::Unknown,
                Integrity::Truncated,
                Detail::Truncated("cfb: directory chain loops"),
            );
        }
        visited.push(sector);
        let off = u64::from(sector.saturating_add(1)) * sector_size as u64;
        let Ok(sec) = src.read_at(off, sector_size) else {
            break;
        };
        if sec.len() < sector_size {
            return (
                CanonicalType::Unknown,
                Integrity::Truncated,
                Detail::Truncated("cfb: directory sector short"),
            );
        }
        for entry in sec.chunks_exact(128) {
            if dir_entries >= CFB_ENTRY_CAP {
                break;
            }
            dir_entries += 1;
            let name_len = usize::from(u16::from_le_bytes([entry[64], entry[65]]));
            let kind = entry[66];
            if kind == 0 || name_len < 2 || name_len > 64 {
                continue;
            }
            let name = decode_utf16le(&entry[..name_len - 2]);
            match name.as_str() {
                "WordDocument" => push_once(&mut found, "WordDocument"),
                "Workbook" | "Book" => push_once(&mut found, "Workbook"),
                "PowerPoint Document" => push_once(&mut found, "PowerPoint Document"),
                _ => {}
            }
        }
        let next = fat.get(sector as usize).copied().unwrap_or(END_OF_CHAIN);
        if next == sector {
            break;
        }
        sector = next;
    }

    if found.contains(&"WordDocument") {
        return (
            CanonicalType::Doc,
            Integrity::Complete,
            Detail::CfbEntries(&["WordDocument"]),
        );
    }
    if found.contains(&"Workbook") {
        return (
            CanonicalType::Xls,
            Integrity::Complete,
            Detail::CfbEntries(&["Workbook"]),
        );
    }
    if found.contains(&"PowerPoint Document") {
        return (
            CanonicalType::Ppt,
            Integrity::Complete,
            Detail::CfbEntries(&["PowerPoint Document"]),
        );
    }
    (
        CanonicalType::Unknown,
        Integrity::Complete,
        Detail::CfbWithoutOfficeEntry,
    )
}

fn push_once(list: &mut Vec<&'static str>, value: &'static str) {
    if !list.contains(&value) {
        list.push(value);
    }
}

fn decode_utf16le(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    char::decode_utf16(units)
        .map(|r| r.unwrap_or('\u{FFFD}'))
        .collect()
}

// ---------------------------------------------------------------------------
// ISOBMFF brands
// ---------------------------------------------------------------------------

fn classify_isobmff(prefix: &[u8]) -> (CanonicalType, Integrity, Detail) {
    if prefix.len() < 16 {
        return (
            CanonicalType::Unknown,
            Integrity::Truncated,
            Detail::Truncated("isobmff: ftyp truncated"),
        );
    }
    let box_size = u32::from_be_bytes([prefix[0], prefix[1], prefix[2], prefix[3]]) as usize;
    let (payload, box_end) = if box_size == 1 {
        if prefix.len() < 24 {
            return (
                CanonicalType::Unknown,
                Integrity::Truncated,
                Detail::Truncated("isobmff: largesize truncated"),
            );
        }
        let large = u64::from_be_bytes([
            prefix[8], prefix[9], prefix[10], prefix[11], prefix[12], prefix[13], prefix[14],
            prefix[15],
        ]);
        let end = usize::try_from(large)
            .unwrap_or(prefix.len())
            .min(prefix.len());
        (16usize, end)
    } else if box_size >= 16 {
        (8usize, box_size.min(prefix.len()))
    } else {
        (8usize, prefix.len().min(16 + 4 * 16))
    };

    let mut brands: Vec<[u8; 4]> = Vec::new();
    let mut at = payload;
    while at + 4 <= box_end {
        brands.push([prefix[at], prefix[at + 1], prefix[at + 2], prefix[at + 3]]);
        at += 4;
    }
    if brands.is_empty() {
        return (
            CanonicalType::Unknown,
            Integrity::Complete,
            Detail::Unsupported("isobmff: ftyp without brands"),
        );
    }

    let has = |b: &[u8; 4]| brands.iter().any(|x| x == b);
    let b = |s: &[u8; 4]| has(s);

    // AVIF wins over any HEIF brand: `avis` = AVIF image sequence.
    if b(b"avif") || b(b"avis") {
        return (
            CanonicalType::Avif,
            Integrity::Complete,
            Detail::Brand("ftyp avif"),
        );
    }
    if b(b"heic") || b(b"heix") || b(b"hevc") || b(b"hevx") || b(b"heim") || b(b"heis") {
        return (
            CanonicalType::Heic,
            Integrity::Complete,
            Detail::Brand("ftyp heic family"),
        );
    }
    if b(b"M4A ") || b(b"M4B ") || b(b"M4P ") {
        return (
            CanonicalType::M4a,
            Integrity::Complete,
            Detail::Brand("ftyp M4A"),
        );
    }
    if b(b"qt  ") {
        return (
            CanonicalType::Mov,
            Integrity::Complete,
            Detail::Brand("ftyp qt"),
        );
    }
    for brand in [
        b"isom", b"iso2", b"iso4", b"iso5", b"iso6", b"mp41", b"mp42", b"avc1", b"dash", b"MSNV",
        b"M4V ", b"F4V ",
    ] {
        if b(brand) {
            return (
                CanonicalType::Mp4,
                Integrity::Complete,
                Detail::Brand("ftyp isom family"),
            );
        }
    }
    if b(b"mif1") || b(b"msf1") {
        // HEIF base brand without a codec hint: could be HEVC or AV1 payload.
        return (
            CanonicalType::Unknown,
            Integrity::Complete,
            Detail::Unsupported("heif: base brand without a codec hint"),
        );
    }
    (
        CanonicalType::Unknown,
        Integrity::Complete,
        Detail::Unsupported("isobmff: unrecognised brand"),
    )
}

// ---------------------------------------------------------------------------
// MPEG audio
// ---------------------------------------------------------------------------

fn classify_mpeg_audio(prefix: &[u8]) -> Option<(CanonicalType, Detail)> {
    if starts_with(prefix, sig::ID3) {
        return Some((CanonicalType::Mp3, Detail::Signature("ID3v2 tag")));
    }
    // Only the start of the file. Scanning for a bare sync pattern inside
    // arbitrary bytes is how binary files become "MP3" — measured in review:
    // a NUL-laced blob carrying FF FE at offset 21 was classified Mp3.
    if prefix.len() < 4 || prefix[0] != 0xFF || (prefix[1] & 0xE0) != 0xE0 {
        return None;
    }
    let version = (prefix[1] >> 3) & 0x03;
    let layer = (prefix[1] >> 1) & 0x03;
    let bitrate = (prefix[2] >> 4) & 0x0F;
    let rate = (prefix[2] >> 2) & 0x03;
    if version == 1 || bitrate == 0x0F || rate == 0x03 {
        return None;
    }
    // layer bits 00 = no MPEG layer → ADTS AAC.
    Some(if layer == 0 {
        (CanonicalType::Aac, Detail::Signature("adts aac sync"))
    } else {
        (
            CanonicalType::Mp3,
            Detail::Signature("mpeg audio frame sync"),
        )
    })
}

// ---------------------------------------------------------------------------
// Declared geometry (rule 5, pixels)
// ---------------------------------------------------------------------------

/// Pixels a still image's **header** claims, read from the 64 KiB prefix only.
///
/// `None` means the header does not state its geometry inside the prefix (or the
/// format has no cheap prefix-only geometry, e.g. HEIC/AVIF/SVG). That is a named
/// gap, not a pass: such a file still meets the byte cap and the engine kill.
/// No decoder runs here and nothing is allocated per pixel.
fn declared_pixels(ty: CanonicalType, prefix: &[u8]) -> Option<(u64, u64)> {
    match ty {
        CanonicalType::Png => png_pixels(prefix),
        CanonicalType::Gif => gif_pixels(prefix),
        CanonicalType::Tiff => tiff_pixels(prefix),
        CanonicalType::Bmp => bmp_pixels(prefix),
        CanonicalType::WebP => webp_pixels(prefix),
        CanonicalType::Jpeg => jpeg_pixels(prefix),
        _ => None,
    }
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at + 4)?;
    Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

/// PNG: the first chunk of a valid stream is IHDR, whose payload opens with
/// width and height as big-endian u32 (bytes 16..24 of the file).
fn png_pixels(p: &[u8]) -> Option<(u64, u64)> {
    if p.len() < 24 || &p[12..16] != b"IHDR" {
        return None;
    }
    Some((u64::from(be32(p, 16)?), u64::from(be32(p, 20)?)))
}

/// GIF: the logical screen is two little-endian u16 at bytes 6..10. Frames can
/// exceed the canvas, so this is a floor for animated files, which is what a
/// cap needs to be.
fn gif_pixels(p: &[u8]) -> Option<(u64, u64)> {
    if p.len() < 10 {
        return None;
    }
    Some((
        u64::from(u16::from_le_bytes([p[6], p[7]])),
        u64::from(u16::from_le_bytes([p[8], p[9]])),
    ))
}

/// BMP: the DIB header carries signed width/height at bytes 18..26; a negative
/// height means top-down rows, so the absolute value is the geometry.
fn bmp_pixels(p: &[u8]) -> Option<(u64, u64)> {
    if p.len() < 26 {
        return None;
    }
    let w = i32::from_le_bytes([p[18], p[19], p[20], p[21]]);
    let h = i32::from_le_bytes([p[22], p[23], p[24], p[25]]);
    Some((u64::from(w.unsigned_abs()), u64::from(h.unsigned_abs())))
}

/// WebP: only the extended (VP8X) form states the canvas up front, as two
/// 24-bit little-endian `size - 1` fields at bytes 24..30. A plain lossy VP8
/// stream keeps its geometry inside the first frame header, which this does not
/// walk — reported as a gap instead of a guess.
fn webp_pixels(p: &[u8]) -> Option<(u64, u64)> {
    if p.len() < 30 || &p[12..16] != b"VP8X" {
        return None;
    }
    let w = 1 + (u64::from(p[24]) | (u64::from(p[25]) << 8) | (u64::from(p[26]) << 16));
    let h = 1 + (u64::from(p[27]) | (u64::from(p[28]) << 8) | (u64::from(p[29]) << 16));
    Some((w, h))
}

/// TIFF: byte-order magic, then the first IFD, whose `ImageWidth` (256) and
/// `ImageLength` (257) entries hold the geometry. Only the first IFD is read
/// and only entries that fit inside the prefix; a malformed offset ends the
/// search instead of following it.
fn tiff_pixels(p: &[u8]) -> Option<(u64, u64)> {
    if p.len() < 8 {
        return None;
    }
    let le = match &p[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let at16 = |o: usize| -> Option<u16> {
        let s = p.get(o..o + 2)?;
        Some(if le {
            u16::from_le_bytes([s[0], s[1]])
        } else {
            u16::from_be_bytes([s[0], s[1]])
        })
    };
    let at32 = |o: usize| -> Option<u32> {
        let s = p.get(o..o + 4)?;
        Some(if le {
            u32::from_le_bytes([s[0], s[1], s[2], s[3]])
        } else {
            u32::from_be_bytes([s[0], s[1], s[2], s[3]])
        })
    };
    if at16(2)? != 42 {
        return None;
    }
    let ifd = usize::try_from(at32(4)?).ok()?;
    let count = usize::from(at16(ifd)?).min(512);
    let mut width = None;
    let mut height = None;
    for i in 0..count {
        let entry = ifd.checked_add(2 + i * 12)?;
        let tag = at16(entry)?;
        if tag != 256 && tag != 257 {
            continue;
        }
        let value = match at16(entry + 2)? {
            // SHORT values sit in the first two bytes of the value field.
            3 => u64::from(at16(entry + 8)?),
            4 => u64::from(at32(entry + 8)?),
            _ => continue,
        };
        if tag == 256 {
            width = Some(value);
        } else {
            height = Some(value);
        }
        if width.is_some() && height.is_some() {
            break;
        }
    }
    Some((width?, height?))
}

/// JPEG: walk the marker chain (never the entropy-coded data, because a frame
/// header always precedes it) and read the frame header's height/width. Any
/// desync — a byte that is not a marker where one must be, or a segment that
/// runs past the prefix — ends the walk with `None`.
fn jpeg_pixels(p: &[u8]) -> Option<(u64, u64)> {
    let mut i = 2usize; // past SOI
    while i + 1 < p.len() {
        if p[i] != 0xFF {
            return None;
        }
        let marker = p[i + 1];
        // SOF0..SOF15 minus DHT (C4), JPG (C8) and DAC (CC).
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let s = p.get(i + 5..i + 9)?;
            let h = u64::from(u16::from_be_bytes([s[0], s[1]]));
            let w = u64::from(u16::from_be_bytes([s[2], s[3]]));
            return Some((w, h));
        }
        // Standalone markers carry no length.
        if marker == 0x01 || marker == 0xD8 || (0xD0..=0xD7).contains(&marker) {
            i += 2;
            continue;
        }
        let seg = p.get(i + 2..i + 4)?;
        let len = usize::from(u16::from_be_bytes([seg[0], seg[1]]));
        if len < 2 {
            return None;
        }
        i = i.checked_add(2 + len)?;
    }
    None
}

// ---------------------------------------------------------------------------
// Text shapes
// ---------------------------------------------------------------------------

/// Decode the prefix as text, tolerating a multi-byte character cut at the
/// 64 KiB boundary. Returns `None` for anything with NULs or control bytes.
fn decode_text(data: &[u8]) -> Option<String> {
    let text = if data.starts_with(&[0xFF, 0xFE]) {
        let body = &data[2..];
        let units: Vec<u16> = body
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        char::decode_utf16(units)
            .map(|r| r.unwrap_or('\u{FFFD}'))
            .collect()
    } else if data.starts_with(&[0xFE, 0xFF]) {
        let body = &data[2..];
        let units: Vec<u16> = body
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        char::decode_utf16(units)
            .map(|r| r.unwrap_or('\u{FFFD}'))
            .collect()
    } else {
        match std::str::from_utf8(data) {
            Ok(s) => s.to_string(),
            Err(e) if e.error_len().is_none() => {
                // Truncated final character at the prefix boundary.
                std::str::from_utf8(&data[..e.valid_up_to()])
                    .ok()?
                    .to_string()
            }
            Err(_) => return None,
        }
    };

    if text.is_empty() {
        return None;
    }
    let mut control = 0usize;
    for ch in text.chars() {
        if ch == '\u{0}' {
            return None;
        }
        if ch < ' ' && !matches!(ch, '\t' | '\n' | '\r' | '\u{0b}' | '\u{0c}') {
            control += 1;
        }
    }
    // A few stray control bytes are tolerated; a binary blob is not.
    if control * 100 > text.chars().count().max(1) * 2 {
        return None;
    }
    Some(text)
}

/// Name of the first element-ish tag, skipping XML declaration, comments and
/// doctype. Used only by the text heuristics.
fn first_element_name(s: &str) -> Option<String> {
    let mut rest = s.trim_start_matches('\u{feff}').trim_start();
    let mut hops = 0;
    while hops < 8 {
        hops += 1;
        if let Some(after) = rest.strip_prefix("<?") {
            rest = &after[after.find("?>")? + 2..];
            continue;
        }
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = &after[after.find("-->")? + 3..];
            continue;
        }
        if let Some(after) = rest.strip_prefix("<!") {
            rest = &after[after.find('>')? + 1..];
            continue;
        }
        if let Some(after) = rest.strip_prefix('<') {
            let name: String = after
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == ':' || *c == '-' || *c == '_')
                .collect();
            if name.is_empty() {
                return None;
            }
            return Some(name.to_ascii_lowercase());
        }
        return None;
    }
    None
}

fn looks_like_svg(text: &str) -> bool {
    if first_element_name(text).as_deref() == Some("svg") {
        return true;
    }
    let head: String = text.chars().take(4096).collect();
    head.contains("http://www.w3.org/2000/svg")
}

fn looks_like_html(text: &str) -> bool {
    let head: String = text
        .chars()
        .take(4096)
        .collect::<String>()
        .to_ascii_lowercase();
    head.contains("<!doctype html") || first_element_name(&head).as_deref() == Some("html")
}

fn looks_like_csv(text: &str) -> bool {
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(50)
        .collect();
    if lines.len() < 2 {
        return false;
    }
    if lines.iter().any(|l| l.chars().count() > 4096) {
        return false;
    }
    for delim in [',', ';', '\t', '|'] {
        let counts: Vec<usize> = lines
            .iter()
            .map(|l| count_outside_quotes(l, delim))
            .collect();
        let first = counts[0];
        let stable = first >= 1 && counts.iter().all(|c| *c == first);
        if !stable {
            continue;
        }
        // A single delimiter per line is a sentence with a comma far more often
        // than it is a two-column table, unless the file is clearly a list.
        if first >= 2 || lines.len() >= 3 {
            return true;
        }
    }
    false
}

/// Count `delim` outside double-quoted runs, honouring `""` escapes.
///
/// Quoted fields are why a naive `matches(delim).count()` misclassifies real
/// CSV: `"comma, inside"` adds a comma that belongs to the data.
fn count_outside_quotes(line: &str, delim: char) -> usize {
    let mut count = 0;
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            if in_quotes && chars.peek() == Some(&'"') {
                chars.next(); // escaped quote inside a quoted field
            } else {
                in_quotes = !in_quotes;
            }
        } else if c == delim && !in_quotes {
            count += 1;
        }
    }
    count
}

fn looks_like_markdown(text: &str) -> bool {
    text.lines().take(200).any(|line| {
        let t = line.trim_start();
        if t.starts_with("```") {
            return true;
        }
        if let Some(rest) = t.strip_prefix('#') {
            if rest.starts_with(' ') || rest.starts_with('#') {
                return true;
            }
        }
        if t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ") {
            return true;
        }
        if t.contains("](") && t.contains('[') {
            return true;
        }
        // Ordered list: "1. text"
        let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() && t[digits.len()..].starts_with(". ") {
            return true;
        }
        false
    })
}

fn classify_text(text: &str) -> (CanonicalType, Integrity, Detail) {
    if looks_like_svg(text) {
        (
            CanonicalType::Svg,
            Integrity::Complete,
            Detail::Text(TextShape::Svg),
        )
    } else if looks_like_html(text) {
        (
            CanonicalType::Html,
            Integrity::Complete,
            Detail::Text(TextShape::Html),
        )
    } else if looks_like_csv(text) {
        (
            CanonicalType::Csv,
            Integrity::Complete,
            Detail::Text(TextShape::Csv),
        )
    } else if looks_like_markdown(text) {
        (
            CanonicalType::Markdown,
            Integrity::Complete,
            Detail::Text(TextShape::Markdown),
        )
    } else {
        (
            CanonicalType::Txt,
            Integrity::Complete,
            Detail::Text(TextShape::Plain),
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::test_support::{fixture, read_fixture};
    use super::*;

    /// The corpus table: fixture file → expected type. Every canonical type
    /// except `Unknown` must appear here at least once.
    const CORPUS: &[(&str, CanonicalType)] = &[
        ("sample.jpg", CanonicalType::Jpeg),
        ("sample.png", CanonicalType::Png),
        ("sample.webp", CanonicalType::WebP),
        ("sample.tif", CanonicalType::Tiff),
        ("sample.bmp", CanonicalType::Bmp),
        ("sample.gif", CanonicalType::Gif),
        ("sample.heic", CanonicalType::Heic),
        ("sample.avif", CanonicalType::Avif),
        ("sample.svg", CanonicalType::Svg),
        ("sample.pdf", CanonicalType::Pdf),
        ("sample.mp3", CanonicalType::Mp3),
        ("sample.wav", CanonicalType::Wav),
        ("sample.flac", CanonicalType::Flac),
        ("sample.m4a", CanonicalType::M4a),
        ("sample.aac", CanonicalType::Aac),
        ("sample.ogg", CanonicalType::Ogg),
        ("sample.opus", CanonicalType::Opus),
        ("sample.mp4", CanonicalType::Mp4),
        ("sample.mkv", CanonicalType::Mkv),
        ("sample.mov", CanonicalType::Mov),
        ("sample.webm", CanonicalType::WebM),
        ("sample.docx", CanonicalType::Docx),
        ("sample.xlsx", CanonicalType::Xlsx),
        ("sample.pptx", CanonicalType::Pptx),
        ("sample.doc", CanonicalType::Doc),
        ("sample.xls", CanonicalType::Xls),
        ("sample.ppt", CanonicalType::Ppt),
        ("sample.rtf", CanonicalType::Rtf),
        ("sample.csv", CanonicalType::Csv),
        ("sample.md", CanonicalType::Markdown),
        ("sample.html", CanonicalType::Html),
        ("sample.txt", CanonicalType::Txt),
    ];

    fn detect_file(name: &str) -> Detected {
        let path = fixture(name);
        assert!(path.exists(), "missing fixture: {}", path.display());
        detect_path(&path)
    }

    #[test]
    fn every_corpus_fixture_detects_its_expected_type_completely() {
        for (name, expected) in CORPUS {
            let d = detect_file(name);
            assert_eq!(
                d.ty,
                *expected,
                "{name}: detected {} ({}) — expected {}",
                d.ty,
                d.detail.as_str(),
                expected
            );
            assert_eq!(
                d.integrity,
                Integrity::Complete,
                "{name}: integrity {}",
                d.integrity.as_str()
            );
            assert!(d.is_routable(), "{name} must be routable");
        }
    }

    #[test]
    fn the_corpus_covers_every_canonical_type_except_unknown() {
        use super::super::types::ALL_CANONICAL_TYPES;
        for ty in ALL_CANONICAL_TYPES {
            assert!(
                CORPUS.iter().any(|(_, t)| t == ty),
                "no fixture covers canonical type {ty}"
            );
        }
    }

    // -- declared-pixel cap (Q1 finding F2) --------------------------------

    /// The measured bomb: a 1.55 MB PNG declaring 20,000x20,000. Byte caps do
    /// not see it; the header does.
    fn png_with_dims(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        out.extend_from_slice(&13u32.to_be_bytes());
        out.extend_from_slice(b"IHDR");
        out.extend_from_slice(&w.to_be_bytes());
        out.extend_from_slice(&h.to_be_bytes());
        out.extend_from_slice(&[8, 2, 0, 0, 0]);
        out.extend_from_slice(&[0, 0, 0, 0]); // CRC is not checked here
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(b"IEND");
        out.extend_from_slice(&[0, 0, 0, 0]);
        out
    }

    fn gif_with_dims(w: u16, h: u16) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"GIF89a");
        out.extend_from_slice(&w.to_le_bytes());
        out.extend_from_slice(&h.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0]);
        out.push(0x3B);
        out
    }

    fn tiff_le_with_dims(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"II*\x00");
        out.extend_from_slice(&8u32.to_le_bytes()); // IFD at byte 8
        out.extend_from_slice(&2u16.to_le_bytes()); // two entries
        for (tag, value) in [(256u16, w), (257u16, h)] {
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&4u16.to_le_bytes()); // LONG
            out.extend_from_slice(&1u32.to_le_bytes());
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
        out
    }

    fn jpeg_with_dims(w: u16, h: u16) -> Vec<u8> {
        let mut out = vec![0xFF, 0xD8]; // SOI
        out.extend_from_slice(&[0xFF, 0xC0]); // SOF0
        out.extend_from_slice(&17u16.to_be_bytes());
        out.push(8); // precision
        out.extend_from_slice(&h.to_be_bytes());
        out.extend_from_slice(&w.to_be_bytes());
        out.extend_from_slice(&[3, 1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0]); // 3 components
        out.extend_from_slice(&[0xFF, 0xD9]); // EOI
        out
    }

    fn refused_for_pixels(data: &[u8], name: &str) -> Detected {
        let d = detect_bytes_named(data, Some(name));
        assert_eq!(d.ty, CanonicalType::Unknown, "{name} must not be routed");
        assert_eq!(d.integrity, Integrity::Unreadable);
        assert!(!d.is_routable(), "{name} must offer no targets");
        assert!(
            matches!(d.detail, Detail::TooManyPixels { .. }),
            "{name}: expected TooManyPixels, got {:?}",
            d.detail
        );
        d
    }

    #[test]
    fn a_png_declaring_400_megapixels_is_refused_before_any_decode() {
        let d = refused_for_pixels(&png_with_dims(20_000, 20_000), "pngbomb.png");
        assert_eq!(
            d.detail,
            Detail::TooManyPixels {
                width: 20_000,
                height: 20_000,
                limit: MAX_DECLARED_PIXELS
            }
        );
    }

    #[test]
    fn a_gif_a_tiff_and_a_jpeg_over_the_cap_are_refused_too() {
        refused_for_pixels(&gif_with_dims(65_535, 65_535), "bomb.gif");
        refused_for_pixels(&tiff_le_with_dims(20_000, 20_000), "bomb.tif");
        refused_for_pixels(&jpeg_with_dims(65_535, 65_535), "bomb.jpg");
    }

    #[test]
    fn a_header_that_lies_about_its_geometry_is_refused_without_panicking() {
        // Both axes at u32::MAX: the product must not overflow or wrap into
        // "small enough".
        refused_for_pixels(&png_with_dims(u32::MAX, u32::MAX), "dimlie.png");
        refused_for_pixels(&gif_with_dims(u16::MAX, u16::MAX), "dimlie.gif");
    }

    #[test]
    fn the_cap_is_a_ceiling_not_a_floor() {
        // Exactly at the cap, and ordinary geometry, stay routable: the refusal
        // must not creep down onto real photographs.
        let at_cap = detect_bytes_named(&png_with_dims(20_000, 12_500), Some("panorama.png"));
        assert_eq!(at_cap.ty, CanonicalType::Png, "{:?}", at_cap.detail);
        assert!(at_cap.is_routable());

        let normal = detect_bytes_named(&png_with_dims(4_032, 3_024), Some("photo.png"));
        assert_eq!(normal.ty, CanonicalType::Png);
        assert!(normal.is_routable());
    }

    #[test]
    fn a_header_the_prefix_cannot_hold_is_a_gap_not_a_panic() {
        // A TIFF whose IFD offset points past the prefix, and a JPEG whose SOF
        // segment would run past it: both fall through (byte cap + engine kill
        // still apply) instead of panicking or guessing.
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II*\x00");
        tiff.extend_from_slice(&0x00FF_FFFFu32.to_le_bytes());
        tiff.extend_from_slice(&[0u8; 32]);
        let d = detect_bytes_named(&tiff, Some("far.tif"));
        assert_eq!(d.ty, CanonicalType::Tiff);
        assert!(d.is_routable());

        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xC0, 0xFF, 0xFF, 8, 0x10, 0x00];
        let d = detect_bytes_named(&jpeg, Some("cut.jpg"));
        assert_eq!(d.ty, CanonicalType::Jpeg);
    }

    #[test]
    fn jpeg_renamed_png_resolves_to_jpeg_by_content() {
        let d = detect_file("renamed_jpeg_as.png");
        assert_eq!(d.ty, CanonicalType::Jpeg, "content must win");
        assert_eq!(d.claimed.as_ref().map(|c| c.as_str()), Some("png"));
        assert_eq!(
            d.claimed_matches_content(),
            Some(false),
            "the mismatch must be recorded, not resolved"
        );
        // And the bytes are identical to the honest JPEG fixture, i.e. this is
        // really the same content under a lying name.
        assert_eq!(
            read_fixture("renamed_jpeg_as.png"),
            read_fixture("sample.jpg"),
            "renamed fixture must be byte-identical to sample.jpg"
        );
    }

    #[test]
    fn renamed_jpeg_offers_jpeg_targets_not_png_targets() {
        use super::super::matrix::offered_targets;
        use super::super::types::TargetFormat;
        let renamed = detect_file("renamed_jpeg_as.png");
        let honest = detect_file("sample.jpg");
        assert_eq!(
            offered_targets(renamed.ty),
            offered_targets(honest.ty),
            "a renamed JPEG must offer exactly what a real JPEG offers"
        );
        assert!(
            offered_targets(renamed.ty).contains(&TargetFormat::Png),
            "jpg→png is proven (V2)"
        );
    }

    // -- acceptance case 2: truncated files ---------------------------------

    #[test]
    fn truncated_jpeg_is_flagged_truncated_but_still_jpeg() {
        let d = detect_file("truncated.jpg");
        assert_eq!(d.ty, CanonicalType::Jpeg);
        assert_eq!(d.integrity, Integrity::Truncated);
        assert!(d.is_routable(), "a truncated JPEG may still decode");
        assert!(
            d.detail.as_str().contains("EOI"),
            "detail: {}",
            d.detail.as_str()
        );
    }

    #[test]
    fn truncated_png_is_flagged_truncated() {
        let d = detect_file("truncated.png");
        assert_eq!(d.ty, CanonicalType::Png);
        assert_eq!(d.integrity, Integrity::Truncated);
        assert!(
            d.detail.as_str().contains("IEND"),
            "detail: {}",
            d.detail.as_str()
        );
    }

    #[test]
    fn truncated_docx_is_unknown_and_truncated_never_a_guess() {
        let d = detect_file("truncated.docx");
        assert_eq!(
            d.ty,
            CanonicalType::Unknown,
            "a ZIP without its central directory cannot be validated as a document"
        );
        assert_eq!(d.integrity, Integrity::Truncated);
        assert!(!d.is_routable());
        assert!(
            d.detail.as_str().contains("end-of-central-directory"),
            "detail: {}",
            d.detail.as_str()
        );
    }

    #[test]
    fn truncated_pdf_is_flagged_truncated() {
        let bytes = read_fixture("sample.pdf");
        let cut = &bytes[..bytes.len() / 2];
        let d = detect_bytes(cut, None);
        assert_eq!(d.ty, CanonicalType::Pdf);
        assert_eq!(d.integrity, Integrity::Truncated);
    }

    // -- acceptance case 3: the measured silent-garbage trap ---------------

    #[test]
    fn text_renamed_docx_is_text_not_a_document() {
        let d = detect_file("text_renamed.docx");
        assert_eq!(
            d.ty,
            CanonicalType::Txt,
            "ARCHITECTURE 6.2 N3: a text file named .docx must never reach soffice"
        );
        assert_eq!(d.claimed_matches_content(), Some(false));
        assert_eq!(
            super::super::matrix::offered_targets(d.ty),
            super::super::matrix::offered_targets(CanonicalType::Txt),
            "it must offer text targets, not document targets"
        );
    }

    // -- unknown / empty / unreadable ---------------------------------------

    #[test]
    fn zero_byte_file_is_unknown_and_empty() {
        let d = detect_file("empty.bin");
        assert_eq!(d.ty, CanonicalType::Unknown);
        assert_eq!(d.detail, Detail::Empty);
        assert!(!d.is_routable(), "nothing may be offered for an empty file");
    }

    #[test]
    fn random_bytes_are_unknown_not_guessed() {
        let bytes: Vec<u8> = (0u16..4096)
            .map(|i| (i.wrapping_mul(37) % 251) as u8 | 0x80)
            .collect();
        let d = detect_bytes(&bytes, None);
        assert_eq!(d.ty, CanonicalType::Unknown);
        assert_eq!(d.detail, Detail::Unsupported("no known signature"));
    }

    #[test]
    fn a_zip_without_an_ooxml_part_is_not_a_document() {
        let d = detect_file("plain.zip");
        assert_eq!(d.ty, CanonicalType::Unknown);
        assert_eq!(d.detail, Detail::ZipWithoutOfficePart);
        assert_eq!(d.integrity, Integrity::Complete);
    }

    #[test]
    fn a_cfb_without_an_office_stream_is_not_a_document() {
        let d = detect_file("plain.cfb");
        assert_eq!(d.ty, CanonicalType::Unknown);
        assert_eq!(d.detail, Detail::CfbWithoutOfficeEntry);
    }

    #[test]
    fn missing_path_is_unreadable_not_unknown_silence() {
        let d = detect_path(Path::new("/definitely/not/here/sample.png"));
        assert_eq!(d.integrity, Integrity::Unreadable);
        assert_eq!(d.ty, CanonicalType::Unknown);
        assert!(
            d.detail.as_str().starts_with("io:"),
            "detail: {}",
            d.detail.as_str()
        );
        assert!(!d.is_routable());
    }

    #[test]
    fn directory_path_is_unreadable() {
        let d = detect_path(&fixture(""));
        assert_eq!(d.integrity, Integrity::Unreadable);
    }

    // -- extensions never decide -------------------------------------------

    #[test]
    fn every_fixture_under_a_wrong_extension_still_resolves_by_content() {
        // Take each corpus fixture in memory and hand detection a lying name;
        // content must win every time.
        for (name, expected) in CORPUS {
            let bytes = read_fixture(name);
            for lie in ["png", "docx", "mp4", "txt", "zip"] {
                let d = detect_bytes_named(&bytes, Some(&format!("x.{lie}")));
                assert_eq!(
                    d.ty,
                    *expected,
                    "{name} renamed .{lie} detected as {} ({})",
                    d.ty,
                    d.detail.as_str()
                );
            }
        }
    }

    // -- text heuristics ----------------------------------------------------

    #[test]
    fn text_shapes_resolve_in_the_documented_order() {
        let cases: &[(&[u8], CanonicalType, TextShape)] = &[
            (b"<?xml version=\"1.0\"?>\n<!-- c -->\n<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>\n", CanonicalType::Svg, TextShape::Svg),
            (b"<!DOCTYPE html>\n<html><body><h1>hi</h1></body></html>\n", CanonicalType::Html, TextShape::Html),
            (b"<html><body>no doctype</body></html>\n", CanonicalType::Html, TextShape::Html),
            (b"a,b,c\n1,2,3\n4,5,6\n", CanonicalType::Csv, TextShape::Csv),
            (b"a;b\n1;2\n3;4\n", CanonicalType::Csv, TextShape::Csv),
            (b"# Title\n\nSome *emphasis* and a [link](https://example.com).\n", CanonicalType::Markdown, TextShape::Markdown),
            (b"1. first\n2. second\n3. third\n", CanonicalType::Markdown, TextShape::Markdown),
            (b"just a sentence with no structure at all\nand a second line\n", CanonicalType::Txt, TextShape::Plain),
            (b"", CanonicalType::Unknown, TextShape::Plain),
        ];
        for (bytes, ty, shape) in cases {
            let d = detect_bytes(bytes, None);
            if *ty == CanonicalType::Unknown {
                assert_eq!(d.ty, CanonicalType::Unknown);
                continue;
            }
            assert_eq!(
                d.ty,
                *ty,
                "bytes {:?} → {} ({})",
                String::from_utf8_lossy(bytes),
                d.ty,
                d.detail.as_str()
            );
            if matches!(
                shape,
                TextShape::Svg
                    | TextShape::Html
                    | TextShape::Csv
                    | TextShape::Markdown
                    | TextShape::Plain
            ) {
                assert_eq!(d.detail, Detail::Text(*shape));
            }
        }
    }

    #[test]
    fn csv_needs_stable_delimiter_counts() {
        // Commas in prose do not a CSV make: counts differ per line.
        let prose = b"Hello, world\nThis line has, like, three commas, here\nno commas\n";
        assert_eq!(detect_bytes(prose, None).ty, CanonicalType::Txt);
    }

    #[test]
    fn binary_with_nul_bytes_is_never_text() {
        let mut bytes = b"# looks like markdown\n".to_vec();
        bytes.push(0);
        bytes.extend_from_slice(b"\xff\xfe garbage");
        assert_eq!(detect_bytes(&bytes, None).ty, CanonicalType::Unknown);
    }

    #[test]
    fn utf16_text_with_bom_is_text() {
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "hello\nworld\n".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(detect_bytes(&bytes, None).ty, CanonicalType::Txt);
    }

    #[test]
    fn a_multibyte_character_cut_at_the_prefix_boundary_is_tolerated() {
        let mut bytes = vec![b'a'; PREFIX_LEN - 1];
        bytes.extend_from_slice("é".as_bytes()); // 2 bytes, second one is cut off
        let d = detect_bytes(&bytes, None);
        assert_eq!(d.ty, CanonicalType::Txt, "detail: {}", d.detail.as_str());
    }

    // -- signature precedence ----------------------------------------------

    #[test]
    fn avif_brand_wins_over_a_heif_compatible_brand() {
        let mut ftyp = vec![0u8, 0, 0, 24];
        ftyp.extend_from_slice(b"ftyp");
        ftyp.extend_from_slice(b"mif1");
        ftyp.extend_from_slice(&[0, 0, 0, 0]);
        ftyp.extend_from_slice(b"mif1");
        ftyp.extend_from_slice(b"avif");
        let d = detect_bytes(&ftyp, None);
        assert_eq!(d.ty, CanonicalType::Avif);
    }

    #[test]
    fn heif_base_brand_without_a_codec_hint_is_unknown() {
        let mut ftyp = vec![0u8, 0, 0, 20];
        ftyp.extend_from_slice(b"ftyp");
        ftyp.extend_from_slice(b"mif1");
        ftyp.extend_from_slice(&[0, 0, 0, 0]);
        ftyp.extend_from_slice(b"mif1");
        let d = detect_bytes(&ftyp, None);
        assert_eq!(d.ty, CanonicalType::Unknown);
        assert_eq!(
            d.detail,
            Detail::Unsupported("heif: base brand without a codec hint")
        );
    }

    #[test]
    fn ebml_without_a_doctype_is_unknown_not_a_guess() {
        let mut bytes = vec![0x1A, 0x45, 0xDF, 0xA3, 0x80];
        bytes.extend(std::iter::repeat(0u8).take(64));
        let d = detect_bytes(&bytes, None);
        assert_eq!(d.ty, CanonicalType::Unknown);
        assert!(
            d.detail.as_str().contains("ebml"),
            "detail: {}",
            d.detail.as_str()
        );
    }

    #[test]
    fn adts_aac_is_not_mistaken_for_mp3() {
        // ADTS sync: ff f1 (layer bits 00), then a plausible frame header.
        let bytes = [0xFF, 0xF1, 0x50, 0x80, 0x00, 0x1F, 0xFC, 0x00];
        assert_eq!(detect_bytes(&bytes, None).ty, CanonicalType::Aac);
        // MP3 frame sync: ff fb (layer III).
        let mp3 = [0xFF, 0xFB, 0x90, 0x64, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(detect_bytes(&mp3, None).ty, CanonicalType::Mp3);
    }

    #[test]
    fn riff_disambiguation_needs_the_form_type() {
        let mut webp = b"RIFF".to_vec();
        webp.extend_from_slice(&[0x24, 0, 0, 0]);
        webp.extend_from_slice(b"WEBPVP8 ");
        assert_eq!(detect_bytes(&webp, None).ty, CanonicalType::WebP);

        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&[0x24, 0, 0, 0]);
        wav.extend_from_slice(b"WAVEfmt ");
        assert_eq!(detect_bytes(&wav, None).ty, CanonicalType::Wav);
    }

    // -- limits and guards -------------------------------------------------

    #[test]
    fn oversized_input_is_refused_before_parsing() {
        // The constant itself, plus the verdict shape, without allocating 2 GiB.
        assert_eq!(MAX_INPUT_BYTES, 2 * 1024 * 1024 * 1024);
        assert_eq!(MAX_ZIP_TOTAL_UNCOMPRESSED, 8 * 1024 * 1024 * 1024);
        assert_eq!(MAX_CONTAINER_ENTRIES, 10_000);
    }

    #[test]
    fn zip_directory_entry_cap_is_enforced() {
        let zip = synthetic_zip_with_entries(MAX_CONTAINER_ENTRIES + 1, 1);
        let d = detect_bytes(&zip, None);
        assert_eq!(d.ty, CanonicalType::Unknown);
        assert!(
            d.detail.as_str().contains("entry cap"),
            "detail: {}",
            d.detail.as_str()
        );
    }

    #[test]
    fn zip_bomb_guard_uses_declared_uncompressed_size() {
        // Three entries each declaring 4,294,967,295 B (the u32 maximum):
        // ~12 GiB > the 8 GiB guard. (A single 4 GiB declaration does not fit
        // a classic ZIP field — that is what ZIP64 is for, and ZIP64 is a
        // documented Unsupported gap.)
        let zip = synthetic_zip_with_entries(3, u32::MAX);
        let d = detect_bytes(&zip, None);
        assert_eq!(d.ty, CanonicalType::Unknown);
        assert!(
            d.detail.as_str().contains("bomb guard"),
            "detail: {}",
            d.detail.as_str()
        );
    }

    #[test]
    fn a_zip_with_the_ooxml_parts_is_a_docx() {
        let zip = synthetic_zip(&["[Content_Types].xml", "word/document.xml"]);
        let d = detect_bytes(&zip, None);
        assert_eq!(d.ty, CanonicalType::Docx);
        assert_eq!(d.detail, Detail::ZipParts(DOCX_PARTS));
    }

    /// Build an in-memory ZIP with a valid local header, central directory and
    /// EOCD, containing `names`, so the directory parser can be tested without
    /// a ZIP writer. Entry payloads are empty; only the directory matters here.
    fn synthetic_zip(names: &[&str]) -> Vec<u8> {
        zip_with(names, 0)
    }

    fn synthetic_zip_with_entries(count: usize, declared: u32) -> Vec<u8> {
        let names: Vec<String> = (0..count).map(|i| format!("f{i}.txt")).collect();
        let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        zip_with(&refs, declared)
    }

    fn zip_with(names: &[&str], declared: u32) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        // one empty local file entry so the local signature is present
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        out.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let cd_off = out.len() as u32;
        for name in names {
            out.extend_from_slice(b"PK\x01\x02");
            out.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            out.extend_from_slice(&declared.to_le_bytes()); // crc
            out.extend_from_slice(&0u32.to_le_bytes()); // compressed
            out.extend_from_slice(&declared.to_le_bytes()); // uncompressed
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra
            out.extend_from_slice(&0u16.to_le_bytes()); // comment
            out.extend_from_slice(&0u16.to_le_bytes()); // disk
            out.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            out.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            out.extend_from_slice(&0u32.to_le_bytes()); // local header offset
            out.extend_from_slice(name.as_bytes());
        }
        let cd_size = out.len() as u32 - cd_off;
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&[0, 0, 0, 0]);
        let count = names.len().min(0xFFFE) as u16;
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_off.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }
}
