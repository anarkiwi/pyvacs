"""Shared pytest fixtures, including downloaded Fountain Force 2 ROM fixtures.

The Fountain Force 2 was the Australian-market Arcadia 2001 clone console; its
cartridges are standard Arcadia 2001 (Signetics 2650) ROM images.  These ROMs
are used as real-world disassembler/round-trip fixtures.  They are NOT checked
into the repository: the fixture downloads the TOSEC Arcadia 2001 set from the
Internet Archive on first use and caches it under ``tests/.romcache/`` (which is
git-ignored).  If the download is unavailable the ROM-dependent tests skip.
"""

from __future__ import annotations

import io
import os
import urllib.error
import urllib.request
import zipfile
from pathlib import Path
from typing import Dict

import pytest

# Stable TOSEC Arcadia 2001 ROM set on the Internet Archive.
TOSEC_URL = (
    "https://archive.org/download/Emerson_Arcadia_2001_TOSEC_2012_04_23/"
    "Emerson_Arcadia_2001_TOSEC_2012_04_23.zip"
)

CACHE_DIR = Path(__file__).parent / ".romcache"
CACHE_ZIP = CACHE_DIR / "arcadia_tosec.zip"

# Number of ROM images to expose to the tests (kept small for CI speed).
MAX_ROMS = 8


def _download_tosec() -> bytes:
    """Fetch the TOSEC zip, using the on-disk cache when present."""
    if CACHE_ZIP.exists():
        return CACHE_ZIP.read_bytes()
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    request = urllib.request.Request(TOSEC_URL, headers={"User-Agent": "pyvacs-tests"})
    with urllib.request.urlopen(request, timeout=60) as response:  # nosec B310
        payload = response.read()
    CACHE_ZIP.write_bytes(payload)
    return payload


def _extract_roms(payload: bytes) -> Dict[str, bytes]:
    """Pull a handful of ``.bin`` ROM images out of the nested TOSEC zip."""
    roms: Dict[str, bytes] = {}
    with zipfile.ZipFile(io.BytesIO(payload)) as outer:
        names = sorted(n for n in outer.namelist() if n.lower().endswith(".zip"))
        for name in names:
            if len(roms) >= MAX_ROMS:
                break
            with zipfile.ZipFile(io.BytesIO(outer.read(name))) as inner:
                for member in inner.namelist():
                    if member.lower().endswith(".bin"):
                        roms[Path(member).name] = inner.read(member)
                        break
    return roms


@pytest.fixture(scope="session")
def rom_images() -> Dict[str, bytes]:
    """Return a mapping of ROM filename to bytes, or skip if unavailable."""
    if os.environ.get("PYVACS_SKIP_ROM_TESTS"):
        pytest.skip("ROM tests disabled via PYVACS_SKIP_ROM_TESTS")
    try:
        payload = _download_tosec()
    except (urllib.error.URLError, OSError, TimeoutError) as exc:
        pytest.skip(f"ROM set unavailable: {exc}")
    try:
        roms = _extract_roms(payload)
    except zipfile.BadZipFile as exc:  # pragma: no cover - corrupt download
        pytest.skip(f"ROM set corrupt: {exc}")
    if not roms:  # pragma: no cover - unexpected archive layout
        pytest.skip("no ROM images found in archive")
    return roms
