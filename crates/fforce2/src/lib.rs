//! Fountain Force 2: the Australian PAL clone of the Emerson Arcadia 2001.
//!
//! Memory map (15-bit addresses):
//!
//! | address                    | contents                                         |
//! |----------------------------|--------------------------------------------------|
//! | `a & 0x1000 != 0`          | UVI window, offset `a & 0x3FF` (`0x300-0x3FF` mirror `0x100-0x1FF`) |
//! | window `0x100-0x17F`       | CE area: keypads and console panel (read only)   |
//! | `$2000-$2FFF`              | cartridge bytes `0x1000-0x1FFF`                  |
//! | everything else            | cartridge bytes `0x0000-0x0FFF` (`a & 0xFFF`)    |
//!
//! Missing cartridge bytes read `0xFF`; ROM writes are ignored.
//!
//! Keypads are given as 12-bit masks, bit `k` = key [`KEYS`]`[k]` (telephone order).
//! The hardware reads them by column, one nibble per column with the top row in bit 3:
//! column 1 (`1 4 7 Clear`) at `$1900`/`$1904`, column 2 (`2 5 8 0`, also the side fire
//! buttons) at `$1901`/`$1905`, column 3 (`3 6 9 Enter`) at `$1902`/`$1906` for players
//! 1/2. The console panel at `$1908` is bit 0 Start, bit 1 Option (A), bit 2
//! Difficulty/Select (B).

use s2650::uvi::WIDTH;
use s2650::{Bus, Cpu, Uvi, UviTiming};

/// Keypad key names by mask bit.
pub const KEYS: [&str; 12] = [
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "Clear", "0", "Enter",
];
/// Panel mask bits.
pub const START: u8 = 1;
pub const OPTION: u8 = 2;
pub const SELECT: u8 = 4;

/// RGB for each UVI colour code.
pub const PALETTE: [[u8; 3]; 8] = [
    [0xff, 0xff, 0xff],
    [0xff, 0xff, 0x00],
    [0x00, 0xff, 0xff],
    [0x00, 0xff, 0x00],
    [0xff, 0x00, 0xff],
    [0xff, 0x00, 0x00],
    [0x00, 0x00, 0xff],
    [0x00, 0x00, 0x00],
];

const CART: usize = 0x2000;
const CE: u16 = 0x100;

/// Video standard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    /// Fountain Force 2 (colours inverted while FLAG is high).
    Pal,
    /// Emerson Arcadia 2001.
    Ntsc,
}

impl Region {
    pub fn timing(self) -> UviTiming {
        match self {
            Region::Pal => UviTiming::PAL,
            Region::Ntsc => UviTiming::NTSC,
        }
    }
}

/// Why a run returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The requested number of frames completed.
    Frames,
    /// The requested number of clocks elapsed.
    Clocks,
    /// The next instruction is at this breakpoint address.
    Breakpoint(u16),
}

/// Everything on the CPU's bus: cartridge, UVI and input latches.
pub struct Board {
    pub cart: Box<[u8; CART]>,
    pub uvi: Uvi,
    /// CE-area bytes `0x100..0x110`: keypad columns and the panel.
    ce: [u8; 16],
    flag: bool,
}

#[inline(always)]
fn window(a: u16) -> u16 {
    let o = a & 0x3ff;
    if o >= 0x300 {
        o - 0x200
    } else {
        o
    }
}

#[inline(always)]
fn rom(a: u16) -> usize {
    if a & 0x7000 == 0x2000 {
        0x1000 | (a & 0xfff) as usize
    } else {
        (a & 0xfff) as usize
    }
}

impl Board {
    #[inline(always)]
    fn ce(&self, o: u16) -> u8 {
        self.ce.get((o - CE) as usize).copied().unwrap_or(0)
    }

    /// Bus read without side effects.
    pub fn peek(&self, a: u16) -> u8 {
        if a & 0x1000 == 0 {
            return self.cart[rom(a)];
        }
        match window(a) {
            o @ 0x100..=0x17f => self.ce(o),
            o => self.uvi.peek(o),
        }
    }

    /// Store into whatever backs `a`, ROM included; the CE area is not writable.
    pub fn poke(&mut self, a: u16, v: u8) {
        if a & 0x1000 == 0 {
            self.cart[rom(a)] = v;
        } else if !(0x100..0x180).contains(&window(a)) {
            self.uvi.mem[window(a) as usize] = v;
        }
    }
}

impl Bus for Board {
    #[inline(always)]
    fn read(&mut self, a: u16) -> u8 {
        if a & 0x1000 == 0 {
            return self.cart[rom(a)];
        }
        match window(a) {
            o @ 0x100..=0x17f => self.ce(o),
            o => self.uvi.read(o),
        }
    }

    #[inline(always)]
    fn write(&mut self, a: u16, v: u8) {
        if a & 0x1000 != 0 {
            let o = window(a);
            if !(0x100..0x180).contains(&o) {
                self.uvi.write(o, v);
            }
        }
    }

    #[inline(always)]
    fn sense(&mut self) -> bool {
        self.uvi.sense()
    }

    #[inline(always)]
    fn flag(&mut self, level: bool) {
        self.flag = level;
    }
}

/// A complete console: CPU, board, raster scheduler, breakpoints and coverage.
pub struct Machine {
    pub cpu: Cpu,
    pub board: Board,
    acc: u32,
    breakpoints: Box<[u64; 0x200]>,
    nbreak: usize,
    coverage: Option<Box<[u8; 0x8000]>>,
}

