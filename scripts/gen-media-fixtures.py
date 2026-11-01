#!/usr/bin/env python3
"""Generate the **media** fixtures C2 adds to the corpus, and merge their rows
into `fixtures/PROVENANCE.md` without touching C1's rows.

Why a second generator: `scripts/gen-fixtures.py` regenerates all 40 detection
fixtures from local tool versions, which would churn the sha256 of every
audio/video file C1 pinned. This script only writes the files below and rewrites
the provenance table by *merging* — existing rows are kept byte-for-byte, new
rows are added, and it is idempotent.

Files produced (see the module docstrings in `docs/MEDIA.md` for why each one
exists):

- `srgb-v2-magic.icc` — a real 736 B sRGB v2 profile (CC0-1.0), vendored so the
  ICC-preservation test needs no network;
- `exif_icc.jpg` — 400x200 JPEG carrying EXIF `Orientation=6` **and** that ICC
  profile (built with the local libvips `icc_transform`);
- `tagged.mp3` — 1.2 s sine with title/artist/album/date and an attached cover
  picture (the lofty merge input);
- `tone24_96k.wav` — 24-bit / 96 kHz PCM (the rate-clamp and wide-bit path);
- `sample_vfr.mkv` — genuinely variable-frame-rate video (r_frame_rate and
  avg_frame_rate disagree), the CFR-forcing input.
- `sample_anim.gif` — 4-frame animated GIF (image input that is *not* a still).

Usage:
    python3 scripts/gen-media-fixtures.py --vips /path/to/vips \
        --ffmpeg /path/to/ffmpeg --icc /path/to/sRGB-v2-magic.icc

Exit code is non-zero when a required input is missing: a half-generated corpus
must never look complete.
"""

from __future__ import annotations

import argparse
import hashlib
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIX = ROOT / "fixtures"
DEFAULT_VIPS = Path("/opt/data/profiles/wright/cache/scratch/eng/prefix/usr/bin/vips")
DEFAULT_VIPS_LIBS = Path("/opt/data/profiles/wright/cache/scratch/eng/prefix/usr/lib/x86_64-linux-gnu")
DEFAULT_FFMPEG = Path(
    "/opt/data/profiles/wright/cache/scratch/eng/ffmpeg-master-latest-linux64-lgpl/bin/ffmpeg"
)
DEFAULT_EXIF_SRC = Path("/opt/data/converter/build/windows/fixtures/orient6.jpg")
DEFAULT_ICC = Path("/opt/data/profiles/wright/cache/scratch/eng/srgb.icc")

NEW_ROWS: list[tuple[str, int, str, str, str]] = []


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def record(name: str, origin: str, how: str) -> None:
    path = FIX / name
    if not path.is_file():
        raise SystemExit(f"expected {path} to exist")
    NEW_ROWS.append((name, path.stat().st_size, sha256(path), origin, how))


