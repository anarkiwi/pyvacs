"""Disassembler for the Signetics 2650.

The disassembler is *round-trip safe*: every line it emits, when fed back to the
assembler at the same address, reproduces the original bytes exactly.  It
achieves this by decoding each opcode, rendering it as text, re-assembling that
single instruction, and falling back to a ``db`` byte directive whenever the
re-assembled bytes do not match (undefined opcodes, data bytes, or instructions
the assembler would encode differently).
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import List

from . import isa
from .assembler import Assembler


@dataclass
class Line:
    """One disassembled line."""

    address: int
    data: bytes
    text: str

    def listing(self) -> str:
        """Return an address + hex-bytes + text listing row."""
        hexbytes = " ".join(f"{b:02X}" for b in self.data)
        return f"{self.address:04X}  {hexbytes:<11}  {self.text}"


def _fmt_value(value: int) -> str:
    return f"${value:02X}"


def _fmt_addr(value: int) -> str:
    return f"${value:04X}"


def _indirect_prefix(byte: int) -> str:
    return "*" if byte & isa.INDIRECT_BIT else ""


def _decode_text(mnemonic: str, mode: str, selector, data: bytes, addr: int) -> str:
    """Render the textual form of one decoded instruction."""
    renderers = {
        "inh": lambda: mnemonic,
        "reg": lambda: f"{mnemonic} r{selector}",
        "reg13": lambda: f"{mnemonic} r{selector}",
        "regz": lambda: f"{mnemonic} r{selector}",
        "creg": lambda: f"{mnemonic},r{selector}",
        "ccond": lambda: f"{mnemonic},{isa.CONDITION_NAMES[selector]}",
        "imm": lambda: f"{mnemonic} {_fmt_value(data[1])}",
        "regimm": lambda: f"{mnemonic},r{selector} {_fmt_value(data[1])}",
        "zbr": lambda: _render_zbr(mnemonic, data),
        "regrel": lambda: _render_rel(mnemonic, f"r{selector}", data, addr),
        "crel": lambda: _render_rel(mnemonic, _cond(selector), data, addr),
        "crel3": lambda: _render_rel(mnemonic, _cond(selector), data, addr),
        "regabs": lambda: _render_abs(mnemonic, f"r{selector}", data),
        "cabs": lambda: _render_abs(mnemonic, _cond(selector), data),
        "cabs3": lambda: _render_abs(mnemonic, _cond(selector), data),
        "idxabs": lambda: _render_idxabs(mnemonic, data),
        "absx": lambda: _render_absx(mnemonic, selector, data, addr),
    }
    return renderers[mode]()


def _cond(selector: int) -> str:
    return isa.CONDITION_NAMES[selector]


def _render_zbr(mnemonic: str, data: bytes) -> str:
    target = 8128 + (data[1] & 0x3F)
    return f"{mnemonic} {_indirect_prefix(data[1])}{_fmt_addr(target)}"


def _render_rel(mnemonic: str, selector: str, data: bytes, addr: int) -> str:
    disp = data[1] & 0x7F
    if disp >= 0x40:
        disp -= 0x80
    target = addr + 2 + disp
    return f"{mnemonic},{selector} {_indirect_prefix(data[1])}{_fmt_addr(target)}"


def _render_abs(mnemonic: str, selector: str, data: bytes) -> str:
    target = ((data[1] & 0x7F) << 8) | data[2]
    return f"{mnemonic},{selector} {_indirect_prefix(data[1])}{_fmt_addr(target)}"


def _render_idxabs(mnemonic: str, data: bytes) -> str:
    target = ((data[1] & 0x7F) << 8) | data[2]
    return f"{mnemonic} {_indirect_prefix(data[1])}{_fmt_addr(target)}"


def _render_absx(mnemonic: str, reg_field: int, data: bytes, addr: int) -> str:
    index_ctl = data[1] & 0x60
    page = (addr >> 13) << 13
    target = page + (((data[1] & 0x1F) << 8) | data[2])
    indirect = _indirect_prefix(data[1])
    if index_ctl == isa.INDEX_NONE:
        return f"{mnemonic},r{reg_field} {indirect}{_fmt_addr(target)}"
    suffix = {isa.INDEX_AUTO_INC: ",+", isa.INDEX_AUTO_DEC: ",-", isa.INDEX_PLAIN: ""}[
        index_ctl
    ]
    return f"{mnemonic},r0 {indirect}{_fmt_addr(target)},r{reg_field}{suffix}"


class Disassembler:
    """Decode 2650 machine code into assembler source."""

    def __init__(self) -> None:
        self._verifier = Assembler()

    def disassemble(self, data: bytes, origin: int = 0) -> List[Line]:
        """Disassemble ``data`` (loaded at ``origin``) into a list of lines."""
        lines: List[Line] = []
        offset = 0
        length = len(data)
        while offset < length:
            addr = origin + offset
            line = self._decode_one(data, offset, addr, length)
            lines.append(line)
            offset += len(line.data)
        return lines

    def to_source(self, data: bytes, origin: int = 0) -> str:
        """Disassemble ``data`` to reassemblable source text."""
        body = [f"        org {_fmt_addr(origin)}"]
        for line in self.disassemble(data, origin):
            body.append(f"        {line.text}")
        body.append("        end")
        return "\n".join(body) + "\n"

    def _decode_one(self, data: bytes, offset: int, addr: int, length: int) -> Line:
        opcode = data[offset]
        entry = isa.DECODE_TABLE[opcode]
        if entry is not None:
            mnemonic, mode, selector = entry
            size = isa.MODE_LENGTHS[mode]
            if offset + size <= length:
                chunk = data[offset : offset + size]
                text = _decode_text(mnemonic, mode, selector, chunk, addr)
                if self._verify(text, addr, chunk):
                    return Line(addr, chunk, text)
        return Line(addr, data[offset : offset + 1], f"db {_fmt_value(opcode)}")

    def _verify(self, text: str, addr: int, expected: bytes) -> bool:
        """Re-assemble ``text`` at ``addr`` and confirm it matches ``expected``."""
        source = f"        org {_fmt_addr(addr)}\n        {text}\n        end\n"
        try:
            program = Assembler().assemble(source)
        except Exception:  # pylint: disable=broad-except
            return False
        return program.to_bytes() == expected


def disassemble(data: bytes, origin: int = 0) -> List[Line]:
    """Convenience wrapper returning disassembled lines."""
    return Disassembler().disassemble(data, origin)


def to_source(data: bytes, origin: int = 0) -> str:
    """Convenience wrapper returning reassemblable source text."""
    return Disassembler().to_source(data, origin)
