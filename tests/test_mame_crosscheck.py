"""Differential test against MAME's ``arcadia`` driver on the whole game set.

Each game runs in both emulators (NTSC): screen memory that has settled in MAME
must match ours byte for byte, and pressing Start must change the screen in ours
exactly when it does in MAME.
"""

from __future__ import annotations

import os
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import numpy as np
import pytest

import mame_ref

emu = pytest.importorskip("pyvacs.emu")

SCREEN = np.zeros(mame_ref.UVI_SIZE, bool)
SCREEN[0x000:0x0D0] = SCREEN[0x200:0x2D0] = True

VBLANK = (
    "MAME holds SENSE for 46 of 262 lines; the 2637 VRST is 20 (matches MAME timing)"
)
INPUTS = "MAME's paddle defaults (0x80/0x6F, outside VRST too) and taken-RETC +6 clocks"
COLLIDE = "MAME reads the collision registers' unused bits as 0; the 2637 reads 1"
CHARLINE = "polls CHARLINE, which MAME leaves stale above the character field"

DIVERGENT = {
    "Colours Demo (19xx)(-)(PD).bin": VBLANK,
    "Boxing (198x)(UA)(EU).bin": VBLANK,
    "Horse Racing (198x)(UA)(EU).bin": VBLANK,
    "Grand Slam Tennis (1982)(UA)(EU-US)[aka Tennis Yannick Noah].bin": INPUTS,
    "Space Mission (1982)(UA)(EU-US).bin": INPUTS,
    "Space Raiders (1982)(UA)(EU-US).bin": COLLIDE,
    "Space Squadron (1982)(UA)(EU).bin": COLLIDE,
    "Circus (1983)(UA)(EU)[aka Acrobat].bin": CHARLINE,
}
START_DIVERGENT = {"Circus (1983)(UA)(EU)[aka Acrobat].bin": CHARLINE}
IDLE = (280, 290, 300)
LATE = 600
START = (300, 310)


def _ours(image: bytes, start: bool):
    machine = emu.FountainForce2(image, region="ntsc")
    dumps, now = {}, 0
    for frame in IDLE + (LATE,):
        if start and now <= START[0] < frame:
            machine.run_frames(START[0] - now)
            machine.set_panel(1)
            machine.run_frames(START[1] - START[0])
            machine.set_panel(0)
            now = START[1]
        machine.run_frames(frame - now)
        now = frame
        dumps[frame] = np.frombuffer(machine.read(mame_ref.UVI_BASE, 0x300), np.uint8)
    return dumps


def _compare(item, work: Path):
    """(name, settled idle screen matches MAME, Start response matches MAME)."""
    name, image = item
    idle = mame_ref.dump_uvi(image, IDLE + (LATE,), work / "idle")
    pressed = mame_ref.dump_uvi(image, (LATE,), work / "start", start=START)
    ours_idle, ours_pressed = _ours(image, False), _ours(image, True)
    mask = SCREEN & mame_ref.settled(idle[f] for f in IDLE)
    same = bool((ours_idle[IDLE[-1]][mask] == idle[IDLE[-1]][mask]).all())
    reacts = (pressed[LATE] != idle[LATE])[SCREEN].any()
    ours_reacts = (ours_pressed[LATE] != ours_idle[LATE])[SCREEN].any()
    return name, same, bool(reacts == ours_reacts)


@pytest.fixture(scope="module")
def results(game_images, tmp_path_factory):
    work = Path(os.environ.get("PYVACS_MAME_WORK", tmp_path_factory.mktemp("mame")))
    if mame_ref.mame_command(work) is None:
        pytest.skip("mame not available")
    with ThreadPoolExecutor(8) as pool:
        return list(
            pool.map(
                lambda item: _compare(item, work / str(abs(hash(item[0])))),
                game_images.items(),
            )
        )


@pytest.mark.integration
def test_settled_screen_matches_mame(results):
    mismatched = sorted(n for n, same, _ in results if not same and n not in DIVERGENT)
    stale = sorted(n for n, same, _ in results if same and n in DIVERGENT)
    assert not mismatched, f"differ from MAME: {mismatched}"
    assert not stale, f"now match MAME, drop from DIVERGENT: {stale}"


@pytest.mark.integration
def test_start_button_response_matches_mame(results):
    mismatched = sorted(n for n, _, r in results if not r and n not in START_DIVERGENT)
    stale = sorted(n for n, _, r in results if r and n in START_DIVERGENT)
    assert not mismatched, f"Start response differs from MAME: {mismatched}"
    assert not stale, f"now match MAME, drop from START_DIVERGENT: {stale}"
