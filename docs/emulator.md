# Emulator

`pyvacs` ships a native (Rust) emulator of the Signetics 2650 and the Signetics 2637
UVI, plus a Fountain Force 2 / Emerson Arcadia 2001 machine built from them. Emulation
runs with the GIL released, so independent machines scale across threads, and the
extension declares free-threaded CPython support.

## Layout

| Path | Role |
|---|---|
| `crates/s2650` | Generic, machine-agnostic: `Bus` trait, `Cpu` (2650), `Uvi` (2637), `FlatBus` (32 KiB RAM). No PyO3. |
| `crates/fforce2` | The Fountain Force 2 / Arcadia machine: memory map, cartridge, keypads, panel, paddles, line scheduling. |
| `crates/pyvacs-emu` | PyO3 extension `pyvacs._emu`: `S2650` (CPU over `FlatBus`) and `FountainForce2`. |
| `src/pyvacs/emu.py` | numpy views, PNG output, `run_parallel`, `speed`. |
| `crates/fforce2/examples/bench.rs` | `cargo run --release -p fforce2 --example bench -- CART [FRAMES]` |

## Python

```python
from pyvacs import assemble
from pyvacs.emu import S2650, FountainForce2, keymask, run_parallel

cpu = S2650(assemble(src).to_bytes())     # bare CPU over 32 KiB RAM
cpu.run(100_000, stop_pcs=[0x0040])       # GIL released; returns "pc", "clocks" or "halt"
cpu.r0, cpu.psl, cpu.read(0x1000, 16)

m = FountainForce2(cart)                  # region="pal" (default) or "ntsc"
m.run_frames(600)                         # GIL released
m.set_panel(1); m.set_keys(0, keymask(["2"])); m.set_paddle(0, 1, 0xF0)
m.frame_array()                           # (H, 192) colour codes 0-7
m.save_png("shot.png")
m.coverage(True); m.executed()            # per-address opcode execution counts
machines = run_parallel(carts, 600, threads=8)
```

`pyvacs-run CART --frames 600 --png out.png` runs a cartridge headless with a progress
bar and reports the speed as a multiple of real time.

## Timing

| | NTSC (Emerson) | PAL (Fountain Force 2) |
|---|---|---|
| UVI clock | 3.579545 MHz | 3.546895 MHz |
| CPU clock | UVI ÷ 4 | UVI ÷ 4 |
| CPU clocks per line | 227 ÷ 4 | 227 ÷ 4 |
| Lines per frame | 262 | 312 |
| VRST (SENSE high) | last 20 lines | last 43 lines |
| FLAG inverts colours | no | yes |

Lines are scheduled from the CPU clock with exact integer arithmetic. CPU timing is
counted in clocks (one machine cycle is three): 2/3/4-cycle instructions cost 6/9/12,
indirection adds 6, and RETC/RETE cost 9 whether or not they return.

Line 0 is the first line after VRST. VSCROLL and the sprite coordinates are latched
there and the collision registers are set to all-ones; everything else (colours,
character data, UDG/sprite imagery, sizes, modes) is sampled as each line is drawn, so
mid-frame changes take effect on the following lines. CHARLINE follows the documented
sequence (`$FF`, `$F0`..`$FC`, `$FD`, `$FF`). Collision and paddle registers are
read-once/VRST-only as documented; their unused bits read 1.

## Sources

- Signetics 2650 semantics: the 2650 datasheet, cross-checked against MAME's
  `s2650.cpp` (MAME is used as a behavioural reference only).
- UVI and Arcadia behaviour and timing: James Jacobs' (Amigan Software)
  [Signetics-based Machines Coding/Gaming Guide](https://amigan.yatho.com/s-coding.html),
  and MAME's `arcadia` driver.

## Validation

- `crates/*/tests`: per-instruction CPU tests (addressing modes, flags, banks, RAS,
  interrupts, clocks), UVI rendering/collision/CHARLINE/SENSE tests, machine tests.
- `tests/test_emu.py`: programs assembled with `pyvacs` run on `S2650`; the
  `examples/helloworld` cartridge renders a solid green frame in both regions.
- `tests/test_games.py`: every game in the TOSEC Arcadia set (bad/over/hacked dumps,
  the Dictionary sub-cartridges and the Palladium-only Golf excluded) runs 600 frames in
  both regions with no undefined opcodes, deterministically, faster than 20× real time.
- `tests/test_mame_crosscheck.py` (`pytest -m integration`): each game runs in MAME and
  in pyvacs; settled screen memory must match byte for byte, and pressing Start must
  change the screen exactly when it does in MAME. Games that differ for a known reason
  are listed with it in `DIVERGENT`, and the test fails if one of them starts to match.
  The listed divergences come from MAME's own simplifications (46-line vblank, paddle
  defaults, an extra 6 clocks on a taken RETC, collision unused bits, stale CHARLINE);
  with those reproduced, every game but Circus (which polls CHARLINE) matched.

Run the MAME tests locally with Docker (the work directory must be bind-mountable):

```sh
docker build -t pyvacs-mame -f docker/mame.Dockerfile docker
PYVACS_MAME_DOCKER=pyvacs-mame PYVACS_MAME_WORK=$PWD/tests/.mamerun pytest -m integration --no-cov
```

## Limitations

- The UVI's bus contention during display DMA (when the CPU may be paused) is not
  modelled; it is not characterised in available documentation.
- Sound registers are stored and `tone_hz` reported; no audio is synthesised.
- Only the standard cartridge mapping ($0000 and $2000 blocks); the Palladium/Golf
  $4000 mapping and the 2636 PVI (Interton VC 4000) are not implemented.
