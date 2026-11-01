//! Emulation speed: `cargo run --release -p fforce2 --example bench -- [--slow] FRAMES CART...`.

use fforce2::{Machine, Region};
use std::time::Instant;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let slow = args.first().is_some_and(|a| a == "--slow");
    if slow {
        args.remove(0);
    }
    let frames: u64 = args[0].parse().expect("FRAMES");
    let (mut emulated, mut wall) = (0.0, 0.0);
    let mut speeds = Vec::new();
    for path in &args[1..] {
        let cart = std::fs::read(path).expect("read cart");
        let mut m = Machine::new(&cart, Region::Pal);
        m.fast = !slow;
        let t = Instant::now();
        m.run_frames(frames);
        let w = t.elapsed().as_secs_f64();
        let e = m.cpu.clocks as f64 / m.timing().cpu_hz as f64;
        speeds.push(e / w);
        emulated += e;
        wall += w;
    }
    speeds.sort_by(f64::total_cmp);
    println!(
        "{} carts x {frames} frames: {:.0}x real time overall, per cart min {:.0}x median {:.0}x max {:.0}x",
        speeds.len(),
        emulated / wall,
        speeds[0],
        speeds[speeds.len() / 2],
        speeds[speeds.len() - 1]
    );
}
