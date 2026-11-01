use s2650::cpu::{psl, psu};
use s2650::{Bus, Cpu, FlatBus, Port};

/// FlatBus plus recorded FLAG changes, port I/O and data-read counting.
#[derive(Default)]
struct TBus {
    ram: FlatBus,
    flags: Vec<bool>,
    out: Vec<(Port, u8)>,
    ins: Vec<Port>,
    reads: usize,
    sense: bool,
    vector: u8,
}

impl Bus for TBus {
    fn read(&mut self, addr: u16) -> u8 {
        self.reads += 1;
        self.ram.read(addr)
    }
    fn write(&mut self, addr: u16, value: u8) {
        self.ram.write(addr, value)
    }
    fn fetch(&mut self, addr: u16) -> u8 {
        self.ram.read(addr)
    }
    fn read_port(&mut self, port: Port) -> u8 {
        self.ins.push(port);
        match port {
            Port::Ctrl => 0x80,
            Port::Data => 0x00,
            Port::Ext(p) => p.wrapping_add(1),
        }
    }
    fn write_port(&mut self, port: Port, value: u8) {
        self.out.push((port, value));
    }
    fn sense(&mut self) -> bool {
        self.sense
    }
    fn flag(&mut self, level: bool) {
        self.flags.push(level);
    }
    fn intack(&mut self) -> u8 {
        self.vector
    }
}

impl TBus {
    fn poke(&mut self, addr: u16, bytes: &[u8]) {
        for (i, &b) in bytes.iter().enumerate() {
            self.ram.mem[addr as usize + i] = b;
        }
    }
    fn peek(&self, addr: u16) -> u8 {
        self.ram.mem[addr as usize]
    }
}

fn setup(prog: &[u8], org: u16) -> (Cpu, TBus) {
    let mut bus = TBus::default();
    bus.poke(org, prog);
    let mut cpu = Cpu::new();
    cpu.pc = org;
    (cpu, bus)
}

fn steps(cpu: &mut Cpu, bus: &mut impl Bus, n: usize) -> Vec<u32> {
    (0..n).map(|_| cpu.step(bus)).collect()
}

fn cc(cpu: &Cpu) -> u8 {
    cpu.psl & psl::CC
}

fn run_to_halt(cpu: &mut Cpu, bus: &mut impl Bus) {
    for _ in 0..10_000 {
        if cpu.halted {
            return;
        }
        cpu.step(bus);
    }
    panic!("program did not halt");
}

#[test]
fn new_reset_and_registers() {
    let mut cpu = Cpu::default();
    assert_eq!(cpu.pc, 0);
    assert_eq!((cpu.psl, cpu.psu, cpu.r, cpu.ras), (0, 0, [0; 7], [0; 8]));
    cpu.r = [1, 2, 3, 4, 5, 6, 7];
    cpu.psl = psl::RS;
    assert_eq!(
        (cpu.reg(0), cpu.reg(1), cpu.reg(2), cpu.reg(3)),
        (1, 5, 6, 7)
    );
    cpu.set_reg(2, 0x99);
    assert_eq!(cpu.r[5], 0x99);
    cpu.set_reg(0, 0x77);
    assert_eq!(cpu.r[0], 0x77);
    cpu.psl = 0;
    cpu.set_reg(2, 0x88);
    assert_eq!(cpu.r[2], 0x88);
    (cpu.psu, cpu.pc, cpu.ras[3], cpu.halted, cpu.irq) = (0x27, 0x1234, 9, true, true);
    (cpu.clocks, cpu.illegal) = (100, 3);
    cpu.reset();
    assert_eq!(
        (cpu.pc, cpu.psl, cpu.psu, cpu.r, cpu.ras),
        (0, 0, 0, [0; 7], [0; 8])
    );
    assert!(!cpu.halted && !cpu.irq);
    assert_eq!((cpu.clocks, cpu.illegal), (0, 0));
}

#[test]
fn load_immediate_and_zero_set_cc() {
    let (mut cpu, mut bus) = setup(&[0x04, 0x00, 0x05, 0x80, 0x06, 0x7F, 0x01, 0x00], 0x100);
    assert_eq!(cpu.step(&mut bus), 6);
    assert_eq!((cpu.reg(0), cc(&cpu)), (0, 0x00));
    cpu.step(&mut bus);
    assert_eq!((cpu.reg(1), cc(&cpu)), (0x80, 0x80));
    cpu.step(&mut bus);
    assert_eq!((cpu.reg(2), cc(&cpu)), (0x7F, 0x40));
    assert_eq!(cpu.step(&mut bus), 6);
    assert_eq!((cpu.reg(0), cc(&cpu)), (0x80, 0x80));
    cpu.psl &= !psl::CC;
    cpu.step(&mut bus);
    assert_eq!((cpu.reg(0), cc(&cpu)), (0x80, 0x80));
    assert_eq!(cpu.pc, 0x108);
    assert_eq!(cpu.clocks, 30);
}

#[test]
fn relative_forward_negative_and_indirect() {
    let (mut cpu, mut bus) = setup(&[0x08, 0x05, 0x09, 0x7C, 0x0A, 0x83], 0x100);
    bus.poke(0x107, &[0x42]);
    bus.poke(0x109, &[0x12, 0x34]);
    bus.poke(0x1234, &[0x99]);
    assert_eq!(steps(&mut cpu, &mut bus, 3), [9, 9, 15]);
    assert_eq!((cpu.reg(0), cpu.reg(1), cpu.reg(2)), (0x42, 0x08, 0x99));
    assert_eq!(cc(&cpu), 0x80);
}

