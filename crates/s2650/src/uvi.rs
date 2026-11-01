//! Signetics 2637 UVI (Universal Video Interface): character/block display, four
//! sprites, collision detection, paddle ADC, and the tone/noise sound registers.
//!
//! The UVI is addressed through its own 768-byte window (offsets `0x000..0x300`):
//! `0x000-0x0FF` and `0x200-0x2FF` are its external RAM (screen, sprite coordinates,
//! VSCROLL/sound/CHARLINE registers), `0x180-0x1FF` its internal registers and sprite/UDG
//! imagery. `0x100-0x17F` is the CE (chip-enable) window decoded by the host machine
//! (keypads on the Arcadia); the UVI never answers it. Offsets below are UVI-relative;
//! the Arcadia maps the window at `$1800`.
//!
//! The beam is advanced one raster line at a time with [`Uvi::line`]. Line 0 is the first
//! line after the trailing edge of VRST; lines `0..active()` are rendered into
//! [`Uvi::frame`], and the remaining `timing.vrst_lines` lines are vertical reset, during
//! which SENSE is high.

/// Raster geometry and CPU-to-raster clock ratio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UviTiming {
    /// Raster lines per frame.
    pub lines: u16,
    /// Lines of vertical reset at the end of each frame (SENSE high).
    pub vrst_lines: u16,
    /// CPU clocks per raster line as the fraction `clocks_num / clocks_den`.
    pub clocks_num: u32,
    pub clocks_den: u32,
    /// CPU clock in Hz (for converting emulated time to wall-clock time).
    pub cpu_hz: u32,
}

impl UviTiming {
    /// NTSC: 3.579545 MHz UVI clock, CPU at a quarter of it, 227 UVI clocks per line,
    /// 262 lines of which 20 are VRST.
    pub const NTSC: Self = Self {
        lines: 262,
        vrst_lines: 20,
        clocks_num: 227,
        clocks_den: 4,
        cpu_hz: 894_886,
    };
    /// PAL: 3.546895 MHz UVI clock (4/5 of the 4.433619 MHz subcarrier), CPU at a quarter
    /// of it, 227 UVI clocks per line, 312 lines of which 43 are VRST.
    pub const PAL: Self = Self {
        lines: 312,
        vrst_lines: 43,
        clocks_num: 227,
        clocks_den: 4,
        cpu_hz: 886_724,
    };

    /// Rendered (non-VRST) lines per frame.
    pub const fn active(&self) -> u16 {
        self.lines - self.vrst_lines
    }

    /// Frames per second.
    pub fn fps(&self) -> f64 {
        self.cpu_hz as f64 * self.clocks_den as f64 / (self.clocks_num as f64 * self.lines as f64)
    }
}

/// Framebuffer width in pixels: the 128-pixel character field with a 32-pixel border on
/// each side.
pub const WIDTH: usize = 192;
/// X of the character field's left edge in the framebuffer.
pub const XPOS: usize = 32;

