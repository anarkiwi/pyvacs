//! The fast path (idle-loop fast-forward, closed-form delay loops, unobserved frames
//! not drawn) must be indistinguishable from plain stepping.

use fforce2::{Machine, Region};

fn state(m: &Machine) -> (u64, u16, u16, [u8; 7], Vec<u8>, Vec<u8>) {
    let mem = (0x1800..0x1b00).map(|a| m.peek(a)).collect();
    (
        m.cpu.clocks,
        m.cpu.pc,
        m.uvi().line,
        m.cpu.r,
        mem,
        m.frame().to_vec(),
    )
}

fn check(cart: &[u8]) {
    for region in [Region::Pal, Region::Ntsc] {
        let mut slow = Machine::new(cart, region);
        slow.fast = false;
        let mut fast = Machine::new(cart, region);
        let mut batched = Machine::new(cart, region);
        for f in 1..=40u64 {
            slow.run_frames(1);
            fast.run_frames(1);
            assert_eq!(state(&slow), state(&fast), "{region:?} frame {f}");
            if f % 10 == 0 {
                batched.run_frames(10);
                assert_eq!(state(&slow), state(&batched), "{region:?} batch {f}");
                for n in [1u64, 57, 1000, 17_711] {
                    for m in [&mut slow, &mut fast, &mut batched] {
                        m.run_clocks(n);
                    }
                    assert_eq!(state(&slow), state(&fast), "{region:?} clocks {n}");
                    assert_eq!(state(&slow), state(&batched), "{region:?} clocks {n}");
                }
                for m in [&mut slow, &mut fast, &mut batched] {
                    m.run_frames(1);
                }
            }
        }
    }
}

#[test]
fn sense_poll() {
    // spsu; tmi,r0 $80; bcfr,eq -5; lodi,r0 5; stra,r0 $19f9; bctr,un self
    check(&[
        0x12, 0xf4, 0x80, 0x98, 0x7b, 0x04, 0x05, 0xcc, 0x19, 0xf9, 0x1b, 0x7e,
    ]);
}

#[test]
fn vblank_edges_poll() {
    // wait for VRST start, then its end, bump the background colour, repeat
    check(&[
        0xb4, 0x80, 0x98, 0x7c, 0xb4, 0x80, 0x18, 0x7c, 0x0c, 0x19, 0xf9, 0x84, 0x01, 0xcc, 0x19,
        0xf9, 0x1f, 0x00, 0x00,
    ]);
}

#[test]
fn charline_poll_with_vscroll_changing_each_frame() {
    // lodi,r1 $ed; stra,r1 $18fc; loop: lodi,r0 $fd; coma,r0 $18ff; bcfr,eq -5;
    // loda,r1 $18fc; eori,r1 $10; stra,r1 $18fc; bcta,un loop
    check(&[
        0x05, 0xed, 0xcd, 0x18, 0xfc, 0x04, 0xfd, 0xec, 0x18, 0xff, 0x98, 0x7b, 0x0d, 0x18, 0xfc,
        0x25, 0x10, 0xcd, 0x18, 0xfc, 0x1f, 0x00, 0x05,
    ]);
}

#[test]
fn counted_delay_loops() {
    // lodi,r0 3; lodi,r1 0; bdrr,r1 self; bdrr,r0 -4; lodi,r2 $f0; birr,r2 self;
    // stra,r0 $18d0; birr,r3 self; bcta,un 0
    check(&[
        0x04, 0x03, 0x05, 0x00, 0xf9, 0x7e, 0xf8, 0x7c, 0x06, 0xf0, 0xda, 0x7e, 0xcc, 0x18, 0xd0,
        0xdb, 0x7e, 0x1f, 0x00, 0x00,
    ]);
}

#[test]
fn sprites_collide_in_unobserved_frames() {
    // place sprites 0 and 1 overlapping over a solid character, then poll collisions
    // into RAM each VRST: stra latches to $18d0/$18d1
    let mut p = vec![
        0x04, 0xff, 0xcc, 0x19, 0x80, 0xcc, 0x19, 0x88, // sprite 0/1 row 0 = $ff
        0x04, 0x60, 0xcc, 0x18, 0xf0, 0xcc, 0x18, 0xf2, // y
        0x04, 0x50, 0xcc, 0x18, 0xf1, 0xcc, 0x18, 0xf3, // x
        0x04, 0x03, 0xcc, 0x18, 0x00, // solid block in the first cell
        0x04, 0xed, 0xcc, 0x18, 0xfc, // vscroll
    ];
    let lp = p.len() as u8;
    p.extend([
        0xb4, 0x80, 0x98, 0x7c, // wait VRST
        0x0c, 0x19, 0xfc, 0xcc, 0x18, 0xd0, 0x0c, 0x19, 0xfd, 0xcc, 0x18,
        0xd1, // latch copies
        0xb4, 0x80, 0x18, 0x7c, // wait end of VRST
        0x1f, 0x00, lp,
    ]);
    check(&p);
}
