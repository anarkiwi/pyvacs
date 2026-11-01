"""Tests for the Rust emulator bindings and the pyvacs.emu numpy front end."""

from __future__ import annotations

import importlib
import sys
from pathlib import Path

import numpy as np
import pytest

from pngutil import decode_png_rgb
from pyvacs import assemble

emu = pytest.importorskip("pyvacs.emu")

HELLOWORLD = (
    Path(__file__).resolve().parent.parent
    / "examples"
    / "helloworld"
    / "helloworld.asm"
)
HEIGHTS = {"pal": 269, "ntsc": 242}

SENSE_PROGRAM = """
        org 0
wait:   spsu
        tmi,r0 $80
        bcfr,eq wait
{flag}
        lodi,r0 {colour}
        stra,r0 $19F9
spin:   bctr,un spin
        end
"""


def build(source: str):
    program = assemble(source)
    return program.to_bytes(), program.symbols


def cpu_run(source: str, max_clocks: int = 100_000):
    image, symbols = build(source)
    cpu = emu.S2650(image)
    return cpu, cpu.run(max_clocks), symbols


def sense_cart(colour: int, flag: bool = False) -> bytes:
    return build(
        SENSE_PROGRAM.format(colour=colour, flag="        ppsu $40" if flag else "")
    )[0]


@pytest.fixture(scope="module")
def helloworld():
    return build(HELLOWORLD.read_text())


def test_sum_loop():
    cpu, why, _ = cpu_run("""
        org 0
        eorz r0
        lodi,r1 10
loop:   addz r1
        bdrr,r1 loop
        stra,r0 $100
        halt
        end
""")
    assert why == "halt" and cpu.halted
    assert cpu.read(0x100, 1) == bytes([55])
    assert (cpu.r0, cpu.r1) == (55, 0)
    assert cpu.illegal == 0


@pytest.mark.parametrize("a,b", [(0x38, 0x45), (0x99, 0x01), (0x50, 0x49), (0, 0)])
def test_bcd_add_with_dar(a, b):
    cpu, why, _ = cpu_run(f"""
        org 0
        lodi,r0 ${a:02X}
        addi,r0 $66
        addi,r0 ${b:02X}
        dar,r0
        halt
        end
""")
    total = int(f"{a:x}") + int(f"{b:x}")
    assert why == "halt"
    assert cpu.r0 == int(str(total % 100), 16)
    assert cpu.psl & 1 == (total >= 100)


SUBROUTINES = """
        org 0
        lodi,r0 1
        bsta,un double
        bsta,un double
        bsta,un double
        halt
double: bsta,un twice
        retc,un
twice:  addz r0
        retc,un
        end
"""


def test_subroutines_and_stop_pcs():
    cpu, why, symbols = cpu_run(SUBROUTINES)
    assert why == "halt"
    assert (cpu.r0, cpu.sp) == (8, 0)

    image, _ = build(SUBROUTINES)
    cpu = emu.S2650(image)
    assert cpu.run(10_000, stop_pcs=[symbols["TWICE"]]) == "pc"
    assert (cpu.pc, cpu.sp, cpu.r0) == (symbols["TWICE"], 2, 1)
    assert cpu.ras[1:3] == [5, symbols["DOUBLE"] + 3]
    assert cpu.run(10_000, stop_pcs=[symbols["TWICE"]]) == "pc"
    assert cpu.r0 == 2
    assert cpu.run(5) == "clocks"


def test_registers_banks_and_memory():
    cpu = emu.S2650(b"\x76\x40\x12\x10\x40", origin=0x1000)
    cpu.pc = 0x1000
    cpu.r1 = 0x11
    cpu.psl = 0x10
    cpu.r1, cpu.r2, cpu.r3, cpu.r0 = 0x21, 0x22, 0x23, 0x01
    assert cpu.regs == [0x01, 0x11, 0, 0, 0x21, 0x22, 0x23]
    cpu.regs = list(range(7))
    assert (cpu.r0, cpu.r1, cpu.r2, cpu.r3) == (0, 4, 5, 6)
    with pytest.raises(ValueError):
        cpu.regs = [1, 2]
    cpu.sense = True
    assert [cpu.step(), cpu.step()] == [9, 6]
    assert cpu.flag and cpu.sense
    assert cpu.r0 & 0xC0 == 0xC0 and cpu.psu & 0x40
    cpu.psu = 0
    assert cpu.step() == 6 and cpu.illegal == 1
    assert cpu.run(100) == "halt" and cpu.clocks == 27
    cpu.write(0x7FFE, b"\xaa\xbb\xcc")
    assert cpu.read(0x7FFE, 3) == b"\xaa\xbb\xcc"
    assert cpu.read(0, 1) == b"\xcc"
    cpu.reset()
    assert (cpu.pc, cpu.clocks, cpu.halted) == (0, 0, False)
    assert cpu.read(0x1000, 2) == b"\x76\x40"


def test_interrupt_wakes_halt():
    image, _ = build("""
        org 0
        halt
        org $10
        lodi,r0 $42
        halt
        end
""")
    cpu = emu.S2650(image)
    assert cpu.run(1000) == "halt"
    assert not cpu.irq and cpu.intack == 0
    cpu.intack, cpu.irq = 0x10, True
    assert cpu.step() > 0 and cpu.pc == 0x10 and cpu.psu & 0x20
    cpu.irq = False
    assert cpu.run(1000) == "halt" and cpu.r0 == 0x42


