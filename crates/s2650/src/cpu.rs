//! Signetics 2650 CPU.
//!
//! Timing is counted in CPU clock periods (one machine cycle is three clocks): a 2-cycle
//! instruction costs 6, a 3-cycle 9, a 4-cycle 12, and indirection adds 6.

use crate::bus::Bus;

/// PSL bits.
pub mod psl {
    pub const C: u8 = 0x01;
    pub const COM: u8 = 0x02;
    pub const OVF: u8 = 0x04;
    pub const WC: u8 = 0x08;
    pub const RS: u8 = 0x10;
    pub const IDC: u8 = 0x20;
    pub const CC: u8 = 0xC0;
}

/// PSU bits.
pub mod psu {
    pub const SP: u8 = 0x07;
    pub const UNUSED: u8 = 0x18;
    pub const II: u8 = 0x20;
    pub const F: u8 = 0x40;
    pub const S: u8 = 0x80;
}

/// CPU state.
///
/// `r` holds the physical registers: `r[0]` is R0, `r[1..=3]` bank 0 R1-R3 and
/// `r[4..=6]` bank 1 R1'-R3'. PSL.RS selects which bank instructions see; switching
/// banks never copies registers.
#[derive(Clone, Debug)]
pub struct Cpu {
    pub r: [u8; 7],
    pub psl: u8,
    /// PSU as last written; bit 7 (SENSE) is sampled from the bus on every read.
    pub psu: u8,
    /// 15-bit instruction address (page in bits 14-13, IAR in bits 12-0).
    pub pc: u16,
    /// Return address stack, indexed by PSU.SP.
    pub ras: [u16; 8],
    /// Set by `HALT`; cleared when an interrupt is taken or on reset.
    pub halted: bool,
    /// Level of the INTREQ input.
    pub irq: bool,
    /// Total clocks executed since reset.
    pub clocks: u64,
    /// Number of undefined opcodes executed (each runs as a 6-clock no-op).
    pub illegal: u64,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu {
    /// A CPU in its reset state.
    pub fn new() -> Self {
        todo!()
    }

    /// Hardware reset: PC=0, registers and RAS cleared, PSU=0, PSL=0, not halted.
    pub fn reset(&mut self) {
        todo!()
    }

    /// Register `n` (0-3) of the bank selected by PSL.RS.
    #[inline]
    pub fn reg(&self, n: usize) -> u8 {
        self.r[Self::index(self.psl, n)]
    }

    /// Set register `n` (0-3) of the bank selected by PSL.RS.
    #[inline]
    pub fn set_reg(&mut self, n: usize, value: u8) {
        self.r[Self::index(self.psl, n)] = value;
    }

    #[inline]
    fn index(psl: u8, n: usize) -> usize {
        if n == 0 || psl & psl::RS == 0 {
            n
        } else {
            n + 3
        }
    }

    /// Execute one instruction (taking a pending interrupt first if INTREQ is high and
    /// PSU.II is clear) and return the clocks it took. While halted, returns 6 without
    /// fetching. Adds the result to `self.clocks`.
    pub fn step<B: Bus>(&mut self, bus: &mut B) -> u32 {
        let _ = bus;
        todo!()
    }
}