/// Built-in character glyphs for codes `0x00..0x38`, top row first, bit 7 leftmost.
const GLYPHS: [[u8; 8]; 0x38] = [
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80],
    [0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01],
    [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
    [0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    [0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff],
    [0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0],
    [0xff, 0xff, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03],
    [0xff, 0xff, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0],
    [0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xff, 0xff],
    [0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0xff, 0xff],
    [0x01, 0x03, 0x07, 0x0f, 0x1f, 0x3f, 0x7f, 0xff],
    [0x80, 0xc0, 0xe0, 0xf0, 0xf8, 0xfc, 0xfe, 0xff],
    [0xff, 0xfe, 0xfc, 0xf8, 0xf0, 0xe0, 0xc0, 0x80],
    [0xff, 0x7f, 0x3f, 0x1f, 0x0f, 0x07, 0x03, 0x01],
    [0x00, 0x1c, 0x22, 0x26, 0x2a, 0x32, 0x22, 0x1c],
    [0x00, 0x08, 0x18, 0x08, 0x08, 0x08, 0x08, 0x1c],
    [0x00, 0x1c, 0x22, 0x02, 0x0c, 0x10, 0x20, 0x3e],
    [0x00, 0x3e, 0x02, 0x04, 0x0c, 0x02, 0x22, 0x1c],
    [0x00, 0x04, 0x0c, 0x14, 0x24, 0x3e, 0x04, 0x04],
    [0x00, 0x3e, 0x20, 0x3c, 0x02, 0x02, 0x22, 0x1c],
    [0x00, 0x0c, 0x10, 0x20, 0x3c, 0x22, 0x22, 0x1c],
    [0x00, 0x7c, 0x02, 0x04, 0x08, 0x10, 0x10, 0x10],
    [0x00, 0x1c, 0x22, 0x22, 0x1c, 0x22, 0x22, 0x1c],
    [0x00, 0x1c, 0x22, 0x22, 0x3e, 0x02, 0x04, 0x18],
    [0x00, 0x08, 0x14, 0x22, 0x22, 0x3e, 0x22, 0x22],
    [0x00, 0x3c, 0x22, 0x22, 0x3c, 0x22, 0x22, 0x3c],
    [0x00, 0x1c, 0x22, 0x20, 0x20, 0x20, 0x22, 0x1c],
    [0x00, 0x3c, 0x22, 0x22, 0x22, 0x22, 0x22, 0x3c],
    [0x00, 0x3e, 0x20, 0x20, 0x3c, 0x20, 0x20, 0x3e],
    [0x00, 0x3e, 0x20, 0x20, 0x3c, 0x20, 0x20, 0x20],
    [0x00, 0x1e, 0x20, 0x20, 0x20, 0x26, 0x22, 0x1e],
    [0x00, 0x22, 0x22, 0x22, 0x3e, 0x22, 0x22, 0x22],
    [0x00, 0x1c, 0x08, 0x08, 0x08, 0x08, 0x08, 0x1c],
    [0x00, 0x02, 0x02, 0x02, 0x02, 0x02, 0x22, 0x1c],
    [0x00, 0x22, 0x24, 0x28, 0x30, 0x28, 0x24, 0x22],
    [0x00, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x3e],
    [0x00, 0x22, 0x36, 0x2a, 0x2a, 0x22, 0x22, 0x22],
    [0x00, 0x22, 0x22, 0x32, 0x2a, 0x26, 0x22, 0x22],
    [0x00, 0x1c, 0x22, 0x22, 0x22, 0x22, 0x22, 0x1c],
    [0x00, 0x3c, 0x22, 0x22, 0x3c, 0x20, 0x20, 0x20],
    [0x00, 0x1c, 0x22, 0x22, 0x22, 0x2a, 0x24, 0x1a],
    [0x00, 0x3c, 0x22, 0x22, 0x3c, 0x28, 0x24, 0x22],
    [0x00, 0x1c, 0x22, 0x20, 0x1c, 0x02, 0x22, 0x1c],
    [0x00, 0x3e, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08],
    [0x00, 0x22, 0x22, 0x22, 0x22, 0x22, 0x22, 0x1c],
    [0x00, 0x22, 0x22, 0x22, 0x22, 0x22, 0x14, 0x08],
    [0x00, 0x22, 0x22, 0x22, 0x2a, 0x2a, 0x36, 0x22],
    [0x00, 0x22, 0x22, 0x14, 0x08, 0x14, 0x22, 0x22],
    [0x00, 0x22, 0x22, 0x14, 0x08, 0x08, 0x08, 0x08],
    [0x00, 0x3e, 0x02, 0x04, 0x08, 0x10, 0x20, 0x3e],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x08, 0x10],
    [0x00, 0x00, 0x08, 0x08, 0x3e, 0x08, 0x08, 0x00],
    [0x00, 0x08, 0x1e, 0x28, 0x1c, 0x0a, 0x3c, 0x08],
];

/// Block-graphics half-row pattern for a 3-bit cell group (2+3+3 pixels wide).
const TRI: [u8; 8] = [0x00, 0x03, 0x1c, 0x1f, 0xe0, 0xe3, 0xfc, 0xff];

/// Byte-per-pixel mask of a bit pattern: little-endian byte `k` is `0xFF` iff bit `7 - k` is set.
const EXPAND: [u64; 256] = {
    let mut t = [0u64; 256];
    let mut b = 0;
    while b < 256 {
        let mut k = 0;
        while k < 8 {
            if b & (0x80 >> k) != 0 {
                t[b] |= 0xff << (8 * k);
            }
            k += 1;
        }
        b += 1;
    }
    t
};

/// SPRITECOLLIDE bit for each sprite pair `(i, j)`, `i < j`.
const PAIR_BIT: [[u8; 4]; 4] = [[0, 1, 2, 4], [0, 0, 8, 16], [0, 0, 0, 32], [0; 4]];

/// One bit per framebuffer pixel of a raster line.
type Bits = [u64; WIDTH / 64];

