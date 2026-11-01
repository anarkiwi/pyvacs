use s2650::uvi::{WIDTH, XPOS};
use s2650::{Uvi, UviTiming};

const ZERO: [u8; 8] = [0x00, 0x1c, 0x22, 0x26, 0x2a, 0x32, 0x22, 0x1c];
const BACKSLASH: [u8; 8] = [1, 2, 4, 8, 16, 32, 64, 128];
const PAL0: u16 = 0x1f8;
const PAL1: u16 = 0x1f9;
const SCROLL: u16 = 0x0fc;
const FIELD_TOP: usize = 18;

fn uvi() -> Uvi {
    Uvi::new(UviTiming::NTSC)
}

fn field(u: &mut Uvi, pal1: u8) {
    u.write(SCROLL, (255 - FIELD_TOP) as u8);
    u.write(PAL1, pal1);
}

fn run_to(u: &mut Uvi, line: u16) {
    while u.line != line {
        u.line(false);
    }
}

fn frame(u: &mut Uvi) {
    for _ in 0..u.timing.lines {
        u.line(false);
    }
}

fn px(u: &Uvi, x: usize, y: usize) -> u8 {
    u.frame[y * WIDTH + x]
}

fn row8(u: &Uvi, x: usize, y: usize) -> [u8; 8] {
    u.frame[y * WIDTH + x..y * WIDTH + x + 8]
        .try_into()
        .unwrap()
}

fn paint(pat: u8, fg: u8, bg: u8) -> [u8; 8] {
    std::array::from_fn(|k| if pat << k & 0x80 != 0 { fg } else { bg })
}

fn block(code: u8, r: usize) -> u8 {
    let c = if r < 4 { code } else { code >> 3 };
    [(1, 0x03), (2, 0x1c), (4, 0xe0)]
        .iter()
        .filter(|(m, _)| c & m != 0)
        .fold(0, |a, (_, p)| a | p)
}

#[test]
fn solid_background() {
    let mut u = uvi();
    u.write(PAL1, 0x05);
    frame(&mut u);
    assert_eq!(u.frame.len(), WIDTH * 242);
    assert!(u.frame.iter().all(|&c| c == 5));
    assert_eq!(u.frames, 1);
}

#[test]
fn glyph_8_line_rows() {
    let mut u = uvi();
    field(&mut u, 0x80 | 0x08 | 7);
    u.write(0x000, 0x10);
    u.write(0x011, 0x10);
    frame(&mut u);
    for (r, &g) in ZERO.iter().enumerate() {
        assert_eq!(row8(&u, XPOS, FIELD_TOP + r), paint(g, 1, 7));
        assert_eq!(row8(&u, XPOS + 8, FIELD_TOP + r), [7; 8]);
        assert_eq!(row8(&u, XPOS + 8, FIELD_TOP + 8 + r), paint(g, 1, 7));
    }
    assert!(u.frame[(FIELD_TOP - 1) * WIDTH..FIELD_TOP * WIDTH]
        .iter()
        .all(|&c| c == 7));
}

#[test]
fn glyph_16_line_rows_and_vscroll() {
    for top in [FIELD_TOP, 47] {
        let mut u = uvi();
        u.write(SCROLL, (255 - top) as u8);
        u.write(PAL1, 0x08 | 7);
        u.write(0x000, 0x10);
        frame(&mut u);
        for (r, &g) in ZERO.iter().enumerate() {
            assert_eq!(row8(&u, XPOS, top + 2 * r), paint(g, 1, 7));
            assert_eq!(row8(&u, XPOS, top + 2 * r + 1), paint(g, 1, 7));
        }
        assert_eq!(row8(&u, XPOS, top + 16), [7; 8]);
        assert_eq!(row8(&u, XPOS, top - 1), [7; 8]);
    }
}

#[test]
fn colours_normal_mode() {
    for (pal1, low) in [(0x80 | 2, 0), (0x80 | 0x08 | 2, 1)] {
        let mut u = uvi();
        field(&mut u, pal1);
        for (j, b) in [0x07u8, 0x47, 0x87, 0xc7].into_iter().enumerate() {
            u.write(j as u16, b);
        }
        frame(&mut u);
        for j in 0..4 {
            let fg = low | (2 * j as u8);
            assert_eq!(row8(&u, XPOS + 8 * j, FIELD_TOP), paint(0xc0, fg, 2));
        }
        assert_eq!(px(&u, 0, FIELD_TOP), 2);
    }
}

