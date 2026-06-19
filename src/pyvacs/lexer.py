"""Tokeniser and expression evaluator for the 2650 assembler.

The number formats and operator set follow the VACS assembler: ``$`` / ``%`` /
``&`` prefixes for hex / binary / octal, and ``H`` / ``O`` / ``Q`` / ``B`` /
``D`` suffixes.  ``$`` on its own is the location counter.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, List, Optional

REGISTERS = {"R0": 0, "R1": 1, "R2": 2, "R3": 3}

# Reserved words that act as operators inside expressions.
WORD_OPERATORS = {
    "AND",
    "OR",
    "XOR",
    "NOT",
    "MOD",
    "SHL",
    "SHR",
    "HI",
    "LO",
    "HW",
    "LW",
    "ABS",
}

# Multi-character punctuation, matched longest-first.
_MULTI_OPS = ("<<", ">>", "<=", ">=", "<>", "!!")
_SINGLE_OPS = set("!%&()*+,-/:<=>\\~^|")


class LexError(ValueError):
    """Raised for malformed tokens."""


@dataclass
class Token:
    """A single lexical token."""

    kind: str  # NUMBER, IDENT, REG, STRING, OP, DOLLAR, EOL
    text: str
    value: object = None


def _is_ident_start(ch: str) -> bool:
    return ch.isalpha() or ch in "_."


def _is_ident_char(ch: str) -> bool:
    return ch.isalnum() or ch in "_.$"


def _parse_int(text: str) -> int:
    """Parse a VACS numeric literal (without a ``$``/``%``/``&`` prefix)."""
    suffixes = {"H": 16, "O": 8, "Q": 4, "B": 2, "D": 10}
    base = 10
    digits = text
    last = text[-1].upper()
    if last in suffixes:
        base = suffixes[last]
        digits = text[:-1]
    if not digits:
        raise LexError(f"invalid number {text!r}")
    try:
        return int(digits, base)
    except ValueError as exc:
        raise LexError(f"invalid number {text!r} for base {base}") from exc


def tokenize(line: str) -> List[Token]:
    """Tokenise one source line, stripping any trailing comment."""
    tokens: List[Token] = []
    i = 0
    length = len(line)
    while i < length:
        ch = line[i]
        if ch in " \t":
            i += 1
            continue
        if ch == ";":
            break
        if ch in "\"'":
            i = _lex_string(line, i, ch, tokens)
            continue
        if ch == "$" and (i + 1 >= length or not _is_hex(line[i + 1])):
            tokens.append(Token("DOLLAR", "$"))
            i += 1
            continue
        if ch in "$%&" and i + 1 < length and _prefixed_digit(ch, line[i + 1]):
            i = _lex_prefixed_number(line, i, ch, tokens)
            continue
        if ch.isdigit():
            i = _lex_number(line, i, tokens)
            continue
        if _is_ident_start(ch):
            i = _lex_ident(line, i, tokens)
            continue
        i = _lex_operator(line, i, tokens)
    tokens.append(Token("EOL", ""))
    return tokens


def _is_hex(ch: str) -> bool:
    return ch in "0123456789abcdefABCDEF"


def _prefixed_digit(prefix: str, ch: str) -> bool:
    if prefix == "$":
        return _is_hex(ch)
    if prefix == "%":
        return ch in "01"
    return ch in "01234567"  # &, octal


def _lex_string(line: str, i: int, quote: str, tokens: List[Token]) -> int:
    i += 1
    start = i
    while i < len(line) and line[i] != quote:
        i += 1
    if i >= len(line):
        raise LexError("unterminated string")
    text = line[start:i]
    tokens.append(Token("STRING", text, text))
    return i + 1


def _lex_prefixed_number(line: str, i: int, prefix: str, tokens: List[Token]) -> int:
    base = {"$": 16, "%": 2, "&": 8}[prefix]
    i += 1
    start = i
    while i < len(line) and _prefixed_digit(prefix, line[i]):
        i += 1
    digits = line[start:i]
    tokens.append(Token("NUMBER", prefix + digits, int(digits, base)))
    return i


def _lex_number(line: str, i: int, tokens: List[Token]) -> int:
    start = i
    while i < len(line) and line[i].isalnum():
        i += 1
    text = line[start:i]
    tokens.append(Token("NUMBER", text, _parse_int(text)))
    return i


def _lex_ident(line: str, i: int, tokens: List[Token]) -> int:
    start = i
    while i < len(line) and _is_ident_char(line[i]):
        i += 1
    text = line[start:i]
    upper = text.upper()
    if upper in REGISTERS:
        tokens.append(Token("REG", text, REGISTERS[upper]))
    else:
        tokens.append(Token("IDENT", text, upper))
    return i


def _lex_operator(line: str, i: int, tokens: List[Token]) -> int:
    for op in _MULTI_OPS:
        if line.startswith(op, i):
            tokens.append(Token("OP", op))
            return i + len(op)
    ch = line[i]
    if ch in _SINGLE_OPS:
        tokens.append(Token("OP", ch))
        return i + 1
    raise LexError(f"unexpected character {ch!r}")


# Map punctuation/word operators onto canonical operator names.
_OP_NAMES = {
    "|": "OR",
    "!": "OR",
    "^": "XOR",
    "!!": "XOR",
    "&": "AND",
    "\\": "NOT",
    "~": "NOT",
    "%": "MOD",
    "<<": "SHL",
    ">>": "SHR",
}


class ExprError(ValueError):
    """Raised when an expression cannot be evaluated."""


class Evaluator:
    """Recursive-descent evaluator over a token list.

    ``resolve`` maps a symbol name to its integer value or ``None`` when the
    symbol is not yet defined.  Undefined symbols make the result undefined; the
    caller decides whether that is an error (pass two) or tolerable (pass one).
    """

    def __init__(
        self,
        tokens: List[Token],
        pos: int,
        resolve: Callable[[str], Optional[int]],
        location: int,
    ) -> None:
        self.tokens = tokens
        self.pos = pos
        self.resolve = resolve
        self.location = location
        self.defined = True

    @property
    def current(self) -> Token:
        """Return the token at the current position."""
        return self.tokens[self.pos]

    def _advance(self) -> None:
        self.pos += 1

    def _op_name(self) -> Optional[str]:
        tok = self.current
        if tok.kind == "OP":
            return _OP_NAMES.get(tok.text)
        if tok.kind == "IDENT" and tok.value in WORD_OPERATORS:
            return str(tok.value)
        return None

    def evaluate(self) -> int:
        """Evaluate the full expression and return its value."""
        return self._or_expr()

    def _or_expr(self) -> int:
        result = self._and_expr()
        while self._op_name() in ("OR", "XOR"):
            name = self._op_name()
            self._advance()
            rhs = self._and_expr()
            result = result | rhs if name == "OR" else result ^ rhs
        return result

    def _and_expr(self) -> int:
        result = self._compare()
        while self._op_name() == "AND":
            self._advance()
            result &= self._compare()
        return result

    def _compare(self) -> int:
        if self._op_name() == "NOT":
            self._advance()
            return ~self._compare()
        result = self._sum()
        ops = {"=": "==", "<>": "!=", "<": "<", "<=": "<=", ">": ">", ">=": ">="}
        if self.current.kind == "OP" and self.current.text in ops:
            symbol = self.current.text
            self._advance()
            rhs = self._sum()
            return -1 if _apply_compare(symbol, result, rhs) else 0
        return result

    def _sum(self) -> int:
        result = self._factor()
        while self.current.kind == "OP" and self.current.text in ("+", "-"):
            symbol = self.current.text
            self._advance()
            rhs = self._factor()
            result = result + rhs if symbol == "+" else result - rhs
        return result

    def _factor(self) -> int:
        result = self._term()
        while self._op_name() in ("MOD", "SHL", "SHR") or (
            self.current.kind == "OP" and self.current.text in ("*", "/")
        ):
            result = self._apply_factor(result)
        return result

    def _apply_factor(self, result: int) -> int:
        name = self._op_name()
        symbol = self.current.text
        self._advance()
        rhs = self._term()
        if symbol == "*":
            return result * rhs
        if symbol == "/":
            return _safe_div(result, rhs)
        if name == "MOD":
            return _safe_mod(result, rhs)
        if name == "SHL":
            return result << rhs
        return result >> rhs  # SHR

    def _term(self) -> int:
        tok = self.current
        if tok.kind == "OP" and tok.text == "-":
            self._advance()
            return -self._term()
        if tok.kind == "OP" and tok.text == "+":
            self._advance()
            return self._term()
        if tok.kind == "OP" and tok.text == "(":
            self._advance()
            value = self._or_expr()
            self._expect(")")
            return value
        name = self._op_name()
        if name in ("HI", "LO", "HW", "LW", "ABS"):
            self._advance()
            return _apply_unary(name, self._term())
        if tok.kind == "DOLLAR":
            self._advance()
            return self.location
        if tok.kind == "NUMBER":
            self._advance()
            return int(tok.value)
        if tok.kind == "STRING":
            self._advance()
            return _string_value(str(tok.value))
        if tok.kind == "IDENT":
            self._advance()
            value = self.resolve(str(tok.value))
            if value is None:
                self.defined = False
                return 0
            return value
        raise ExprError(f"unexpected token {tok.text!r}")

    def _expect(self, text: str) -> None:
        if self.current.kind == "OP" and self.current.text == text:
            self._advance()
        else:
            raise ExprError(f"expected {text!r}")


def _apply_compare(symbol: str, left: int, right: int) -> bool:
    table = {
        "=": left == right,
        "<>": left != right,
        "<": left < right,
        "<=": left <= right,
        ">": left > right,
        ">=": left >= right,
    }
    return table[symbol]


def _apply_unary(name: str, value: int) -> int:
    if name == "HI":
        return (value >> 8) & 0xFF
    if name == "LO":
        return value & 0xFF
    if name == "HW":
        return (value >> 16) & 0xFFFF
    if name == "LW":
        return value & 0xFFFF
    return abs(value)  # ABS


def _string_value(text: str) -> int:
    value = 0
    for ch in reversed(text):
        value = (value << 8) + ord(ch)
    return value


def _safe_div(left: int, right: int) -> int:
    if right == 0:
        raise ExprError("division by zero")
    return int(left / right) if (left < 0) != (right < 0) else left // right


def _safe_mod(left: int, right: int) -> int:
    if right == 0:
        raise ExprError("division by zero")
    return left - _safe_div(left, right) * right


def evaluate(
    tokens: List[Token],
    pos: int,
    resolve: Callable[[str], Optional[int]],
    location: int,
) -> "tuple[int, bool, int]":
    """Evaluate an expression starting at ``tokens[pos]``.

    Returns ``(value, defined, next_pos)``.
    """
    evaluator = Evaluator(tokens, pos, resolve, location)
    value = evaluator.evaluate()
    return value, evaluator.defined, evaluator.pos
