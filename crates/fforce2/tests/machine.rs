use fforce2::{Machine, Region, Stop, KEYS, OPTION, PALETTE, SELECT, START};
use s2650::Bus;

const SPIN: u16 = 0x000a;

/// Wait for SENSE, optionally raise FLAG, set the background colour, then spin.
fn program(colour: u8, flag: bool) -> Vec<u8> {
    let mut p = vec![0x12, 0xf4, 0x80, 0x98, 0x7b];
    if flag {
        p.extend([0x76, 0x40]);
    }
    p.extend([0x04, colour, 0xcc, 0x19, 0xf9, 0x1b, 0x7e]);
    p
}

fn pattern(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 7 + (i >> 8) * 13) as u8).collect()
}

fn key(name: &str) -> u16 {
    1 << KEYS.iter().position(|&k| k == name).unwrap()
}

#[test]
fn memory_map_and_mirrors() {
    let cart = pattern(0x2000);
    let mut m = Machine::new(&cart, Region::Pal);
    for a in [0x0000u16, 0x0123, 0x0fff] {
        let v = cart[a as usize];
        assert_eq!(m.peek(a), v);
        assert_eq!(m.peek(a | 0x4000), v);
        assert_eq!(m.peek(a | 0x6000), v);
        assert_eq!(m.peek(a | 0x2000), cart[0x1000 + a as usize]);
        assert_eq!(m.board.read(a | 0x2000), cart[0x1000 + a as usize]);
    }
    assert_eq!(m.peek(0x6000 | 0x0123), cart[0x123]);

    m.board.write(0x1800, 0x5a);
    for a in [0x1800u16, 0x1000, 0x1400, 0x3800, 0x5000, 0x7800] {
        assert_eq!(m.board.read(a), 0x5a, "{a:04x}");
    }
    m.board.write(0x1bf9, 0x03);
    assert_eq!(m.peek(0x19f9), 0x03);
    m.board.write(0x1a10, 0x77);
    assert_eq!(m.uvi().mem[0x210], 0x77);

    m.board.write(0x0000, !cart[0]);
    m.board.write(0x2000, !cart[0x1000]);
    assert_eq!(m.peek(0x0000), cart[0]);
    assert_eq!(m.peek(0x2000), cart[0x1000]);

    m.poke(0x0010, 0xaa);
    m.poke(0x2010, 0xbb);
    m.poke(0x1810, 0xcc);
    m.poke(0x1900, 0xdd);
    assert_eq!(
        [
            m.peek(0x0010),
            m.peek(0x2010),
            m.peek(0x1810),
            m.peek(0x1900)
        ],
        [0xaa, 0xbb, 0xcc, 0x00]
    );
}

#[test]
fn cart_sizes() {
    for k in [2usize, 4, 6, 8] {
        let cart = pattern(k * 1024);
        let m = Machine::new(&cart, Region::Pal);
        let image: Vec<u8> = (0..0x1000u16)
            .chain(0x2000..0x3000)
            .map(|a| m.peek(a))
            .collect();
        let mut want = vec![0xff; 0x2000];
        want[..cart.len()].copy_from_slice(&cart);
        assert_eq!(image, want, "{k}K");
    }
    let m = Machine::new(&pattern(0x3000), Region::Pal);
    assert_eq!(m.peek(0x2fff), pattern(0x2000)[0x1fff]);
}

#[test]
fn keypads_and_panel() {
    let mut m = Machine::new(&[], Region::Pal);
    let cols = |m: &mut Machine, base: u16| -> Vec<u8> {
        (0..3).map(|c| m.board.read(base + c)).collect()
    };
    assert_eq!(cols(&mut m, 0x1900), [0, 0, 0]);
    m.set_keys(0, key("1") | key("Clear") | key("5") | key("Enter"));
    assert_eq!(cols(&mut m, 0x1900), [0b1001, 0b0100, 0b0001]);
    assert_eq!(cols(&mut m, 0x1904), [0, 0, 0]);
    m.set_keys(1, key("4") | key("2") | key("0") | key("9") | key("3"));
    assert_eq!(cols(&mut m, 0x1904), [0b0100, 0b1001, 0b1010]);
    m.set_keys(0, 0xfff);
    assert_eq!(cols(&mut m, 0x1900), [0xf, 0xf, 0xf]);
    assert_eq!(cols(&mut m, 0x1b00), [0xf, 0xf, 0xf]);

    m.set_panel(START | SELECT);
    assert_eq!(m.board.read(0x1908), 5);
    m.set_panel(OPTION | 0xf0);
    assert_eq!(m.board.read(0x1908), 2);
    for o in [0x1903u16, 0x1907, 0x1909, 0x1950, 0x197f] {
        assert_eq!(m.board.read(o), 0, "{o:04x}");
    }
    m.board.write(0x1900, 0x00);
    assert_eq!(m.board.read(0x1900), 0xf);
}

#[test]
fn paddles() {
    let mut m = Machine::new(&[], Region::Ntsc);
    m.set_paddle(0, 0, 0x11);
    m.set_paddle(0, 1, 0x22);
    m.set_paddle(1, 0, 0x33);
    assert_eq!(m.peek(0x19ff), 0xff);
    while !m.uvi().sense() {
        m.step();
    }
    assert_eq!([m.peek(0x19ff), m.peek(0x19fe)], [0x11, 0x33]);
    m.poke(0x19f9, 0x40);
    assert_eq!(m.peek(0x19ff), 0x22);
}

