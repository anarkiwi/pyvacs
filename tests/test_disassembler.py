"""Tests for the disassembler."""

from pyvacs import disassemble, to_source
from pyvacs.disassembler import Disassembler


def texts(data, origin=0):
    return [line.text for line in disassemble(data, origin)]


def test_inherent_and_register():
    assert texts(bytes([0xC0])) == ["NOP"]
    assert texts(bytes([0x20])) == ["EORZ r0"]


def test_condition_rendered_symbolically():
    assert texts(bytes([0x17])) == ["RETC,un"]
    assert texts(bytes([0x14])) == ["RETC,eq"]


def test_immediate():
    assert texts(bytes([0x04, 0xAB])) == ["LODI,r0 $AB"]


def test_relative_branch_target():
    # bctr,un at 0x100 with disp 0 targets 0x102.
    assert texts(bytes([0x1B, 0x00]), origin=0x100) == ["BCTR,un $0102"]


def test_absolute_branch():
    assert texts(bytes([0x1F, 0x12, 0x34])) == ["BCTA,un $1234"]


def test_indirect_rendered():
    assert texts(bytes([0x0C, 0x81, 0x23])) == ["LODA,r0 *$0123"]


def test_absx_auto_increment():
    assert texts(bytes([0x0D, 0x21, 0x23])) == ["LODA,r0 $0123,r1,+"]


def test_idxabs():
    assert texts(bytes([0x9F, 0x01, 0x23])) == ["BXA $0123"]


def test_undefined_opcode_falls_back_to_db():
    # 0x10 is not a defined opcode.
    assert texts(bytes([0x10])) == ["db $10"]


def test_truncated_instruction_falls_back_to_db():
    # A 3-byte LODA opcode with only one byte available.
    assert texts(bytes([0x0C])) == ["db $0C"]


def test_lodz_r0_byte_is_not_emitted_as_lodz():
    # 0x00 would decode as "LODZ r0" but reassembles to 0x60, so it becomes db.
    assert texts(bytes([0x00])) == ["db $00"]


def test_to_source_is_wrapped_with_org_and_end():
    source = to_source(bytes([0xC0]), origin=0x100)
    assert source.startswith("        org $0100")
    assert source.strip().endswith("end")
    assert "NOP" in source


def test_listing_format():
    lines = Disassembler().disassemble(bytes([0x1F, 0x12, 0x34]), origin=0x100)
    listing = lines[0].listing()
    assert listing.startswith("0100  1F 12 34")
    assert listing.endswith("BCTA,un $1234")
