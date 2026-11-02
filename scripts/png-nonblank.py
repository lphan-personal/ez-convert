#!/usr/bin/env python3
"""Decide whether a screenshot actually shows something.

The overlay acceptance card asks for a screenshot of the summoned ring. A file
existing is not evidence: `screencapture` on a runner with no console session writes a
perfectly valid black (or transparent) PNG, and a screenshot of an empty desktop passes
any "file size > 0" check. This script decodes the PNG with the standard library only
(no Pillow on a CI runner), crops it to the wheel's rect, and reports numbers a human
can argue with:

    width, height, unique colours, luminance standard deviation, and the fraction of
    pixels that differ from the top-left corner (the "background" sample)

Exit code 0 only when the crop is not blank *and* the wheel's centre differs from its
corner, which is the signature of a ring drawn over a desktop: an empty capture fails
both, and a capture of the desktop without the ring fails the second.

Usage:
    png-nonblank.py SHOT.png [--crop X,Y,W,H] [--require-variance 2.0]
                              [--centre-corner-delta 8] [--json]
"""

import argparse
import json
import struct
import sys
import zlib


def read_png(path):
    """Returns (width, height, rows) with rows a list of bytearrays, 8-bit RGBA."""
    with open(path, "rb") as handle:
        data = handle.read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise SystemExit(f"{path}: not a PNG")
    pos = 8
    width = height = bit_depth = colour_type = interlace = 0
    idat = []
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        kind = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            width, height, bit_depth, colour_type, _comp, _filt, interlace = struct.unpack(
                ">IIBBBBB", body
            )
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"IEND":
            break
    if width == 0:
        raise SystemExit(f"{path}: no IHDR")
    if bit_depth != 8 or colour_type not in (2, 6) or interlace != 0:
        raise SystemExit(
            f"{path}: unsupported PNG (bit depth {bit_depth}, colour type {colour_type}, "
            f"interlace {interlace}); re-save it as 8-bit RGB/RGBA, non-interlaced"
        )
    channels = 4 if colour_type == 6 else 3
    raw = zlib.decompress(b"".join(idat))
    stride = width * channels
    rows = []
    previous = bytearray(stride)
    offset = 0
    for _ in range(height):
        filter_type = raw[offset]
        offset += 1
        line = bytearray(raw[offset : offset + stride])
        offset += stride
        if filter_type == 1:  # Sub
            for i in range(channels, stride):
                line[i] = (line[i] + line[i - channels]) & 0xFF
        elif filter_type == 2:  # Up
            for i in range(stride):
                line[i] = (line[i] + previous[i]) & 0xFF
        elif filter_type == 3:  # Average
            for i in range(stride):
                left = line[i - channels] if i >= channels else 0
                line[i] = (line[i] + ((left + previous[i]) >> 1)) & 0xFF
        elif filter_type == 4:  # Paeth
            for i in range(stride):
                a = line[i - channels] if i >= channels else 0
                b = previous[i]
                c = previous[i - channels] if i >= channels else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                predictor = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + predictor) & 0xFF
        elif filter_type != 0:
            raise SystemExit(f"{path}: unknown filter type {filter_type}")
        rows.append(line)
        previous = line
    return width, height, rows, channels


def pixel(rows, channels, x, y):
    row = rows[y]
    base = x * channels
    if channels == 4:
        return row[base], row[base + 1], row[base + 2], row[base + 3]
    return row[base], row[base + 1], row[base + 2], 255


def luminance(rgb):
    return 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("png")
    parser.add_argument("--crop", default=None, help="X,Y,W,H in image pixels")
    parser.add_argument("--require-variance", type=float, default=2.0)
    parser.add_argument("--centre-corner-delta", type=float, default=8.0)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    width, height, rows, channels = read_png(args.png)
    left, top = 0, 0
    if args.crop:
        left, top, want_w, want_h = (int(value) for value in args.crop.split(","))
        # Clamp: the wheel may be partly off the captured screen.
        left = max(0, min(left, width - 1))
        top = max(0, min(top, height - 1))
        width = max(1, min(want_w, width - left))
        height = max(1, min(want_h, height - top))

    step_x = max(1, width // 256)
    step_y = max(1, height // 256)
    samples = []
    for y in range(top, top + height, step_y):
        for x in range(left, left + width, step_x):
            samples.append(pixel(rows, channels, x, y))

    count = len(samples)
    mean = sum(luminance(pixel_) for pixel_ in samples) / count
    variance = sum((luminance(pixel_) - mean) ** 2 for pixel_ in samples) / count
    stddev = variance ** 0.5
    quantised = {tuple(value // 8 for value in sample[:3]) for sample in samples}
    corner = pixel(rows, channels, left, top)
    centre = pixel(rows, channels, left + width // 2, top + height // 2)
    delta = (
        (corner[0] - centre[0]) ** 2 + (corner[1] - centre[1]) ** 2 + (corner[2] - centre[2]) ** 2
    ) ** 0.5
    differs = sum(
        1
        for sample in samples
        if abs(sample[0] - corner[0]) + abs(sample[1] - corner[1]) + abs(sample[2] - corner[2]) > 24
    ) / count

    facts = {
        "png": args.png,
        "width": width,
        "height": height,
        "crop": [left, top, width, height],
        "samples": count,
        "unique_colours": len(quantised),
        "luminance_mean": round(mean, 3),
        "luminance_stddev": round(stddev, 3),
        "corner_rgba": list(corner),
        "centre_rgba": list(centre),
        "centre_corner_delta": round(delta, 3),
        "fraction_differs_from_corner": round(differs, 4),
    }
    checks = {
        # A blank or solid capture has no luminance spread at all; a real screen
        # (even a plain desktop) does. The colour count is reported, not asserted:
        # a three-colour ring is a legitimate image.
        "not-blank": stddev >= args.require_variance,
        "ring-not-background": delta >= args.centre_corner_delta or differs >= 0.15,
    }
    facts["checks"] = checks
    facts["ok"] = all(checks.values())

    if args.json:
        print(json.dumps(facts, indent=2))
    else:
        for key, value in facts.items():
            print(f"{key}: {value}")
    return 0 if facts["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
