#!/usr/bin/env python3
"""Unit test for scripts/png-nonblank.py — runs anywhere, no image libraries.

The overlay evidence depends on this checker telling a ring apart from an empty
capture, so the checker itself has to be proved in both directions: a blank frame must
fail, a ring drawn over a desktop must pass.
"""

import math
import struct
import subprocess
import sys
import tempfile
import zlib
from pathlib import Path

CHECKER = Path(__file__).resolve().parent / "png-nonblank.py"


def write_png(path, width, height, colour_at):
    raw = bytearray()
    for y in range(height):
        raw.append(0)  # filter: none
        for x in range(width):
            raw += bytes(colour_at(x, y))

    def chunk(kind, body):
        return (
            struct.pack(">I", len(body))
            + kind
            + body
            + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
        )

    data = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(raw)))
        + chunk(b"IEND", b"")
    )
    path.write_bytes(data)


def ring(x, y):
    distance = math.hypot(x - 100, y - 100)
    if distance < 40:
        return (40, 40, 46, 255)
    if 60 < distance < 95:
        return (210, 215, 225, 255)
    return (30, 60, 90, 255)


def blank(x, y):
    return (0, 0, 0, 0)


def desktop(x, y):
    return (30, 60, 90, 255)


def run(png, *args):
    return subprocess.run(
        [sys.executable, str(CHECKER), str(png), "--json", *args],
        capture_output=True,
        text=True,
    )


def main():
    failures = []
    with tempfile.TemporaryDirectory() as work:
        work = Path(work)

        (work / "ring.png").write_bytes(b"")
        write_png(work / "ring.png", 200, 200, ring)
        write_png(work / "blank.png", 200, 200, blank)
        write_png(work / "desktop.png", 200, 200, desktop)

        result = run(work / "ring.png")
        print(f"ring      exit={result.returncode} {result.stdout.splitlines()[-3:]}")
        if result.returncode != 0:
            failures.append("a ring over a desktop must pass")

        result = run(work / "blank.png")
        print(f"blank     exit={result.returncode} {result.stdout.splitlines()[-3:]}")
        if result.returncode == 0:
            failures.append("a transparent/blank capture must fail")

        result = run(work / "desktop.png")
        print(f"desktop   exit={result.returncode} {result.stdout.splitlines()[-3:]}")
        if result.returncode == 0:
            failures.append("a desktop without the ring must fail (no centre/corner delta)")

        # The crop path is what the harness uses on a full-screen capture: the ring
        # lives in the top-left 200x200 of a 400x300 desktop.
        write_png(
            work / "fullscreen.png",
            400,
            300,
            lambda x, y: ring(x, y) if x < 200 and y < 200 else desktop(x, y),
        )
        result = run(work / "fullscreen.png", "--crop", "0,0,200,200")
        print(f"crop      exit={result.returncode}")
        if result.returncode != 0:
            failures.append("a crop of the ring region must pass")
        result = run(work / "fullscreen.png", "--crop", "200,200,200,100")
        print(f"crop-miss exit={result.returncode}")
        if result.returncode == 0:
            failures.append("a crop of empty desktop must fail")

    for failure in failures:
        print(f"FAIL {failure}")
    print(f"{'PASS' if not failures else 'FAIL'} png-nonblank: {5 - len(failures)}/5")
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