/// OR `bits` (bit `k` = pixel `x + k`) into `m`, clipped to the framebuffer.
#[inline(always)]
fn place(m: &mut Bits, bits: u8, x: i32) {
    if x <= -8 || x >= WIDTH as i32 {
        return;
    }
    let (v, x) = if x < 0 {
        ((bits >> -x) as u64, 0)
    } else {
        (bits as u64, x as usize)
    };
    let (w, o) = (x >> 6, x & 63);
    m[w] |= v << o;
    if o > 56 && w + 1 < m.len() {
        m[w + 1] |= v >> (64 - o);
    }
}

#[inline(always)]
fn overlaps(a: &Bits, b: &Bits) -> bool {
    a.iter().zip(b).any(|(x, y)| x & y != 0)
}

const SPLAT: u64 = 0x0101_0101_0101_0101;
const CHARLINE: usize = 0x0ff;
const VSCROLL: usize = 0x0fc;
const PITCH: usize = 0x0fd;
const SOUND: usize = 0x0fe;
const UDG: usize = 0x180;
const PAL0: usize = 0x1f8;
const PAL1: usize = 0x1f9;
const BGCOLLIDE: usize = 0x1fc;
const SPRITECOLLIDE: usize = 0x1fd;
const ROWS: usize = 13;

/// The 2637 UVI: its address window, beam position, framebuffer and host-side inputs.
pub struct Uvi {
    /// Backing store for the whole window (special registers are intercepted on read).
    pub mem: [u8; 0x300],
    pub timing: UviTiming,
    /// Raster line the beam is on (`0..timing.lines`).
    pub line: u16,
    /// Completed frames.
    pub frames: u64,
    /// `WIDTH * timing.active()` colour codes (0 white .. 7 black), row-major.
    pub frame: Vec<u8>,
    /// Paddle ADC inputs `[paddle][axis]`, axis 0 = Y, 1 = X.
    pub pots: [[u8; 2]; 2],
    /// Invert all colours while the CPU FLAG output is high.
    pub invert_on_flag: bool,
    vscroll: u8,
    spos: [[u8; 2]; 4],
    /// Lines `sprite_lines.0..sprite_lines.1` can hold sprite pixels this frame.
    sprite_lines: (u16, u16),
    shift: usize,
    bg_hit: u8,
    spr_hit: u8,
    /// Pixels are drawn only while `frames >= render_from`; earlier lines compute
    /// collisions alone.
    pub render_from: u64,
}

impl Uvi {
    pub fn new(timing: UviTiming) -> Self {
        let mut u = Self {
            mem: [0; 0x300],
            timing,
            line: 0,
            frames: 0,
            frame: vec![0; WIDTH * timing.active() as usize],
            pots: [[0x70; 2]; 2],
            invert_on_flag: true,
            vscroll: 0,
            spos: [[0; 2]; 4],
            sprite_lines: (0, 0),
            shift: 0,
            bg_hit: 0xff,
            spr_hit: 0xff,
            render_from: 0,
        };
        u.reset();
        u
    }

    /// Clear the window, beam and framebuffer; host inputs (`pots`, `invert_on_flag`) persist.
    pub fn reset(&mut self) {
        self.mem = [0; 0x300];
        self.line = 0;
        self.frames = 0;
        self.frame.fill(0);
        self.vscroll = 0;
        self.spos = [[0; 2]; 4];
        self.sprite_lines = (0, 0);
        self.shift = 0;
        self.bg_hit = 0xff;
        self.spr_hit = 0xff;
    }

    /// SENSE input level: high during VRST.
    pub fn sense(&self) -> bool {
        self.line >= self.timing.active()
    }

    /// Row height in raster lines (8, or 16 when double-scanned).
    fn row_height(&self) -> u16 {
        if self.mem[PAL1] & 0x80 != 0 {
            8
        } else {
            16
        }
    }

    /// Character row under the beam and the raster within it, if inside the field.
    fn field_pos(&self) -> Option<(usize, u16)> {
        let d = self.line.checked_sub(255 - self.vscroll as u16)?;
        let h = self.row_height();
        let rows = if self.mem[PAL0] & 0x40 != 0 { 26 } else { 13 };
        let row = (d / h) as usize;
        (row < rows).then_some((row, d % h))
    }