#[test]
fn relative_and_sequential_wrap_within_page() {
    let (mut cpu, mut bus) = setup(&[0x08, 0x01], 0x3FFE);
    bus.poke(0x2001, &[0x5A]);
    cpu.step(&mut bus);
    assert_eq!((cpu.reg(0), cpu.pc), (0x5A, 0x2000));
    bus.poke(0x2000, &[0x09, 0x7E]);
    cpu.step(&mut bus);
    assert_eq!((cpu.reg(1), cpu.pc), (0x09, 0x2002));
}

#[test]
fn indirect_pointer_second_byte_wraps_within_page() {
    let (mut cpu, mut bus) = setup(&[0x0C, 0x9F, 0xFF], 0x100);
    bus.poke(0x1FFF, &[0x05]);
    bus.poke(0x0000, &[0x67]);
    bus.poke(0x0567, &[0xAB]);
    assert_eq!(cpu.step(&mut bus), 18);
    assert_eq!(cpu.reg(0), 0xAB);

    let (mut cpu, mut bus) = setup(&[0x0C, 0x8F, 0xFF], 0x6100);
    bus.poke(0x6000 + 0x0FFF, &[0x01, 0x23]);
    bus.poke(0x0123, &[0x3C]);
    cpu.step(&mut bus);
    assert_eq!(cpu.reg(0), 0x3C);
}

#[test]
fn absolute_indexed_modes() {
    let prog = [
        0x0D, 0x60, 0x50, // LODA,R0 $0050,R1
        0x0D, 0x20, 0x50, // LODA,R0 $0050,R1+
        0x0D, 0x40, 0x50, // LODA,R0 $0050,R1-
        0x0E, 0x00, 0x52, // LODA,R2 $0052
    ];
    let (mut cpu, mut bus) = setup(&prog, 0x100);
    bus.poke(0x50, &[0x10, 0x11, 0x12, 0x13]);
    cpu.set_reg(1, 2);
    assert_eq!(steps(&mut cpu, &mut bus, 1), [12]);
    assert_eq!((cpu.reg(0), cpu.reg(1)), (0x12, 2));
    cpu.step(&mut bus);
    assert_eq!((cpu.reg(0), cpu.reg(1)), (0x13, 3));
    cpu.step(&mut bus);
    assert_eq!((cpu.reg(0), cpu.reg(1)), (0x12, 2));
    cpu.step(&mut bus);
    assert_eq!((cpu.reg(0), cpu.reg(2)), (0x12, 0x12));
}

#[test]
fn auto_decrement_underflow_and_index_wraps_in_page() {
    let (mut cpu, mut bus) = setup(&[0x0D, 0x40, 0x50, 0x0E, 0x7F, 0xFF], 0x2100);
    bus.poke(0x214F, &[0x4F]);
    bus.poke(0x2002, &[0x22]);
    cpu.step(&mut bus);
    assert_eq!((cpu.reg(0), cpu.reg(1)), (0x4F, 0xFF));
    cpu.set_reg(2, 3);
    cpu.step(&mut bus);
    assert_eq!(cpu.reg(0), 0x22);
}

#[test]
fn indirect_resolved_before_indexing() {
    let (mut cpu, mut bus) = setup(&[0x0D, 0xE0, 0x60, 0x0D, 0xE0, 0x62], 0x100);
    bus.poke(0x60, &[0x04, 0x00, 0x5F, 0xFF]);
    bus.poke(0x401, &[0x77]);
    bus.poke(0x4001, &[0x88]);
    cpu.set_reg(1, 1);
    assert_eq!(cpu.step(&mut bus), 18);
    assert_eq!(cpu.reg(0), 0x77);
    cpu.set_reg(1, 2);
    cpu.step(&mut bus);
    assert_eq!(cpu.reg(0), 0x88);
}

#[test]
fn stores() {
    let prog = [
        0xC8, 0x10, // STRR,R0 $+0x10 -> $0112
        0xCF, 0x60, 0x80, // STRA,R0 $0080,R3
        0xC9, 0x8E, // STRR,R1 *$0115 -> $0300
        0xC1, // STRZ R1
        0xC0, // NOP
    ];
    let (mut cpu, mut bus) = setup(&prog, 0x100);
    bus.poke(0x115, &[0x03, 0x00]);
    cpu.set_reg(0, 0x11);
    cpu.set_reg(1, 0x22);
    cpu.set_reg(3, 4);
    assert_eq!(steps(&mut cpu, &mut bus, 3), [9, 12, 15]);
    assert_eq!(
        (bus.peek(0x112), bus.peek(0x84), bus.peek(0x300)),
        (0x11, 0x11, 0x22)
    );
    assert_eq!(cc(&cpu), 0);
    assert_eq!(cpu.step(&mut bus), 6);
    assert_eq!((cpu.reg(1), cc(&cpu)), (0x11, 0x40));
    cpu.psl |= psl::CC;
    assert_eq!(cpu.step(&mut bus), 6);
    assert_eq!(cc(&cpu), psl::CC);
}

