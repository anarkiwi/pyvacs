//! `pyvacs._emu`: Python bindings for the generic 2650 CPU and the Fountain Force 2.
//!
//! Long-running calls release the GIL (or the free-threaded interpreter's per-object
//! critical section), so independent machines run in parallel threads.

use fforce2::{Machine, Region, Stop};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};
use s2650::{Cpu, FlatBus};

const MEM: usize = 0x8000;

fn span(addr: usize, n: usize) -> impl Iterator<Item = u16> {
    (addr..addr + n).map(|a| (a % MEM) as u16)
}

/// A bare 2650 with 32 KiB of flat RAM.
#[pyclass(module = "pyvacs._emu", subclass)]
struct S2650 {
    cpu: Cpu,
    bus: FlatBus,
}

#[pymethods]
impl S2650 {
    #[new]
    #[pyo3(signature = (image = b"".as_slice(), origin = 0))]
    fn new(image: &[u8], origin: u16) -> Self {
        Self {
            cpu: Cpu::new(),
            bus: FlatBus::with_image(image, origin),
        }
    }

    /// Reset the CPU; memory is kept.
    fn reset(&mut self) {
        self.cpu.reset();
    }

    #[getter]
    fn r0(&self) -> u8 {
        self.cpu.reg(0)
    }
    #[setter]
    fn set_r0(&mut self, v: u8) {
        self.cpu.set_reg(0, v)
    }
    #[getter]
    fn r1(&self) -> u8 {
        self.cpu.reg(1)
    }
    #[setter]
    fn set_r1(&mut self, v: u8) {
        self.cpu.set_reg(1, v)
    }
    #[getter]
    fn r2(&self) -> u8 {
        self.cpu.reg(2)
    }
    #[setter]
    fn set_r2(&mut self, v: u8) {
        self.cpu.set_reg(2, v)
    }
    #[getter]
    fn r3(&self) -> u8 {
        self.cpu.reg(3)
    }
    #[setter]
    fn set_r3(&mut self, v: u8) {
        self.cpu.set_reg(3, v)
    }

    /// All seven physical registers: R0, R1-R3 (bank 0), R1'-R3' (bank 1).
    #[getter]
    fn regs<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        PyList::new(py, self.cpu.r)
    }
    #[setter]
    fn set_regs(&mut self, r: Vec<u8>) -> PyResult<()> {
        self.cpu.r = r
            .try_into()
            .map_err(|_| PyValueError::new_err("regs needs 7 values"))?;
        Ok(())
    }
    #[getter]
    fn psl(&self) -> u8 {
        self.cpu.psl
    }
    #[setter]
    fn set_psl(&mut self, v: u8) {
        self.cpu.psl = v;
    }
    #[getter]
    fn psu(&self) -> u8 {
        self.cpu.psu
    }
    #[setter]
    fn set_psu(&mut self, v: u8) {
        self.cpu.psu = v;
    }
    #[getter]
    fn pc(&self) -> u16 {
        self.cpu.pc
    }
    #[setter]
    fn set_pc(&mut self, v: u16) {
        self.cpu.pc = v & 0x7fff;
    }
    /// Stack pointer (PSU bits 0-2).
    #[getter]
    fn sp(&self) -> u8 {
        self.cpu.psu & s2650::cpu::psu::SP
    }
    /// Return address stack.
    #[getter]
    fn ras(&self) -> [u16; 8] {
        self.cpu.ras
    }
    #[getter]
    fn halted(&self) -> bool {
        self.cpu.halted
    }
    #[getter]
    fn clocks(&self) -> u64 {
        self.cpu.clocks
    }
    #[getter]
    fn illegal(&self) -> u64 {
        self.cpu.illegal
    }
    /// FLAG output level.
    #[getter]
    fn flag(&self) -> bool {
        self.bus.flag
    }
    #[getter]
    fn sense(&self) -> bool {
        self.bus.sense
    }
    #[setter]
    fn set_sense(&mut self, v: bool) {
        self.bus.sense = v;
    }
    #[getter]
    fn irq(&self) -> bool {
        self.cpu.irq
    }
    #[setter]
    fn set_irq(&mut self, v: bool) {
        self.cpu.irq = v;
    }
    /// Byte supplied on interrupt acknowledge.
    #[getter]
    fn intack(&self) -> u8 {
        self.bus.intack
    }
    #[setter]
    fn set_intack(&mut self, v: u8) {
        self.bus.intack = v;
    }

    /// `n` bytes from `addr`, wrapping at 32 KiB.
    fn read<'py>(&self, py: Python<'py>, addr: usize, n: usize) -> Bound<'py, PyBytes> {
        let v: Vec<u8> = span(addr, n).map(|a| self.bus.mem[a as usize]).collect();
        PyBytes::new(py, &v)
    }

    /// Store `data` at `addr`, wrapping at 32 KiB.
    fn write(&mut self, addr: usize, data: &[u8]) {
        for (a, &v) in span(addr, data.len()).zip(data) {
            self.bus.mem[a as usize] = v;
        }
    }

    /// Execute one instruction; returns its clocks.
    fn step(&mut self) -> u32 {
        self.cpu.step(&mut self.bus)
    }

    /// Run up to `max_clocks` clocks. Returns `"pc"` when about to execute an address in
    /// `stop_pcs` (never the starting one), `"halt"` on HALT, else `"clocks"`.
    #[pyo3(signature = (max_clocks, stop_pcs = None))]
    fn run(&mut self, py: Python<'_>, max_clocks: u64, stop_pcs: Option<Vec<u16>>) -> &'static str {
        let mut stops = vec![0u64; MEM / 64];
        for p in stop_pcs.into_iter().flatten() {
            stops[(p as usize % MEM) >> 6] |= 1 << (p & 63);
        }
        let (cpu, bus) = (&mut self.cpu, &mut self.bus);
        py.detach(move || {
            let target = cpu.clocks + max_clocks;
            let mut first = true;
            loop {
                if cpu.halted && !(cpu.irq && cpu.psu & s2650::cpu::psu::II == 0) {
                    return "halt";
                }
                if cpu.clocks >= target {
                    return "clocks";
                }
                if !first && stops[(cpu.pc as usize) >> 6] & (1 << (cpu.pc & 63)) != 0 {
                    return "pc";
                }
                first = false;
                cpu.step(bus);
            }
        })
    }
}

