"""Generate the buddy app mark: clay tile + paper loop + bench.

No third-party deps. Writes app.png (256) and a multi-size ICO.
"""

from __future__ import annotations

import math
import struct
import zlib
from pathlib import Path

CLAY = (0xC5, 0x65, 0x4A, 255)
PAPER = (0xEF, 0xEB, 0xE2, 255)
INK = (0x23, 0x21, 0x1C, 255)


def lerp(a: float, b: float, t: float) -> float:
    return a + (b - a) * t


def mix(c0: tuple[int, int, int, int], c1: tuple[int, int, int, int], t: float) -> tuple[int, int, int, int]:
    t = max(0.0, min(1.0, t))
    return tuple(int(lerp(c0[i], c1[i], t) + 0.5) for i in range(4))  # type: ignore[return-value]


def sd_rounded_rect(px: float, py: float, cx: float, cy: float, hw: float, hh: float, r: float) -> float:
    dx = abs(px - cx) - (hw - r)
    dy = abs(py - cy) - (hh - r)
    ox, oy = max(dx, 0.0), max(dy, 0.0)
    return math.hypot(ox, oy) + min(max(dx, dy), 0.0) - r


def sd_circle(px: float, py: float, cx: float, cy: float, r: float) -> float:
    return math.hypot(px - cx, py - cy) - r


def cover(sd: float) -> float:
    return max(0.0, min(1.0, 0.5 - sd))


def render(size: int) -> bytes:
    s = float(size)
    pixels = bytearray(size * size * 4)
    # Geometry in a 256-space, then scaled.
    scale = s / 256.0
    tile_c, tile_hw, tile_r = 128.0, 116.0, 52.0
    ring_c = (128.0, 118.0)
    ring_r, ring_w = 58.0, 20.0
    bench_c, bench_hw, bench_hh, bench_r = (128.0, 196.0), 58.0, 9.0, 5.0
    leg_w, leg_h = 8.0, 14.0

    for y in range(size):
        py = (y + 0.5) / scale
        for x in range(size):
            px = (x + 0.5) / scale
            rgba = (0, 0, 0, 0)

            tile = cover(sd_rounded_rect(px, py, tile_c, tile_c, tile_hw, tile_hw, tile_r))
            if tile > 0:
                rgba = mix((0, 0, 0, 0), CLAY, tile)

            ring = cover(abs(sd_circle(px, py, ring_c[0], ring_c[1], ring_r)) - ring_w * 0.5)
            if ring > 0 and rgba[3] > 0:
                rgba = mix(rgba, PAPER, ring)

            bench = cover(sd_rounded_rect(px, py, bench_c[0], bench_c[1], bench_hw, bench_hh, bench_r))
            if bench > 0 and rgba[3] > 0:
                rgba = mix(rgba, PAPER, bench)

            for lx in (88.0, 168.0):
                leg = cover(sd_rounded_rect(px, py, lx, 214.0, leg_w * 0.5, leg_h * 0.5, 2.5))
                if leg > 0 and rgba[3] > 0:
                    rgba = mix(rgba, PAPER, leg * 0.92)

            # Hairline ink on the ring inner edge so 16px still reads as a loop.
            if size <= 32:
                inner = cover(abs(sd_circle(px, py, ring_c[0], ring_c[1], ring_r - ring_w * 0.5)) - 0.8)
                if inner > 0 and rgba[3] > 0:
                    rgba = mix(rgba, INK, inner * 0.18)

            i = (y * size + x) * 4
            pixels[i : i + 4] = bytes(rgba)
    return bytes(pixels)


def write_png(path: Path, w: int, h: int, rgba: bytes) -> None:
    def chunk(tag: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    raw = b"".join(b"\x00" + rgba[y * w * 4 : (y + 1) * w * 4] for y in range(h))
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    )


def write_ico(path: Path, sizes: list[int]) -> None:
    images = []
    for size in sizes:
        buf = Path("__tmp_icon.png")
        write_png(buf, size, size, render(size))
        data = buf.read_bytes()
        buf.unlink()
        images.append((size, data))

    header = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries = b""
    blobs = b""
    for size, data in images:
        w = 0 if size >= 256 else size
        h = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", w, h, 0, 0, 1, 32, len(data), offset)
        blobs += data
        offset += len(data)
    path.write_bytes(header + entries + blobs)


def main() -> None:
    here = Path(__file__).resolve().parent
    write_png(here / "app.png", 256, 256, render(256))
    write_ico(here / "app.ico", [16, 32, 48, 256])
    print(f"wrote {here / 'app.png'} and {here / 'app.ico'}")


if __name__ == "__main__":
    main()
