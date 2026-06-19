"""Signetics/Philips 2650 instruction set definition.

This module is the single source of truth for the 2650 opcode table shared by
the assembler and the disassembler.  The opcode values and addressing-mode
behaviour mirror the VACS 1.24 assembler by A.C. Verschueren and W.H. Taphoorn
(see https://github.com/Dennis1000/VACS).

Each instruction is described by a base opcode and an addressing-mode name.
The mode determines how operand bytes are formed (see :func:`encode`) and how a
byte stream is decoded back into operands (see the disassembler).
"""

from __future__ import annotations

from typing import Dict, List, Optional, Tuple

# Condition codes (also predefined as symbols by the assembler).
CONDITION_NAMES = ("eq", "gt", "lt", "un")
CONDITION_VALUES = {
    "eq": 0,
    "z": 0,
    "gt": 1,
    "p": 1,
    "lt": 2,
    "n": 2,
    "un": 3,
}

# Index-control bits encoded in the second byte of an absolute-indexed operand.
INDEX_NONE = 0x00
INDEX_AUTO_INC = 0x20
INDEX_AUTO_DEC = 0x40
INDEX_PLAIN = 0x60

INDIRECT_BIT = 0x80

# mnemonic -> (base opcode, addressing-mode name)
#
# Addressing modes (named after the VACS "ModeN" procedures):
#   inh     - no operand                      (1 byte)
#   reg     - " r"   register 0..3            (1 byte)
#   reg13   - " r"   register 1..3            (1 byte)
#   regz    - " r"   register 0..3, r0 special(1 byte, LODZ)
#   creg    - ",r"   register 0..3            (1 byte)
#   ccond   - ",c"   condition 0..3           (1 byte)
#   imm     - " v"   8-bit value              (2 bytes)
#   regimm  - ",r v" register + 8-bit value   (2 bytes)
#   zbr     - " [*]z" zero-page branch        (2 bytes)
#   regrel  - ",r [*]d" register + relative   (2 bytes)
#   crel    - ",c [*]d" condition + relative  (2 bytes)
#   crel3   - ",c [*]d" condition 0..2        (2 bytes)
#   regabs  - ",r [*]a" register + absolute   (3 bytes)
#   cabs    - ",c [*]a" condition + absolute  (3 bytes)
#   cabs3   - ",c [*]a" condition 0..2        (3 bytes)
#   idxabs  - " [*]a" absolute, implied r3    (3 bytes)
#   absx    - ",r [*]p[,x][,+/-]" abs indexed (3 bytes)
INSTRUCTIONS: Dict[str, Tuple[int, str]] = {
    "ADDA": (0x8C, "absx"),
    "ADDI": (0x84, "regimm"),
    "ADDR": (0x88, "regrel"),
    "ADDZ": (0x80, "reg"),
    "ANDA": (0x4C, "absx"),
    "ANDI": (0x44, "regimm"),
    "ANDR": (0x48, "regrel"),
    "ANDZ": (0x41, "reg13"),
    "BCFA": (0x9C, "cabs3"),
    "BCFR": (0x98, "crel3"),
    "BCTA": (0x1C, "cabs"),
    "BCTR": (0x18, "crel"),
    "BDRA": (0xFC, "regabs"),
    "BDRR": (0xF8, "regrel"),
    "BIRA": (0xDC, "regabs"),
    "BIRR": (0xD8, "regrel"),
    "BRNA": (0x5C, "regabs"),
    "BRNR": (0x58, "regrel"),
    "BSFA": (0xBC, "cabs3"),
    "BSFR": (0xB8, "crel3"),
    "BSNA": (0x7C, "cabs"),
    "BSNR": (0x78, "crel"),
    "BSTA": (0x3C, "cabs"),
    "BSTR": (0x38, "crel"),
    "BSXA": (0xBF, "idxabs"),
    "BXA": (0x9F, "idxabs"),
    "COMA": (0xEC, "absx"),
    "COMI": (0xE4, "regimm"),
    "COMR": (0xE8, "regrel"),
    "COMZ": (0xE0, "reg"),
    "CPSL": (0x75, "imm"),
    "CPSU": (0x74, "imm"),
    "DAR": (0x94, "creg"),
    "EORA": (0x2C, "absx"),
    "EORI": (0x24, "regimm"),
    "EORR": (0x28, "regrel"),
    "EORZ": (0x20, "reg"),
    "HALT": (0x40, "inh"),
    "IORA": (0x6C, "absx"),
    "IORI": (0x64, "regimm"),
    "IORR": (0x68, "regrel"),
    "IORZ": (0x60, "reg"),
    "LODA": (0x0C, "absx"),
    "LODI": (0x04, "regimm"),
    "LODR": (0x08, "regrel"),
    "LODZ": (0x00, "regz"),
    "LPSL": (0x93, "inh"),
    "LPSU": (0x92, "inh"),
    "NOP": (0xC0, "inh"),
    "PPSL": (0x77, "imm"),
    "PPSU": (0x76, "imm"),
    "REDC": (0x30, "creg"),
    "REDD": (0x70, "creg"),
    "REDE": (0x54, "regimm"),
    "RETC": (0x14, "ccond"),
    "RETE": (0x34, "ccond"),
    "RRL": (0xD0, "creg"),
    "RRR": (0x50, "creg"),
    "SPSL": (0x13, "inh"),
    "SPSU": (0x12, "inh"),
    "STRA": (0xCC, "absx"),
    "STRR": (0xC8, "regrel"),
    "STRZ": (0xC1, "reg13"),
    "SUBA": (0xAC, "absx"),
    "SUBI": (0xA4, "regimm"),
    "SUBR": (0xA8, "regrel"),
    "SUBZ": (0xA0, "reg"),
    "TMI": (0xF4, "regimm"),
    "TPSL": (0xB5, "imm"),
    "TPSU": (0xB4, "imm"),
    "WRTC": (0xB0, "creg"),
    "WRTD": (0xF0, "creg"),
    "WRTE": (0xD4, "regimm"),
    "ZBRR": (0x9B, "zbr"),
    "ZBSR": (0xBB, "zbr"),
}

