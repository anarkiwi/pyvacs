"""Headless runs of every game in the downloaded Arcadia / Fountain Force 2 set."""

from __future__ import annotations

import time

import numpy as np
import pytest

emu = pytest.importorskip("pyvacs.emu")

FRAMES = 600
MIN_SPEED = 200.0


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


@pytest.mark.parametrize("region", ["pal", "ntsc"])
def test_fast_path_matches_plain_stepping(game_images, region):
    def state(m):
        return m.clocks, m.pc, m.line, m.read(0x1800, 0x300), m.frame

    for name, image in game_images.items():
        slow = emu.FountainForce2(image, region=region)
        slow.fast = False
        fast = emu.FountainForce2(image, region=region)
        batched = emu.FountainForce2(image, region=region)
        for frame in range(1, 301):
            slow.run_frames(1)
            fast.run_frames(1)
            assert state(slow) == state(fast), (name, frame)
            if frame % 100 == 0:
                batched.run_frames(100)
                assert state(slow) == state(batched), (name, frame)
                for n in (1, 57, 17_711):
                    for m in (slow, fast, batched):
                        m.run_clocks(n)
                    assert state(slow) == state(fast) == state(batched), (name, n)
                for m in (slow, fast, batched):
                    m.run_frames(1)


def test_games_run_far_faster_than_real_time(game_images):
    start = time.thread_time()
    emulated = sum(_run(image).seconds for image in game_images.values())
    assert emulated / (time.thread_time() - start) > MIN_SPEED


def test_parallel_runs_match_serial(game_images):
    carts = list(game_images.values())[:8]
    serial = [_run(cart).frame for cart in carts]
    parallel = emu.run_parallel(carts, FRAMES, threads=len(carts))
    assert [m.frame for m in parallel] == serial
