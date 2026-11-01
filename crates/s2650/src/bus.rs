//! The 2650's view of the outside world.

/// A non-memory I/O port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Port {
    /// Non-extended control port (`REDC`/`WRTC`, D/~C low).
    Ctrl,
    /// Non-extended data port (`REDD`/`WRTD`, D/~C high).
    Data,
    /// Extended port addressed by the instruction's second byte (`REDE`/`WRTE`).
    Ext(u8),
}

/// Everything the CPU touches outside its registers.
///
/// Addresses are 15-bit (`0x0000..=0x7FFF`). Only `read` and `write` are required.
pub trait Bus {
    /// Data read (operands, indirect pointers).
    fn read(&mut self, addr: u16) -> u8;
    /// Data write.
    fn write(&mut self, addr: u16, value: u8);
    /// Opcode and operand fetch; defaults to `read`.
    #[inline]
    fn fetch(&mut self, addr: u16) -> u8 {
        self.read(addr)
    }
    /// `REDC`/`REDD`/`REDE`.
    #[inline]
    fn read_port(&mut self, _port: Port) -> u8 {
        0
    }
    /// `WRTC`/`WRTD`/`WRTE`.
    #[inline]
    fn write_port(&mut self, _port: Port, _value: u8) {}
    /// Level of the SENSE input (PSU bit 7), sampled whenever PSU is read.
    #[inline]
    fn sense(&mut self) -> bool {
        false
    }
    /// Called when the FLAG output (PSU bit 6) changes.
    #[inline]
    fn flag(&mut self, _level: bool) {}
    /// Data bus byte supplied during interrupt acknowledge (the `ZBSR` displacement).
    #[inline]
    fn intack(&mut self) -> u8 {
        0
    }
}

/// 32 KiB of flat RAM with no peripherals: the generic bus for running bare programs.
pub struct FlatBus {
    pub mem: Box<[u8; 0x8000]>,
    pub sense: bool,
    pub flag: bool,
    pub intack: u8,
}

impl Default for FlatBus {
    fn default() -> Self {
        Self {
            mem: Box::new([0; 0x8000]),
            sense: false,
            flag: false,
            intack: 0,
        }
    }
}

impl FlatBus {
    /// RAM preloaded with `image` at `origin` (truncated at the top of memory).
    pub fn with_image(image: &[u8], origin: u16) -> Self {
        let mut bus = Self::default();
        let start = (origin & 0x7FFF) as usize;
        let n = image.len().min(0x8000 - start);
        bus.mem[start..start + n].copy_from_slice(&image[..n]);
        bus
    }
}

impl Bus for FlatBus {
    #[inline]
    fn read(&mut self, addr: u16) -> u8 {
        self.mem[(addr & 0x7FFF) as usize]
    }
    #[inline]
    fn write(&mut self, addr: u16, value: u8) {
        self.mem[(addr & 0x7FFF) as usize] = value;
    }
    #[inline]
    fn sense(&mut self) -> bool {
        self.sense
    }
    #[inline]
    fn flag(&mut self, level: bool) {
        self.flag = level;
    }
    #[inline]
    fn intack(&mut self) -> u8 {
        self.intack
    }
}
