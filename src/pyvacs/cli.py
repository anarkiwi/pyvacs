"""Command-line interface for pyvacs.

Provides two console entry points:

* ``pyvacs-asm`` - assemble 2650 source into a binary.
* ``pyvacs-dasm`` - disassemble a binary into 2650 source.
"""

from __future__ import annotations

import argparse
import sys
from typing import List, Optional

from . import __version__
from .assembler import Assembler, AssemblyError
from .disassembler import Disassembler


def _read_text(path: str) -> str:
    with open(path, "r", encoding="utf-8") as handle:
        return handle.read()


def _read_bytes(path: str) -> bytes:
    with open(path, "rb") as handle:
        return handle.read()


def assemble_main(argv: Optional[List[str]] = None) -> int:
    """Entry point for ``pyvacs-asm``."""
    parser = argparse.ArgumentParser(
        prog="pyvacs-asm", description="Assemble Signetics 2650 source into a binary."
    )
    parser.add_argument("source", help="assembly source file")
    parser.add_argument("-o", "--output", help="output binary (default: stdout)")
    parser.add_argument(
        "-f",
        "--fill",
        default="0",
        help="pad byte for gaps between ORG blocks (default: 0)",
    )
    parser.add_argument("--version", action="version", version=f"pyvacs {__version__}")
    args = parser.parse_args(argv)

    try:
        program = Assembler().assemble(_read_text(args.source))
    except AssemblyError as exc:
        print(f"pyvacs-asm: {exc}", file=sys.stderr)
        return 1
    except OSError as exc:
        print(f"pyvacs-asm: {exc}", file=sys.stderr)
        return 1

    data = program.to_bytes(fill=int(args.fill, 0))
    if args.output:
        with open(args.output, "wb") as handle:
            handle.write(data)
    else:
        sys.stdout.buffer.write(data)
    return 0


def disassemble_main(argv: Optional[List[str]] = None) -> int:
    """Entry point for ``pyvacs-dasm``."""
    parser = argparse.ArgumentParser(
        prog="pyvacs-dasm",
        description="Disassemble a Signetics 2650 binary into source.",
    )
    parser.add_argument("binary", help="input binary file")
    parser.add_argument("-o", "--output", help="output source (default: stdout)")
    parser.add_argument(
        "-r",
        "--origin",
        default="0",
        help="load address of the binary (default: 0)",
    )
    parser.add_argument(
        "-l",
        "--listing",
        action="store_true",
        help="emit an address/hex listing instead of reassemblable source",
    )
    parser.add_argument("--version", action="version", version=f"pyvacs {__version__}")
    args = parser.parse_args(argv)

    try:
        data = _read_bytes(args.binary)
    except OSError as exc:
        print(f"pyvacs-dasm: {exc}", file=sys.stderr)
        return 1

    origin = int(args.origin, 0)
    disassembler = Disassembler()
    if args.listing:
        text = (
            "\n".join(line.listing() for line in disassembler.disassemble(data, origin))
            + "\n"
        )
    else:
        text = disassembler.to_source(data, origin)

    if args.output:
        with open(args.output, "w", encoding="utf-8") as handle:
            handle.write(text)
    else:
        sys.stdout.write(text)
    return 0
