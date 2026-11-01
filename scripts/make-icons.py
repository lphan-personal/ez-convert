"""Generate the EZ Convert app icon with no third-party deps.

Draws a 1024x1024 PPM by hand (dark glass disc + radial wheel spokes + hub),
then ffmpeg downsamples it into the PNG sizes Tauri expects. Also builds a
PNG-compressed .ico (Vista+ format) from the 256px render.
"""

import math
import os
import struct
import subprocess
import sys

SCRATCH = os.path.dirname(os.path.abspath(__file__))
ICON_DIR = sys.argv[1] if len(sys.argv) > 1 else os.path.join(SCRATCH, "icons")
S = 1024
CX = CY = S / 2.0

BG = (11, 13, 18)
RING = (110, 168, 255)
RING_DIM = (58, 84, 128)
HUB = (16, 20, 28)
INK = (238, 241, 247)


def lerp(a, b, t):
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def pixel(x, y):
    dx, dy = x + 0.5 - CX, y + 0.5 - CY
    r = math.hypot(dx, dy)
    # Soft outer disc edge with a subtle vertical gradient background.
    outer = 0.47 * S
    if r > outer:
        return (0, 0, 0, 0)  # transparent outside the disc

    # Disc body: radial glass gradient.
    t = r / outer
    body = lerp((26, 32, 44), (12, 15, 22), t)

    # Outer rim highlight.
    if outer - 6 < r <= outer:
        return lerp(body, (150, 175, 210), 0.35) + (255,)

    # Wheel: 6 spokes between 0.52*outer and 0.86*outer.
    ang = (math.atan2(dy, dx) + 2 * math.pi) % (2 * math.pi)
    seg = ang / (2 * math.pi) * 6.0
    if 0.52 * outer < r < 0.86 * outer:
        frac = seg - math.floor(seg)
        # Spoke occupies 55% of each segment with soft edges.
        edge = min(frac, 1 - frac)
        if edge > 0.09:
            glow = 0.35 + 0.65 * (1 - (r - 0.52 * outer) / (0.34 * outer))
            return lerp(body, RING, min(1.0, glow)) + (255,)
        return lerp(body, RING_DIM, 0.45) + (255,)

    # Hub disc.
    if r < 0.40 * outer:
        hub = lerp((30, 36, 48), HUB, r / (0.40 * outer))
        # Centre glyph: stacked chevrons suggesting a conversion arrow.
        ax, ay = abs(dx) / outer, dy / outer
        if 0.10 < ay < 0.16 and ax < 0.16:
            return lerp(hub, INK, 0.85) + (255,)
        if 0.16 <= ay < 0.19 and ax < 0.16 - (ay - 0.16) * 1.2:
            return lerp(hub, INK, 0.7) + (255,)
        return hub + (255,)

    return body + (255,)


def write_png(path):
    """Minimal RGBA PNG writer (stdlib only: zlib + struct)."""
    import zlib

    raw = bytearray()
    for y in range(S):
        raw.append(0)  # filter type 0 (None)
        for x in range(S):
            raw.extend(pixel(x, y))

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(">IIBBBBB", S, S, 8, 6, 0, 0, 0)  # 8-bit RGBA
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as f:
        f.write(png)


def main():
    os.makedirs(ICON_DIR, exist_ok=True)
    src = os.path.join(SCRATCH, "icon_src.png")
    write_png(src)
    print("wrote", src, os.path.getsize(src), "bytes")

    png_targets = {
        "icon.png": 512,
        "128x128@2x.png": 256,
        "128x128.png": 128,
        "32x32.png": 32,
        "icon-256.png": 256,
    }
    for name, size in png_targets.items():
        out = os.path.join(ICON_DIR, name)
        subprocess.run(
            ["ffmpeg", "-v", "error", "-y", "-i", src,
             "-vf", "scale=%d:%d:flags=lanczos" % (size, size),
             "-pix_fmt", "rgba", out],
            check=True,
        )
        print("wrote", out, os.path.getsize(out), "bytes")

    # ICO with a single PNG-compressed 256x256 entry (Vista+ layout).
    png256 = os.path.join(ICON_DIR, "icon-256.png")
    data = open(png256, "rb").read()
    header = struct.pack("<HHH", 0, 1, 1)
    entry = struct.pack("<BBBBHHII", 0, 0, 0, 0, 1, 32, len(data), 22)
    ico = os.path.join(ICON_DIR, "icon.ico")
    with open(ico, "wb") as f:
        f.write(header + entry + data)
    print("wrote", ico, os.path.getsize(ico), "bytes")



if __name__ == "__main__":
    main()
