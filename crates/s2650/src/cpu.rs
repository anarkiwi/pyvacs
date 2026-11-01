//! Signetics 2650 CPU.
//!
//! Timing is counted in CPU clock periods (one machine cycle is three clocks): a 2-cycle
//! instruction costs 6, a 3-cycle 9, a 4-cycle 12, and indirection adds 6.

use crate::bus::{Bus, Port};

/// Page bits of a 15-bit address.
pub const PAGE: u16 = 0x6000;
/// Offset-within-page bits of a 15-bit address.
pub const IAR: u16 = 0x1FFF;
/// Address space mask.
pub const AMSK: u16 = 0x7FFF;

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
        Self {
            r: [0; 7],
            psl: 0,
            psu: 0,
            pc: 0,
            ras: [0; 8],
            halted: false,
            irq: false,
            clocks: 0,
            illegal: 0,
        }
    }

    /// Hardware reset: PC=0, registers and RAS cleared, PSU=0, PSL=0, not halted.
    pub fn reset(&mut self) {
        *self = Self::new();
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

    #[inline(always)]
    fn index(psl: u8, n: usize) -> usize {
        if n == 0 || psl & psl::RS == 0 {
            n
        } else {
            n + 3
        }
    }

    #[inline(always)]
    fn ri(&self, n: u8) -> usize {
        Self::index(self.psl, n as usize)
    }

    /// `addr + delta` wrapped within the 8K page of `addr`.
    #[inline(always)]
    fn in_page(addr: u16, delta: u16) -> u16 {
        (addr & PAGE) | (addr.wrapping_add(delta) & IAR)
    }

    /// Sign-extended 7-bit displacement.
    #[inline(always)]
    fn disp(d: u8) -> u16 {
        (((d << 1) as i8) >> 1) as u16
    }

    #[inline(always)]
    fn next<B: Bus>(&mut self, bus: &mut B) -> u8 {
        let b = bus.fetch(self.pc);
        self.pc = Self::in_page(self.pc, 1);
        b
    }

    #[inline(always)]
    fn skip(&mut self, n: u16) {
        self.pc = Self::in_page(self.pc, n);
    }

    /// Big-endian 15-bit pointer at `addr`, second byte wrapping within the page.
    #[inline(always)]
    fn pointer<B: Bus>(bus: &mut B, addr: u16) -> u16 {
        let hi = bus.read(addr) as u16;
        let lo = bus.read(Self::in_page(addr, 1)) as u16;
        ((hi << 8) | lo) & AMSK
    }

    /// Resolve the indirect flag: `(ea, extra clocks)`.
    #[inline(always)]
    fn deref<B: Bus>(bus: &mut B, ea: u16, indirect: bool) -> (u16, u32) {
        if indirect {
            (Self::pointer(bus, ea), 6)
        } else {
            (ea, 0)
        }
    }

    /// Relative EA from the next operand byte; `zero` makes it relative to address 0.
    #[inline(always)]
    fn rel_ea<B: Bus>(&mut self, bus: &mut B, zero: bool) -> (u16, u32) {
        let d = self.next(bus);
        let ea = if zero {
            Self::disp(d) & IAR
        } else {
            Self::in_page(self.pc, Self::disp(d))
        };
        Self::deref(bus, ea, d & 0x80 != 0)
    }

    /// Absolute non-branch EA: `(operation register index, ea, extra clocks)`.
    #[inline(always)]
    fn abs_ea<B: Bus>(&mut self, bus: &mut B, r: u8) -> (usize, u16, u32) {
        let hr = self.next(bus);
        let dr = self.next(bus);
        let ea = (self.pc & PAGE) | (u16::from_be_bytes([hr, dr]) & IAR);
        let (ea, extra) = Self::deref(bus, ea, hr & 0x80 != 0);
        let x = self.ri(r);
        match hr & 0x60 {
            0x00 => return (x, ea, extra),
            0x20 => self.r[x] = self.r[x].wrapping_add(1),
            0x40 => self.r[x] = self.r[x].wrapping_sub(1),
            _ => {}
        }
        (0, Self::in_page(ea, self.r[x] as u16), extra)
    }

    /// Absolute branch EA (15-bit, bit 15 indirect).
    #[inline(always)]
    fn branch_ea<B: Bus>(&mut self, bus: &mut B) -> (u16, u32) {
        let hr = self.next(bus);
        let dr = self.next(bus);
        Self::deref(bus, u16::from_be_bytes([hr, dr]) & AMSK, hr & 0x80 != 0)
    }

    /// Operand of a Z/I/R/A-mode instruction: `(destination index, value, clocks)`.
    #[inline(always)]
    fn operand<B: Bus>(&mut self, bus: &mut B, op: u8) -> (usize, u8, u32) {
        let r = op & 3;
        match op & 0x0C {
            0x00 => (0, self.r[self.ri(r)], 6),
            0x04 => (self.ri(r), self.next(bus), 6),
            0x08 => {
                let (ea, extra) = self.rel_ea(bus, false);
                (self.ri(r), bus.read(ea), 9 + extra)
            }
            _ => {
                let (d, ea, extra) = self.abs_ea(bus, r);
                (d, bus.read(ea), 12 + extra)
            }
        }
    }

    #[inline(always)]
    fn set_cc(&mut self, v: u8) {
        self.psl = (self.psl & !psl::CC) | (((v != 0) as u8) << 6 << (v >> 7));
    }

    #[inline(always)]
    fn set_cc_bits(&mut self, cc: u8) {
        self.psl = (self.psl & !psl::CC) | cc;
    }

    #[inline(always)]
    fn load(&mut self, d: usize, v: u8) {
        self.r[d] = v;
        self.set_cc(v);
    }

    #[inline(always)]
    fn add(&mut self, d: usize, s: u8) {
        let a = self.r[d];
        let res = a as u16 + s as u16 + (self.psl >> 3 & self.psl & psl::C) as u16;
        let v = res as u8;
        let mut f = self.psl & !(psl::C | psl::OVF | psl::IDC);
        f |= (res >> 8) as u8;
        f |= (v ^ a ^ s) << 1 & psl::IDC;
        f |= ((a ^ v) & (s ^ v)) >> 5 & psl::OVF;
        self.psl = f;
        self.load(d, v);
    }

    #[inline(always)]
    fn sub(&mut self, d: usize, s: u8) {
        let a = self.r[d];
        let borrow = self.psl >> 3 & !self.psl & psl::C;
        let res = (a as u16).wrapping_sub(s as u16 + borrow as u16);
        let v = res as u8;
        let mut f = self.psl & !(psl::C | psl::OVF | psl::IDC);
        f |= ((res >> 8) as u8 & 1) ^ psl::C;
        f |= !(v ^ a ^ s) << 1 & psl::IDC;
        f |= ((a ^ s) & (a ^ v)) >> 5 & psl::OVF;
        self.psl = f;
        self.load(d, v);
    }

    #[inline(always)]
    fn com(&mut self, a: u8, b: u8) {
        let ord = if self.psl & psl::COM != 0 {
            a.cmp(&b)
        } else {
            (a as i8).cmp(&(b as i8))
        };
        self.set_cc_bits(match ord {
            core::cmp::Ordering::Less => 0x80,
            core::cmp::Ordering::Equal => 0x00,
            core::cmp::Ordering::Greater => 0x40,
        });
    }

    /// Rotate by one bit; `left` selects RRL. With WC set the rotation passes through C
    /// and IDC takes the new bit 5.
    #[inline(always)]
    fn rotate(&mut self, d: usize, left: bool) {
        let a = self.r[d];
        let wc = self.psl & psl::WC != 0;
        let (v, out) = match (left, wc) {
            (true, false) => (a.rotate_left(1), None),
            (false, false) => (a.rotate_right(1), None),
            (true, true) => (a << 1 | self.psl & psl::C, Some(a >> 7)),
            (false, true) => (a >> 1 | (self.psl & psl::C) << 7, Some(a & 1)),
        };
        if let Some(c) = out {
            self.psl = (self.psl & !(psl::C | psl::IDC)) | c | (v & psl::IDC);
        }
        self.load(d, v);
        self.psl = (self.psl & !psl::OVF) | ((v ^ a) >> 5 & psl::OVF);
    }

    #[inline(always)]
    fn dar(&mut self, d: usize) {
        let mut v = self.r[d];
        if self.psl & psl::C == 0 {
            v = v.wrapping_add(0xA0);
        }
        if self.psl & psl::IDC == 0 {
            v = (v & 0xF0) | (v.wrapping_add(0x0A) & 0x0F);
        }
        self.r[d] = v;
    }

    #[inline(always)]
    fn read_psu<B: Bus>(&self, bus: &mut B) -> u8 {
        self.psu | if bus.sense() { psu::S } else { 0 }
    }

    #[inline(always)]
    fn write_psu<B: Bus>(&mut self, bus: &mut B, v: u8) {
        if (v ^ self.psu) & psu::F != 0 {
            bus.flag(v & psu::F != 0);
        }
        self.psu = v;
    }

    #[inline(always)]
    fn push(&mut self) {
        let sp = (self.psu.wrapping_add(1)) & psu::SP;
        self.psu = (self.psu & !psu::SP) | sp;
        self.ras[sp as usize] = self.pc;
    }

    #[inline(always)]
    fn pop(&mut self) {
        let sp = self.psu & psu::SP;
        self.pc = self.ras[sp as usize];
        self.psu = (self.psu & !psu::SP) | (sp.wrapping_sub(1) & psu::SP);
    }

    /// Condition-true test of the register field against PSL.CC (3 = always).
    #[inline(always)]
    fn cond(&self, r: u8) -> bool {
        r == 3 || self.psl >> 6 == r
    }

    /// Conditional branch or call; op bit 2 selects absolute over relative.
    #[inline(always)]
    fn branch<B: Bus>(&mut self, bus: &mut B, op: u8, taken: bool, call: bool) -> u32 {
        let abs = op & 4 != 0;
        if !taken {
            self.skip(1 + abs as u16);
            return 9;
        }
        let (ea, extra) = if abs {
            self.branch_ea(bus)
        } else {
            self.rel_ea(bus, false)
        };
        self.jump(ea, call);
        9 + extra
    }

    #[inline(always)]
    fn jump(&mut self, ea: u16, call: bool) {
        if call {
            self.push();
        }
        self.pc = ea;
    }

    /// `ZBRR`/`ZBSR` (`abs` false) or `BXA`/`BSXA` (`abs` true).
    #[inline(always)]
    fn branch_special<B: Bus>(&mut self, bus: &mut B, abs: bool, call: bool) -> u32 {
        let (ea, extra) = if abs {
            let (ea, extra) = self.branch_ea(bus);
            ((ea + self.reg(3) as u16) & AMSK, extra)
        } else {
            self.rel_ea(bus, true)
        };
        self.jump(ea, call);
        9 + extra
    }

    #[inline(always)]
    fn interrupt<B: Bus>(&mut self, bus: &mut B) -> u32 {
        self.halted = false;
        let d = bus.intack();
        let (ea, extra) = Self::deref(bus, Self::disp(d) & IAR, d & 0x80 != 0);
        self.push();
        self.psu |= psu::II;
        self.pc = ea;
        9 + extra
    }

    /// Execute one instruction (taking a pending interrupt first if INTREQ is high and
    /// PSU.II is clear) and return the clocks it took. While halted, returns 6 without
    /// fetching. Adds the result to `self.clocks`.
    pub fn step<B: Bus>(&mut self, bus: &mut B) -> u32 {
        let c = if self.irq && self.psu & psu::II == 0 {
            self.interrupt(bus)
        } else if self.halted {
            6
        } else {
            self.execute(bus)
        };
        self.clocks += c as u64;
        c
    }

    #[inline(always)]
    fn execute<B: Bus>(&mut self, bus: &mut B) -> u32 {
        let op = self.next(bus);
        let r = op & 3;
        let x = self.ri(r);
        match op {
            0x40 => {
                self.halted = true;
                6
            }
            0xC0 => 6,
            0xC1..=0xC3 => {
                self.load(x, self.r[0]);
                6
            }
            0x10 | 0x11 | 0x90 | 0x91 | 0xB6 | 0xB7 | 0xC4..=0xC7 => {
                self.illegal += 1;
                6
            }
            0x00..=0x0F => {
                let (d, v, c) = self.operand(bus, op);
                self.load(d, v);
                c
            }
            0x20..=0x2F => {
                let (d, v, c) = self.operand(bus, op);
                self.load(d, self.r[d] ^ v);
                c
            }
            0x41..=0x4F => {
                let (d, v, c) = self.operand(bus, op);
                self.load(d, self.r[d] & v);
                c
            }
            0x60..=0x6F => {
                let (d, v, c) = self.operand(bus, op);
                self.load(d, self.r[d] | v);
                c
            }
            0x80..=0x8F => {
                let (d, v, c) = self.operand(bus, op);
                self.add(d, v);
                c
            }
            0xA0..=0xAF => {
                let (d, v, c) = self.operand(bus, op);
                self.sub(d, v);
                c
            }
            0xE0..=0xEF => {
                let (d, v, c) = self.operand(bus, op);
                self.com(self.r[d], v);
                c
            }
            0xC8..=0xCB => {
                let (ea, extra) = self.rel_ea(bus, false);
                bus.write(ea, self.r[x]);
                9 + extra
            }
            0xCC..=0xCF => {
                let (d, ea, extra) = self.abs_ea(bus, r);
                bus.write(ea, self.r[d]);
                12 + extra
            }
            0x12 => {
                let v = self.read_psu(bus) & !psu::UNUSED;
                self.load(0, v);
                6
            }
            0x13 => {
                self.load(0, self.psl);
                6
            }
            0x14..=0x17 => {
                if self.cond(r) {
                    self.pop();
                }
                9
            }
            0x34..=0x37 => {
                if self.cond(r) {
                    self.pop();
                    self.psu &= !psu::II;
                }
                9
            }
            0x30..=0x33 => {
                let v = bus.read_port(Port::Ctrl);
                self.load(x, v);
                6
            }
            0x70..=0x73 => {
                let v = bus.read_port(Port::Data);
                self.load(x, v);
                6
            }
            0x54..=0x57 => {
                let p = self.next(bus);
                let v = bus.read_port(Port::Ext(p));
                self.load(x, v);
                9
            }
            0xB0..=0xB3 => {
                bus.write_port(Port::Ctrl, self.r[x]);
                6
            }
            0xF0..=0xF3 => {
                bus.write_port(Port::Data, self.r[x]);
                6
            }
            0xD4..=0xD7 => {
                let p = self.next(bus);
                bus.write_port(Port::Ext(p), self.r[x]);
                9
            }
            0x50..=0x53 => {
                self.rotate(x, false);
                6
            }
            0xD0..=0xD3 => {
                self.rotate(x, true);
                6
            }
            0x74 => {
                let m = self.next(bus) & !psu::S;
                self.write_psu(bus, self.psu & !m);
                9
            }
            0x76 => {
                let m = self.next(bus) & !(psu::S | psu::UNUSED);
                self.write_psu(bus, self.psu | m);
                9
            }
            0x92 => {
                self.write_psu(bus, self.r[0] & !(psu::S | psu::UNUSED));
                6
            }
            0x75 => {
                self.psl &= !self.next(bus);
                9
            }
            0x77 => {
                self.psl |= self.next(bus);
                9
            }
            0x93 => {
                self.psl = self.r[0];
                6
            }
            0x94..=0x97 => {
                self.dar(x);
                9
            }
            0xB4 => {
                let m = self.next(bus);
                let cc = if self.read_psu(bus) & m != m { 0x80 } else { 0 };
                self.set_cc_bits(cc);
                9
            }
            0xB5 => {
                let m = self.next(bus);
                let cc = if self.psl & m != m { 0x80 } else { 0 };
                self.set_cc_bits(cc);
                9
            }
            0xF4..=0xF7 => {
                let m = self.next(bus);
                let cc = if self.r[x] & m != m { 0x80 } else { 0 };
                self.set_cc_bits(cc);
                9
            }
            0x9B => self.branch_special(bus, false, false),
            0xBB => self.branch_special(bus, false, true),
            0x9F => self.branch_special(bus, true, false),
            0xBF => self.branch_special(bus, true, true),
            0x18..=0x1F => self.branch(bus, op, self.cond(r), false),
            0x38..=0x3F => self.branch(bus, op, self.cond(r), true),
            0x98..=0x9E => self.branch(bus, op, self.psl >> 6 != r, false),
            0xB8..=0xBE => self.branch(bus, op, self.psl >> 6 != r, true),
            0x58..=0x5F => self.branch(bus, op, self.r[x] != 0, false),
            0x78..=0x7F => self.branch(bus, op, self.r[x] != 0, true),
            0xD8..=0xDF => {
                self.r[x] = self.r[x].wrapping_add(1);
                self.branch(bus, op, self.r[x] != 0, false)
            }
            0xF8..=0xFF => {
                self.r[x] = self.r[x].wrapping_sub(1);
                self.branch(bus, op, self.r[x] != 0, false)
            }
        }
    }
}