fn stop_name(s: Stop) -> &'static str {
    match s {
        Stop::Frames => "frames",
        Stop::Clocks => "clocks",
        Stop::Breakpoint(_) => "pc",
    }
}

/// A Fountain Force 2 (`region="pal"`) or Emerson Arcadia 2001 (`region="ntsc"`).
#[pyclass(module = "pyvacs._emu", subclass)]
struct FountainForce2 {
    m: Machine,
}

#[pymethods]
impl FountainForce2 {
    #[new]
    #[pyo3(signature = (cart, region = "pal"))]
    fn new(cart: &[u8], region: &str) -> PyResult<Self> {
        let r = match region.to_ascii_lowercase().as_str() {
            "pal" => Region::Pal,
            "ntsc" => Region::Ntsc,
            _ => return Err(PyValueError::new_err("region must be 'pal' or 'ntsc'")),
        };
        Ok(Self {
            m: Machine::new(cart, r),
        })
    }

    fn reset(&mut self) {
        self.m.reset();
    }

    /// Run `n` frames; returns `"frames"`, or `"pc"` at a breakpoint.
    fn run_frames(&mut self, py: Python<'_>, n: u64) -> &'static str {
        let m = &mut self.m;
        stop_name(py.detach(move || m.run_frames(n)))
    }

    /// Run `n` clocks; returns `"clocks"`, or `"pc"` at a breakpoint.
    fn run_clocks(&mut self, py: Python<'_>, n: u64) -> &'static str {
        let m = &mut self.m;
        stop_name(py.detach(move || m.run_clocks(n)))
    }

    /// Execute one instruction; returns its clocks.
    fn step(&mut self) -> u32 {
        self.m.step()
    }

    #[pyo3(signature = (pc, enabled = true))]
    fn set_breakpoint(&mut self, pc: u16, enabled: bool) {
        self.m.set_breakpoint(pc, enabled);
    }

    fn clear_breakpoints(&mut self) {
        self.m.clear_breakpoints();
    }

    /// Colour codes (0-7) of the current frame, row-major `height` x `width`.
    #[getter]
    fn frame<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, self.m.frame())
    }