#[test]
fn colours_multicolour_mode() {
    let mut u = uvi();
    field(&mut u, 0x80 | (5 << 3) | 6);
    u.write(PAL0, (3 << 3) | 4);
    u.write(0x0fd, 0x80);
    for (j, b) in [0x07u8, 0x47, 0x87, 0xc7].into_iter().enumerate() {
        u.write(j as u16, b);
    }
    frame(&mut u);
    for (j, (fg, bg)) in [(3, 4), (5, 4), (3, 6), (5, 6)].into_iter().enumerate() {
        assert_eq!(row8(&u, XPOS + 8 * j, FIELD_TOP), paint(0xc0, fg, bg));
    }
    assert_eq!(px(&u, 0, FIELD_TOP), 6);
    assert_eq!(px(&u, 0, 0), 6);
}

#[test]
fn block_mode_switching() {
    let row = [0x01u8, 0xc0, 0x01, 0x2a, 0x40, 0x01];
    for global in [false, true] {
        let mut u = uvi();
        field(&mut u, 0x80 | 7);
        u.write(PAL0, if global { 0x80 } else { 0 });
        for (j, &b) in row.iter().enumerate() {
            u.write(j as u16, b);
            u.write(0x10 + j as u16, b & 0x3f);
        }
        frame(&mut u);
        for (r, &bs) in BACKSLASH.iter().enumerate() {
            let y = FIELD_TOP + r;
            let c0 = if global { block(1, r) } else { bs };
            let want = [c0, 0, block(1, r), block(0x2a, r), 0, bs];
            for (j, &p) in want.iter().enumerate() {
                assert_eq!(row8(&u, XPOS + 8 * j, y), paint(p, 0, 7), "r{r} c{j}");
            }
            let next = if global { block(1, r) } else { bs };
            assert_eq!(row8(&u, XPOS + 16, y + 8), paint(next, 0, 7));
        }
    }
}

#[test]
fn user_defined_glyph_live_update() {
    let mut u = uvi();
    field(&mut u, 0x80 | 7);
    u.write(0x000, 0x3c);
    for r in 0..8 {
        u.write(0x1a0 + r, 0xf0);
    }
    run_to(&mut u, FIELD_TOP as u16 + 4);
    u.write(0x1a4 + 1, 0x0f);
    run_to(&mut u, 0);
    assert_eq!(row8(&u, XPOS, FIELD_TOP + 3), paint(0xf0, 0, 7));
    assert_eq!(row8(&u, XPOS, FIELD_TOP + 5), paint(0x0f, 0, 7));
}

#[test]
fn horizontal_shift_latched_per_row() {
    let mut u = uvi();
    field(&mut u, 0x80 | 7);
    u.write(0x0fe, 5 << 5);
    u.write(0x000, 0x03);
    u.write(0x010, 0x03);
    run_to(&mut u, FIELD_TOP as u16 + 1);
    u.write(0x0fe, 0);
    run_to(&mut u, 0);
    for y in FIELD_TOP..FIELD_TOP + 8 {
        assert_eq!(px(&u, XPOS + 4, y), 7);
        assert_eq!(row8(&u, XPOS + 5, y), [0; 8]);
    }
    assert_eq!(row8(&u, XPOS, FIELD_TOP + 8), [0; 8]);
    assert_eq!(px(&u, XPOS + 8, FIELD_TOP + 8), 7);
}

#[test]
fn twenty_six_row_mode() {
    for lines26 in [false, true] {
        let mut u = uvi();
        field(&mut u, 0x80 | 7);
        u.write(PAL0, if lines26 { 0x40 } else { 0 });
        u.write(0x200, 0x03);
        u.write(0x2cf, 0x03);
        frame(&mut u);
        let fg = if lines26 { 0 } else { 7 };
        assert_eq!(row8(&u, XPOS, FIELD_TOP + 13 * 8), [fg; 8]);
        assert_eq!(row8(&u, XPOS + 120, FIELD_TOP + 26 * 8 - 1), [fg; 8]);
        assert_eq!(row8(&u, XPOS, FIELD_TOP + 26 * 8), [7; 8]);
    }
}