@pytest.mark.parametrize("region", ["pal", "ntsc"])
def test_helloworld_is_solid_green(helloworld, region):
    image, symbols = helloworld
    m = emu.FountainForce2(image, region)
    assert m.run_frames(10) == "frames"
    assert (m.width, m.height) == (192, HEIGHTS[region])
    frame = m.frame_array()
    assert frame.shape == (HEIGHTS[region], 192)
    assert np.all(frame == 3)
    assert np.all(m.rgb_array() == (0, 255, 0))
    assert m.frame_rgb() == m.rgb_array().tobytes()
    assert m.pc == symbols["FOREVER"] and m.illegal == 0 and not m.halted
    assert m.frames == 10 and m.line == 0
    assert m.fps == pytest.approx(50 if region == "pal" else 60, abs=0.5)
    assert m.seconds == pytest.approx(10 / m.fps, rel=1e-3)
    assert m.tone_hz is None


@pytest.mark.parametrize("region,colour", [("pal", 5 ^ 7), ("ntsc", 5)])
def test_flag_inversion_only_on_pal(region, colour):
    m = emu.FountainForce2(sense_cart(5, flag=True), region)
    m.run_frames(3)
    assert np.all(m.frame_array() == colour)
    assert m.psu & 0x40


def test_inputs_peek_poke():
    m = emu.FountainForce2(bytes(range(256)) * 24)
    m.set_keys(0, emu.keymask(["1", "clear", "5", "Enter"]))
    m.set_keys(1, emu.keymask(["2"]))
    m.set_panel(emu.PANEL["start"] | emu.PANEL["select"])
    assert m.read(0x1900, 9) == bytes([9, 4, 1, 0, 0, 8, 0, 0, 5])
    with pytest.raises(ValueError, match="unknown key"):
        emu.keymask(["fire"])
    with pytest.raises(ValueError):
        m.set_keys(2, 0)
    with pytest.raises(ValueError):
        m.set_paddle(0, 2, 0)
    m.set_paddle(1, 0, 0x33)
    m.set_paddle(1, 1, 0x33)
    while not m.peek(0x19FE) == 0x33:
        m.step()
    assert m.peek(0x2005) == 5 and m.peek(0x6005) == 5 and m.peek(0x2FFF) == 0xFF
    m.poke(0x1810, 0xAB)
    m.poke(0x0003, 0xCD)
    assert m.read(0x1810, 1) == b"\xab" and m.peek(0x4003) == 0xCD
    with pytest.raises(ValueError, match="region"):
        emu.FountainForce2(b"", "secam")


def test_breakpoints_coverage_reset():
    image, symbols = build(SENSE_PROGRAM.format(colour=2, flag=""))
    m = emu.FountainForce2(image)
    assert m.executed() is None
    m.coverage(True)
    m.set_breakpoint(symbols["SPIN"])
    assert m.run_frames(5) == "pc" and m.pc == symbols["SPIN"]
    assert m.run_clocks(1000) == "pc"
    m.set_breakpoint(symbols["SPIN"], False)
    assert m.run_clocks(1000) == "clocks"
    m.set_breakpoint(0)
    m.clear_breakpoints()
    assert m.run_frames(2) == "frames"
    hits = np.frombuffer(m.executed(), dtype=np.uint8)
    assert hits.size == 0x8000
    assert np.flatnonzero(hits).tolist() == [0, 1, 3, 5, 7, symbols["SPIN"]]
    m.reset()
    assert (m.frames, m.clocks, m.pc) == (0, 0, 0)
    assert m.regs == [0] * 7 and m.psl == 0
    m.coverage(False)
    assert m.executed() is None


def test_threads_match_serial():
    carts = [sense_cart(c) for c in range(8)] * 2
    serial = []
    for cart in carts:
        m = emu.FountainForce2(cart)
        m.run_frames(20)
        serial.append((m.clocks, m.frame))
    parallel = emu.run_parallel(carts, 20, threads=4)
    assert [(m.clocks, m.frame) for m in parallel] == serial
    assert len({f for _, f in serial}) == 8
    assert len(parallel) == 16


def test_speed_positive():
    assert emu.speed(emu.FountainForce2(sense_cart(1)), 5) > 0


def test_png_roundtrip(tmp_path):
    rgb = np.random.default_rng(1).integers(0, 256, (7, 5, 3), dtype=np.uint8)
    path = tmp_path / "x.png"
    path.write_bytes(emu.encode_png(rgb))
    width, height, rows = decode_png_rgb(path)
    assert (width, height) == (5, 7)
    assert np.array_equal(np.frombuffer(b"".join(rows), np.uint8).reshape(7, 5, 3), rgb)

    m = emu.FountainForce2(sense_cart(6))
    m.run_frames(3)
    m.save_png(tmp_path / "f.png")
    width, height, rows = decode_png_rgb(tmp_path / "f.png")
    assert (width, height) == (192, 269)
    assert set(b"".join(rows)) == {0, 255}
    assert b"".join(rows) == m.frame_rgb()


def test_missing_extension_raises(monkeypatch):
    import pyvacs  # pylint: disable=import-outside-toplevel

    monkeypatch.setitem(sys.modules, "pyvacs._emu", None)
    monkeypatch.delitem(sys.modules, "pyvacs.emu")
    monkeypatch.delattr(pyvacs, "_emu")
    monkeypatch.delattr(pyvacs, "emu", raising=False)
    with pytest.raises(ImportError, match="Rust toolchain"):
        importlib.import_module("pyvacs.emu")


def test_real_roms_run_deterministically(rom_images):
    def run(image):
        m = emu.FountainForce2(image)
        m.coverage(True)
        m.run_frames(100)
        return m

    for name, image in rom_images.items():
        a, b = run(image), run(image)
        assert (a.clocks, a.frame, a.executed()) == (b.clocks, b.frame, b.executed())
        assert a.illegal == 0, name
        if "subcart" in name:
            continue
        assert np.count_nonzero(np.frombuffer(a.executed(), np.uint8)) > 16, name
        assert len(np.unique(a.frame_array())) > 1, name
