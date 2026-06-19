# Hello World — Arcadia 2001 / Fountain Force 2

A minimal Signetics 2650 program for the Emerson Arcadia 2001 and its clones
(including the Australian **Fountain Force 2**), assembled with `pyvacs`.

It does the small amount of Signetics 2637 (UVI) startup an Arcadia cartridge
needs and then fills the whole screen with a single solid colour — green. A
solid colour is a deterministic "test pattern" that is trivial to verify: an
emulator snapshot should be ~100% green pixels.

## Build

```sh
pyvacs-asm helloworld.asm -o helloworld.bin
```

This produces a 2 KiB cartridge image.

## Run

On any Arcadia 2001 emulator, e.g. [MAME](https://www.mamedev.org/):

```sh
mame arcadia -cart helloworld.bin
```

## Verified end-to-end in CI

`tests/test_emulator_integration.py` assembles this program, runs it headless
in MAME's `arcadia` driver, snapshots the screen, and asserts the result is a
solid green frame. The `integration` job in `.github/workflows/ci.yml` runs it
on every push and pull request.

## Changing the colour

The background colour is the low three bits of the register at `$19F9`:

| value | colour  | value | colour |
|-------|---------|-------|--------|
| `$00` | white   | `$04` | magenta |
| `$01` | yellow  | `$05` | red     |
| `$02` | cyan    | `$06` | blue    |
| `$03` | green   | `$07` | black   |

Edit the `GREEN equ $03` line to paint a different test pattern.
