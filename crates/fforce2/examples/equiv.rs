//! Check the fast path against plain stepping on cartridges:
//! `cargo run --release -p fforce2 --example equiv -- CART...`.

use fforce2::{Machine, Region};

fn state(m: &Machine) -> (u64, u16, u16, Vec<u8>) {
    let mem = (0x1800..0x1b00).map(|a| m.peek(a)).collect();
    (m.cpu.clocks, m.cpu.pc, m.uvi().line, mem)
}

fn main() {
    let mut bad = 0;
    for path in std::env::args().skip(1) {
        let cart = std::fs::read(&path).expect("read cart");
        for region in [Region::Pal, Region::Ntsc] {
            let mut slow = Machine::new(&cart, region);
            slow.fast = false;
            let mut fast = Machine::new(&cart, region);
            let mut chunked = Machine::new(&cart, region);
            let mut ok = true;
            for f in 1..=600u64 {
                slow.run_frames(1);
                fast.run_frames(1);
                let (a, b) = (state(&slow), state(&fast));
                if ok && a != b {
                    println!(
                        "  frame {f}: state slow {:?} fast {:?}",
                        (a.0, a.1, a.2),
                        (b.0, b.1, b.2)
                    );
                    for (i, (x, y)) in a.3.iter().zip(&b.3).enumerate() {
                        if x != y {
                            println!("    ${:04X}: slow {x:02x} fast {y:02x}", 0x1800 + i);
                        }
                    }
                }
                if ok && a == b && slow.frame() != fast.frame() {
                    println!("  frame {f}: pixels differ");
                }
                ok &= a == b && slow.frame() == fast.frame();
                if f % 100 == 0 {
                    chunked.run_frames(100);
                    ok &= state(&slow) == state(&chunked) && slow.frame() == chunked.frame();
                    for n in [1u64, 57, 1000, 17_711] {
                        slow.run_clocks(n);
                        fast.run_clocks(n);
                        chunked.run_clocks(n);
                        ok &= state(&slow) == state(&fast) && state(&slow) == state(&chunked);
                    }
                    fast.run_frames(1);
                    slow.run_frames(1);
                    chunked.run_frames(1);
                }
            }
            if !ok {
                bad += 1;
                println!("MISMATCH {region:?} {path}");
            }
        }
    }
    println!("{bad} mismatches");
    std::process::exit(bad.min(1));
}