#[test]
fn stra_indirect_auto_increment() {
    let (mut cpu, mut bus) = setup(&[0xCD, 0xA0, 0x40], 0x100);
    bus.poke(0x40, &[0x05, 0x00]);
    cpu.set_reg(0, 0xEE);
    cpu.set_reg(1, 0x0F);
    assert_eq!(cpu.step(&mut bus), 18);
    assert_eq!((bus.peek(0x510), cpu.reg(1)), (0xEE, 0x10));
}

#[test]
fn logic_ops() {
    let prog = [
        0x41, // ANDZ R1
        0x64, 0x0F, // IORI,R0 $0F
        0x26, 0xFF, // EORI,R2 $FF
        0x20, // EORZ R0
        0x48, 0x7F, // ANDR,R0 $-1 (its own operand byte)
        0x6D, 0x01, 0x10, // IORA,R1 $0110
        0x2E, 0x01, 0x11, // EORA,R2 $0111
        0x04, // filler
    ];
    let (mut cpu, mut bus) = setup(&prog, 0x100);
    bus.poke(0x110, &[0x80, 0x0F]);
    cpu.r[..3].copy_from_slice(&[0xF3, 0x3C, 0x0F]);
    cpu.step(&mut bus);
    assert_eq!((cpu.reg(0), cc(&cpu)), (0x30, 0x40));
    cpu.step(&mut bus);
    assert_eq!(cpu.reg(0), 0x3F);
    cpu.step(&mut bus);
    assert_eq!((cpu.reg(2), cc(&cpu)), (0xF0, 0x80));
    cpu.step(&mut bus);
    assert_eq!((cpu.reg(0), cc(&cpu)), (0, 0));
    cpu.set_reg(0, 0xFF);
    assert_eq!(cpu.step(&mut bus), 9);
    assert_eq!(cpu.reg(0), 0x7F);
    assert_eq!(cpu.step(&mut bus), 12);
    assert_eq!((cpu.reg(1), cc(&cpu)), (0xBC, 0x80));
    cpu.step(&mut bus);
    assert_eq!((cpu.reg(2), cc(&cpu)), (0xFF, 0x80));
}

/// Run `op` (an immediate instruction on R0) from the given state; return (R0, PSL).
fn alu(op: u8, a: u8, b: u8, psl_in: u8) -> (u8, u8) {
    let (mut cpu, mut bus) = setup(&[op, b], 0x100);
    cpu.psl = psl_in;
    cpu.set_reg(0, a);
    assert_eq!(cpu.step(&mut bus), 6);
    (cpu.reg(0), cpu.psl)
}

const ADDI: u8 = 0x84;
const SUBI: u8 = 0xA4;
const F: u8 = psl::C | psl::OVF | psl::IDC | psl::CC;

#[test]
fn add_flags() {
    let cases = [
        (0x7F, 0x01, 0, 0x80, psl::OVF | psl::IDC | 0x80),
        (0xFF, 0x01, 0, 0x00, psl::C | psl::IDC),
        (0x08, 0x08, 0, 0x10, psl::IDC | 0x40),
        (0x80, 0x80, 0, 0x00, psl::C | psl::OVF),
        (0x01, 0x01, psl::C, 0x02, 0x40),
        (0x01, 0x01, psl::C | psl::WC, 0x03, 0x40),
        (0xFF, 0x00, psl::C | psl::WC, 0x00, psl::C | psl::IDC),
        (0x12, 0x34, psl::WC, 0x46, 0x40),
    ];
    for (a, b, pin, r, flags) in cases {
        let (v, p) = alu(ADDI, a, b, pin);
        assert_eq!((v, p & F), (r, flags), "{a:02x}+{b:02x} psl {pin:02x}");
        assert_eq!(p & psl::WC, pin & psl::WC);
    }
}

#[test]
fn sub_flags() {
    let cases = [
        (0x05, 0x03, 0, 0x02, psl::C | psl::IDC | 0x40),
        (0x03, 0x05, 0, 0xFE, 0x80),
        (0x80, 0x01, 0, 0x7F, psl::C | psl::OVF | 0x40),
        (0x10, 0x01, 0, 0x0F, psl::C | 0x40),
        (0x05, 0x05, 0, 0x00, psl::C | psl::IDC),
        (0x05, 0x03, psl::WC, 0x01, psl::C | psl::IDC | 0x40),
        (0x05, 0x03, psl::WC | psl::C, 0x02, psl::C | psl::IDC | 0x40),
        (0x00, 0xFF, psl::WC, 0x00, 0),
        (0x7F, 0xFF, 0, 0x80, psl::OVF | psl::IDC | 0x80),
    ];
    for (a, b, pin, r, flags) in cases {
        let (v, p) = alu(SUBI, a, b, pin);
        assert_eq!((v, p & F), (r, flags), "{a:02x}-{b:02x} psl {pin:02x}");
    }
}

#[test]
fn add_sub_register_and_memory_forms() {
    let prog = [0x81, 0xA2, 0x88, 0x03, 0xAC, 0x01, 0x10];
    let (mut cpu, mut bus) = setup(&prog, 0x100);
    bus.poke(0x107, &[0x05]);
    bus.poke(0x110, &[0x01]);
    cpu.r[..3].copy_from_slice(&[0x10, 0x20, 0x08]);
    assert_eq!(steps(&mut cpu, &mut bus, 4), [6, 6, 9, 12]);
    assert_eq!(cpu.reg(0), 0x10 + 0x20 - 0x08 + 0x05 - 0x01);
}

