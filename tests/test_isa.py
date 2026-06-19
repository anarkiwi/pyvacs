"""Tests for the instruction-set tables and encoder."""

import pytest

from pyvacs import isa


def test_decode_table_has_no_conflicts_and_is_built():
    # Building the table asserts on conflicts; just confirm it is 256 wide.
    assert len(isa.DECODE_TABLE) == 256


def test_every_instruction_appears_in_decode_table():
    seen = set()
    for entry in isa.DECODE_TABLE:
        if entry is not None:
            seen.add(entry[0])
    assert seen == set(isa.INSTRUCTIONS)


def test_known_opcodes():
    assert isa.encode("NOP", {}, 0) == bytes([0xC0])
    assert isa.encode("HALT", {}, 0) == bytes([0x40])
    assert isa.encode("EORZ", {"reg": 0}, 0) == bytes([0x20])
    assert isa.encode("RETC", {"cond": 3}, 0) == bytes([0x17])


def test_lodz_r0_becomes_iorz():
    assert isa.encode("LODZ", {"reg": 0}, 0) == bytes([0x60])
    assert isa.encode("LODZ", {"reg": 1}, 0) == bytes([0x01])


def test_reg13_rejects_r0():
    with pytest.raises(isa.EncodingError):
        isa.encode("ANDZ", {"reg": 0}, 0)
    assert isa.encode("ANDZ", {"reg": 1}, 0) == bytes([0x41])


def test_relative_branch_displacement():
    # BCTR,un to address 0x102 from 0x100: disp = target - (addr + 2) = 0.
    assert isa.encode("BCTR", {"cond": 3, "target": 0x102}, 0x100) == bytes(
        [0x1B, 0x00]
    )
    # Backwards branch wraps in 7 bits.
    assert isa.encode("BCTR", {"cond": 3, "target": 0x100}, 0x100) == bytes(
        [0x1B, 0x7E]
    )


def test_absolute_branch_with_indirect():
    assert isa.encode("BCTA", {"cond": 3, "target": 0x1234}, 0) == bytes(
        [0x1F, 0x12, 0x34]
    )
    assert isa.encode(
        "BCTA", {"cond": 3, "target": 0x1234, "indirect": True}, 0
    ) == bytes([0x1F, 0x92, 0x34])


def test_absx_index_controls():
    base = isa.encode("LODA", {"reg": 0, "target": 0x0123}, 0)
    assert base == bytes([0x0C, 0x01, 0x23])
    plain = isa.encode(
        "LODA", {"reg": 1, "target": 0x0123, "index_ctl": isa.INDEX_PLAIN}, 0
    )
    assert plain == bytes([0x0D, 0x61, 0x23])
    inc = isa.encode(
        "LODA", {"reg": 1, "target": 0x0123, "index_ctl": isa.INDEX_AUTO_INC}, 0
    )
    assert inc == bytes([0x0D, 0x21, 0x23])


def test_absx_page_boundary_rejected():
    with pytest.raises(isa.EncodingError):
        # 0x2000 is in a different 8K page than address 0.
        isa.encode("LODA", {"reg": 0, "target": 0x2000}, 0)


def test_immediate_range_checks():
    assert isa.encode("LODI", {"reg": 0, "value": -1}, 0) == bytes([0x04, 0xFF])
    with pytest.raises(isa.EncodingError):
        isa.encode("LODI", {"reg": 0, "value": 999}, 0)


def test_condition_branch_rejects_un_for_false_branch():
    with pytest.raises(isa.EncodingError):
        isa.encode("BCFA", {"cond": 3, "target": 0x100}, 0)


def test_length_and_mode_helpers():
    assert isa.length_of("NOP") == 1
    assert isa.length_of("LODI") == 2
    assert isa.length_of("LODA") == 3
    assert isa.mode_of("bcta") == "cabs"