    /// Low nibble of CHARLINE: DMA row number, 0xF before the field / in VRST, 0xD after.
    fn dma(&self) -> u8 {
        if self.sense() || self.line < 255 - self.vscroll as u16 {
            0xf
        } else {
            self.field_pos().map_or(0xd, |(row, _)| (row % ROWS) as u8)
        }
    }

    /// How many further [`Uvi::line`] calls keep SENSE (if `sense`) and CHARLINE (if
    /// `charline`) at their current values and the frame count unchanged, assuming the
    /// window is not written meanwhile.
    pub fn stable_lines(&self, sense: bool, charline: bool) -> u16 {
        self.stable_lines_from(self.line, sense, charline)
    }

    /// [`Uvi::stable_lines`] as seen from beam line `l` of the current frame.
    pub fn stable_lines_from(&self, l: u16, sense: bool, charline: bool) -> u16 {
        let (a, n) = (self.timing.active(), self.timing.lines);
        let mut next = n;
        if sense && l < a {
            next = a;
        }
        if charline && l == 0 && self.mem[VSCROLL] != self.vscroll {
            return 0;
        }
        if charline && l < a {
            let f0 = 255 - self.vscroll as u16;
            let h = self.row_height();
            let rows = if self.mem[PAL0] & 0x40 != 0 { 26 } else { 13 };
            let edge = if l < f0 {
                f0
            } else if l < f0 + rows * h {
                f0 + ((l - f0) / h + 1) * h
            } else {
                a
            };
            next = next.min(edge).min(a);
        }
        next - l - 1
    }

    /// Value a CPU read of `off` would return, without clearing the collision latches.
    pub fn peek(&self, off: u16) -> u8 {
        let off = off as usize;
        match off {
            CHARLINE => 0xf0 | self.dma(),
            BGCOLLIDE => self.bg_hit | 0xf0,
            SPRITECOLLIDE => self.spr_hit | 0xc0,
            0x1fe | 0x1ff if self.sense() => {
                self.pots[0x1ff - off][(self.mem[PAL1] >> 6 & 1) as usize]
            }
            0x1fe | 0x1ff => 0xff,
            _ => self.mem[off],
        }
    }

    /// CPU read of `off`; reading a collision register clears its latch.
    pub fn read(&mut self, off: u16) -> u8 {
        let v = self.peek(off);
        match off as usize {
            BGCOLLIDE => self.bg_hit = 0xff,
            SPRITECOLLIDE => self.spr_hit = 0xff,
            _ => {}
        }
        v
    }

    pub fn write(&mut self, off: u16, v: u8) {
        let off = off as usize;
        if off != CHARLINE && !(BGCOLLIDE..=0x1ff).contains(&off) {
            self.mem[off] = v;
        }
    }

    /// Tone frequency in Hz while the tone generator is enabled and audible.
    pub fn tone_hz(&self) -> Option<f64> {
        let s = self.mem[SOUND];
        let t = &self.timing;
        (s & 8 != 0 && s & 7 != 0).then(|| {
            t.cpu_hz as f64 * t.clocks_den as f64
                / (t.clocks_num as f64 * 2.0 * ((self.mem[PITCH] & 0x7f) as f64 + 1.0))
        })
    }

    /// Render the current raster line with the given FLAG level and advance the beam.
    pub fn line(&mut self, flag: bool) {
        if self.line == 0 {
            self.vscroll = self.mem[VSCROLL];
            for (i, p) in self.spos.iter_mut().enumerate() {
                *p = [self.mem[0x0f0 + 2 * i], self.mem[0x0f1 + 2 * i]];
            }
            self.sprite_lines = self
                .spos
                .iter()
                .filter(|p| p[0] != 0)
                .map(|p| 256 - p[0] as u16)
                .fold((u16::MAX, 0), |(lo, hi), top| {
                    (lo.min(top), hi.max(top + 16))
                });
            self.bg_hit = 0xff;
            self.spr_hit = 0xff;
        }
        if !self.sense() {
            self.render(flag);
        }
        self.line += 1;
        if self.line == self.timing.lines {
            self.line = 0;
            self.frames += 1;
        }
    }

