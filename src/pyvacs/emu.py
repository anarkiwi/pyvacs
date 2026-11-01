"""numpy front end for the Rust 2650 / Fountain Force 2 emulator (``pyvacs._emu``).

``FountainForce2`` runs a cartridge image headless; long-running calls release the
GIL, so independent machines scale across threads (see :func:`run_parallel`).
"""

from __future__ import annotations

import struct
import time
import zlib
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import Iterable, List, Optional, Sequence, Union

import numpy as np

try:
    from . import _emu
except ImportError as exc:
    raise ImportError(
        "pyvacs.emu needs the compiled pyvacs._emu extension; reinstall pyvacs with "
        "a Rust toolchain available (pip install -e .)"
    ) from exc

S2650 = _emu.S2650
KEYS: Sequence[str] = tuple(_emu.KEYS)
PALETTE = np.frombuffer(_emu.PALETTE, dtype=np.uint8).reshape(8, 3)
PANEL = {"start": 1, "option": 2, "select": 4}


def keymask(names: Iterable[str]) -> int:
    """Keypad mask for key names from :data:`KEYS` (case-insensitive)."""
    index = {k.lower(): i for i, k in enumerate(KEYS)}
    try:
        return sum(1 << index[n.lower()] for n in set(names))
    except KeyError as exc:
        raise ValueError(
            f"unknown key {exc.args[0]!r}; expected one of {KEYS}"
        ) from exc


def encode_png(rgb: np.ndarray) -> bytes:
    """Encode an (H, W, 3) uint8 array as an 8-bit RGB PNG."""
    rgb = np.ascontiguousarray(rgb, dtype=np.uint8)
    height, width, _ = rgb.shape
    raw = np.zeros((height, 1 + 3 * width), dtype=np.uint8)
    raw[:, 1:] = rgb.reshape(height, -1)

    def chunk(kind: bytes, body: bytes) -> bytes:
        return (
            struct.pack(">I", len(body))
            + kind
            + body
            + struct.pack(">I", zlib.crc32(kind + body))
        )

    return b"".join(
        (
            b"\x89PNG\r\n\x1a\n",
            chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)),
            chunk(b"IDAT", zlib.compress(raw.tobytes(), 9)),
            chunk(b"IEND", b""),
        )
    )


class FountainForce2(_emu.FountainForce2):
    """A Fountain Force 2 (``region="pal"``) or Emerson Arcadia 2001 (``"ntsc"``)."""

    def frame_array(self) -> np.ndarray:
        """Colour codes of the current frame as a read-only (H, W) uint8 array."""
        return np.frombuffer(self.frame, dtype=np.uint8).reshape(
            self.height, self.width
        )

    def rgb_array(self) -> np.ndarray:
        """The current frame as an (H, W, 3) uint8 array."""
        return PALETTE[self.frame_array()]

    def save_png(self, path: Union[str, Path]) -> None:
        """Write the current frame to ``path`` as a PNG."""
        Path(path).write_bytes(encode_png(self.rgb_array()))

    @property
    def seconds(self) -> float:
        """Emulated time since reset."""
        return self.clocks / self.cpu_hz


def speed(machine: FountainForce2, frames: int) -> float:
    """Run ``frames`` frames and return emulated time over wall-clock time."""
    start = time.perf_counter()
    machine.run_frames(frames)
    return frames / machine.fps / max(time.perf_counter() - start, 1e-9)


def run_parallel(
    carts: Sequence[bytes],
    frames: int,
    threads: Optional[int] = None,
    region: str = "pal",
) -> List[FountainForce2]:
    """Run each cartridge for ``frames`` frames on its own machine in a thread pool."""

    def run(cart: bytes) -> FountainForce2:
        machine = FountainForce2(cart, region)
        machine.run_frames(frames)
        return machine

    with ThreadPoolExecutor(max_workers=threads) as pool:
        return list(pool.map(run, carts))
