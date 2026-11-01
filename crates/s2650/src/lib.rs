//! Signetics 2650 CPU and Signetics 2637 UVI (Universal Video Interface).
//!
//! The crate is machine-agnostic: a system implements [`Bus`] to wire the CPU to its
//! memory map and peripherals, and clocks the [`uvi::Uvi`] from the cycles returned by
//! [`cpu::Cpu::step`].

pub mod bus;
pub mod cpu;
pub mod uvi;

pub use bus::{Bus, FlatBus, Port};
pub use cpu::Cpu;
pub use uvi::{Uvi, UviTiming};
