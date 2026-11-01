"""Minimal PNG decoder shared by tests that check rendered frames."""

from __future__ import annotations

import struct
import zlib
from pathlib import Path
from typing import List, Tuple


def decode_png_rgb(path: Path) -> Tuple[int, int, List[bytes]]:
    """Decode a non-interlaced 8-bit RGB PNG into (width, height, rows)."""
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG file"
    pos, width, height, idat = 8, 0, 0, b""
    while pos < len(data):
        length = struct.unpack(">I", data[pos : pos + 4])[0]
        kind = data[pos + 4 : pos + 8]
        chunk = data[pos + 8 : pos + 8 + length]
        if kind == b"IHDR":
            width, height = struct.unpack(">II", chunk[:8])
        elif kind == b"IDAT":
            idat += chunk
        pos += 12 + length
    raw = zlib.decompress(idat)
    stride = width * 3
    rows: List[bytes] = []
    previous = bytes(stride)
    offset = 0
    for _ in range(height):
        filter_type = raw[offset]
        offset += 1
        line = bytearray(raw[offset : offset + stride])
        offset += stride
        _unfilter(line, previous, filter_type, stride)
        rows.append(bytes(line))
        previous = bytes(line)
    return width, height, rows


def _unfilter(line: bytearray, previous: bytes, filter_type: int, stride: int) -> None:
    for index in range(stride):
        left = line[index - 3] if index >= 3 else 0
        up = previous[index]
        up_left = previous[index - 3] if index >= 3 else 0
        if filter_type == 1:
            line[index] = (line[index] + left) & 0xFF
        elif filter_type == 2:
            line[index] = (line[index] + up) & 0xFF
        elif filter_type == 3:
            line[index] = (line[index] + ((left + up) >> 1)) & 0xFF
        elif filter_type == 4:
            line[index] = (line[index] + _paeth(left, up, up_left)) & 0xFF


def _paeth(left: int, up: int, up_left: int) -> int:
    predictor = left + up - up_left
    pa, pb, pc = (
        abs(predictor - left),
        abs(predictor - up),
        abs(predictor - up_left),
    )
    if pa <= pb and pa <= pc:
        return left
    return up if pb <= pc else up_left
