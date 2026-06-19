"""Tests for the two-pass assembler."""

import pytest

from pyvacs import assemble
from pyvacs.assembler import AssemblyError


def asm(source):
    return assemble(source).to_bytes()


def test_inherent_and_register_modes():
    assert asm("nop") == bytes([0xC0])
    assert asm("eorz r0") == bytes([0x20])
    assert asm("comz r3") == bytes([0xE3])


def test_condition_symbols_predefined():
    assert asm("retc,un") == bytes([0x17])
    assert asm("retc,eq") == bytes([0x14])
    assert asm("bcta,gt $1234") == bytes([0x1D, 0x12, 0x34])


def test_numeric_condition_allowed():
    assert asm("retc,3") == bytes([0x17])


def test_immediate_modes():
    assert asm("lodi,r0 $ab") == bytes([0x04, 0xAB])
    assert asm("ppsu 00100000b") == bytes([0x76, 0x20])
    assert asm("cpsl $ff") == bytes([0x75, 0xFF])


def test_org_and_labels():
    program = assemble("        org $0100\n" "start:  bctr,un start\n" "        end\n")
    assert program.symbols["START"] == 0x100
    # bctr,un start: 0x1b, disp = 0x100 - 0x102 = -2 -> 0x7e
    assert program.to_bytes() == bytes([0x1B, 0x7E])


def test_forward_reference():
    program = assemble(
        "        org 0\n"
        "        bcta,un target\n"
        "        nop\n"
        "target: halt\n"
        "        end\n"
    )
    assert program.to_bytes() == bytes([0x1F, 0x00, 0x04, 0xC0, 0x40])


def test_equ_and_set():
    program = assemble("value   equ $2a\n" "        lodi,r0 value\n" "        end\n")
    assert program.symbols["VALUE"] == 0x2A
    assert program.to_bytes() == bytes([0x04, 0x2A])


def test_label_without_colon_then_instruction():
    program = assemble("here halt\n")
    assert program.symbols["HERE"] == 0
    assert program.to_bytes() == bytes([0x40])


def test_db_bytes_and_strings():
    assert asm("db $21,$1e,$ff") == bytes([0x21, 0x1E, 0xFF])
    assert asm('byte "ABC"') == b"ABC"
    assert asm('db "AB",0') == bytes([0x41, 0x42, 0x00])


def test_dbx_pattern():
    assert asm('dbx "....#..."') == bytes([0x08])
    assert asm('dbx "########"') == bytes([0xFF])
    assert asm('dbx "X.X.X.X."') == bytes([0xAA])


def test_dw_and_dd_big_endian():
    assert asm("dw $1234") == bytes([0x12, 0x34])
    assert asm("dw $1234,$5678") == bytes([0x12, 0x34, 0x56, 0x78])
    assert asm("dd $12345678") == bytes([0x12, 0x34, 0x56, 0x78])


def test_ds_storage():
    assert asm("ds 3") == bytes([0, 0, 0])
    assert asm("ds 3,$aa") == bytes([0xAA, 0xAA, 0xAA])


def test_dbfill():
    assert asm("dbfill 4,$01") == bytes([1, 1, 1, 1])


def test_indirect_addressing():
    assert asm("loda,r0 *$0123") == bytes([0x0C, 0x81, 0x23])
    assert asm("bxa *$0123") == bytes([0x9F, 0x81, 0x23])


def test_absx_auto_index_syntax():
    assert asm("loda,r0 $0100,r1+") == bytes([0x0D, 0x21, 0x00])
    assert asm("loda,r0 $0100,r1,+") == bytes([0x0D, 0x21, 0x00])
    assert asm("loda,r0 $0100,r1,-") == bytes([0x0D, 0x41, 0x00])
    assert asm("loda,r0 $0100,r2") == bytes([0x0E, 0x61, 0x00])


def test_idxabs_with_explicit_r3():
    assert asm("bxa $0123,r3") == bytes([0x9F, 0x01, 0x23])


def test_end_with_start_address():
    program = assemble("        org $200\nbegin:  nop\n        end begin\n")
    assert program.start_address == 0x200


def test_listing_directives_ignored():
    program = assemble('name demo\ntitle "x"\npage 60\nwidth 80\nnop\nend\n')
    assert program.to_bytes() == bytes([0xC0])


def test_blank_and_comment_lines():
    assert asm("\n; comment only\n   \nnop\n") == bytes([0xC0])


def test_multiple_org_blocks_padding():
    program = assemble(
        "        org 0\n        db $11\n        org 4\n        db $22\n        end\n"
    )
    assert program.to_bytes(fill=0xFF) == bytes([0x11, 0xFF, 0xFF, 0xFF, 0x22])
    assert len(program.segments) == 2


def test_empty_program():
    program = assemble("")
    assert program.to_bytes() == b""
    assert program.origin == 0


# -- error handling ---------------------------------------------------------


def test_unknown_opcode():
    with pytest.raises(AssemblyError):
        asm("        frobnicate\n")


def test_bare_identifier_in_column_one_is_a_label():
    program = assemble("lonely\n        nop\n")
    assert program.symbols["LONELY"] == 0
    assert program.to_bytes() == bytes([0xC0])


def test_duplicate_symbol():
    with pytest.raises(AssemblyError):
        assemble("foo: nop\nfoo: nop\n")


def test_undefined_symbol_in_pass_two():
    with pytest.raises(AssemblyError) as info:
        asm("lodi,r0 missing")
    assert "undefined" in str(info.value)


def test_register_expected():
    with pytest.raises(AssemblyError):
        asm("eorz $5")


def test_comma_expected():
    with pytest.raises(AssemblyError):
        asm("lodi r0 5")


def test_byte_out_of_range():
    with pytest.raises(AssemblyError):
        asm("db 300")


def test_trailing_operand_rejected():
    with pytest.raises(AssemblyError):
        asm("nop r0")


def test_dbx_requires_eight_chars():
    with pytest.raises(AssemblyError):
        asm('dbx "###"')


def test_assembly_error_reports_line_number():
    with pytest.raises(AssemblyError) as info:
        assemble("        nop\n        nop\n        bogus\n")
    assert info.value.lineno == 3