#[test]
fn rows_clipped_at_active_area() {
    let mut u = uvi();
    field(&mut u, 7);
    u.write(PAL0, 0x40);
    u.write(0x200, 0x03);
    frame(&mut u);
    let last = u.timing.active() as usize - 1;
    assert_eq!(row8(&u, XPOS, FIELD_TOP + 13 * 16), [0; 8]);
    assert_eq!(row8(&u, XPOS, last), [0; 8]);
}

fn charline_seq(lines26: bool) -> Vec<(u16, u8)> {
    let mut u = uvi();
    field(&mut u, 0x80);
    u.write(PAL0, if lines26 { 0x40 } else { 0 });
    let mut seq: Vec<(u16, u8)> = Vec::new();
    for _ in 0..u.timing.lines {
        let v = u.read(0x0ff);
        if seq.last().map(|l| l.1) != Some(v) {
            seq.push((u.line, v));
        }
        u.line(false);
    }
    seq
}

#[test]
fn charline_13_rows() {
    let mut want = vec![(0, 0xff)];
    want.extend((0..13).map(|n| (FIELD_TOP as u16 + 8 * n, 0xf0 | n as u8)));
    want.push((FIELD_TOP as u16 + 104, 0xfd));
    want.push((242, 0xff));
    assert_eq!(charline_seq(false), want);
}

#[test]
fn charline_26_rows() {
    let mut want = vec![(0, 0xff)];
    want.extend((0..26).map(|n| (FIELD_TOP as u16 + 8 * n, 0xf0 | (n % 13) as u8)));
    want.push((FIELD_TOP as u16 + 208, 0xfd));
    want.push((242, 0xff));
    assert_eq!(charline_seq(true), want);
}

#[test]
fn sense_timing() {
    for t in [UviTiming::NTSC, UviTiming::PAL] {
        let mut u = Uvi::new(t);
        let mut high = Vec::new();
        for _ in 0..2 * t.lines {
            if u.sense() {
                high.push(u.line);
            }
            u.line(false);
        }
        let want: Vec<u16> = (t.lines - t.vrst_lines..t.lines).collect();
        assert_eq!(high, [want.clone(), want].concat());
        assert_eq!(u.frames, 2);
        assert_eq!(u.line, 0);
    }
}

fn sprite(u: &mut Uvi, i: u16, top: u16, fbx: i32, img: [u8; 8]) {
    u.write(0x0f0 + 2 * i, (256 - top) as u8);
    u.write(0x0f1 + 2 * i, (fbx + 43 - XPOS as i32) as u8);
    for (r, &b) in img.iter().enumerate() {
        u.write(0x180 + 8 * i + r as u16, b);
    }
}

const IMG: [u8; 8] = [0x80, 0x01, 0xff, 0, 0, 0, 0, 0x81];

#[test]
fn sprite_short_placement_per_sprite() {
    let ctrl = [
        (0x1fb, 0x80, 3),
        (0x1fb, 0x40, 0),
        (0x1fa, 0x80, 3),
        (0x1fa, 0x40, 0),
    ];
    for (i, &(reg, short, sh)) in ctrl.iter().enumerate() {
        let mut u = uvi();
        u.write(PAL1, 7);
        sprite(&mut u, i as u16, 56, 49, IMG);
        u.write(reg, short | (2 << sh));
        frame(&mut u);
        for (r, &b) in IMG.iter().enumerate() {
            assert_eq!(row8(&u, 49, 56 + r), paint(b, 2, 7), "sprite {i} row {r}");
        }
        assert_eq!(row8(&u, 49, 64), [7; 8]);
        assert_eq!(row8(&u, 49, 55), [7; 8]);
        assert_eq!(u.frame.iter().filter(|&&c| c == 2).count(), 12);
    }
}

#[test]
fn sprite_tall_double_scans() {
    let mut u = uvi();
    u.write(PAL1, 7);
    sprite(&mut u, 0, 56, 49, IMG);
    u.write(0x1fb, 4 << 3);
    frame(&mut u);
    for (r, &b) in IMG.iter().enumerate() {
        assert_eq!(row8(&u, 49, 56 + 2 * r), paint(b, 4, 7));
        assert_eq!(row8(&u, 49, 57 + 2 * r), paint(b, 4, 7));
    }
    assert_eq!(row8(&u, 49, 72), [7; 8]);
}