    /// The current frame as packed RGB24.
    fn frame_rgb<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        PyBytes::new_with(py, 3 * self.m.frame().len(), |b| {
            self.m.rgb_frame(b);
            Ok(())
        })
    }

    #[getter]
    fn width(&self) -> usize {
        self.m.width()
    }
    #[getter]
    fn height(&self) -> usize {
        self.m.height()
    }
    #[getter]
    fn fps(&self) -> f64 {
        self.m.timing().fps()
    }
    #[getter]
    fn cpu_hz(&self) -> u32 {
        self.m.timing().cpu_hz
    }
    #[getter]
    fn frames(&self) -> u64 {
        self.m.uvi().frames
    }
    /// Raster line the beam is on.
    #[getter]
    fn line(&self) -> u16 {
        self.m.uvi().line
    }
    #[getter]
    fn clocks(&self) -> u64 {
        self.m.cpu.clocks
    }
    #[getter]
    fn pc(&self) -> u16 {
        self.m.cpu.pc
    }
    #[getter]
    fn psl(&self) -> u8 {
        self.m.cpu.psl
    }
    #[getter]
    fn psu(&self) -> u8 {
        self.m.cpu.psu
    }
    #[getter]
    fn regs<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        PyList::new(py, self.m.cpu.r)
    }
    #[getter]
    fn halted(&self) -> bool {
        self.m.cpu.halted
    }
    #[getter]
    fn illegal(&self) -> u64 {
        self.m.cpu.illegal
    }
    /// Fast-forward idle loops and skip drawing unobserved frames (results identical).
    #[getter]
    fn fast(&self) -> bool {
        self.m.fast
    }
    #[setter]
    fn set_fast(&mut self, v: bool) {
        self.m.fast = v;
    }
    /// Tone generator frequency in Hz, or None when silent.
    #[getter]
    fn tone_hz(&self) -> Option<f64> {
        self.m.uvi().tone_hz()
    }

    /// Side-effect-free bus read.
    fn peek(&self, addr: u16) -> u8 {
        self.m.peek(addr & 0x7fff)
    }

    /// Store into ROM or UVI memory behind `addr`.
    fn poke(&mut self, addr: u16, value: u8) {
        self.m.poke(addr & 0x7fff, value);
    }

    /// `n` side-effect-free bytes from `addr`, wrapping at 32 KiB.
    fn read<'py>(&self, py: Python<'py>, addr: usize, n: usize) -> Bound<'py, PyBytes> {
        let v: Vec<u8> = span(addr, n).map(|a| self.m.peek(a)).collect();
        PyBytes::new(py, &v)
    }

    /// Keypad for `player` (0/1) as a 12-bit mask in `KEYS` order.
    fn set_keys(&mut self, player: usize, mask: u16) -> PyResult<()> {
        check_player(player)?;
        self.m.set_keys(player, mask);
        Ok(())
    }

    /// Panel buttons: bit 0 Start, bit 1 Option (A), bit 2 Select (B).
    fn set_panel(&mut self, mask: u8) {
        self.m.set_panel(mask);
    }

    /// Paddle `axis` (0 = Y, 1 = X) of `player`.
    fn set_paddle(&mut self, player: usize, axis: usize, value: u8) -> PyResult<()> {
        check_player(player)?;
        check_player(axis)?;
        self.m.set_paddle(player, axis, value);
        Ok(())
    }

    /// Enable (resetting counts) or disable execution coverage.
    fn coverage(&mut self, enable: bool) {
        self.m.set_coverage(enable);
    }

    /// 32 KiB of saturating per-address opcode execution counts, or None if disabled.
    fn executed<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.m.executed().map(|c| PyBytes::new(py, c))
    }
}

fn check_player(i: usize) -> PyResult<()> {
    (i < 2)
        .then_some(())
        .ok_or_else(|| PyValueError::new_err("index must be 0 or 1"))
}

#[pymodule(gil_used = false)]
fn _emu(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<S2650>()?;
    m.add_class::<FountainForce2>()?;
    m.add("KEYS", fforce2::KEYS.to_vec())?;
    m.add(
        "PALETTE",
        PyBytes::new(m.py(), fforce2::PALETTE.as_flattened()),
    )?;
    Ok(())
}