#[test]
fn compare_signed_and_unsigned() {
    let cases = [
        (0x01, 0x80, 0, 0x40),
        (0x01, 0x80, psl::COM, 0x80),
        (0x80, 0x01, 0, 0x80),
        (0x80, 0x01, psl::COM, 0x40),
        (0x42, 0x42, 0, 0x00),
        (0xFF, 0xFE, 0, 0x40),
        (0xFE, 0xFF, psl::COM, 0x80),
    ];
    for (a, b, pin, want) in cases {
        let (v, p) = alu(0xE4, a, b, pin | psl::C);
        assert_eq!((v, p), (a, pin | psl::C | want), "{a:02x} vs {b:02x}");
    }
    let (mut cpu, mut bus) = setup(&[0xE1, 0xE9, 0x0D, 0xED, 0x01, 0x20], 0x100);
    bus.poke(0x110, &[0x30]);
    bus.poke(0x120, &[0x50]);
    cpu.r[..2].copy_from_slice(&[0x40, 0x40]);
    assert_eq!(cpu.step(&mut bus), 6);
    assert_eq!(cc(&cpu), 0x00);
    assert_eq!(cpu.step(&mut bus), 9);
    assert_eq!(cc(&cpu), 0x40);
    assert_eq!(cpu.step(&mut bus), 12);
    assert_eq!(cc(&cpu), 0x80);
}

#[test]
fn decimal_adjust() {
    // BCD add: augend biased by 0x66, then ADD, then DAR.
    for (a, b, want, carry) in [
        (0x19, 0x28, 0x47, false),
        (0x15, 0x15, 0x30, false),
        (0x12, 0x34, 0x46, false),
        (0x55, 0x55, 0x10, true),
        (0x99, 0x01, 0x00, true),
    ] {
        let (mut cpu, mut bus) = setup(&[0x84, 0x66, 0x84, b, 0x94], 0x100);
        cpu.set_reg(0, a);
        assert_eq!(steps(&mut cpu, &mut bus, 3), [6, 6, 9]);
        assert_eq!(cpu.reg(0), want, "{a:02x}+{b:02x}");
        assert_eq!(cpu.psl & psl::C != 0, carry);
    }
    // BCD subtract: SUB then DAR.
    for (a, b, want) in [(0x47, 0x28, 0x19), (0x30, 0x15, 0x15), (0x10, 0x20, 0x90)] {
        let (mut cpu, mut bus) = setup(&[0xA7, b, 0x97], 0x100);
        cpu.set_reg(3, a);
        steps(&mut cpu, &mut bus, 2);
        assert_eq!(cpu.reg(3), want, "{a:02x}-{b:02x}");
    }
}

#[test]
fn rotates() {
    // (opcode, value, psl in, result, psl out masked by C|IDC|OVF|CC)
    let cases = [
        (0xD0, 0x81, psl::C, 0x03, psl::C | psl::OVF | 0x40),
        (0xD0, 0x81, psl::IDC, 0x03, psl::IDC | psl::OVF | 0x40),
        (0xD0, 0x20, 0, 0x40, 0x40),
        (0x50, 0x01, 0, 0x80, psl::OVF | 0x80),
        (0x50, 0x00, psl::C, 0x00, psl::C),
        (0xD0, 0x40, psl::WC | psl::C, 0x81, psl::OVF | 0x80),
        (
            0xD0,
            0x90,
            psl::WC,
            0x20,
            psl::C | psl::IDC | psl::OVF | 0x40,
        ),
        (0xD0, 0x10, psl::WC | psl::IDC, 0x20, psl::IDC | 0x40),
        (0x50, 0x02, psl::WC | psl::C, 0x81, psl::OVF | 0x80),
        (0x50, 0x41, psl::WC, 0x20, psl::C | psl::IDC | 0x40),
        (0x50, 0x01, psl::WC, 0x00, psl::C),
    ];
    for (op, v, pin, r, pout) in cases {
        let (mut cpu, mut bus) = setup(&[op], 0x100);
        cpu.psl = pin;
        cpu.set_reg(0, v);
        assert_eq!(cpu.step(&mut bus), 6);
        assert_eq!(
            (cpu.reg(0), cpu.psl & F),
            (r, pout),
            "op {op:02x} {v:02x} {pin:02x}"
        );
    }
    let (mut cpu, mut bus) = setup(&[0xD3, 0x52], 0x100);
    cpu.r[3] = 0x01;
    cpu.r[2] = 0x01;
    steps(&mut cpu, &mut bus, 2);
    assert_eq!((cpu.r[3], cpu.r[2]), (0x02, 0x80));
}

#[test]
fn test_under_mask() {
    let (mut cpu, mut bus) = setup(&[0xF5, 0x30, 0xF5, 0x0F, 0xF4, 0x00], 0x100);
    cpu.set_reg(1, 0xF0);
    assert_eq!(cpu.step(&mut bus), 9);
    assert_eq!(cc(&cpu), 0x00);
    cpu.step(&mut bus);
    assert_eq!(cc(&cpu), 0x80);
    cpu.step(&mut bus);
    assert_eq!(cc(&cpu), 0x00);
}

