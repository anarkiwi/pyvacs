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
        self.cpu_hz as f64 * self.clocks_den as f64
            / (self.clocks_num as f64 * self.lines as f64)
    }
}

/// Framebuffer width in pixels: the 128-pixel character field with a 32-pixel border on
/// each side.
pub const WIDTH: usize = 192;
/// X of the character field's left edge in the framebuffer.
pub const XPOS: usize = 32;
