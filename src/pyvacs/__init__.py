"""pyvacs: a Python 2650 (Signetics/Philips) assembler and disassembler.

A clean-room Python reimplementation of the VACS 1.24 assembler for the
Signetics 2650 microprocessor family, plus a round-trip-safe disassembler.
"""

from .assembler import AssembledProgram, AssemblyError, Assembler, assemble
from .disassembler import Disassembler, Line, disassemble, to_source

__version__ = "0.1.0"

__all__ = [
    "AssembledProgram",
    "AssemblyError",
    "Assembler",
    "assemble",
    "Disassembler",
    "Line",
    "disassemble",
    "to_source",
    "__version__",
]