#[test]
fn test_psl_and_psu() {
    let (mut cpu, mut bus) = setup(&[0xB5, 0x0A, 0xB5, 0x0B, 0xB4, 0xA0, 0xB4, 0xA0], 0x100);
    cpu.psl = psl::COM | psl::WC;
    assert_eq!(cpu.step(&mut bus), 9);
    assert_eq!(cpu.psl, psl::COM | psl::WC);
    cpu.step(&mut bus);
    assert_eq!(cpu.psl, psl::COM | psl::WC | 0x80);
    cpu.psu = psu::II;
    bus.sense = true;
    assert_eq!(cpu.step(&mut bus), 9);
    assert_eq!(cc(&cpu), 0x00);
    bus.sense = false;
    cpu.step(&mut bus);
    assert_eq!(cc(&cpu), 0x80);
}

#[test]
fn store_psu_psl_sample_sense() {
    let (mut cpu, mut bus) = setup(&[0x12, 0x12, 0x13], 0x100);
    cpu.psu = psu::II | 5;
    bus.sense = true;
    assert_eq!(cpu.step(&mut bus), 6);
    assert_eq!((cpu.reg(0), cc(&cpu)), (0xA5, 0x80));
    assert_eq!(cpu.psu, psu::II | 5);
    bus.sense = false;
    cpu.step(&mut bus);
    assert_eq!((cpu.reg(0), cc(&cpu)), (0x25, 0x40));
    cpu.psl = psl::COM;
    assert_eq!(cpu.step(&mut bus), 6);
    assert_eq!((cpu.reg(0), cpu.psl), (psl::COM, psl::COM | 0x40));
}

#[test]
fn psu_writes_mask_bits_and_report_flag_changes() {
    let prog = [
        0x92, // LPSU (R0=0xFF)
        0x74, 0xFF, // CPSU all
        0x76, 0xFF, // PPSU all
        0x76, 0x40, // PPSU F (no change)
        0x74, 0x40, // CPSU F
        0x92, // LPSU (R0=0x00)
    ];
    let (mut cpu, mut bus) = setup(&prog, 0x100);
    cpu.set_reg(0, 0xFF);
    assert_eq!(cpu.step(&mut bus), 6);
    assert_eq!(cpu.psu, 0x67);
    assert_eq!(cpu.step(&mut bus), 9);
    assert_eq!(cpu.psu, 0);
    assert_eq!(cpu.step(&mut bus), 9);
    assert_eq!(cpu.psu, 0x67);
    steps(&mut cpu, &mut bus, 2);
    assert_eq!(cpu.psu, 0x27);
    cpu.set_reg(0, 0);
    cpu.step(&mut bus);
    assert_eq!(cpu.psu, 0);
    assert_eq!(bus.flags, [true, false, true, false]);
}

#[test]
fn register_bank_switching() {
    let prog = [
        0x77, 0x10, // PPSL RS
        0x05, 0x22, // LODI,R1 $22
        0x75, 0x10, // CPSL RS
        0x93, // LPSL (R0 = RS)
        0x01, // LODZ R1
    ];
    let (mut cpu, mut bus) = setup(&prog, 0x100);
    cpu.r[1] = 0x11;
    assert_eq!(cpu.step(&mut bus), 9);
    assert_eq!(cpu.psl, psl::RS);
    cpu.step(&mut bus);
    assert_eq!((cpu.r[1], cpu.r[4]), (0x11, 0x22));
    assert_eq!(cpu.step(&mut bus), 9);
    assert_eq!((cpu.psl & psl::RS, cpu.reg(1)), (0, 0x11));
    cpu.r[0] = psl::RS;
    assert_eq!(cpu.step(&mut bus), 6);
    assert_eq!(cpu.psl, psl::RS);
    cpu.step(&mut bus);
    assert_eq!(cpu.r[0], 0x22);
}

#[test]
fn indexing_uses_selected_bank() {
    let (mut cpu, mut bus) = setup(&[0x0F, 0x60, 0x40], 0x100);
    bus.poke(0x40, &[0, 1, 2, 3, 4, 5, 6, 7]);
    cpu.r[3] = 2;
    cpu.r[6] = 6;
    cpu.psl = psl::RS;
    cpu.step(&mut bus);
    assert_eq!(cpu.r[0], 6);
}

#[test]
fn port_io() {
    let prog = [0x31, 0x72, 0x57, 0x55, 0xB1, 0xF2, 0xD7, 0x77];
    let (mut cpu, mut bus) = setup(&prog, 0x100);
    assert_eq!(steps(&mut cpu, &mut bus, 6), [6, 6, 9, 6, 6, 9]);
    assert_eq!((cpu.reg(1), cpu.reg(2), cpu.reg(3)), (0x80, 0x00, 0x56));
    assert_eq!(cc(&cpu), 0x40);
    assert_eq!(bus.ins, [Port::Ctrl, Port::Data, Port::Ext(0x55)]);
    assert_eq!(
        bus.out,
        [
            (Port::Ctrl, 0x80),
            (Port::Data, 0x00),
            (Port::Ext(0x77), 0x56)
        ]
    );
    assert_eq!(cpu.pc, 0x108);
}

#[test]
fn red_sets_cc_from_port() {
    let (mut cpu, mut bus) = setup(&[0x70, 0x30], 0x100);
    cpu.step(&mut bus);
    assert_eq!(cc(&cpu), 0);
    cpu.step(&mut bus);
    assert_eq!(cc(&cpu), 0x80);
}

