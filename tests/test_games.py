"""Headless runs of every game in the downloaded Arcadia / Fountain Force 2 set."""

from __future__ import annotations

import time

import numpy as np
import pytest

emu = pytest.importorskip("pyvacs.emu")

FRAMES = 600
MIN_SPEED = 20.0


def _run(image: bytes, region: str = "pal") -> "emu.FountainForce2":
    machine = emu.FountainForce2(image, region=region)
    machine.coverage(True)
    machine.run_frames(FRAMES)
    return machine


@pytest.mark.parametrize("region", ["pal", "ntsc"])
def test_games_run_cleanly(game_images, region):
    failures = []
    for name, image in game_images.items():
        machine = _run(image, region)
        executed = np.count_nonzero(np.frombuffer(machine.executed(), np.uint8))
        if machine.illegal or machine.halted:
            failures.append(
                f"{name}: illegal={machine.illegal} halted={machine.halted}"
            )
        if executed < 32 or len(np.unique(machine.frame_array())) < 2:
            failures.append(f"{name}: executed={executed} blank screen")
        if machine.frames != FRAMES:
            failures.append(f"{name}: frames={machine.frames}")
    assert not failures, "\n".join(failures)


def test_games_are_deterministic(game_images):
    for name, image in game_images.items():
        a, b = _run(image), _run(image)
        assert (a.clocks, a.frame, a.read(0x1800, 0x300)) == (
            b.clocks,
            b.frame,
            b.read(0x1800, 0x300),
        ), name


def test_games_run_far_faster_than_real_time(game_images):
    start = time.thread_time()
    emulated = sum(_run(image).seconds for image in game_images.values())
    assert emulated / (time.thread_time() - start) > MIN_SPEED


def test_parallel_runs_match_serial(game_images):
    carts = list(game_images.values())[:8]
    serial = [_run(cart).frame for cart in carts]
    parallel = emu.run_parallel(carts, FRAMES, threads=len(carts))
    assert [m.frame for m in parallel] == serial