# Opcode value of IORZ, used when assembling "LODZ r0" (VACS Mode2b quirk).
_IORZ_OPCODE = INSTRUCTIONS["IORZ"][0]

# Number of bytes emitted for each addressing mode.
MODE_LENGTHS: Dict[str, int] = {
    "inh": 1,
    "reg": 1,
    "reg13": 1,
    "regz": 1,
    "creg": 1,
    "ccond": 1,
    "imm": 2,
    "regimm": 2,
    "zbr": 2,
    "regrel": 2,
    "crel": 2,
    "crel3": 2,
    "regabs": 3,
    "cabs": 3,
    "cabs3": 3,
    "idxabs": 3,
    "absx": 3,
}


class EncodingError(ValueError):
    """Raised when operands cannot be encoded for an instruction."""


def mode_of(mnemonic: str) -> str:
    """Return the addressing-mode name for ``mnemonic``."""
    return INSTRUCTIONS[mnemonic.upper()][1]


def length_of(mnemonic: str) -> int:
    """Return the encoded length in bytes for ``mnemonic``."""
    return MODE_LENGTHS[mode_of(mnemonic)]


def _check(condition: bool, message: str) -> None:
    if not condition:
        raise EncodingError(message)


def encode(mnemonic: str, ops: Dict[str, int], address: int) -> bytes:
    """Encode a single instruction to bytes.

    ``ops`` carries the resolved operand values for the instruction's mode.
    ``address`` is the location counter of the first byte (needed for relative
    and page-relative addressing).
    """
    base, mode = INSTRUCTIONS[mnemonic.upper()]
    indirect = INDIRECT_BIT if ops.get("indirect") else 0
    return _ENCODERS[mode](base, mode, ops, address, indirect)


def _encode_reg(
    base: int, mode: str, ops: Dict[str, int], _addr: int, _ind: int
) -> bytes:
    reg = int(ops["reg"])
    if mode == "reg13":
        _check(1 <= reg <= 3, "register must be r1..r3")
        return bytes([base + reg - 1])
    _check(0 <= reg <= 3, "register out of range")
    if mode == "regz" and reg == 0:
        return bytes([_IORZ_OPCODE])  # VACS rewrites "LODZ r0" to "IORZ r0"
    return bytes([base + reg])


def _encode_ccond(
    base: int, _mode: str, ops: Dict[str, int], _a: int, _i: int
) -> bytes:
    cond = int(ops["cond"])
    _check(0 <= cond <= 3, "condition out of range")
    return bytes([base + cond])


def _encode_imm(base: int, _mode: str, ops: Dict[str, int], _a: int, _i: int) -> bytes:
    value = int(ops["value"])
    _check(-128 <= value <= 255, "immediate out of range")
    return bytes([base, value & 0xFF])


def _encode_regimm(
    base: int, _mode: str, ops: Dict[str, int], _a: int, _i: int
) -> bytes:
    reg = int(ops["reg"])
    value = int(ops["value"])
    _check(0 <= reg <= 3, "register out of range")
    _check(-128 <= value <= 255, "immediate out of range")
    return bytes([base + reg, value & 0xFF])


