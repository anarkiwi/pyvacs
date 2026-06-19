"""Tests for the command-line entry points."""

import pytest

from pyvacs.cli import assemble_main, disassemble_main


def test_assemble_to_file(tmp_path):
    src = tmp_path / "prog.asm"
    src.write_text("        org 0\n        nop\n        halt\n        end\n")
    out = tmp_path / "prog.bin"
    rc = assemble_main([str(src), "-o", str(out)])
    assert rc == 0
    assert out.read_bytes() == bytes([0xC0, 0x40])


def test_assemble_to_stdout(tmp_path, capsysbinary):
    src = tmp_path / "p.asm"
    src.write_text("nop\nend\n")
    rc = assemble_main([str(src)])
    assert rc == 0
    assert capsysbinary.readouterr().out == bytes([0xC0])


def test_assemble_error_returns_nonzero(tmp_path, capsys):
    src = tmp_path / "bad.asm"
    src.write_text("        bogus\n")
    rc = assemble_main([str(src)])
    assert rc == 1
    assert "pyvacs-asm" in capsys.readouterr().err


def test_assemble_missing_file(capsys):
    rc = assemble_main(["/no/such/file.asm"])
    assert rc == 1
    assert "pyvacs-asm" in capsys.readouterr().err


def test_assemble_with_fill(tmp_path):
    src = tmp_path / "p.asm"
    src.write_text("org 0\ndb $11\norg 3\ndb $22\nend\n")
    out = tmp_path / "p.bin"
    assemble_main([str(src), "-o", str(out), "--fill", "0xff"])
    assert out.read_bytes() == bytes([0x11, 0xFF, 0xFF, 0x22])


def test_disassemble_to_source(tmp_path):
    binary = tmp_path / "x.bin"
    binary.write_bytes(bytes([0xC0, 0x40]))
    out = tmp_path / "x.asm"
    rc = disassemble_main([str(binary), "-o", str(out)])
    assert rc == 0
    text = out.read_text()
    assert "NOP" in text and "HALT" in text


def test_disassemble_listing_to_stdout(tmp_path, capsys):
    binary = tmp_path / "x.bin"
    binary.write_bytes(bytes([0x1F, 0x12, 0x34]))
    rc = disassemble_main([str(binary), "--listing", "--origin", "0x100"])
    assert rc == 0
    assert "0100  1F 12 34" in capsys.readouterr().out


def test_disassemble_missing_file(capsys):
    rc = disassemble_main(["/no/such/file.bin"])
    assert rc == 1
    assert "pyvacs-dasm" in capsys.readouterr().err


def test_assemble_version(capsys):
    with pytest.raises(SystemExit) as info:
        assemble_main(["--version"])
    assert info.value.code == 0
    assert "pyvacs" in capsys.readouterr().out
