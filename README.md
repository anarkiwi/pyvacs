# pyvacs

A Python assembler **and** disassembler for the Signetics/Philips **2650**
microprocessor family.

`pyvacs` is a clean-room Python reimplementation of the classic
[VACS 1.24](https://github.com/Dennis1000/VACS) assembler by A.C. Verschueren and
W.H. Taphoorn (later ported to Windows by D.D. Spreen). It targets the same
2650 instruction set and opcode encodings, and adds a **round-trip-safe
disassembler** so you can take a ROM image apart and put it back together
byte-for-byte.

The 2650 powered consoles such as the Emerson Arcadia 2001 and its
international clones — including the Australian-market **Fountain Force 2** —
as well as the Interton VC 4000 and a number of arcade boards.

## Features

- Full 2650 instruction set with every addressing mode (inherent, register,
  immediate, relative, absolute, zero-page branch, and absolute-indexed with
  auto-increment/decrement).
- Two-pass assembler with labels, `EQU`/`SET`, expressions, and the
  `ORG`, `DB`/`BYTE`, `DW`/`WORD`, `DD`, `DS`, `DBX`, `DBFILL` directives.
- VACS-style number literals (`$1F`, `%1010`, `&17`, `0FFh`, `1010b`) and a
  full expression evaluator (`+ - * / MOD`, shifts, bitwise, comparisons,
  `HI`/`LO`, `$` location counter, character constants).
- Predefined condition symbols (`eq`, `gt`, `lt`, `un`, plus `z`/`p`/`n`).
- Round-trip-safe disassembler: any byte it cannot decode as a faithful
  instruction is emitted as a `db`, guaranteeing reassembly reproduces the
  original bytes.
- Two command-line tools and a small importable API.

## Install

```sh
pip install pyvacs
```

Or from a checkout:

```sh
pip install -e ".[dev]"
```

## Command line

Assemble source to a binary:

```sh
pyvacs-asm program.asm -o program.bin
```

Disassemble a binary (with an address/hex listing):

```sh
pyvacs-dasm program.bin --listing
```

Disassemble to reassemblable source loaded at a given address:

```sh
pyvacs-dasm game.bin --origin 0x0000 -o game.asm
```

## Library

```python
from pyvacs import assemble, to_source

binary = assemble("""
        org $0000
start:  eorz r0
        bctr,un start
        end start
""").to_bytes()

source = to_source(binary, origin=0)   # round-trips back to `binary`
```

## Example

```asm
        org $0000
start:  eorz  r0              ; clear register 0
        bctr,un go
        retc,un
go:     lodi,r0 $00
        loda,r0 msg,r1,+      ; load, auto-increment index r1
        comi,r0 $ff
        bcfr,eq done
        bcta,un go
done:   nop
msg:    db    $21,$1e,$ff
        end   start
```

## Development

```sh
pip install -e ".[dev]"
pytest                 # tests + coverage (>85% enforced)
black --check src tests
pylint src/pyvacs
```

### ROM test fixtures

The test-suite round-trips real **Fountain Force 2 / Arcadia 2001** cartridge
ROMs through the disassembler and assembler. The ROM images are **not** checked
into this repository: on first run the tests download the
[TOSEC Arcadia 2001 set](https://archive.org/details/Emerson_Arcadia_2001_TOSEC_2012_04_23)
from the Internet Archive and cache it under `tests/.romcache/` (git-ignored).
If the download is unavailable the ROM-dependent tests skip automatically, so
the core suite still runs offline. Set `PYVACS_SKIP_ROM_TESTS=1` to skip them
explicitly.

## License

Apache License 2.0. See [LICENSE](LICENSE).

The 2650 instruction encodings are derived by inspection of the public-domain
VACS sources; no VACS code is included here.
