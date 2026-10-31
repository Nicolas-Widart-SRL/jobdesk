#!/usr/bin/env python3
"""Draws the app icon with the standard library, so it can be regenerated anywhere.

Writes assets/icon.png and assets/icon.ico. On macOS it also writes assets/icon.icns.
"""
import shutil
import struct
import subprocess
import tempfile
import zlib
from pathlib import Path

ASSETS = Path(__file__).resolve().parent.parent / "assets"
GROUND = (22, 25, 29)
MARK = (232, 234, 237)


def rounded_box(x, y, cx, cy, half_w, half_h, radius):
    """Signed distance to a rounded box. Negative is inside."""
    dx = abs(x - cx) - half_w + radius
    dy = abs(y - cy) - half_h + radius
    outside = (max(dx, 0) ** 2 + max(dy, 0) ** 2) ** 0.5
    return outside + min(max(dx, dy), 0) - radius


def coverage(distance):
    return max(0.0, min(1.0, 0.5 - distance))


def pixel(x, y, s):
    """Colour and alpha at a point. `s` is the icon size."""
    plate = rounded_box(x, y, s / 2, s / 2, s * 0.41, s * 0.41, s * 0.09)
    alpha = coverage(plate)
    if alpha == 0:
        return (0, 0, 0, 0)
    stroke = s * 0.042
    case = abs(rounded_box(x, y, s / 2, s * 0.55, s * 0.24, s * 0.17, s * 0.03)) - stroke / 2
    handle = abs(rounded_box(x, y, s / 2, s * 0.36, s * 0.08, s * 0.06, s * 0.02)) - stroke / 2
    handle = max(handle, y - s * 0.38)
    band = max(abs(y - s * 0.52) - stroke / 2, abs(x - s / 2) - s * 0.24)
    ink = coverage(min(case, handle, band))
    colour = tuple(round(g + (m - g) * ink) for g, m in zip(GROUND, MARK))
    return (*colour, round(alpha * 255))


def png(size):
    rows = []
    for y in range(size):
        row = bytearray([0])
        for x in range(size):
            row.extend(pixel(x + 0.5, y + 0.5, size))
        rows.append(bytes(row))

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(b"".join(rows), 9)) + chunk(b"IEND", b"")


def ico(images):
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    directory, body = b"", b""
    for size, data in images:
        directory += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset + len(body))
        body += data
    return header + directory + body


def icns():
    iconutil = shutil.which("iconutil")
    if iconutil is None:
        return
    with tempfile.TemporaryDirectory() as temporary:
        iconset = Path(temporary) / "icon.iconset"
        iconset.mkdir()
        for points in (16, 32, 128, 256, 512):
            (iconset / f"icon_{points}x{points}.png").write_bytes(png(points))
            (iconset / f"icon_{points}x{points}@2x.png").write_bytes(png(points * 2))
        subprocess.run([iconutil, "-c", "icns", str(iconset), "-o", str(ASSETS / "icon.icns")], check=True)


def main():
    ASSETS.mkdir(exist_ok=True)
    (ASSETS / "icon.png").write_bytes(png(512))
    (ASSETS / "icon.ico").write_bytes(ico([(size, png(size)) for size in (16, 32, 48, 256)]))
    icns()


if __name__ == "__main__":
    main()
