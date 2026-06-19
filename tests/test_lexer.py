"""Tests for the tokeniser and expression evaluator."""

import pytest

from pyvacs.lexer import ExprError, LexError, evaluate, tokenize


def _value(text, symbols=None, location=0):
    symbols = symbols or {}
    tokens = tokenize(text)
    value, defined, _ = evaluate(tokens, 0, lambda n: symbols.get(n), location)
    return value, defined


def test_decimal_and_negative():
    assert _value("42") == (42, True)
    assert _value("-5") == (-5, True)
    assert _value("+7") == (7, True)


def test_hex_formats():
    assert _value("$1F")[0] == 0x1F
    assert _value("0FFh")[0] == 0xFF
    assert _value("1Ah")[0] == 0x1A


def test_binary_and_octal_formats():
    assert _value("%1010")[0] == 0b1010
    assert _value("00100000b")[0] == 0x20
    assert _value("&17")[0] == 0o17
    assert _value("17o")[0] == 0o17
    # VACS treats the 'Q' suffix as base 4: 33q == 3*4 + 3 == 15.
    assert _value("33q")[0] == 15


def test_decimal_suffix():
    assert _value("123d")[0] == 123


def test_arithmetic_precedence():
    assert _value("2+3*4")[0] == 14
    assert _value("(2+3)*4")[0] == 20
    assert _value("10-2-3")[0] == 5
    assert _value("8/2/2")[0] == 2
    assert _value("7 MOD 3")[0] == 1


def test_shifts_and_bitwise():
    assert _value("1 SHL 4")[0] == 16
    assert _value("256 SHR 2")[0] == 64
    assert _value("$F0 AND $0F")[0] == 0
    assert _value("$F0 OR $0F")[0] == 0xFF
    assert _value("$FF XOR $0F")[0] == 0xF0
    assert _value("1 << 3")[0] == 8
    assert _value("64 >> 1")[0] == 32


def test_unary_operators():
    assert _value("HI $1234")[0] == 0x12
    assert _value("LO $1234")[0] == 0x34
    assert _value("HW $12345678")[0] == 0x1234
    assert _value("LW $12345678")[0] == 0x5678
    assert _value("ABS -9")[0] == 9
    assert _value("NOT 0")[0] == -1


def test_comparisons_return_negative_one():
    assert _value("3 = 3")[0] == -1
    assert _value("3 <> 4")[0] == -1
    assert _value("3 < 4")[0] == -1
    assert _value("4 <= 4")[0] == -1
    assert _value("5 > 4")[0] == -1
    assert _value("5 >= 6")[0] == 0


def test_location_counter():
    assert _value("$+2", location=0x100)[0] == 0x102


def test_symbol_resolution():
    assert _value("foo+1", {"FOO": 0x10}) == (0x11, True)
    value, defined = _value("bar", {})
    assert (value, defined) == (0, False)


def test_char_literals():
    assert _value("'A'")[0] == 0x41
    # VACS packs later characters into the high byte (last char shifted up).
    assert _value("'AB'")[0] == 0x4241


def test_division_by_zero():
    with pytest.raises(ExprError):
        _value("1/0")
    with pytest.raises(ExprError):
        _value("1 MOD 0")


def test_negative_division_truncates_toward_zero():
    assert _value("-7/2")[0] == -3
    assert _value("-7 MOD 2")[0] == -1


def test_dollar_token_vs_hex():
    tokens = tokenize("$ $10")
    assert tokens[0].kind == "DOLLAR"
    assert tokens[1].kind == "NUMBER"
    assert tokens[1].value == 0x10


def test_registers_tokenized():
    tokens = tokenize("r0 R3")
    assert tokens[0].kind == "REG" and tokens[0].value == 0
    assert tokens[1].kind == "REG" and tokens[1].value == 3


def test_comment_stripped():
    tokens = tokenize("42 ; this is ignored")
    assert tokens[0].value == 42
    assert tokens[1].kind == "EOL"


def test_unterminated_string():
    with pytest.raises(LexError):
        tokenize('"oops')


def test_bad_number():
    with pytest.raises(LexError):
        tokenize("9z9h")


def test_unexpected_character():
    with pytest.raises(LexError):
        tokenize("?")


def test_missing_paren():
    with pytest.raises(ExprError):
        _value("(1+2")


def test_unexpected_token_in_term():
    with pytest.raises(ExprError):
        _value(")")
