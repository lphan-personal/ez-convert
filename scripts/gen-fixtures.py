#!/usr/bin/env python3
"""Regenerate the detection fixture corpus in `fixtures/`.

Engineering rule 5: every converter gets a fixture corpus (known input ->
known output properties) committed to the repo. This corpus is for *detection*:
each file's known property is the canonical type it must resolve to, including
the deliberately broken cases (renamed, truncated, empty, unknown, lying).

Nothing here is downloaded. Images and audio/video are synthesised with the
local ffmpeg; the document fixtures come from the S3 spike's real fixtures
(`spikes/s3`), the PDF from the measured D1 product, and the HEIC from the S2
spike's real libheif sample. Legacy `.doc/.xls/.ppt` are produced by the pinned
LibreOffice build. `fixtures/PROVENANCE.md` records, per file, how it was made
and its sha256, so a silent change is visible in review.

Usage:
    python3 scripts/gen-fixtures.py
    python3 scripts/gen-fixtures.py --s3-dir /path/to/s3-evidence/fixtures \
        --pdf /path/to/docx2pdf.pdf --heic /path/to/orient6.heic

Environment overrides: S3_FIXTURES, S3_PDF, SOURCE_HEIC, SOFFICE,
LO_LIBRARY_PATH. Exit code is non-zero if a required source is missing, so a
half-generated corpus can never be mistaken for a complete one.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIX = ROOT / "fixtures"

DEFAULT_S3 = Path(
    "/opt/data/kanban/boards/ezconvert/workspaces/t_d910a4c8/s3-evidence/fixtures"
)
DEFAULT_PDF = Path(
    "/opt/data/kanban/boards/ezconvert/workspaces/t_d910a4c8/s3-evidence/products/docx2pdf.pdf"
)
DEFAULT_HEIC = Path("/opt/data/converter/build/windows/fixtures/orient6.heic")
DEFAULT_SOFFICE = Path(
    "/opt/data/converter/vendor/libreoffice-26.8.2/opt/libreoffice26.8/program/soffice"
)
DEFAULT_LO_LIBS = "/opt/data/converter/vendor/lo-deps/usr/lib/x86_64-linux-gnu"

ROWS: list[tuple[str, int, str, str, str]] = []


# ---------------------------------------------------------------------------
# helpers
# ---------------------------------------------------------------------------


def record(name: str, origin: str, how: str) -> None:
    data = (FIX / name).read_bytes()
    ROWS.append(
        (name, len(data), hashlib.sha256(data).hexdigest(), origin, how)
    )


def run(cmd: list[str | Path], env: dict | None = None) -> None:
    printable = " ".join(str(c) for c in cmd)
    print(f"  $ {printable}")
    subprocess.run([str(c) for c in cmd], check=True, env=env)


def ffmpeg(*args: str) -> None:
    run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", *args])


def write_bytes(name: str, data: bytes) -> None:
    (FIX / name).write_bytes(data)


def write_text(name: str, text: str) -> None:
    (FIX / name).write_bytes(text.encode("utf-8"))


def require(path: Path, what: str) -> Path:
    if not path.exists():
        sys.exit(
            f"error: {what} not found at {path}\n"
            "Pass the right location (see --help) — a corpus with missing "
            "families is worse than no corpus."
        )
    return path


# ---------------------------------------------------------------------------
# generators
# ---------------------------------------------------------------------------


def gen_images(avif_fallback: Path | None) -> None:
    print("images (ffmpeg, 64x48)")
    src = ["-f", "lavfi", "-i", "testsrc=size=64x48:rate=1", "-frames:v", "1"]
    ffmpeg(*src, "-q:v", "4", str(FIX / "sample.jpg"))
    record("sample.jpg", "generated", "ffmpeg testsrc, 1 frame, mjpeg q=4")
    ffmpeg(*src, "-c:v", "png", str(FIX / "sample.png"))
    record("sample.png", "generated", "ffmpeg testsrc, 1 frame, PNG")
    ffmpeg(*src, "-c:v", "libwebp", str(FIX / "sample.webp"))
    record("sample.webp", "generated", "ffmpeg testsrc, 1 frame, libwebp")
    ffmpeg(*src, "-c:v", "tiff", str(FIX / "sample.tif"))
    record("sample.tif", "generated", "ffmpeg testsrc, 1 frame, TIFF")
    ffmpeg(*src, "-c:v", "bmp", str(FIX / "sample.bmp"))
    record("sample.bmp", "generated", "ffmpeg testsrc, 1 frame, BMP")
    ffmpeg(*src, "-f", "gif", str(FIX / "sample.gif"))
    record("sample.gif", "generated", "ffmpeg testsrc, 1 frame, GIF")

    avif_ok = True
    try:
        ffmpeg(
            *src,
            "-c:v",
            "libaom-av1",
            "-still-picture",
            "1",
            "-crf",
            "40",
            "-pix_fmt",
            "yuv420p",
            "-f",
            "avif",
            str(FIX / "sample.avif"),
        )
        record("sample.avif", "generated", "ffmpeg testsrc, libaom-av1 still picture")
    except subprocess.CalledProcessError:
        avif_ok = False
    if not avif_ok:
        if avif_fallback is None or not avif_fallback.exists():
            sys.exit(
                "error: this ffmpeg cannot mux AVIF and no --avif-fallback was "
                "given; refusing to ship a corpus without an avif fixture"
            )
        shutil.copyfile(avif_fallback, FIX / "sample.avif")
        record("sample.avif", "copied", f"real AVIF sample copied from {avif_fallback}")


def gen_documents(s3: Path, pdf: Path, heic: Path, soffice: Path, lo_libs: str) -> None:
    print("documents (S3 fixtures + pinned LibreOffice)")
    for name in ("sample.docx", "sample.xlsx", "sample.pptx"):
        shutil.copyfile(s3 / name, FIX / name)
        record(name, "copied", f"S3 spike fixture {s3 / name} (python-docx/openpyxl/python-pptx)")

    shutil.copyfile(pdf, FIX / "sample.pdf")
    record("sample.pdf", "copied", f"measured D1 product (docx -> pdf) {pdf}")

    # Legacy CFB formats: produced by the pinned LibreOffice, not hand-crafted,
    # so they are real OLE2 containers with real directory entries.
    with tempfile.TemporaryDirectory() as tmp:
        profile = Path(tmp) / "profile"
        out = Path(tmp) / "out"
        out.mkdir()
        env = dict(os.environ)
        env["LD_LIBRARY_PATH"] = lo_libs
        env["HOME"] = tmp
        for src_name, target in (
            ("sample.docx", "doc"),
            ("sample.xlsx", "xls"),
            ("sample.pptx", "ppt"),
        ):
            run(
                [
                    soffice,
                    "--headless",
                    "--norestore",
                    "--nolockcheck",
                    "--nodefault",
                    "--nologo",
                    f"-env:UserInstallation=file://{profile}",
                    "--convert-to",
                    target,
                    "--outdir",
                    out,
                    FIX / src_name,
                ],
                env=env,
            )
        for ext in ("doc", "xls", "ppt"):
            produced = out / f"sample.{ext}"
            if not produced.exists():
                sys.exit(f"error: LibreOffice did not produce sample.{ext}")
            shutil.copyfile(produced, FIX / f"sample.{ext}")
            record(
                f"sample.{ext}",
                "generated",
                f"pinned LibreOffice 26.8.2 converted the S3 fixture to {ext}",
            )

    shutil.copyfile(heic, FIX / "sample.heic")
    record("sample.heic", "copied", f"real libheif-encoded HEIC (S2 fixture) {heic}")


def gen_audio_video() -> None:
    print("audio + video (ffmpeg)")
    sine = ["-f", "lavfi", "-i", "sine=frequency=440:duration=0.25"]
    ffmpeg(*sine, "-c:a", "pcm_s16le", str(FIX / "sample.wav"))
    record("sample.wav", "generated", "ffmpeg sine 440 Hz, PCM s16le")
    ffmpeg(*sine, "-c:a", "libmp3lame", "-b:a", "64k", str(FIX / "sample.mp3"))
    record("sample.mp3", "generated", "ffmpeg sine, libmp3lame 64k")
    ffmpeg(*sine, "-c:a", "flac", str(FIX / "sample.flac"))
    record("sample.flac", "generated", "ffmpeg sine, FLAC")
    ffmpeg(*sine, "-c:a", "aac", "-b:a", "64k", "-f", "ipod", str(FIX / "sample.m4a"))
    record("sample.m4a", "generated", "ffmpeg sine, AAC/ipod muxer (M4A brand)")
    ffmpeg(*sine, "-c:a", "aac", "-b:a", "64k", "-f", "adts", str(FIX / "sample.aac"))
    record("sample.aac", "generated", "ffmpeg sine, AAC ADTS")
    ffmpeg(*sine, "-c:a", "libvorbis", "-f", "ogg", str(FIX / "sample.ogg"))
    record("sample.ogg", "generated", "ffmpeg sine, Vorbis in Ogg")
    ffmpeg(*sine, "-c:a", "libopus", "-f", "ogg", str(FIX / "sample.opus"))
    record("sample.opus", "generated", "ffmpeg sine, Opus in Ogg")

    video = ["-f", "lavfi", "-i", "testsrc=size=64x48:rate=10", "-t", "1", "-an"]
    ffmpeg(*video, "-c:v", "mpeg4", "-q:v", "10", "-f", "mp4", str(FIX / "sample.mp4"))
    record("sample.mp4", "generated", "ffmpeg testsrc, MPEG-4 part 2 in MP4")
    ffmpeg(*video, "-c:v", "mpeg4", "-q:v", "10", "-f", "mov", str(FIX / "sample.mov"))
    record("sample.mov", "generated", "ffmpeg testsrc, MPEG-4 part 2 in MOV")
    ffmpeg(*video, "-c:v", "mpeg4", "-q:v", "10", "-f", "matroska", str(FIX / "sample.mkv"))
    record("sample.mkv", "generated", "ffmpeg testsrc, MPEG-4 part 2 in Matroska")
    ffmpeg(
        *video,
        "-c:v",
        "libvpx-vp9",
        "-b:v",
        "100k",
        "-f",
        "webm",
        str(FIX / "sample.webm"),
    )
    record("sample.webm", "generated", "ffmpeg testsrc, VP9 in WebM")


def gen_text() -> None:
    print("text shapes (hand-written)")
    write_text(
        "sample.svg",
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<svg xmlns="http://www.w3.org/2000/svg" width="64" height="48" viewBox="0 0 64 48">\n'
        '  <rect x="0" y="0" width="64" height="48" fill="#1c1c1e"/>\n'
        '  <circle cx="32" cy="24" r="12" fill="#0a84ff"/>\n'
        "</svg>\n",
    )
    record("sample.svg", "generated", "hand-written SVG (rect + circle)")
    write_text(
        "sample.html",
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head><meta charset=\"utf-8\">"
        "<title>fixture</title></head>\n<body><h1>Fixture</h1>"
        "<p>caf\u00e9 \u2014 UTF-8</p></body>\n</html>\n",
    )
    record("sample.html", "generated", "hand-written HTML with a doctype and UTF-8 text")
    write_text(
        "sample.csv",
        "id,name,amount,note\n"
        "1,Widget A,10.50,\"comma, inside\"\n"
        "2,Widget B,3.25,caf\u00e9\n"
        "3,Widget C,7.00,\"quote \"\" inside\"\"\"\n",
    )
    record("sample.csv", "generated", "hand-written CSV: 4 columns, quoted commas, UTF-8")
    write_text(
        "sample.md",
        "# Fixture document\n\n"
        "A paragraph with *emphasis*, a `code span` and a [link](https://example.com).\n\n"
        "- first item\n- second item\n\n"
        "```rust\nfn main() {}\n```\n",
    )
    record("sample.md", "generated", "hand-written Markdown: heading, list, fence, link")
    write_text(
        "sample.txt",
        "This is a plain text fixture.\n"
        "It has no headings, no delimiters in a stable count, and no markup.\n"
        "Line three, with a comma, but the previous lines do not match it.\n",
    )
    record("sample.txt", "generated", "hand-written plain text with irregular commas")
    write_text(
        "sample.rtf",
        "{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0 Helvetica;}}"
        "\\f0\\fs24 Plain RTF fixture with caf\\'e9.\\par}\n",
    )
    record("sample.rtf", "generated", "hand-written minimal RTF 1.0")


def gen_broken(s3: Path) -> None:
    print("deliberately broken / hostile cases")
    jpg = (FIX / "sample.jpg").read_bytes()
    write_bytes("renamed_jpeg_as.png", jpg)
    record(
        "renamed_jpeg_as.png",
        "derived",
        "byte-identical copy of sample.jpg under a .png name (acceptance case)",
    )
    write_bytes("truncated.jpg", jpg[: len(jpg) * 3 // 5])
    record("truncated.jpg", "derived", "sample.jpg cut at 60%: SOI present, EOI absent")
    png = (FIX / "sample.png").read_bytes()
    write_bytes("truncated.png", png[: len(png) * 3 // 5])
    record("truncated.png", "derived", "sample.png cut at 60%: IEND absent")
    docx = (FIX / "sample.docx").read_bytes()
    write_bytes("truncated.docx", docx[: len(docx) * 3 // 5])
    record(
        "truncated.docx",
        "derived",
        "sample.docx cut at 60%: ZIP local header present, central directory absent",
    )
    write_bytes("empty.bin", b"")
    record("empty.bin", "derived", "zero-byte file")
    write_text(
        "text_renamed.docx",
        "This is plain text, not a document.\n"
        "LibreOffice would happily turn it into a 12,388 B PDF at exit 0 "
        "(ARCHITECTURE 6.2 N3) - detection is what stops it.\n",
    )
    record(
        "text_renamed.docx",
        "derived",
        "plain UTF-8 text under a .docx name (the N3 silent-garbage trap)",
    )
    # A valid ZIP with no OOXML part at all.
    with zipfile.ZipFile(FIX / "plain.zip", "w", zipfile.ZIP_DEFLATED) as zf:
        zf.writestr("notes.txt", "just a zip, not a document\n")
        zf.writestr("dir/data.bin", bytes(range(32)))
    record("plain.zip", "generated", "python zipfile with notes.txt + dir/data.bin, no Office part")
    write_bytes("plain.cfb", minimal_cfb_without_office_entries())
    record(
        "plain.cfb",
        "generated",
        "hand-built OLE2/CFB with Root Entry + one 'Foo' stream and no Office entries",
    )


def minimal_cfb_without_office_entries() -> bytes:
    """A structurally valid 512-byte-sector CFB with no Word/Excel/PPT stream.

    Layout: sector 0 = FAT, sector 1 = directory. One FAT sector, one directory
    sector, no mini FAT, no DIFAT chain. This exists to prove that "has the OLE2
    magic" is not the same as "is an Office document".
    """
    ENDOFCHAIN = 0xFFFFFFFE
    FREESECT = 0xFFFFFFFF
    header = bytearray(512)
    header[0:8] = bytes([0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1])
    struct.pack_into("<H", header, 24, 0x003E)  # minor version
    struct.pack_into("<H", header, 26, 0x0003)  # major version
    struct.pack_into("<H", header, 28, 0xFFFE)  # byte order
    struct.pack_into("<H", header, 30, 9)  # sector shift: 512
    struct.pack_into("<H", header, 32, 6)  # mini sector shift: 64
    struct.pack_into("<I", header, 40, 0)  # directory sectors (v3 must be 0)
    struct.pack_into("<I", header, 44, 1)  # number of FAT sectors
    struct.pack_into("<I", header, 48, 1)  # first directory sector
    struct.pack_into("<I", header, 56, 4096)  # mini stream cutoff
    struct.pack_into("<I", header, 60, ENDOFCHAIN)  # first mini FAT
    struct.pack_into("<I", header, 64, 0)  # mini FAT count
    struct.pack_into("<I", header, 68, ENDOFCHAIN)  # first DIFAT
    struct.pack_into("<I", header, 72, 0)  # DIFAT count
    struct.pack_into("<I", header, 76, 0)  # DIFAT[0] = FAT sector 0
    for i in range(1, 109):
        struct.pack_into("<I", header, 76 + i * 4, FREESECT)

    fat = bytearray(512)
    struct.pack_into("<I", fat, 0, ENDOFCHAIN)  # FAT sector itself
    struct.pack_into("<I", fat, 4, ENDOFCHAIN)  # directory sector
    for i in range(2, 128):
        struct.pack_into("<I", fat, i * 4, FREESECT)

    directory = bytearray(512)

    def entry(offset: int, name: str, kind: int) -> None:
        encoded = name.encode("utf-16-le")
        directory[offset : offset + len(encoded)] = encoded
        struct.pack_into("<H", directory, offset + 64, len(encoded) + 2)
        directory[offset + 66] = kind

    entry(0, "Root Entry", 5)
    entry(128, "Foo", 2)
    return bytes(header + fat + directory)


# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--s3-dir", default=os.environ.get("S3_FIXTURES", DEFAULT_S3))
    parser.add_argument("--pdf", default=os.environ.get("S3_PDF", DEFAULT_PDF))
    parser.add_argument("--heic", default=os.environ.get("SOURCE_HEIC", DEFAULT_HEIC))
    parser.add_argument("--soffice", default=os.environ.get("SOFFICE", DEFAULT_SOFFICE))
    parser.add_argument(
        "--lo-libs", default=os.environ.get("LO_LIBRARY_PATH", DEFAULT_LO_LIBS)
    )
    parser.add_argument(
        "--avif-fallback",
        default=os.environ.get("AVIF_FALLBACK", "/opt/data/cache/scratch/conv/o.avif"),
        help="real AVIF to copy if this ffmpeg cannot mux AVIF",
    )
    args = parser.parse_args()

    s3 = Path(args.s3_dir)
    require(s3, "S3 fixture directory")
    require(Path(args.pdf), "measured docx->pdf product")
    require(Path(args.heic), "real HEIC sample")
    require(Path(args.soffice), "pinned LibreOffice launcher")

    FIX.mkdir(parents=True, exist_ok=True)
    gen_images(Path(args.avif_fallback))
    gen_documents(s3, Path(args.pdf), Path(args.heic), Path(args.soffice), args.lo_libs)
    gen_audio_video()
    gen_text()
    gen_broken(s3)

    rows = sorted(ROWS)
    total = sum(r[1] for r in rows)
    body = [
        "# Fixture corpus — provenance",
        "",
        "Generated by `scripts/gen-fixtures.py`. Do not edit these files by hand:",
        "regenerate and re-run `cargo test` (or `scripts/test-model.sh`).",
        "",
        f"{len(rows)} files, {total} bytes total.",
        "",
        "Origin legend: **generated** = synthesised by the script; **copied** = a",
        "real artefact from a named spike; **derived** = a byte-level derivative of",
        "another fixture (renamed, truncated, emptied).",
        "",
        "| file | bytes | sha256 | origin | how |",
        "|---|---|---|---|---|",
    ]
    for name, size, digest, origin, how in rows:
        body.append(f"| `{name}` | {size} | `{digest}` | {origin} | {how} |")
    body.append("")
    (FIX / "PROVENANCE.md").write_text("\n".join(body), encoding="utf-8")

    print(f"\n{len(rows)} fixtures, {total} bytes -> {FIX}")
    print("wrote fixtures/PROVENANCE.md")


if __name__ == "__main__":
    main()