#[test]
fn conditional_relative_branches() {
    for (op, ccv, taken) in [
        (0x1B, 0x80, true),
        (0x18, 0x00, true),
        (0x18, 0x40, false),
        (0x19, 0x40, true),
        (0x1A, 0x80, true),
        (0x1A, 0x00, false),
        (0x98, 0x40, true),
        (0x98, 0x00, false),
        (0x9A, 0x80, false),
        (0x99, 0x80, true),
    ] {
        let (mut cpu, mut bus) = setup(&[op, 0x10], 0x100);
        cpu.psl = ccv;
        assert_eq!(cpu.step(&mut bus), 9);
        assert_eq!(cpu.pc, if taken { 0x112 } else { 0x102 }, "op {op:02x}");
        assert_eq!(cpu.psu, 0);
    }
}

#[test]
fn relative_branch_negative_and_indirect() {
    let (mut cpu, mut bus) = setup(&[0x1B, 0x40], 0x100);
    cpu.step(&mut bus);
    assert_eq!(cpu.pc, 0x0C2);
    let (mut cpu, mut bus) = setup(&[0x1B, 0x82], 0x100);
    bus.poke(0x104, &[0x45, 0x67]);
    assert_eq!(cpu.step(&mut bus), 15);
    assert_eq!(cpu.pc, 0x4567);
}

#[test]
fn conditional_absolute_branches() {
    for (op, ccv, taken) in [
        (0x1F, 0x80, true),
        (0x1D, 0x40, true),
        (0x1D, 0x80, false),
        (0x9C, 0x40, true),
        (0x9E, 0x80, false),
    ] {
        let (mut cpu, mut bus) = setup(&[op, 0x62, 0x34], 0x100);
        cpu.psl = ccv;
        assert_eq!(cpu.step(&mut bus), 9);
        assert_eq!(cpu.pc, if taken { 0x6234 } else { 0x103 }, "op {op:02x}");
    }
}

#[test]
fn absolute_branch_indirect_and_untaken_skips_pointer() {
    let (mut cpu, mut bus) = setup(&[0x1F, 0x90, 0x00], 0x100);
    bus.poke(0x1000, &[0x23, 0x45]);
    assert_eq!(cpu.step(&mut bus), 15);
    assert_eq!((cpu.pc, bus.reads), (0x2345, 2));
    let (mut cpu, mut bus) = setup(&[0x1C, 0x90, 0x00, 0x18, 0x90], 0x100);
    cpu.psl = 0x40;
    assert_eq!(steps(&mut cpu, &mut bus, 2), [9, 9]);
    assert_eq!((cpu.pc, bus.reads), (0x105, 0));
}

#[test]
fn untaken_branch_skip_wraps_within_page() {
    let (mut cpu, mut bus) = setup(&[0x1C, 0x00, 0x00], 0x1FFD);
    cpu.psl = 0x40;
    cpu.step(&mut bus);
    assert_eq!(cpu.pc, 0x0000);
    let (mut cpu, mut bus) = setup(&[0x1C], 0x3FFF);
    cpu.psl = 0x40;
    cpu.step(&mut bus);
    assert_eq!(cpu.pc, 0x2002);
}

#[test]
fn register_branches() {
    // (opcode, initial R1, final R1, taken)
    for (op, r, after, taken) in [
        (0x59u8, 0u8, 0u8, false),
        (0x59, 5, 5, true),
        (0x5D, 1, 1, true),
        (0x5D, 0, 0, false),
        (0xD9, 0xFF, 0x00, false),
        (0xD9, 0x00, 0x01, true),
        (0xDD, 0x7F, 0x80, true),
        (0xF9, 0x01, 0x00, false),
        (0xF9, 0x00, 0xFF, true),
        (0xFD, 0x02, 0x01, true),
    ] {
        let (mut cpu, mut bus) = setup(&[op, 0x05, 0x00], 0x100);
        cpu.set_reg(1, r);
        assert_eq!(cpu.step(&mut bus), 9);
        let abs = op & 4 != 0;
        let target = if abs { 0x0500 } else { 0x107 };
        let fall = if abs { 0x103 } else { 0x102 };
        assert_eq!(cpu.pc, if taken { target } else { fall }, "op {op:02x}");
        assert_eq!(cpu.reg(1), after);
    }
}

#[test]
fn zero_page_relative_branches() {
    let (mut cpu, mut bus) = setup(&[0x9B, 0x7E], 0x2100);
    assert_eq!(cpu.step(&mut bus), 9);
    assert_eq!(cpu.pc, 0x1FFE);
    let (mut cpu, mut bus) = setup(&[0x9B, 0x10], 0x4100);
    cpu.step(&mut bus);
    assert_eq!(cpu.pc, 0x0010);
    let (mut cpu, mut bus) = setup(&[0x9B, 0x82], 0x100);
    bus.poke(0x0002, &[0x31, 0x23]);
    assert_eq!(cpu.step(&mut bus), 15);
    assert_eq!(cpu.pc, 0x3123);
    let (mut cpu, mut bus) = setup(&[0xBB, 0xC0], 0x2345);
    bus.poke(0x1FC0, &[0x00, 0x80]);
    assert_eq!(cpu.step(&mut bus), 15);
    assert_eq!((cpu.pc, cpu.psu, cpu.ras[1]), (0x0080, 1, 0x2347));
}