impl Machine {
    /// A machine with `cart` inserted (bytes past 8K ignored), in its reset state.
    /// PAL inverts colours on FLAG like the Fountain Force 2; NTSC does not.
    pub fn new(cart: &[u8], region: Region) -> Self {
        let mut c = Box::new([0xff; CART]);
        let n = cart.len().min(CART);
        c[..n].copy_from_slice(&cart[..n]);
        let mut uvi = Uvi::new(region.timing());
        uvi.invert_on_flag = region == Region::Pal;
        Self {
            cpu: Cpu::new(),
            board: Board {
                cart: c,
                uvi,
                ce: [0; 16],
                flag: false,
            },
            acc: 0,
            breakpoints: Box::new([0; 0x200]),
            nbreak: 0,
            coverage: None,
        }
    }

    /// Power-on reset of CPU, UVI and scheduler; cartridge, inputs, breakpoints and
    /// coverage persist.
    pub fn reset(&mut self) {
        self.cpu.reset();
        self.board.uvi.reset();
        self.board.flag = false;
        self.acc = 0;
    }

    pub fn uvi(&self) -> &Uvi {
        &self.board.uvi
    }

    pub fn uvi_mut(&mut self) -> &mut Uvi {
        &mut self.board.uvi
    }

    pub fn timing(&self) -> UviTiming {
        self.board.uvi.timing
    }

    pub fn width(&self) -> usize {
        WIDTH
    }

    pub fn height(&self) -> usize {
        self.timing().active() as usize
    }

    /// Colour codes of the frame being drawn, `width() * height()` row-major.
    pub fn frame(&self) -> &[u8] {
        &self.board.uvi.frame
    }

    /// The frame as RGB24 into `out` (at least `3 * width() * height()` bytes).
    pub fn rgb_frame(&self, out: &mut [u8]) {
        for (px, &c) in out.as_chunks_mut::<3>().0.iter_mut().zip(self.frame()) {
            *px = PALETTE[c as usize & 7];
        }
    }

    pub fn peek(&self, a: u16) -> u8 {
        self.board.peek(a)
    }

    pub fn poke(&mut self, a: u16, v: u8) {
        self.board.poke(a, v)
    }

    /// Keypad state for `player` (0 or 1) as a [`KEYS`]-ordered mask.
    pub fn set_keys(&mut self, player: usize, mask: u16) {
        for c in 0..3 {
            self.board.ce[4 * player + c] =
                (0..4).fold(0, |n, r| n | ((mask >> (3 * r + c) & 1) as u8) << (3 - r));
        }
    }

    /// Console panel buttons ([`START`], [`OPTION`], [`SELECT`]).
    pub fn set_panel(&mut self, mask: u8) {
        self.board.ce[8] = mask & 0xf;
    }

    /// Paddle position for `player`, axis 0 = Y, 1 = X (centre 0x70).
    pub fn set_paddle(&mut self, player: usize, axis: usize, v: u8) {
        self.board.uvi.pots[player][axis] = v;
    }

    /// Stop before executing `pc` (enabled) or remove that breakpoint.
    pub fn set_breakpoint(&mut self, pc: u16, enabled: bool) {
        let (w, b) = ((pc & 0x7fff) as usize >> 6, 1u64 << (pc & 63));
        let was = self.breakpoints[w] & b != 0;
        if was != enabled {
            self.breakpoints[w] ^= b;
            self.nbreak = if enabled {
                self.nbreak + 1
            } else {
                self.nbreak - 1
            };
        }
    }

    pub fn clear_breakpoints(&mut self) {
        self.breakpoints.fill(0);
        self.nbreak = 0;
    }

    /// Enable (clearing previous counts) or disable per-address execution counting.
    pub fn set_coverage(&mut self, enabled: bool) {
        self.coverage = enabled.then(|| Box::new([0; 0x8000]));
    }

    /// Saturating count of opcodes executed at each address, if coverage is enabled.
    pub fn executed(&self) -> Option<&[u8; 0x8000]> {
        self.coverage.as_deref()
    }

    #[inline(always)]
    fn is_break(&self, pc: u16) -> bool {
        self.breakpoints[(pc as usize) >> 6] & (1 << (pc & 63)) != 0
    }

    /// One CPU instruction plus the raster lines it spans; returns its clocks.
    #[inline(always)]
    pub fn step(&mut self) -> u32 {
        if let Some(cov) = self.coverage.as_deref_mut() {
            let cpu = &self.cpu;
            if !cpu.halted && !(cpu.irq && cpu.psu & s2650::cpu::psu::II == 0) {
                let n = &mut cov[cpu.pc as usize];
                *n = n.saturating_add(1);
            }
        }
        let c = self.cpu.step(&mut self.board);
        let t = self.board.uvi.timing;
        self.acc += c * t.clocks_den;
        while self.acc >= t.clocks_num {
            self.acc -= t.clocks_num;
            self.board.uvi.line(self.board.flag);
        }
        c
    }

    fn run_while(&mut self, mut more: impl FnMut(&Self) -> bool, done: Stop) -> Stop {
        if self.nbreak == 0 {
            while more(self) {
                self.step();
            }
            return done;
        }
        let mut first = true;
        while more(self) {
            let pc = self.cpu.pc;
            if !first && self.is_break(pc) {
                return Stop::Breakpoint(pc);
            }
            first = false;
            self.step();
        }
        done
    }

    /// Run until `n` more frames complete or a breakpoint (other than at the
    /// starting PC) is reached.
    pub fn run_frames(&mut self, n: u64) -> Stop {
        let target = self.board.uvi.frames + n;
        self.run_while(|m| m.board.uvi.frames < target, Stop::Frames)
    }

    /// Run at least `n` clocks (finishing the last instruction) or until a breakpoint.
    pub fn run_clocks(&mut self, n: u64) -> Stop {
        let target = self.cpu.clocks + n;
        self.run_while(|m| m.cpu.clocks < target, Stop::Clocks)
    }
}
