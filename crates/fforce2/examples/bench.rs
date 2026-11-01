//! Emulation speed of a cartridge: `cargo run --release --example bench -- CART [FRAMES]`.

use fforce2::{Machine, Region};
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: bench CART [FRAMES]");
    let frames: u64 = args.next().map_or(500, |s| s.parse().expect("FRAMES"));
    let cart = std::fs::read(&path).expect("read cart");
    let mut m = Machine::new(&cart, Region::Pal);
    let t = Instant::now();
    m.run_frames(frames);
    let wall = t.elapsed().as_secs_f64();
    let emulated = m.cpu.clocks as f64 / m.timing().cpu_hz as f64;
    println!(
        "{frames} frames, {emulated:.2}s emulated in {wall:.3}s: {:.0}x real time",
        emulated / wall
    );
}