#[test]
fn sprite_disabled_and_clipped() {
    let mut u = uvi();
    u.write(PAL1, 7);
    sprite(&mut u, 0, 10, -6, [0xff; 8]);
    sprite(&mut u, 1, 10, 188, [0xff; 8]);
    sprite(&mut u, 2, 30, 60, [0xff; 8]);
    u.write(0x0f4, 0);
    u.write(0x1fb, 0xc0);
    frame(&mut u);
    for y in 10..18 {
        assert_eq!(&u.frame[y * WIDTH..y * WIDTH + 3], &[0, 0, 7]);
        assert_eq!(&u.frame[y * WIDTH + 187..(y + 1) * WIDTH], &[7, 0, 0, 0, 0]);
    }
    assert!(u.frame[30 * WIDTH..46 * WIDTH].iter().all(|&c| c == 7));
}

#[test]
fn sprite_coordinates_latched_at_frame_start() {
    let mut u = uvi();
    u.write(PAL1, 7);
    sprite(&mut u, 0, 56, 49, [0xff; 8]);
    u.write(0x1fb, 0x80);
    u.line(false);
    u.write(0x0f0, (256 - 100) as u8);
    frame(&mut u);
    assert_eq!(row8(&u, 49, 56), [0; 8]);
    assert_eq!(row8(&u, 49, 100), [7; 8]);
}

#[test]
fn sprite_overlap_ors_colours_and_collides() {
    let mut u = uvi();
    u.write(PAL1, 7);
    sprite(&mut u, 0, 56, 49, [0xf0; 8]);
    sprite(&mut u, 1, 56, 51, [0xf0; 8]);
    u.write(0x1fb, 0xc0 | (1 << 3) | 2);
    frame(&mut u);
    assert_eq!(row8(&u, 49, 56), [1, 1, 3, 3, 2, 2, 7, 7]);
    assert_eq!(u.read(0x1fd), 0xfe);
    assert_eq!(u.read(0x1fd), 0xff);
    assert_eq!(u.read(0x1fc), 0xff);
}

#[test]
fn sprite_pair_collision_bits() {
    let pairs = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];
    for (bit, &(i, j)) in pairs.iter().enumerate() {
        let mut u = uvi();
        u.write(0x1fa, 0xc0);
        u.write(0x1fb, 0xc0);
        for k in 0..4 {
            sprite(&mut u, k, 20 + 20 * k, 40, [0x80; 8]);
        }
        sprite(&mut u, i, 120, 60, [0x01; 8]);
        sprite(&mut u, j, 127, 67, [0x80; 8]);
        frame(&mut u);
        assert_eq!(u.read(0x1fd), !(1u8 << bit), "pair {i},{j}");
    }
}

#[test]
fn sprite_adjacent_no_collision() {
    let mut u = uvi();
    u.write(0x1fb, 0xc0);
    sprite(&mut u, 0, 50, 60, [0xff; 8]);
    sprite(&mut u, 1, 50, 68, [0xff; 8]);
    frame(&mut u);
    assert_eq!(u.read(0x1fd), 0xff);
}

#[test]
fn background_collision() {
    let mut u = uvi();
    field(&mut u, 0x80 | 7);
    u.write(0x000, 0x03);
    u.write(0x002, 0x10);
    u.write(0x1fa, 0xc0 | (2 << 3));
    u.write(0x1fb, 0xc0);
    sprite(&mut u, 2, FIELD_TOP as u16 + 2, XPOS as i32 + 4, [0xff; 8]);
    sprite(
        &mut u,
        1,
        FIELD_TOP as u16,
        XPOS as i32 + 16,
        [0xff, 0, 0, 0, 0, 0, 0, 0],
    );
    sprite(&mut u, 3, 150, 60, [0xff; 8]);
    sprite(&mut u, 0, 10, 60, [0xff; 8]);
    frame(&mut u);
    assert_eq!(row8(&u, XPOS, FIELD_TOP + 2), [0, 0, 0, 0, 2, 2, 2, 2]);
    assert_eq!(u.read(0x1fc), 0xfb);
    assert_eq!(u.read(0x1fc), 0xff);
    frame(&mut u);
    sprite(&mut u, 2, 150, 120, [0xff; 8]);
    frame(&mut u);
    assert_eq!(u.read(0x1fc), 0xff);
    assert_eq!(u.read(0x1fd), 0xff);
}

