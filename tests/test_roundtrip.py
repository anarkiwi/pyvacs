"""Round-trip tests: disassembling then reassembling must be byte-exact."""

from pyvacs import assemble, to_source
from pyvacs.disassembler import Disassembler


def roundtrip(data, origin=0):
    return assemble(to_source(data, origin)).to_bytes()


def test_every_opcode_roundtrips():
    """Each opcode, as the head of a 3-byte instruction, round-trips exactly."""
    dis = Disassembler()
    for opcode in range(256):
        blob = bytes([opcode, 0xAB, 0xCD])
        source = dis.to_source(blob, origin=0x100)
        out = assemble(source).to_bytes()
        assert out == blob, f"opcode 0x{opcode:02X} did not round-trip"


def test_full_byte_sweep_roundtrips():
    blob = bytes(range(256)) + bytes([0xAB, 0xCD, 0xEF] * 32)
    assert roundtrip(blob, origin=0) == blob


def test_roundtrip_across_page_boundary():
    blob = bytes([opcode % 256 for opcode in range(0x1FF0, 0x2010)])
    assert roundtrip(blob, origin=0x1FF0) == blob


def test_program_roundtrip():
    source = """
        org $0100
start:  eorz r0
        bctr,un go
        retc,un
go:     lodi,r0 0
        loda,r0 msg,r1,+
        comi,r0 $ff
        bcfr,eq *start
        stra,r0 $1f9f
        bcta,un start
        ppsu 00100000b
        bxa  *go
        zbrr $1f00
        nop
msg:    db $21,$1e,$ff
        dbx "....#..."
        dw $1234,start
        dd $12345678
        ds 3,$aa
        end start
"""
    binary = assemble(source).to_bytes()
    assert roundtrip(binary, origin=0x100) == binary


def test_disassembly_is_stable_under_second_pass():
    """Disassembling the reassembly should yield identical text."""
    blob = bytes(range(64))
    first = to_source(blob, origin=0)
    binary = assemble(first).to_bytes()
    second = to_source(binary, origin=0)
    assert first == second