    /// Pattern byte and screen byte of each of the 16 cells of field row `n`, glyph row `r`.
    #[inline(always)]
    fn cells(&self, n: usize, r: usize) -> [(u8, u8); 16] {
        let mem = &self.mem;
        let base = if n < ROWS {
            n * 16
        } else {
            0x200 + (n - ROWS) * 16
        };
        let mut block = mem[PAL0] & 0x80 != 0;
        let mut out = [(0, 0); 16];
        for (o, &b) in out.iter_mut().zip(&mem[base..base + 16]) {
            let code = (b & 0x3f) as usize;
            if code == 0 {
                block = match b {
                    0xc0 => true,
                    0x40 => false,
                    _ => block,
                };
            }
            let pat = if block {
                TRI[code >> (if r < 4 { 0 } else { 3 }) & 7]
            } else if code < GLYPHS.len() {
                GLYPHS[code][r]
            } else {
                mem[UDG + 8 * (code - GLYPHS.len()) + r]
            };
            *o = (pat, b);
        }
        out
    }

    /// Render (or, with `render` off, only collision-check) the current raster line.
    fn render(&mut self, flag: bool) {
        let field = self.field_pos();
        if let Some((_, 0)) = field {
            self.shift = (self.mem[SOUND] >> 5) as usize;
        }
        let render = self.frames >= self.render_from;
        if !render && !(self.sprite_lines.0..self.sprite_lines.1).contains(&self.line) {
            return;
        }
        let mem = &self.mem;
        let mut sprites = [Bits::default(); 4];
        let mut live = 0u8;
        let mut col = [0u8; 4];
        for (i, &[sy, sx]) in self.spos.iter().enumerate() {
            let ctrl = mem[0x1fb - (i >> 1)];
            let short = ctrl & (0x80 >> (i & 1)) != 0;
            col[i] = ctrl >> (3 * (1 - (i & 1))) & 7;
            let d = self.line.wrapping_sub(256 - sy as u16);
            if sy == 0 || d >= if short { 8 } else { 16 } {
                continue;
            }
            let b = mem[UDG + 8 * i + (d >> (!short as u16)) as usize];
            if b != 0 {
                place(
                    &mut sprites[i],
                    b.reverse_bits(),
                    sx as i32 - 43 + XPOS as i32,
                );
                live |= 1 << i;
            }
        }
        if !render && live == 0 {
            return;
        }
        let x0 = XPOS + self.shift;
        let (pal0, pal1) = (mem[PAL0], mem[PAL1]);
        let cells = field.map(|(n, sub)| self.cells(n, (sub >> (self.row_height() >> 4)) as usize));
        if live != 0 {
            let mut fg = Bits::default();
            for (j, &(pat, _)) in cells.iter().flatten().enumerate() {
                place(&mut fg, pat.reverse_bits(), (x0 + 8 * j) as i32);
            }
            for i in 0..4 {
                if live >> i & 1 != 0 {
                    if overlaps(&sprites[i], &fg) {
                        self.bg_hit &= !(1 << i);
                    }
                    for j in i + 1..4 {
                        if live >> j & 1 != 0 && overlaps(&sprites[i], &sprites[j]) {
                            self.spr_hit &= !PAIR_BIT[i][j];
                        }
                    }
                }
            }
        }
        if !render {
            return;
        }
        let y = self.line as usize;
        let row = &mut self.frame[y * WIDTH..(y + 1) * WIDTH];
        row.fill(pal1 & 7);
        let multi = mem[PITCH] & 0x80 != 0;
        for (j, &(pat, b)) in cells.iter().flatten().enumerate() {
            let (fc, bc) = if multi {
                let f = if b & 0x40 != 0 { pal1 } else { pal0 };
                let g = if b & 0x80 != 0 { pal1 } else { pal0 };
                (f >> 3 & 7, g & 7)
            } else {
                (pal1 >> 3 & 1 | b >> 5 & 6, pal1 & 7)
            };
            let m = EXPAND[pat as usize];
            let px = (m & (fc as u64 * SPLAT)) | (!m & (bc as u64 * SPLAT));
            let x = x0 + 8 * j;
            row[x..x + 8].copy_from_slice(&px.to_le_bytes());
        }
        if live != 0 {
            let mut scol = [0u8; 16];
            for s in 1..16 {
                scol[s] = scol[s & (s - 1)] | col[s.trailing_zeros() as usize];
            }
            for w in 0..WIDTH / 64 {
                let mut any = sprites.iter().fold(0, |a, s| a | s[w]);
                while any != 0 {
                    let t = any.trailing_zeros();
                    let set = (0..4).fold(0, |a, i| a | ((sprites[i][w] >> t & 1) as usize) << i);
                    row[w * 64 + t as usize] = scol[set];
                    any &= any - 1;
                }
            }
        }
        if self.invert_on_flag && flag {
            row.iter_mut().for_each(|p| *p ^= 7);
        }
    }
}