def run(argv: list[str], env: dict[str, str] | None = None) -> None:
    result = subprocess.run(argv, capture_output=True, text=True, env=env)
    if result.returncode != 0:
        raise SystemExit(
            f"command failed ({result.returncode}): {' '.join(argv)}\n"
            f"{result.stdout}\n{result.stderr}"
        )


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--vips", default=str(DEFAULT_VIPS))
    ap.add_argument("--vips-libs", default=str(DEFAULT_VIPS_LIBS))
    ap.add_argument("--ffmpeg", default=str(DEFAULT_FFMPEG))
    ap.add_argument("--icc", default=str(DEFAULT_ICC))
    ap.add_argument("--exif-src", default=str(DEFAULT_EXIF_SRC))
    args = ap.parse_args()

    vips = Path(args.vips)
    ffmpeg = Path(args.ffmpeg)
    icc = Path(args.icc)
    exif_src = Path(args.exif_src)
    for path, what in [
        (vips, "libvips CLI"),
        (ffmpeg, "LGPL ffmpeg"),
        (icc, "sRGB ICC profile"),
        (exif_src, "EXIF orientation-6 JPEG (S2 fixture)"),
    ]:
        if not path.exists():
            raise SystemExit(f"missing {what}: {path}")

    FIX.mkdir(parents=True, exist_ok=True)
    import os

    vips_env = dict(os.environ)
    vips_env["LD_LIBRARY_PATH"] = args.vips_libs

    # 1. the profile itself
    shutil.copyfile(icc, FIX / "srgb-v2-magic.icc")
    record(
        "srgb-v2-magic.icc",
        "copied",
        "CC0-1.0 sRGB v2 profile from github.com/saucecontrol/Compact-ICC-Profiles "
        "(profiles/sRGB-v2-magic.icc), 736 B",
    )

    # 2. EXIF + ICC in one still
    run(
        [
            str(vips),
            "icc_transform",
            f"{exif_src}[fail-on=error]",
            f"{FIX / 'exif_icc.jpg'}[Q=95]",
            str(FIX / "srgb-v2-magic.icc"),
        ],
        env=vips_env,
    )
    record(
        "exif_icc.jpg",
        "derived",
        "S2 orient6.jpg (EXIF Orientation=6) re-saved through vips icc_transform "
        "with srgb-v2-magic.icc embedded",
    )

    # 3. tagged audio with cover art
    run(
        [
            str(ffmpeg), "-nostdin", "-y", "-hide_banner", "-loglevel", "error",
            "-f", "lavfi", "-i", "sine=frequency=440:duration=1.2",
            "-i", str(FIX / "sample.jpg"),
            "-map", "0:a", "-map", "1:v",
            "-c:a", "libmp3lame", "-b:a", "128k",
            "-c:v", "mjpeg", "-disposition:v", "attached_pic",
            "-metadata", "title=C2 Tag Probe",
            "-metadata", "artist=EZ Convert",
            "-metadata", "album=Fixtures",
            "-metadata", "date=2026",
            str(FIX / "tagged.mp3"),
        ]
    )
    record(
        "tagged.mp3",
        "generated",
        "ffmpeg sine + cover art (sample.jpg), ID3v2 title/artist/album/date",
    )

    # 4. 24-bit / 96 kHz PCM: the rate-clamp and wide-bit path
    run(
        [
            str(ffmpeg), "-nostdin", "-y", "-hide_banner", "-loglevel", "error",
            "-f", "lavfi", "-i", "sine=frequency=660:duration=0.5",
            "-ar", "96000", "-c:a", "pcm_s24le",
            str(FIX / "tone24_96k.wav"),
        ]
    )
    record(
        "tone24_96k.wav",
        "generated",
        "ffmpeg sine 660 Hz, pcm_s24le, 96 kHz (clamp + wide-bit fixtures)",
    )

    # 5. genuinely variable frame rate
    run(
        [
            str(ffmpeg), "-nostdin", "-y", "-hide_banner", "-loglevel", "error",
            "-f", "lavfi", "-i", "testsrc=size=320x240:rate=30:duration=3",
            "-vf", "setpts='if(lt(N,45),N/30/TB,(N-45)*2/30/TB)'",
            "-fps_mode", "vfr", "-c:v", "mpeg4", "-q:v", "5",
            str(FIX / "sample_vfr.mkv"),
        ]
    )
    record(
        "sample_vfr.mkv",
        "generated",
        "ffmpeg testsrc with a setpts step: 45 frames at 30 fps then half rate (VFR)",
    )

    # 6. animated GIF: image-family input that is not a still
    run(
        [
            str(ffmpeg), "-nostdin", "-y", "-hide_banner", "-loglevel", "error",
            "-f", "lavfi", "-i", "testsrc=size=64x48:rate=4:duration=1",
            "-vf", "split[a][b];[a]palettegen[p];[b][p]paletteuse",
            "-loop", "0", str(FIX / "sample_anim.gif"),
        ]
    )
    record(
        "sample_anim.gif",
        "generated",
        "ffmpeg testsrc, 4 frames, palettegen/paletteuse (animated GIF input)",
    )

    merge_provenance()
    print(f"{len(NEW_ROWS)} media fixtures written to {FIX}")
    for name, size, digest, origin, _ in NEW_ROWS:
        print(f"  {name:22} {size:>8} B  {digest[:16]}…  {origin}")
    return 0


def merge_provenance() -> None:
    """Keep existing rows, replace rows for the files this script owns."""
    path = FIX / "PROVENANCE.md"
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()

    header: list[str] = []
    rows: list[str] = []
    tail: list[str] = []
    in_table = False
    for line in lines:
        if line.startswith("| file | bytes |"):
            in_table = True
            header.append(line)
            continue
        if in_table and line.startswith("|---"):
            header.append(line)
            continue
        if in_table and line.startswith("|"):
            rows.append(line)
            continue
        if in_table and not line.startswith("|"):
            in_table = False
            tail.append(line)
            continue
        if not in_table:
            header.append(line)

    ours = {name for name, *_ in NEW_ROWS}
    kept = [
        row
        for row in rows
        if row.split("|")[1].strip().strip("`") not in ours
    ]
    added = [
        f"| `{name}` | {size} | `{digest}` | {origin} | {how} |"
        for name, size, digest, origin, how in NEW_ROWS
    ]
    merged = sorted(kept + added, key=lambda r: r.split("|")[1].strip().strip("`"))

    # Rebuild the count line ("N files, M bytes total.").
    total = 0
    count = 0
    for row in merged:
        parts = [p.strip() for p in row.split("|")]
        try:
            total += int(parts[2])
            count += 1
        except (IndexError, ValueError):
            continue
    for i, line in enumerate(header):
        if " files, " in line and line.endswith(" bytes total."):
            header[i] = f"{count} files, {total} bytes total."
            break

    out = header + merged + [""] + tail
    text = "\n".join(out).rstrip() + "\n"
    text = text.replace(
        "Generated by `scripts/gen-fixtures.py`. Do not edit these files by hand:",
        "Generated by `scripts/gen-fixtures.py` (detection corpus) and\n"
        "`scripts/gen-media-fixtures.py` (C2 media fixtures, merged in place). Do not edit\n"
        "these files by hand:",
    )
    path.write_text(text, encoding="utf-8")


if __name__ == "__main__":
    sys.exit(main())
