"""Round-trip tests against real Fountain Force 2 / Arcadia 2001 ROM images.

These exercise the disassembler and assembler on genuine 2650 machine code.
They skip automatically when the ROM set cannot be downloaded.
"""

from pyvacs import assemble, to_source
from pyvacs.disassembler import Disassembler


def test_rom_fixtures_look_like_2650_code(rom_images):
    # Arcadia 2001 cartridges are multiples of 1K and start in low memory.
    for name, data in rom_images.items():
        assert len(data) >= 1024, name
        assert len(data) % 1024 == 0, name


def test_roms_roundtrip_exactly(rom_images):
    dis = Disassembler()
    for name, data in rom_images.items():
        source = dis.to_source(data, origin=0)
        rebuilt = assemble(source).to_bytes()
        assert rebuilt == data, f"{name} did not round-trip ({len(data)} bytes)"


def test_rom_disassembly_decodes_some_instructions(rom_images):
    # A real ROM should decode to mostly instructions, not all "db" fallbacks.
    dis = Disassembler()
    name, data = next(iter(sorted(rom_images.items())))
    lines = dis.disassemble(data, origin=0)
    instruction_lines = [ln for ln in lines if not ln.text.startswith("db ")]
    assert instruction_lines, name
    # Sanity check that to_source wraps the body.
    assert to_source(data, origin=0).startswith("        org $0000")