def _encode_zbr(base: int, _mode: str, ops: Dict[str, int], _a: int, ind: int) -> bytes:
    target = int(ops["target"])
    return bytes([base, (target & 0x3F) | ind])


def _encode_rel(
    base: int, mode: str, ops: Dict[str, int], addr: int, ind: int
) -> bytes:
    selector = _selector(mode, ops)
    disp = (int(ops["target"]) - (addr + 2)) & 0x7F
    return bytes([base + selector, ind | disp])


def _encode_abs(
    base: int, mode: str, ops: Dict[str, int], _addr: int, ind: int
) -> bytes:
    selector = _selector(mode, ops)
    target = int(ops["target"])
    _check(0 <= target <= 0x7FFF, "absolute address out of range")
    return bytes([base + selector, ind | ((target >> 8) & 0x7F), target & 0xFF])


def _encode_idxabs(
    base: int, _mode: str, ops: Dict[str, int], _a: int, ind: int
) -> bytes:
    target = int(ops["target"])
    _check(0 <= target <= 0x7FFF, "absolute address out of range")
    return bytes([base, ind | ((target >> 8) & 0x7F), target & 0xFF])


def _selector(mode: str, ops: Dict[str, int]) -> int:
    """Return the register or condition that biases the base opcode."""
    if mode.startswith("c"):
        cond = int(ops["cond"])
        if mode in ("crel3", "cabs3"):
            _check(0 <= cond <= 2, "condition must be eq/gt/lt for this branch")
        else:
            _check(0 <= cond <= 3, "condition out of range")
        return cond
    reg = int(ops["reg"])
    _check(0 <= reg <= 3, "register out of range")
    return reg


def _encode_absx(
    base: int, _mode: str, ops: Dict[str, int], addr: int, ind: int
) -> bytes:
    reg_field = int(ops["reg"])
    _check(0 <= reg_field <= 3, "register out of range")
    index_ctl = int(ops.get("index_ctl", INDEX_NONE))
    page = (addr >> 13) << 13
    offset = int(ops["target"]) - page
    _check(0 <= offset <= 0x1FFF, "address crosses an 8K page boundary")
    byte1 = ind | index_ctl | ((offset >> 8) & 0x1F)
    return bytes([base + reg_field, byte1, offset & 0xFF])


# mode name -> encoder. All encoders share the
# (base, mode, ops, address, indirect) signature so dispatch stays uniform.
_ENCODERS = {
    "inh": lambda base, *_: bytes([base]),
    "reg": _encode_reg,
    "reg13": _encode_reg,
    "regz": _encode_reg,
    "creg": _encode_reg,
    "ccond": _encode_ccond,
    "imm": _encode_imm,
    "regimm": _encode_regimm,
    "zbr": _encode_zbr,
    "regrel": _encode_rel,
    "crel": _encode_rel,
    "crel3": _encode_rel,
    "regabs": _encode_abs,
    "cabs": _encode_abs,
    "cabs3": _encode_abs,
    "idxabs": _encode_idxabs,
    "absx": _encode_absx,
}


def _add_footprint(
    table: List[Optional[Tuple[str, str, Optional[int]]]],
    mnemonic: str,
    base: int,
    mode: str,
) -> None:
    """Populate the decode table for one instruction's opcode footprint."""
    spots: List[Tuple[int, Optional[int]]]
    if mode in ("inh", "imm", "zbr", "idxabs"):
        spots = [(base, None)]
    elif mode == "reg13":
        spots = [(base + r - 1, r) for r in (1, 2, 3)]
    elif mode in ("crel3", "cabs3"):
        spots = [(base + c, c) for c in (0, 1, 2)]
    else:  # reg, regz, creg, ccond, regimm, regrel, crel, regabs, cabs, absx
        spots = [(base + n, n) for n in range(4)]

    for opcode, selector in spots:
        if table[opcode] is not None:
            raise AssertionError(
                f"opcode 0x{opcode:02X} claimed by {table[opcode]} and {mnemonic}"
            )
        table[opcode] = (mnemonic, mode, selector)


def _build_decode_table() -> List[Optional[Tuple[str, str, Optional[int]]]]:
    table: List[Optional[Tuple[str, str, Optional[int]]]] = [None] * 256
    for mnemonic, (base, mode) in INSTRUCTIONS.items():
        _add_footprint(table, mnemonic, base, mode)
    return table


# DECODE_TABLE[byte] -> (mnemonic, mode, selector) or None for an undefined byte.
DECODE_TABLE: List[Optional[Tuple[str, str, Optional[int]]]] = _build_decode_table()
