"""Two-pass assembler for the Signetics 2650.

The assembler accepts VACS-style source: an optional label, a mnemonic or
directive, operands, and an optional ``;`` comment.  Because every 2650
instruction has a fixed length, the first pass only needs to track the location
counter and record label addresses; the second pass evaluates expressions and
emits bytes.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple

from . import isa
from .lexer import Token, evaluate, tokenize

DIRECTIVES = {
    "ORG",
    "EQU",
    "SET",
    "DB",
    "BYTE",
    "ASCII",
    "DW",
    "WORD",
    "DD",
    "DS",
    "DBX",
    "DBFILL",
    "END",
    "NAME",
    "TITLE",
    "PAGE",
    "PAGELEN",
    "PGLEN",
    "WIDTH",
    "LMARG",
    "LEFTMARG",
    "TABS",
    "OFFS",
    "LIST",
    "NOLIST",
    "NOLST",
    "EJECT",
    "SBTTL",
    "STITLE",
}


class AssemblyError(Exception):
    """Raised on a source error, with the offending line number attached."""

    def __init__(self, message: str, lineno: int) -> None:
        super().__init__(f"line {lineno}: {message}")
        self.lineno = lineno
        self.raw_message = message


@dataclass
class Segment:
    """A contiguous run of emitted bytes starting at ``start``."""

    start: int
    data: bytearray = field(default_factory=bytearray)


@dataclass
class AssembledProgram:
    """Result of assembling a source file."""

    segments: List[Segment]
    symbols: Dict[str, int]
    start_address: int

    def to_bytes(self, fill: int = 0x00) -> bytes:
        """Flatten all segments into one binary, padding gaps with ``fill``."""
        if not self.segments:
            return b""
        lowest = min(seg.start for seg in self.segments)
        highest = max(seg.start + len(seg.data) for seg in self.segments)
        out = bytearray([fill & 0xFF]) * (highest - lowest)
        for seg in self.segments:
            offset = seg.start - lowest
            out[offset : offset + len(seg.data)] = seg.data
        return bytes(out)

    @property
    def origin(self) -> int:
        """Lowest address emitted (0 when nothing was emitted)."""
        if not self.segments:
            return 0
        return min(seg.start for seg in self.segments)


class Assembler:
    """Assemble 2650 source text into bytes."""

    def __init__(self) -> None:
        self.symbols: Dict[str, int] = {}
        self.location = 0
        self.start_address = 0
        self._pass = 1
        self._segments: List[Segment] = []
        self._current: Optional[Segment] = None
        self._lineno = 0
        for name, value in isa.CONDITION_VALUES.items():
            self.symbols[name.upper()] = value

    # -- public API -------------------------------------------------------

    def assemble(self, source: str) -> AssembledProgram:
        """Assemble ``source`` and return the resulting program."""
        lines = source.splitlines()
        self._run_pass(lines, first=True)
        self._run_pass(lines, first=False)
        return AssembledProgram(
            segments=self._segments,
            symbols=dict(self.symbols),
            start_address=self.start_address,
        )

    # -- pass driver ------------------------------------------------------

    def _run_pass(self, lines: List[str], first: bool) -> None:
        self._pass = 1 if first else 2
        self.location = 0
        self.start_address = 0
        self._segments = []
        self._current = None
        for index, line in enumerate(lines, start=1):
            self._lineno = index
            tokens = self._safe_tokenize(line)
            if tokens is None:
                continue
            column0 = bool(line) and not line[0].isspace()
            if self._handle_line(tokens, column0):
                break

    def _safe_tokenize(self, line: str) -> Optional[List[Token]]:
        try:
            return tokenize(line)
        except ValueError as exc:
            raise AssemblyError(str(exc), self._lineno) from exc

    # -- line handling ----------------------------------------------------

    def _handle_line(self, tokens: List[Token], column0: bool) -> bool:
        """Process one tokenised line.  Returns True at END."""
        pos = self._consume_label(tokens, column0)
        tok = tokens[pos]
        if tok.kind == "EOL":
            return False
        if tok.kind != "IDENT":
            raise AssemblyError(f"unexpected token {tok.text!r}", self._lineno)
        name = str(tok.value)
        if name == "END":
            self._directive_end(tokens, pos + 1)
            return True
        if name in DIRECTIVES:
            self._handle_directive(name, tokens, pos + 1)
            return False
        if name in isa.INSTRUCTIONS:
            self._assemble_instruction(name, tokens, pos + 1)
            return False
        raise AssemblyError(f"unknown opcode {tok.text!r}", self._lineno)

    def _consume_label(self, tokens: List[Token], column0: bool) -> int:
        """Define a leading label if present; return the next token index.

        ``column0`` is True when the line had no leading whitespace, which (per
        VACS convention) is what lets a bare identifier with no opcode be a
        label rather than a mistyped instruction.
        """
        first = tokens[0]
        if first.kind != "IDENT":
            return 0
        nxt = tokens[1]
        if nxt.kind == "OP" and nxt.text == ":":
            self._define_label(str(first.value))
            return 2
        if str(first.value) in isa.INSTRUCTIONS or str(first.value) in DIRECTIVES:
            return 0  # the leading word is itself an opcode/directive
        if nxt.kind == "IDENT" and nxt.value in ("EQU", "SET"):
            self._define_label(str(first.value))
            return 1  # the EQU/SET directive sits at index 1
        if nxt.kind == "EOL" and not column0:
            return 0  # an indented lone word is an (unknown) opcode, not a label
        self._define_label(str(first.value))
        return 1

    def _define_label(self, name: str) -> None:
        self._set_symbol(name, self.location)

    def _set_symbol(self, name: str, value: int) -> None:
        key = name.upper()
        if self._pass == 1 and key in self.symbols and key not in _PREDEFINED:
            raise AssemblyError(f"duplicate symbol {name!r}", self._lineno)
        self.symbols[key] = value

    # -- expression helpers ----------------------------------------------

    def _resolve(self, name: str) -> Optional[int]:
        return self.symbols.get(name.upper())

    def _eval(self, tokens: List[Token], pos: int) -> Tuple[int, int]:
        try:
            value, defined, new_pos = evaluate(
                tokens, pos, self._resolve, self.location
            )
        except ValueError as exc:
            raise AssemblyError(str(exc), self._lineno) from exc
        if self._pass == 2 and not defined:
            raise AssemblyError("undefined symbol in expression", self._lineno)
        return value, new_pos

    # -- byte emission ----------------------------------------------------

    def _emit(self, data: bytes) -> None:
        if self._pass == 2:
            if self._current is None or self.location != (
                self._current.start + len(self._current.data)
            ):
                self._current = Segment(self.location)
                self._segments.append(self._current)
            self._current.data.extend(data)
        self.location += len(data)

    # -- directives -------------------------------------------------------

    def _handle_directive(self, name: str, tokens: List[Token], pos: int) -> None:
        handler = {
            "ORG": self._directive_org,
            "EQU": self._directive_equ,
            "SET": self._directive_equ,
            "DB": self._directive_db,
            "BYTE": self._directive_db,
            "ASCII": self._directive_db,
            "DW": self._directive_dw,
            "WORD": self._directive_dw,
            "DD": self._directive_dd,
            "DS": self._directive_ds,
            "DBX": self._directive_dbx,
            "DBFILL": self._directive_dbfill,
        }.get(name)
        if handler is None:
            return  # listing-only directive: ignore the rest of the line
        handler(tokens, pos)

    def _directive_org(self, tokens: List[Token], pos: int) -> None:
        value, _ = self._eval(tokens, pos)
        self.location = value
        self._current = None

    def _directive_equ(self, tokens: List[Token], pos: int) -> None:
        # The label was not consumed yet; it sits at tokens[0].
        if tokens[0].kind != "IDENT":
            raise AssemblyError("EQU needs a label", self._lineno)
        name = str(tokens[0].value)
        value, _ = self._eval(tokens, pos)
        self.symbols[name.upper()] = value

    def _directive_db(self, tokens: List[Token], pos: int) -> None:
        for item, next_pos in self._iter_items(tokens, pos):
            if item.kind == "STRING":
                self._emit(bytes(ord(ch) & 0xFF for ch in str(item.value)))
                pos = next_pos
            else:
                value, pos = self._eval(tokens, pos)
                self._check_range(value, -128, 255, "byte")
                self._emit(bytes([value & 0xFF]))
            pos = self._skip_comma(tokens, pos)

    def _directive_dw(self, tokens: List[Token], pos: int) -> None:
        pos = self._emit_words(tokens, pos, width=2, low=-32768, high=65535)

    def _directive_dd(self, tokens: List[Token], pos: int) -> None:
        pos = self._emit_words(tokens, pos, width=4, low=None, high=None)

    def _emit_words(
        self,
        tokens: List[Token],
        pos: int,
        width: int,
        low: Optional[int],
        high: Optional[int],
    ) -> int:
        while True:
            value, pos = self._eval(tokens, pos)
            if low is not None and high is not None:
                self._check_range(value, low, high, "word")
            self._emit((value & ((1 << (8 * width)) - 1)).to_bytes(width, "big"))
            pos = self._skip_comma(tokens, pos)
            if tokens[pos].kind == "EOL":
                return pos

    def _directive_ds(self, tokens: List[Token], pos: int) -> None:
        count, pos = self._eval(tokens, pos)
        self._check_range(count, 0, 32767, "storage size")
        fill = 0
        if tokens[pos].kind == "OP" and tokens[pos].text == ",":
            fill, pos = self._eval(tokens, pos + 1)
        self._emit(bytes([fill & 0xFF]) * count)

    def _directive_dbfill(self, tokens: List[Token], pos: int) -> None:
        count, pos = self._eval(tokens, pos)
        self._check_range(count, 0, 32767, "fill count")
        fill = 0
        if tokens[pos].kind == "OP" and tokens[pos].text == ",":
            fill, pos = self._eval(tokens, pos + 1)
        self._emit(bytes([fill & 0xFF]) * count)

    def _directive_dbx(self, tokens: List[Token], pos: int) -> None:
        tok = tokens[pos]
        if tok.kind != "STRING":
            raise AssemblyError("DBX needs a quoted pattern", self._lineno)
        pattern = str(tok.value)
        if len(pattern) != 8:
            raise AssemblyError("DBX needs 8 pattern characters", self._lineno)
        value = 0
        for index, ch in enumerate(pattern):
            if ch not in (" ", "."):
                value |= 1 << (7 - index)
        self._emit(bytes([value]))

    def _directive_end(self, tokens: List[Token], pos: int) -> None:
        if tokens[pos].kind != "EOL":
            value, _ = self._eval(tokens, pos)
            self.start_address = value

    # -- item iteration helpers ------------------------------------------

    def _iter_items(self, tokens: List[Token], pos: int):
        """Yield ``(token, next_pos)`` for each comma-separated DB item."""
        while tokens[pos].kind != "EOL":
            yield tokens[pos], pos + 1
            if tokens[pos].kind == "STRING":
                pos += 1
            else:
                _, pos = self._eval(tokens, pos)
            pos = self._skip_comma(tokens, pos)

    @staticmethod
    def _skip_comma(tokens: List[Token], pos: int) -> int:
        if tokens[pos].kind == "OP" and tokens[pos].text == ",":
            return pos + 1
        return pos

    def _check_range(self, value: int, low: int, high: int, what: str) -> None:
        if not low <= value <= high:
            raise AssemblyError(
                f"{what} value {value} out of range {low}..{high}", self._lineno
            )

    # -- instruction assembly --------------------------------------------

    def _assemble_instruction(
        self, mnemonic: str, tokens: List[Token], pos: int
    ) -> None:
        base, mode = isa.INSTRUCTIONS[mnemonic]
        del base
        address = self.location
        ops, pos = self._parse_operands(mode, tokens, pos)
        self._expect_eol(tokens, pos)
        try:
            data = isa.encode(mnemonic, ops, address)
        except isa.EncodingError as exc:
            raise AssemblyError(str(exc), self._lineno) from exc
        self._emit(data)

    def _parse_operands(
        self, mode: str, tokens: List[Token], pos: int
    ) -> Tuple[Dict[str, int], int]:
        parser = {
            "inh": self._parse_inh,
            "reg": self._parse_reg,
            "reg13": self._parse_reg,
            "regz": self._parse_reg,
            "creg": self._parse_creg,
            "ccond": self._parse_ccond,
            "imm": self._parse_imm,
            "regimm": self._parse_regimm,
            "zbr": self._parse_zbr,
            "regrel": self._parse_regaddr,
            "crel": self._parse_caddr,
            "crel3": self._parse_caddr,
            "regabs": self._parse_regaddr,
            "cabs": self._parse_caddr,
            "cabs3": self._parse_caddr,
            "idxabs": self._parse_idxabs,
            "absx": self._parse_absx,
        }[mode]
        return parser(tokens, pos)

    def _parse_inh(self, _tokens: List[Token], pos: int) -> Tuple[Dict[str, int], int]:
        return {}, pos

    def _parse_reg(self, tokens: List[Token], pos: int) -> Tuple[Dict[str, int], int]:
        reg, pos = self._read_register(tokens, pos)
        return {"reg": reg}, pos

    def _parse_creg(self, tokens: List[Token], pos: int) -> Tuple[Dict[str, int], int]:
        pos = self._expect_comma(tokens, pos)
        reg, pos = self._read_register(tokens, pos)
        return {"reg": reg}, pos

    def _parse_ccond(self, tokens: List[Token], pos: int) -> Tuple[Dict[str, int], int]:
        pos = self._expect_comma(tokens, pos)
        cond, pos = self._read_condition(tokens, pos)
        return {"cond": cond}, pos

    def _parse_imm(self, tokens: List[Token], pos: int) -> Tuple[Dict[str, int], int]:
        value, pos = self._eval(tokens, pos)
        return {"value": value}, pos

    def _parse_regimm(
        self, tokens: List[Token], pos: int
    ) -> Tuple[Dict[str, int], int]:
        pos = self._expect_comma(tokens, pos)
        reg, pos = self._read_register(tokens, pos)
        value, pos = self._eval(tokens, pos)
        return {"reg": reg, "value": value}, pos

    def _parse_zbr(self, tokens: List[Token], pos: int) -> Tuple[Dict[str, int], int]:
        indirect, pos = self._read_indirect(tokens, pos)
        target, pos = self._eval(tokens, pos)
        return {"target": target, "indirect": indirect}, pos

    def _parse_regaddr(
        self, tokens: List[Token], pos: int
    ) -> Tuple[Dict[str, int], int]:
        pos = self._expect_comma(tokens, pos)
        reg, pos = self._read_register(tokens, pos)
        indirect, pos = self._read_indirect(tokens, pos)
        target, pos = self._eval(tokens, pos)
        return {"reg": reg, "indirect": indirect, "target": target}, pos

    def _parse_caddr(self, tokens: List[Token], pos: int) -> Tuple[Dict[str, int], int]:
        pos = self._expect_comma(tokens, pos)
        cond, pos = self._read_condition(tokens, pos)
        indirect, pos = self._read_indirect(tokens, pos)
        target, pos = self._eval(tokens, pos)
        return {"cond": cond, "indirect": indirect, "target": target}, pos

    def _parse_idxabs(
        self, tokens: List[Token], pos: int
    ) -> Tuple[Dict[str, int], int]:
        indirect, pos = self._read_indirect(tokens, pos)
        target, pos = self._eval(tokens, pos)
        # Optional ",3" index designator (implied anyway).
        if tokens[pos].kind == "OP" and tokens[pos].text == ",":
            _, pos = self._read_register_or_value(tokens, pos + 1)
        return {"target": target, "indirect": indirect}, pos

    def _parse_absx(self, tokens: List[Token], pos: int) -> Tuple[Dict[str, int], int]:
        pos = self._expect_comma(tokens, pos)
        reg, pos = self._read_register(tokens, pos)
        indirect, pos = self._read_indirect(tokens, pos)
        target, pos = self._eval(tokens, pos)
        index_ctl = isa.INDEX_NONE
        if tokens[pos].kind == "OP" and tokens[pos].text == ",":
            pos += 1
            if tokens[pos].kind == "REG":
                reg = int(tokens[pos].value)
                index_ctl = isa.INDEX_PLAIN
                pos += 1
            if tokens[pos].kind == "OP" and tokens[pos].text == ",":
                pos += 1
            if tokens[pos].kind == "OP" and tokens[pos].text in ("+", "-"):
                index_ctl = (
                    isa.INDEX_AUTO_INC
                    if tokens[pos].text == "+"
                    else isa.INDEX_AUTO_DEC
                )
                pos += 1
        return {
            "reg": reg,
            "indirect": indirect,
            "target": target,
            "index_ctl": index_ctl,
        }, pos

    # -- low-level operand readers ---------------------------------------

    def _read_register(self, tokens: List[Token], pos: int) -> Tuple[int, int]:
        tok = tokens[pos]
        if tok.kind != "REG":
            raise AssemblyError("register expected", self._lineno)
        return int(tok.value), pos + 1

    def _read_register_or_value(self, tokens: List[Token], pos: int) -> Tuple[int, int]:
        if tokens[pos].kind == "REG":
            return int(tokens[pos].value), pos + 1
        return self._eval(tokens, pos)

    def _read_condition(self, tokens: List[Token], pos: int) -> Tuple[int, int]:
        tok = tokens[pos]
        if tok.kind == "NUMBER":
            value = int(tok.value)
        elif tok.kind == "IDENT":
            resolved = self._resolve(str(tok.value))
            if resolved is None:
                if self._pass == 2:
                    raise AssemblyError("undefined condition", self._lineno)
                value = 0
            else:
                value = resolved
        else:
            raise AssemblyError("condition expected", self._lineno)
        if self._pass == 2 and not 0 <= value <= 3:
            raise AssemblyError("condition must be 0..3", self._lineno)
        return value, pos + 1

    def _read_indirect(self, tokens: List[Token], pos: int) -> Tuple[int, int]:
        if tokens[pos].kind == "OP" and tokens[pos].text == "*":
            return 1, pos + 1
        return 0, pos

    def _expect_comma(self, tokens: List[Token], pos: int) -> int:
        if tokens[pos].kind == "OP" and tokens[pos].text == ",":
            return pos + 1
        raise AssemblyError("comma expected", self._lineno)

    def _expect_eol(self, tokens: List[Token], pos: int) -> None:
        if tokens[pos].kind != "EOL":
            raise AssemblyError(
                f"unexpected operand {tokens[pos].text!r}", self._lineno
            )


# Symbols predefined by the assembler that may be shadowed by user EQUs.
_PREDEFINED = {name.upper() for name in isa.CONDITION_VALUES}


def assemble(source: str) -> AssembledProgram:
    """Convenience wrapper: assemble ``source`` and return the program."""
    return Assembler().assemble(source)