#[test]
fn background_collision_with_shift() {
    let mut u = uvi();
    field(&mut u, 0x80 | 7);
    u.write(0x0fe, 7 << 5);
    u.write(0x00f, 0x03);
    u.write(0x1fb, 0xc0);
    sprite(
        &mut u,
        0,
        FIELD_TOP as u16,
        (XPOS + 128 + 6) as i32,
        [0x80; 8],
    );
    frame(&mut u);
    assert_eq!(u.read(0x1fc), 0xfe);
    sprite(
        &mut u,
        0,
        FIELD_TOP as u16,
        (XPOS + 128 + 7) as i32,
        [0x80; 8],
    );
    frame(&mut u);
    assert_eq!(u.read(0x1fc), 0xff);
}

#[test]
fn paddles_muxed_during_vrst() {
    let mut u = uvi();
    assert_eq!(u.pots, [[0x70; 2]; 2]);
    u.pots = [[0x11, 0x22], [0x33, 0x44]];
    assert_eq!((u.read(0x1fe), u.read(0x1ff)), (0xff, 0xff));
    run_to(&mut u, UviTiming::NTSC.active());
    assert_eq!((u.read(0x1fe), u.read(0x1ff)), (0x33, 0x11));
    u.write(PAL1, 0x40);
    assert_eq!((u.read(0x1fe), u.read(0x1ff)), (0x44, 0x22));
    u.write(0x1fe, 0x99);
    assert_eq!(u.read(0x1fe), 0x44);
}

#[test]
fn flag_inverts_colours() {
    let mut u = uvi();
    u.write(PAL1, 5);
    u.line(true);
    u.line(false);
    u.invert_on_flag = false;
    u.line(true);
    assert_eq!((px(&u, 0, 0), px(&u, 191, 0)), (2, 2));
    assert_eq!((px(&u, 0, 1), px(&u, 0, 2)), (5, 5));
}

#[test]
fn tone_frequency() {
    let mut u = uvi();
    u.write(0x0fd, 0x80 | 0x7f);
    assert_eq!(u.tone_hz(), None);
    u.write(0x0fe, 0x08);
    assert_eq!(u.tone_hz(), None);
    u.write(0x0fe, 0x0f);
    let hz = 894_886.0 * 4.0 / 227.0 / 256.0;
    assert!((u.tone_hz().unwrap() - hz).abs() < 1e-9);
    u.write(0x0fe, 0x17);
    assert_eq!(u.tone_hz(), None);
}

#[test]
fn registers_and_reset() {
    let mut u = uvi();
    u.write(0x2ff, 0x5a);
    u.write(0x0f8, 0xa5);
    u.write(0x0ff, 0x00);
    u.write(0x1fc, 0x00);
    u.write(0x1fd, 0x00);
    assert_eq!((u.read(0x2ff), u.read(0x0f8)), (0x5a, 0xa5));
    assert_eq!(
        (u.read(0x0ff), u.read(0x1fc), u.read(0x1fd)),
        (0xff, 0xff, 0xff)
    );
    u.pots = [[1, 2], [3, 4]];
    u.invert_on_flag = false;
    frame(&mut u);
    u.line(false);
    u.reset();
    assert_eq!((u.read(0x2ff), u.line, u.frames), (0, 0, 0));
    assert_eq!((u.pots, u.invert_on_flag), ([[1, 2], [3, 4]], false));
}

#[test]
fn timing_frame_rate() {
    let ntsc = 894_886.0 * 4.0 / (227.0 * 262.0);
    let pal = 886_724.0 * 4.0 / (227.0 * 312.0);
    assert!((UviTiming::NTSC.fps() - ntsc).abs() < 1e-9);
    assert!((UviTiming::PAL.fps() - pal).abs() < 1e-9);
    assert_eq!(UviTiming::PAL.active(), 269);
}
