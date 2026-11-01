"""Command-line interface for pyvacs.

Console entry points:

* ``pyvacs-asm`` - assemble 2650 source into a binary.
* ``pyvacs-dasm`` - disassemble a binary into 2650 source.
* ``pyvacs-run`` - run a cartridge headless on the Fountain Force 2 emulator.
"""

from __future__ import annotations

import argparse
import sys
import time
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


def run_main(argv: Optional[List[str]] = None) -> int:
    """Entry point for ``pyvacs-run``."""
    parser = argparse.ArgumentParser(
        prog="pyvacs-run",
        description="Run a cartridge headless on the Fountain Force 2 emulator.",
    )
    parser.add_argument("cart", help="cartridge image")
    parser.add_argument("--region", choices=("pal", "ntsc"), default="pal")
    parser.add_argument("--frames", type=int, default=300, help="frames to run")
    parser.add_argument("--png", help="write the final frame to this PNG")
    parser.add_argument(
        "--keys", nargs="+", default=[], help="player 1 keys held throughout"
    )
    parser.add_argument(
        "--panel",
        nargs="+",
        default=[],
        choices=("start", "option", "select"),
        help="console buttons held throughout",
    )
    parser.add_argument(
        "--coverage", help="write 32 KiB of per-address execution counts here"
    )
    parser.add_argument("--version", action="version", version=f"pyvacs {__version__}")
    args = parser.parse_args(argv)

    from tqdm import tqdm  # pylint: disable=import-outside-toplevel

    from . import emu  # pylint: disable=import-outside-toplevel

    try:
        machine = emu.FountainForce2(_read_bytes(args.cart), args.region)
        machine.set_keys(0, emu.keymask(args.keys))
    except (OSError, ValueError) as exc:
        print(f"pyvacs-run: {exc}", file=sys.stderr)
        return 1
    machine.set_panel(sum(emu.PANEL[p] for p in set(args.panel)))
    machine.coverage(bool(args.coverage))

    step = max(1, round(machine.fps))
    start = time.perf_counter()
    with tqdm(total=args.frames, unit="frame", file=sys.stderr) as pbar:
        while machine.frames < args.frames:
            n = min(step, args.frames - machine.frames)
            machine.run_frames(n)
            pbar.update(n)
            elapsed = time.perf_counter() - start
            pbar.set_postfix(speed=f"{machine.seconds / max(elapsed, 1e-9):.1f}x")
    elapsed = max(time.perf_counter() - start, 1e-9)

    if args.png:
        machine.save_png(args.png)
    if args.coverage:
        with open(args.coverage, "wb") as handle:
            handle.write(machine.executed())
    print(
        f"{args.frames} frames ({machine.seconds:.2f}s emulated) in {elapsed:.2f}s: "
        f"{machine.seconds / elapsed:.1f}x real time"
    )
    return 0
