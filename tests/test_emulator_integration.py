"""End-to-end integration test against an open-source Fountain Force 2 emulator.

The Fountain Force 2 was the Australian-market Arcadia 2001 clone; its hardware
is emulated by MAME's ``arcadia`` driver.  This test:

  1. assembles ``examples/helloworld/helloworld.asm`` with pyvacs,
  2. runs the resulting cartridge in MAME (headless, via Xvfb),
  3. has MAME snapshot the screen after it stabilises, and
  4. asserts the rendered test pattern is a solid green screen.

It is the real toolchain, the real emulator, and the real video output, checked
pixel-by-pixel.  When MAME / Xvfb are not installed the test skips, so the
ordinary unit-test run stays self-contained; CI installs them and runs it for
real (see the ``integration`` job in ``.github/workflows/ci.yml``).
"""

from __future__ import annotations

import os
import shutil
import subprocess
from collections import Counter
from pathlib import Path
from typing import Dict, Tuple

import pytest

from pngutil import decode_png_rgb

from pyvacs import assemble

REPO_ROOT = Path(__file__).resolve().parent.parent
HELLOWORLD_ASM = REPO_ROOT / "examples" / "helloworld" / "helloworld.asm"

# The test pattern paints the whole screen green (BGCOLOUR = $03).
EXPECTED_RGB = (0x00, 0xFF, 0x00)
MIN_COVERAGE = 0.95

# Snapshot after this many frames so the program has run its init and painted.
SNAPSHOT_FRAME = 100

SNAPSHOT_LUA = f"""\
-- Keep the subscription in a global so it is not garbage-collected, then take
-- a snapshot once the screen has stabilised and exit.
frames = 0
sub = emu.add_machine_frame_notifier(function()
    frames = frames + 1
    if frames == {SNAPSHOT_FRAME} then
        manager.machine.video:snapshot()
        manager.machine:exit()
    end
end)
"""


def _colour_histogram(path: Path) -> Tuple[int, Dict[Tuple[int, int, int], int]]:
    width, height, rows = decode_png_rgb(path)
    counts: Counter = Counter()
    for row in rows:
        for x in range(width):
            counts[(row[3 * x], row[3 * x + 1], row[3 * x + 2])] += 1
    return width * height, dict(counts)


def _require(tool: str) -> str:
    found = shutil.which(tool)
    if found is None:
        pytest.skip(f"{tool} not installed; skipping emulator integration test")
    return found


@pytest.mark.integration
def test_helloworld_renders_green_on_arcadia(tmp_path: Path) -> None:
    _require("mame")
    _require("xvfb-run")

    rom = tmp_path / "helloworld.bin"
    rom.write_bytes(assemble(HELLOWORLD_ASM.read_text()).to_bytes())

    lua = tmp_path / "snapshot.lua"
    lua.write_text(SNAPSHOT_LUA)

    env = dict(os.environ, HOME=str(tmp_path))
    command = [
        "xvfb-run",
        "-a",
        "mame",
        "arcadia",
        "-cart",
        str(rom),
        "-skip_gameinfo",
        "-nothrottle",
        "-sound",
        "none",
        "-seconds_to_run",
        "5",
        "-snapshot_directory",
        str(tmp_path),
        "-snapname",
        "shot",
        "-autoboot_script",
        str(lua),
        "-cfg_directory",
        str(tmp_path),
        "-nvram_directory",
        str(tmp_path),
    ]
    result = subprocess.run(
        command,
        cwd=tmp_path,
        env=env,
        capture_output=True,
        text=True,
        timeout=180,
        check=False,
    )

    snapshot = tmp_path / "shot.png"
    assert snapshot.exists(), (
        "MAME did not produce a snapshot.\n"
        f"return code: {result.returncode}\n"
        f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
    )

    total, histogram = _colour_histogram(snapshot)
    dominant, dominant_count = max(histogram.items(), key=lambda item: item[1])
    coverage = dominant_count / total

    assert dominant == EXPECTED_RGB, (
        f"expected a green screen {EXPECTED_RGB}, got dominant colour {dominant} "
        f"({coverage:.1%} of {total} pixels)"
    )
    assert coverage >= MIN_COVERAGE, (
        f"green only covered {coverage:.1%} of the screen "
        f"(expected >= {MIN_COVERAGE:.0%})"
    )


def test_helloworld_assembles_to_expected_cartridge() -> None:
    """Non-emulator guard so the example stays valid in the ordinary test run."""
    rom = assemble(HELLOWORLD_ASM.read_text()).to_bytes()
    assert len(rom) == 2048  # padded to a 2 KiB cartridge image
    assert rom[0] == 0x20  # EORZ r0 at the reset vector
    # The program must write the green value ($03) to the BGCOLOUR register
    # ($19F9): LODI,r0 $03 ; STRA,r0 $19F9  ->  04 03 cc 19 f9
    assert bytes([0x04, 0x03, 0xCC, 0x19, 0xF9]) in rom