#[test]
fn indexed_branches_use_r3_of_current_bank() {
    let (mut cpu, mut bus) = setup(&[0x9F, 0x12, 0x00], 0x100);
    cpu.set_reg(3, 5);
    assert_eq!(cpu.step(&mut bus), 9);
    assert_eq!(cpu.pc, 0x1205);
    let (mut cpu, mut bus) = setup(&[0x9F, 0x7F, 0xFF], 0x100);
    cpu.r[3] = 9;
    cpu.r[6] = 2;
    cpu.psl = psl::RS;
    cpu.step(&mut bus);
    assert_eq!(cpu.pc, 0x0001);
    let (mut cpu, mut bus) = setup(&[0xBF, 0x90, 0x00], 0x2100);
    bus.poke(0x1000, &[0x04, 0x00]);
    cpu.set_reg(3, 0x10);
    assert_eq!(cpu.step(&mut bus), 15);
    assert_eq!((cpu.pc, cpu.psu, cpu.ras[1]), (0x0410, 1, 0x2103));
}

#[test]
fn calls_and_returns() {
    let (mut cpu, mut bus) = setup(&[0x3F, 0x12, 0x00], 0x100);
    bus.poke(0x1200, &[0x3B, 0x10]);
    bus.poke(0x1212, &[0x14, 0x17]);
    assert_eq!(steps(&mut cpu, &mut bus, 2), [9, 9]);
    assert_eq!((cpu.pc, cpu.psu & psu::SP), (0x1212, 2));
    assert_eq!(&cpu.ras[1..3], &[0x103, 0x1202]);
    cpu.psl = 0x40;
    assert_eq!(cpu.step(&mut bus), 9);
    assert_eq!((cpu.pc, cpu.psu), (0x1213, 2));
    assert_eq!(cpu.step(&mut bus), 9);
    assert_eq!((cpu.pc, cpu.psu), (0x1202, 1));
    cpu.psl = 0x00;
    bus.poke(0x1202, &[0x14]);
    cpu.step(&mut bus);
    assert_eq!((cpu.pc, cpu.psu), (0x103, 0));
}

#[test]
fn conditional_calls() {
    // (opcode, cc, R1, taken)
    for (op, ccv, r1, taken) in [
        (0x38u8, 0x00u8, 0u8, true),
        (0x39, 0x00, 0, false),
        (0x3C, 0x00, 0, true),
        (0x3E, 0x40, 0, false),
        (0xB8, 0x40, 0, true),
        (0xB9, 0x40, 0, false),
        (0xBC, 0x80, 0, true),
        (0xBE, 0x80, 0, false),
        (0x79, 0x00, 1, true),
        (0x79, 0x00, 0, false),
        (0x7D, 0x00, 3, true),
        (0x7D, 0x00, 0, false),
    ] {
        let (mut cpu, mut bus) = setup(&[op, 0x05, 0x00], 0x100);
        cpu.psl = ccv;
        cpu.set_reg(1, r1);
        assert_eq!(cpu.step(&mut bus), 9);
        let abs = op & 4 != 0;
        let next = if abs { 0x103 } else { 0x102 };
        if taken {
            let target = if abs { 0x0500 } else { 0x107 };
            assert_eq!(
                (cpu.pc, cpu.psu, cpu.ras[1]),
                (target, 1, next),
                "op {op:02x}"
            );
        } else {
            assert_eq!((cpu.pc, cpu.psu), (next, 0), "op {op:02x}");
        }
    }
}

#[test]
fn return_stack_pointer_wraps() {
    let (mut cpu, mut bus) = setup(&[0x3B, 0x00, 0x17], 0x100);
    cpu.psu = 7;
    cpu.step(&mut bus);
    assert_eq!((cpu.psu, cpu.ras[0], cpu.pc), (0, 0x102, 0x102));
    cpu.step(&mut bus);
    assert_eq!((cpu.psu, cpu.pc), (7, 0x102));
    let mut cpu = Cpu::new();
    let mut bus = FlatBus::with_image(&[0x3B, 0x7E], 0);
    for _ in 0..9 {
        cpu.step(&mut bus);
    }
    assert_eq!(cpu.psu, 1);
    assert_eq!(cpu.ras, [2; 8]);
}

#[test]
fn halt_and_interrupt() {
    let (mut cpu, mut bus) = setup(&[0x40], 0x100);
    assert_eq!(cpu.step(&mut bus), 6);
    assert!(cpu.halted);
    assert_eq!(steps(&mut cpu, &mut bus, 3), [6, 6, 6]);
    assert_eq!(cpu.pc, 0x101);
    cpu.irq = true;
    bus.vector = 0x10;
    assert_eq!(cpu.step(&mut bus), 9);
    assert!(!cpu.halted);
    assert_eq!((cpu.pc, cpu.psu, cpu.ras[1]), (0x0010, psu::II | 1, 0x101));
    assert_eq!(cpu.clocks, 6 * 4 + 9);
}

#[test]
fn interrupt_vectors_negative_and_indirect() {
    let (mut cpu, mut bus) = setup(&[0xC0], 0x2345);
    cpu.irq = true;
    bus.vector = 0x7C;
    assert_eq!(cpu.step(&mut bus), 9);
    assert_eq!((cpu.pc, cpu.ras[1]), (0x1FFC, 0x2345));
    let (mut cpu, mut bus) = setup(&[0xC0], 0x100);
    bus.poke(0x0010, &[0x45, 0x00]);
    cpu.irq = true;
    bus.vector = 0x90;
    assert_eq!(cpu.step(&mut bus), 15);
    assert_eq!(cpu.pc, 0x4500);
}