#[test]
fn sense_wait_sets_background() {
    for (region, h) in [(Region::Pal, 269), (Region::Ntsc, 242)] {
        let mut m = Machine::new(&program(5, false), region);
        assert_eq!(m.run_frames(1), Stop::Frames);
        assert!(m.frame().iter().all(|&c| c == 0));
        m.run_frames(2);
        assert_eq!((m.width(), m.height()), (192, h));
        assert_eq!(m.frame().len(), 192 * h);
        assert!(m.frame().iter().all(|&c| c == 5));
        assert_eq!(m.cpu.pc, SPIN);
        assert_eq!(m.cpu.illegal, 0);
        let mut rgb = vec![0; 3 * m.frame().len()];
        m.rgb_frame(&mut rgb);
        assert!(rgb.chunks(3).all(|p| p == PALETTE[5]));
    }
}

#[test]
fn flag_inverts_only_on_pal() {
    for (region, want) in [(Region::Pal, 5 ^ 7), (Region::Ntsc, 5)] {
        let mut m = Machine::new(&program(5, true), region);
        m.run_frames(3);
        assert!(m.frame().iter().all(|&c| c == want), "{region:?}");
    }
}

#[test]
fn frame_timing() {
    for region in [Region::Pal, Region::Ntsc] {
        let mut m = Machine::new(&program(1, false), region);
        let t = m.timing();
        for n in [1u64, 7, 50] {
            m.reset();
            assert_eq!(m.run_frames(n), Stop::Frames);
            let need = (n * t.lines as u64 * t.clocks_num as u64).div_ceil(t.clocks_den as u64);
            assert!(m.cpu.clocks >= need && m.cpu.clocks < need + 12, "{n}");
            assert_eq!((m.uvi().frames, m.uvi().line), (n, 0));
        }
        let fps = m.timing().fps();
        assert!((49.0..51.0).contains(&fps) || (59.0..61.0).contains(&fps));
        m.reset();
        assert_eq!(m.run_clocks(1000), Stop::Clocks);
        assert!((1000..1012).contains(&m.cpu.clocks));
        let lines = m.cpu.clocks * t.clocks_den as u64 / t.clocks_num as u64;
        assert_eq!(m.uvi().line as u64, lines);
    }
}

#[test]
fn deterministic_and_resettable() {
    let run = || {
        let mut m = Machine::new(&program(6, true), Region::Pal);
        m.run_frames(5);
        (m.cpu.clocks, m.frame().to_vec())
    };
    assert_eq!(run(), run());
    let mut m = Machine::new(&program(6, true), Region::Pal);
    m.run_frames(5);
    m.reset();
    assert_eq!((m.cpu.clocks, m.cpu.pc, m.uvi().frames), (0, 0, 0));
    m.run_frames(5);
    assert_eq!((m.cpu.clocks, m.frame().to_vec()), run());
}

#[test]
fn breakpoints() {
    let mut m = Machine::new(&program(2, false), Region::Pal);
    m.set_breakpoint(SPIN, true);
    m.set_breakpoint(SPIN, true);
    assert_eq!(m.run_frames(10), Stop::Breakpoint(SPIN));
    assert!(m.uvi().frames < 2);
    let c = m.cpu.clocks;
    assert_eq!(m.run_frames(10), Stop::Breakpoint(SPIN));
    assert_eq!(m.cpu.clocks, c + 9);
    m.set_breakpoint(SPIN, false);
    m.set_breakpoint(SPIN, false);
    assert_eq!(m.run_clocks(100), Stop::Clocks);
    m.set_breakpoint(0x0000, true);
    m.clear_breakpoints();
    assert_eq!(m.run_frames(1), Stop::Frames);
}

#[test]
fn coverage() {
    let mut m = Machine::new(&program(2, false), Region::Pal);
    assert!(m.executed().is_none());
    m.set_coverage(true);
    m.run_frames(2);
    let cov = m.executed().unwrap();
    let hit: Vec<usize> = (0..0x8000).filter(|&a| cov[a] != 0).collect();
    assert_eq!(hit, [0x00, 0x01, 0x03, 0x05, 0x07, 0x0a]);
    assert_eq!(cov[0x05], 1);
    assert_eq!(cov[SPIN as usize], 255);
    m.set_coverage(false);
    assert!(m.executed().is_none());
}

#[test]
fn halt_keeps_raster_running() {
    let mut m = Machine::new(&[0x40], Region::Pal);
    m.run_frames(2);
    assert!(m.cpu.halted);
    assert_eq!(m.uvi().frames, 2);
}

#[test]
fn peek_preserves_collision_latches() {
    let mut m = Machine::new(&[], Region::Pal);
    let u = m.uvi_mut();
    u.write(0x0fc, 255 - 18);
    u.write(0x1f9, 0x80);
    u.write(0x000, 0x03);
    u.write(0x180, 0xff);
    u.write(0x0f0, 238);
    u.write(0x0f1, 43);
    while m.uvi().line < 30 {
        m.uvi_mut().line(false);
    }
    assert_eq!(m.peek(0x19fc) & 1, 0);
    assert_eq!(m.peek(0x19fc) & 1, 0);
    assert_eq!(m.board.read(0x19fc) & 1, 0);
    assert_eq!(m.peek(0x19fc), 0xff);
}