#[test]
fn interrupt_masked_until_rete_or_cpsu() {
    let (mut cpu, mut bus) = setup(&[0xC0, 0x34, 0x37], 0x100);
    bus.poke(0x0020, &[0x74, 0x20]);
    cpu.irq = true;
    cpu.psu = psu::II;
    cpu.psl = 0x40;
    assert_eq!(cpu.step(&mut bus), 6);
    assert_eq!(cpu.pc, 0x101);
    cpu.ras[0] = 0x0020;
    cpu.psu = psu::II | 1;
    cpu.ras[1] = 0x0020;
    assert_eq!(cpu.step(&mut bus), 9);
    assert_eq!((cpu.pc, cpu.psu), (0x102, psu::II | 1));
    assert_eq!(cpu.step(&mut bus), 9);
    assert_eq!((cpu.pc, cpu.psu), (0x0020, 0));
    bus.vector = 0x08;
    assert_eq!(cpu.step(&mut bus), 9);
    assert_eq!((cpu.pc, cpu.psu, cpu.ras[1]), (0x0008, psu::II | 1, 0x0020));
    cpu.irq = false;
    cpu.pc = 0x0020;
    cpu.step(&mut bus);
    assert_eq!(cpu.psu, 1);
    cpu.irq = true;
    cpu.step(&mut bus);
    assert_eq!((cpu.pc, cpu.ras[2]), (0x0008, 0x0022));
    assert!(bus.flags.is_empty());
}

#[test]
fn illegal_opcodes_are_six_clock_noops() {
    let ops = [0x10, 0x11, 0x90, 0x91, 0xB6, 0xB7, 0xC4, 0xC5, 0xC6, 0xC7];
    let (mut cpu, mut bus) = setup(&ops, 0x100);
    cpu.r = [1, 2, 3, 4, 5, 6, 7];
    cpu.psl = 0x5A;
    assert_eq!(steps(&mut cpu, &mut bus, ops.len()), [6; 10]);
    assert_eq!(cpu.illegal, 10);
    assert_eq!((cpu.pc, cpu.psl, cpu.psu), (0x10A, 0x5A, 0));
    assert_eq!(cpu.r, [1, 2, 3, 4, 5, 6, 7]);
}

#[test]
fn program_sums_table_with_bdrr() {
    let prog = [
        0x04, 0x00, // LODI,R0 0
        0x05, 0x04, // LODI,R1 4
        0x8D, 0x61, 0xFF, // loop: ADDA,R0 $01FF,R1
        0xF9, 0x7B, // BDRR,R1 loop
        0x40, // HALT
    ];
    let mut bus = FlatBus::with_image(&prog, 0);
    bus.mem[0x200..0x204].copy_from_slice(&[1, 2, 3, 4]);
    let mut cpu = Cpu::new();
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!((cpu.reg(0), cpu.reg(1), cpu.pc), (10, 0, 0x0A));
    assert_eq!(cpu.clocks, 6 + 6 + 4 * (12 + 9) + 6);
}

#[test]
fn program_sixteen_bit_add_and_subtract_with_carry() {
    let prog = [
        0x77, 0x08, // PPSL WC
        0x75, 0x01, // CPSL C
        0x05, 0x12, // LODI,R1 $12
        0x06, 0xFF, // LODI,R2 $FF
        0x86, 0x01, // ADDI,R2 $01
        0x85, 0x0F, // ADDI,R1 $0F  -> $2200
        0x77, 0x01, // PPSL C (no borrow)
        0xA6, 0x01, // SUBI,R2 $01
        0xA5, 0x00, // SUBI,R1 $00  -> $21FF
        0x40,
    ];
    let mut bus = FlatBus::with_image(&prog, 0);
    let mut cpu = Cpu::new();
    for _ in 0..6 {
        cpu.step(&mut bus);
    }
    assert_eq!((cpu.reg(1), cpu.reg(2)), (0x22, 0x00));
    assert_eq!(cpu.psl & psl::C, 0);
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!((cpu.reg(1), cpu.reg(2)), (0x21, 0xFF));
    assert_eq!(cpu.psl & psl::C, psl::C);
}

#[test]
fn program_subroutine_copies_block() {
    let prog = [
        0x07, 0x03, // LODI,R3 3
        0x3F, 0x00, 0x10, // BSTA,UN copy
        0x40, // HALT
    ];
    let sub = [
        0x0F, 0x60, 0x7E, // copy: LODA,R0 $007E,R3
        0xCF, 0x60, 0x8E, // STRA,R0 $008E,R3
        0xFB, 0x78, // BDRR,R3 copy
        0x17, // RETC,UN
    ];
    let mut bus = FlatBus::with_image(&prog, 0);
    bus.mem[0x10..0x10 + sub.len()].copy_from_slice(&sub);
    bus.mem[0x7F..0x82].copy_from_slice(&[0xA1, 0xB2, 0xC3]);
    let mut cpu = Cpu::new();
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!(&bus.mem[0x8F..0x92], &[0xA1, 0xB2, 0xC3]);
    assert_eq!((cpu.pc, cpu.psu), (0x06, 0));
}
