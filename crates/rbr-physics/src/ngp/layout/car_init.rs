//! NGP 7.4.778's per-car init (`0x1003f5b0`, run by NGP's state listener `0x100f06b0` as the race starts) and the
//! tyre-definition copy it reads (`0x100f1690`, a car-load listener): the setup options' range checks on the car's
//! definitions, the built-in tyre types (read from the DLL), the per-car scaling by the car's factor
//! ([`ratio`]) and the per-car words in NGP's `.data`, which the ported step functions read. Addresses are NGP.dll's
//! file addresses; [`Data`] places them at the DLL's base in `mem`.
//!
//! Not ported: the CRC check of the car files (`0x100e88e0`, `0x10041810`; a mismatch sets the drive definition's
//! `+0x654` to 3), the per-slot force-feedback percentages (`ForceFeedbackSensitivity<Surface>_Car<slot>`), the listener unlinks (`0x10042940`), the idle table's
//! shared count (`0x101767ec`; nothing in the step reads it) and the game object's two words.

use crate::mem::Mem;
use crate::ngp::atmosphere::atmosphere;
use crate::ngp::diff::{diff_types, drive_type};
use crate::ngp::ffb;
use rbr_formats::ngp::Tables;

const FILE_BASE: u32 = 0x1000_0000;

/// NGP.dll's `.data` at its base in `mem`.
#[derive(Clone, Copy, Debug)]
pub struct Data {
    pub base: u32,
}

impl Data {
    pub fn at(self, file: u32) -> u32 {
        file - FILE_BASE + self.base
    }
    fn f32(self, mem: &Mem, file: u32) -> f32 {
        mem.f32(self.at(file))
    }
    fn set(self, mem: &mut Mem, file: u32, value: f32) {
        mem.set_f32(self.at(file), value);
    }
}

/// The car's objects the init reads.
#[derive(Clone, Copy, Debug)]
pub struct Car {
    pub car: u32,
    /// The `CCompleteCarSetup` (`*0x7ecd00`).
    pub setup: u32,
    /// The `CCompleteCarSetupOptions` (`*0x7ecd04`).
    pub options: u32,
    /// A block of [`IDLE_TABLE_SIZE`] bytes for the idle table the engine init builds (on NGP's heap in the game).
    pub idle_table: u32,
    /// The ini's `ForceFeedbackSensitivity<Surface>` percentages for gravel, tarmac and snow ([`ffb::sensitivity`]).
    pub ffb_sensitivity: [i32; 3],
}

/// `0x101767e8`: the idle table the engine init builds, which the engine wrapper reads: its vtable, the knot count,
/// then the knots' speeds and values as two `std::vector<double>` (begin, end, capacity).
const IDLE_TABLE: u32 = 0x1017_67e8;
/// The idle table's vtable in NGP's `.rdata`; the engine wrapper checks its lookup slot (`+0x14`, `0x1004a700`).
const IDLE_TABLE_VTABLE: u32 = 0x1014_8930;
/// The idle table's lookup (`0x1004a700`, the vtable's `+0x14`).
const IDLE_TABLE_LOOKUP: u32 = 0x1004_a700;
/// The idle table's bytes: the object (0x24) and its two vectors of at most 16 knots.
pub const IDLE_TABLE_SIZE: u32 = 0x24 + 2 * 16 * 8;

/// `0x101767b0`: NGP's speed ring, a `std::deque<float>` (its proxy, map, map size, offset and size) of the engine's
/// last speeds, which the engine wrapper turns over every step.
const SPEED_RING: u32 = 0x1017_67b0;
const RING_LEN: u32 = 0x6b;
const RING_MAP: u32 = 0x20;
const RING_BLOCKS: u32 = RING_LEN.div_ceil(4);
/// The speed ring's bytes: its proxy, its map and the blocks its entries fill.
pub const SPEED_RING_SIZE: u32 = 8 + 4 * RING_MAP + 16 * RING_BLOCKS;

/// The head gasket block's bytes: `1/d` and `(1/d - 1) * 0.01`.
pub const HEAD_GASKET_SIZE: u32 = 8;

/// NGP's head gasket edit (`0x100e71f0`, as the DLL loads, with the ini's `headGasketDurability`, default 1): from 2
/// on, `1/d` and `(1/d - 1) * 0.01` (`0x10145a18`) go to `at` ([`HEAD_GASKET_SIZE`] bytes, on NGP's heap in the game),
/// and `CEngine::SimD870`'s stall roll jumps through code NGP writes into the padding at 0x50da04 that scales its
/// draw by them (`engine::slot_roll`).
pub fn head_gasket(mem: &mut Mem, durability: i32, at: u32) {
    if durability <= 1 {
        return;
    }
    let scale = 1.0 / durability as f32;
    mem.map(at, &[scale.to_le_bytes(), ((scale - 1.0) * f32::from_bits(0x3c23_d70a)).to_le_bytes()].concat());
    let [scale_at, offset_at] = [at, at + 4].map(u32::to_le_bytes);
    mem.map(0x50_d9c4, &[0xeb, 0x3e]);
    mem.map(0x50_da04, &[[0xf3, 0x0f, 0x59, 0x0d], scale_at, [0xeb, 0x06, 0xcc, 0xcc]].concat());
    mem.map(0x50_da14, &[[0xf3, 0x0f, 0x58, 0x0d], offset_at, [0x89, 0xd0, 0xeb, 0xa6]].concat());
}

/// NGP's static init of the speed ring (`0x10001980`, as the DLL loads), at `at` ([`SPEED_RING_SIZE`] bytes): 107
/// zeros, in the 27 blocks of four they fill, with the map at the 32 blocks those pushes grew it to.
pub fn speed_ring(mem: &mut Mem, data: Data, at: u32) {
    let (map, blocks) = (at + 8, at + 8 + 4 * RING_MAP);
    mem.map(at, &vec![0; SPEED_RING_SIZE as usize]);
    let ring = data.at(SPEED_RING);
    mem.set_u32(at, ring);
    for k in 0..RING_BLOCKS {
        mem.set_u32(map + 4 * k, blocks + 16 * k);
    }
    for (k, word) in [at, map, RING_MAP, 0, RING_LEN].into_iter().enumerate() {
        mem.set_u32(ring + 4 * k as u32, word);
    }
}

const WHEELS: u32 = 0x470;
const DEF: u32 = 0x34;
const TYRE_DEF: u32 = 0x56c;
/// `0x101768c8`: the four wheels' tyre definitions as the car loaded them, 0xc5 words each.
const TYRE_COPIES: u32 = 0x1017_68c8;
const TYRE_COPY_SIZE: u32 = 0x314;

/// `0x100f1690`: the wheels' tyre definitions (`+0x56c`, 0xc5 words) into [`TYRE_COPIES`].
pub fn copy_tyre_defs(mem: &mut Mem, data: Data, car: u32) {
    for k in 0..4 {
        let def = mem.u32(mem.u32(car + WHEELS + 4 * k) + DEF);
        mem.copy(data.at(TYRE_COPIES + TYRE_COPY_SIZE * k), def + TYRE_DEF, TYRE_COPY_SIZE as usize);
    }
}

/// `0x100065f0`: keeps `*at` in `[min, max]` (within 2^-23 of a bound counts as in), else resets it to the bound it
/// passed (`max` for a NaN); with both bounds 0 a nonzero value becomes 0. Whether the value was kept. (The game
/// logs a reset to `NGP_SetupErrors.log`.)
fn clamp(mem: &mut Mem, at: u32, min: f32, max: f32) -> bool {
    let v = mem.f32(at);
    match clamped(v, min, max) {
        Some(reset) => {
            mem.set_f32(at, reset);
            false
        }
        None => true,
    }
}

const EPSILON: f32 = 1.1920929e-7;

fn clamped(v: f32, min: f32, max: f32) -> Option<f32> {
    if min == 0.0 && max == 0.0 && v != 0.0 {
        Some(0.0)
    } else if v < min {
        ((min - v).abs() >= EPSILON).then_some(min)
    } else if v <= max || (v - max).abs() < EPSILON {
        None
    } else {
        Some(max)
    }
}

/// [`clamp`] against the options' bounds at `options + bounds` and `+ 0x10`.
fn clamp_to(mem: &mut Mem, at: u32, options: u32, bounds: u32) -> bool {
    let (min, max) = (mem.f32(options + bounds), mem.f32(options + bounds + 0x10));
    clamp(mem, at, min, max)
}

/// An options percentage (an unsigned) as a fraction.
fn percent(mem: &Mem, at: u32) -> f32 {
    mem.u32(at) as f32 / 100.0
}

/// `0x100b7ea0`: the car's factor, 0.000714 over its definition's `+0x16c` (`InverseMass`), 1 without one. x87 at
/// 53 bits, rounded to a float.
pub fn ratio(mem: &Mem, car: u32) -> f32 {
    let def = mem.u32(car + DEF);
    if def == 0 {
        return 1.0;
    }
    let inverse_mass = mem.f32(def + 0x16c);
    if inverse_mass > 0.0 {
        (f64::from(f32::from_bits(0x3a3b_2bbb)) / f64::from(inverse_mass)) as f32
    } else {
        1.0
    }
}

/// `0x1003f5b0`: the per-car init, after `0x100f06b0` raises the car's `+0x148` by its body's `+0x138` less 0.215.
/// Writes `.data`, the car's definitions and objects, and the two exe immediates NGP patches (0x53daa6, 0x4f13a7).
pub fn init(mem: &mut Mem, data: Data, c: Car, tables: &Tables) {
    let car = c.car;
    let body = mem.u32(car + 0x4a0);
    if body != 0 && mem.u32(body + DEF) != 0 {
        let height = mem.f32(mem.u32(body + DEF) + 0x138) - 0.215 + mem.f32(car + 0x148);
        mem.set_f32(car + 0x148, height);
    }
    // The setup getters' (`0x10011cb0`, `0x10013cd0`) cached addresses of the game's setup pointers, and the signal
    // hub getter's (`0x1002cc00`) of the game's signals.
    mem.set_u32(data.at(0x1017_4ba0), 0x7e_cd00);
    mem.set_u32(data.at(0x1017_4bb0), 0x7e_cd04);
    mem.set_u32(data.at(0x1017_60d4), 0x89_3600);
    let def = mem.u32(car + DEF);
    let opt = c.options;
    engine(mem, data, c, tables);
    vcu(mem, data, c);
    drive(mem, data, c);
    clamp_to(mem, def + 0x22c, opt, 0x28);
    clamp_to(mem, def + 0x4460, opt, 0x58);
    clamp_to(mem, def + 0x4470, opt, 0x88);

    data.set(mem, 0x1017_1300, tables.probe_defaults[0]);
    data.set(mem, 0x1017_1304, tables.probe_defaults[1]);
    data.set(mem, 0x1017_1308, tables.probe_defaults[2]);
    let mut lift = 0.0f32;
    for k in 0..4 {
        let wheel = mem.u32(car + WHEELS + 4 * k);
        let wheel_def = mem.u32(wheel + DEF);
        let kind = mem.u32(wheel_def + TYRE_DEF + 0xc);
        let kind = if kind < 7 { kind as usize } else { 0 };
        for &(at, word) in &tables.tyres[kind].definition {
            mem.set_u32(wheel_def + at, word);
        }
        for &(at, word) in &tables.tyres[kind].wheel {
            mem.set_u32(wheel + at, word);
        }
        let copy = TYRE_COPIES + TYRE_COPY_SIZE * k;
        let (hub, tread) = (data.f32(mem, copy + 0x7c), data.f32(mem, copy + 0x8c));
        if hub > 0.0 && tread > 0.0 {
            let width = tread + hub;
            mem.set_f32(def + 0x24c, width);
            data.set(mem, 0x1017_1300, width + 0.085);
            lift = width - 0.315;
        }
        wheel_init(mem, data, c, wheel, k, tables);
    }
    if lift != 0.0 {
        data.set(mem, 0x1017_1304, lift + data.f32(mem, 0x1017_1304));
        data.set(mem, 0x1017_1308, lift + data.f32(mem, 0x1017_1308));
    }
    mem.set_f32(0x53_daa6, data.f32(mem, 0x1017_1308));
    let r = ratio(mem, car);
    scale_damage(mem, car, r);
    data.set(mem, 0x1017_130c, r * 500.0);
    mem.set_f32(0x4f_13a7, r * 11000.0);
    data.set(mem, 0x1017_1310, r * 800.0);
    data.set(mem, 0x1017_1314, r * 4000.0);
    let s = mem.f32(def + 0x44a0);
    let sensitivity = ffb::sensitivity(s, mem.u32(def + 0x44c0), c.ffb_sensitivity);
    if sensitivity.to_bits() != s.to_bits() {
        mem.set_f32(def + 0x44a0, sensitivity);
    }
}

/// The idle table at `at` with `knots` (speed, value), stored at [`IDLE_TABLE`]. Only the vtable's lookup slot is
/// mapped.
fn idle_table(mem: &mut Mem, data: Data, at: u32, knots: &[(f64, f64)]) {
    let n = knots.len() as u32;
    let (xs, ys) = (at + 0x24, at + 0x24 + 8 * n);
    mem.map(at, &vec![0; IDLE_TABLE_SIZE as usize]);
    let vtable = data.at(IDLE_TABLE_VTABLE);
    mem.map(vtable, &[0; 0x18]);
    mem.set_u32(vtable + 0x14, data.at(IDLE_TABLE_LOOKUP));
    for (k, word) in [vtable, n, xs, xs + 8 * n, xs + 8 * n, ys, ys + 8 * n, ys + 8 * n].into_iter().enumerate() {
        mem.set_u32(at + 4 * k as u32, word);
    }
    for (k, &(x, y)) in knots.iter().enumerate() {
        mem.map(xs + 8 * k as u32, &x.to_le_bytes());
        mem.map(ys + 8 * k as u32, &y.to_le_bytes());
    }
    mem.set_u32(data.at(IDLE_TABLE), at);
}

/// `0x100b7930`: the damage handler's (`+0x9f4`) 64 rows scaled by the car's factor (`+0x18`, `+0x38`, `+0x58`,
/// `+0x78` of each 0x88) and their 64 limits divided by it (`+0x2210`, each 0x58).
fn scale_damage(mem: &mut Mem, car: u32, r: f32) {
    let handler = mem.u32(car + 0x9f4);
    if handler == 0 {
        return;
    }
    for i in 0..64 {
        for at in [0x18, 0x38, 0x58, 0x78] {
            let at = handler + at + 0x88 * i;
            mem.set_f32(at, r * mem.f32(at));
        }
        let at = handler + 0x2210 + 0x58 * i;
        mem.set_f32(at, mem.f32(at) / r);
    }
}

/// The words `0x1004b5a0` stores, in [`crate::ngp::atmosphere::Atmosphere`]'s order.
const ATMOSPHERE: [u32; 6] = [0x1017_1adc, 0x1017_1ad8, 0x1017_1ae0, 0x1017_67c8, 0x1017_67cc, 0x1017_67d0];

/// rpm to rad/s (`0x10148a5c`).
const RPM: f32 = 0.104_719_76;

/// `0x1004ab40` on the engine (`car+0x690`: `+4` its definition, `+8` its state).
fn engine(mem: &mut Mem, data: Data, c: Car, tables: &Tables) {
    let (car, opt) = (c.car, c.options);
    let def = mem.u32(car + 0x694);
    let state = mem.u32(car + 0x698);
    let d = |mem: &mut Mem, at, v| data.set(mem, at, v);
    for &(at, value) in &tables.engine_defaults {
        d(mem, at, value);
    }
    let kind = mem.u32(def + 0x26f8);
    mem.set_u32(data.at(0x1017_67d4), kind);
    let max_torque = mem.u32(opt + 0x13d0) as f32;
    d(mem, 0x1017_1aa0, max_torque);
    d(mem, 0x1017_234c, percent(mem, opt + 0x13e0));
    mem.set_u32(def + 0x26f8, kind & 1);
    let air = atmosphere(mem.f32(car + 0x148), &tables.atmosphere);
    let air = [air.temperature, air.pressure, air.density, air.inv_pressure, air.pressure_115, air.pressure_0987];
    for (at, v) in ATMOSPHERE.into_iter().zip(air) {
        d(mem, at, v);
    }
    if (kind & 2) == 0 {
        d(mem, 0x1017_1a98, 400.0);
        d(mem, 0x1017_1ae4, 200.0);
        d(mem, 0x1017_2338, 275.0);
    } else {
        let rpm = mem.u32(opt + 0x13c0) as f32;
        let limit = rpm * RPM;
        d(mem, 0x1017_1a98, limit);
        d(mem, 0x1017_1ae4, rpm * 0.85 * RPM);
        d(mem, 0x1017_1a9c, limit * 0.5);
        d(mem, 0x1017_2338, rpm * 0.95 * RPM);
        // The torque curve's points (`Tq_xx_RPM_NGP`, rpm to rad/s), and the idle table of the first and those with a
        // positive value (`Tq_xx_NGP`; `0x10003a40` skips a speed within 1e-9 of the last).
        let mut knots: Vec<(f64, f64)> = Vec::new();
        for k in 0..16 {
            let at = def + 0x2250 + 0x20 * k;
            let x = mem.f32(at) * RPM;
            mem.set_f32(at, x);
            let (x, y) = (f64::from(x), f64::from(mem.f32(at + 0x10)));
            if (k == 0 || y > 0.0) && knots.last().is_none_or(|&(last, _)| (x - last).abs() >= 1e-9) {
                knots.push((x, y));
            }
        }
        idle_table(mem, data, c.idle_table, &knots);
        let fac = mem.f32(def + 0x26d8) * mem.f32(def + 0x2650) / f32::from_bits(0x3574_46ba); // 9.1e-7
        let fixed = [0x21fc, 0x2200, 0x2204, 0x2214, 0x2218, 0x221c, 0x222c, 0x2230, 0x2234];
        for i in 0..256 {
            for j in 0..6 {
                let at = def + 0x1fc + 0x20 * i + 4 * j;
                mem.set_f32(at, mem.f32(at) * fac);
            }
            for at in fixed {
                mem.set_f32(def + at, fac * mem.f32(def + at));
            }
        }
        for (factor, count, list) in [(0x28, 0x9c, 0xa0), (0x38, 0xa4, 0xa8)] {
            let factor = mem.f32(def + factor);
            if factor <= 0.0 {
                continue;
            }
            for i in 0..mem.u32(def + count) {
                let row = mem.u32(def + list) + 0xc * i;
                for j in 0..mem.u32(row + 4) {
                    let at = mem.u32(row + 8) + 0xc * j + 4;
                    mem.set_f32(at, factor * mem.f32(at));
                }
            }
        }
        for (k, word) in [
            0x38d1_b717u32,
            0x3ba3_d70a,
            0x3c75_c28f,
            0x3d4c_cccd,
            0x3dcc_cccd,
            0x3e4c_cccd,
            0x3eb3_3333,
            0x3f0c_cccd,
            0x3f33_3333,
            0x3f4c_cccd,
            0x3f60_0000,
            0x3f6c_cccd,
            0x3f73_3333,
            0x3f78_51ec,
            0x3f7c_28f6,
            0x3f7e_b852,
            0x3f80_0000,
        ]
        .into_iter()
        .enumerate()
        {
            mem.set_u32(def + 0x2654 + 4 * k as u32, word);
        }
        let acc = max_torque * 1.3;
        d(mem, 0x1017_1acc, acc);
        d(mem, 0x1017_1aa4, max_torque * 3.0);
        d(mem, 0x1017_1ad0, acc * 10.0);
        let vcu_def = mem.u32(car + 0x850);
        let fac = mem.f32(vcu_def + 0x188) / 8000.0;
        d(mem, 0x1017_1aa8, fac * data.f32(mem, 0x1017_1aa8));
        d(mem, 0x1017_1aac, fac * data.f32(mem, 0x1017_1aac));
        d(mem, 0x1017_1ab0, data.f32(mem, 0x1017_1ab0) / fac);
        let rev = limit * 1.2;
        mem.set_f32(car + 0xfec, rev);
        mem.set_f32(car + 0xfe8, rev);
        let misfire = limit * 0.75;
        d(mem, 0x1017_1ab8, misfire);
        d(mem, 0x1017_1ab4, 25.0);
        d(mem, 0x1017_1ad4, misfire / RPM);
        d(mem, 0x1017_1abc, misfire * 0.5);
        mem.set_u32(state + 0x24, mem.u32(def + 0x2764));
        mem.set_u32(state + 0x1c, mem.u32(def + 0x279c));
        mem.set_u32(state + 0x23c, 0x43b0_9333);
        if mem.f32(def + 0x287c) > 0.0 {
            mem.set_u32(def + 0x271c, mem.u32(def + 0x287c));
        }
        d(mem, 0x1017_1ac0, mem.f32(def + 0x26a8) * 26_433_122.0);
        if mem.f32(def + 0x78) > 0.0 {
            d(mem, 0x1017_1ac4, mem.f32(def + 0x78));
        }
        if mem.f32(def + 0x88) > 0.0 {
            d(mem, 0x1017_1ac8, mem.f32(def + 0x88));
        }
        let pressure = data.f32(mem, 0x1017_1ad8) * 1.088_845;
        for at in [0x60, 0x70] {
            mem.set_f32(state + at + 8, pressure);
            gas_density(mem, state + at);
        }
    }
    let slope = -1.0 / (data.f32(mem, 0x1017_2338) - data.f32(mem, 0x1017_1ae4));
    d(mem, 0x1017_67c4, slope);
}

/// `0x100b7fc0` with 287 (the gas constant): `[0]` = `[3] * [2] / ([1] * 287)`, 0 without `[1]`.
fn gas_density(mem: &mut Mem, at: u32) {
    let t = mem.f32(at + 4);
    let v = if t != 0.0 { mem.f32(at + 0xc) * mem.f32(at + 8) / (t * 287.0) } else { 0.0 };
    mem.set_f32(at, v);
}

/// `0x1005d050` on the VCU (`car+0x850`: its definition; `+0x58` the drive).
fn vcu(mem: &mut Mem, data: Data, c: Car) {
    let (car, opt) = (c.car, c.options);
    let def = mem.u32(car + 0x850);
    let drive_def = mem.u32(mem.u32(car + 0x8a8) + 4);
    let launch = mem.f32(def + 0x188) / 7800.0;
    mem.set_u32(def + 0x198, mem.u32(c.setup + 0x6f8));
    mem.set_u32(def + 0x1a8, mem.u32(c.setup + 0x708));
    data.set(mem, 0x1017_2db4, launch * 5000.0);
    let cooldown = mem.f32(drive_def + 0xa64);
    data.set(mem, 0x1017_2db0, if cooldown <= 0.0 { 0.05 } else { cooldown });
    let flags = mem.u32(drive_def + 0x640);
    let kind = drive_type(flags);
    let types = if flags as i32 > 3 { diff_types(flags) } else { [1; 3] };
    let (min, max) = (mem.f32(opt + 0x12f4), mem.f32(opt + 0x1304));
    let rows = |mem: &mut Mem, at: &[u32], step: u32, min: f32, max: f32, i: u32| {
        for &a in at {
            clamp(mem, def + a + step * i, min, max);
        }
    };
    if kind == 1 {
        for i in 0..11 {
            rows(mem, &[0x1b8, 0x478, 0x268, 0x528], 0x10, min, max, i);
            rows(mem, &[0x328, 0x5e8], 0x20, min, max, i);
        }
    }
    if kind == 1 || kind == 3 {
        for i in 0..11 {
            rows(mem, &[0x738, 0x7e8], 0x10, min, max, i);
            rows(mem, &[0x8a8], 0x20, min, max, i);
        }
    }
    if kind == 1 {
        for i in 0..11 {
            rows(mem, &[0x9f8, 0xaa8], 0x10, min, max, i);
            rows(mem, &[0xb68], 0x20, min, max, i);
        }
    }
    // NGP's drivetrains: the bounds from the options' percentages, closed (0) past the first row for a diff that
    // keeps no lock (type not 1), and for an open diff (2) and the left-foot rows of every diff but type 1.
    let diff = |mem: &mut Mem, i: u32, kind: u32, bounds: u32| {
        let (min, max) = (percent(mem, opt + bounds), percent(mem, opt + bounds + 0x10));
        let (main, rest) = if kind == 1 {
            ((min, max), (min, max))
        } else if i != 0 || kind == 2 {
            ((0.0, 0.0), (0.0, 0.0))
        } else {
            ((min, max), (0.0, 0.0))
        };
        (main, rest)
    };
    if kind == 6 || kind == 7 {
        for i in 0..11 {
            let ((min, max), (lmin, lmax)) = diff(mem, i, types[0], 0xdd8);
            rows(mem, &[0x1b8, 0x268], 0x10, min, max, i);
            rows(mem, &[0x478, 0x528], 0x10, lmin, lmax, i);
            rows(mem, &[0x328, 0x5e8], 0x20, lmin, lmax, i);
        }
    }
    if kind == 4 || kind == 6 || kind == 7 {
        for i in 0..11 {
            let ((min, max), (lmin, lmax)) = diff(mem, i, types[1], 0xdf8);
            rows(mem, &[0x738, 0x7e8], 0x10, min, max, i);
            rows(mem, &[0x8a8], 0x20, lmin, lmax, i);
        }
    }
    if kind == 5 || kind == 6 || kind == 7 {
        for i in 0..11 {
            let ((min, max), (lmin, lmax)) = diff(mem, i, types[2], 0xe18);
            rows(mem, &[0x9f8, 0xaa8], 0x10, min, max, i);
            rows(mem, &[0xb68], 0x20, lmin, lmax, i);
        }
    }
    clamp_to(mem, def + 0x198, opt, 0x1354);
    clamp_to(mem, def + 0x1a8, opt, 0x1384);
}

/// `0x10045230` on the drive (`car+0x4b0`: `+4` its definition, `+8` its state).
fn drive(mem: &mut Mem, data: Data, c: Car) {
    let (car, opt, setup) = (c.car, c.options, c.setup);
    let def = mem.u32(car + 0x4b4);
    let state = mem.u32(car + 0x4b8);
    mem.set_u32(car + 0x4b0 + 0x1dc, data.at(0x1017_6628));
    let flags = mem.u32(def + 0x640) | mem.u32(opt + 0xe38);
    mem.set_u32(def + 0x640, flags);
    let mut split = 0.5;
    if drive_type(flags) == 7 {
        split = percent(mem, opt + 0xe48);
        if split < 0.0 {
            split = 0.5;
        } else if split > 1.0 {
            split = 1.0;
        }
    }
    data.set(mem, 0x1017_1a90, split);
    clamp_to(mem, def + 0x58, opt, 0xf58);
    clamp_to(mem, def + 0xb8, opt, 0xf88);
    clamp_to(mem, def + 0x118, opt, 0xfb8);
    let front = mem.f32(setup + 0x5b0);
    let front_min_max = (mem.f32(opt + 0x1138), mem.f32(opt + 0x1148));
    let (front, front_kept) = match clamped(front, front_min_max.0, front_min_max.1) {
        Some(v) => (v, false),
        None => (front, true),
    };
    let rear = mem.f32(setup + 0x660);
    let (rear, rear_kept) = match clamped(rear, mem.f32(opt + 0x12b8), mem.f32(opt + 0x12c8)) {
        Some(v) => (v, false),
        None => (rear, true),
    };
    if !front_kept || !rear_kept {
        let sum = rear + front;
        let (f, r) = if sum <= 0.0 { (0.0, 0.0) } else { (front / sum, rear / sum) };
        mem.set_f32(def + 0x620, sum);
        mem.set_f32(def + 0x218, f);
        mem.set_f32(def + 0x308, f);
        mem.set_f32(def + 0x3f8, r);
        mem.set_f32(def + 0x4e8, r);
    }
    clamp(mem, setup + 0x5c0, 0.0, 1.0);
    let handbrake = mem.f32(setup + 0x5c0);
    data.set(mem, 0x1017_1a94, if handbrake <= 0.0 { 1.0 } else { handbrake });
    let x = (mem.f32(def + 0x674) + mem.f32(def + 0x664)) / 0.2;
    let (a, b) = if x > 0.0 { (0.0005 / x, 0.033_333_335 / x) } else { (0.0005, 0.033_333_335) };
    data.set(mem, 0x1017_1468, a);
    data.set(mem, 0x1017_146c, b);
    for (at, from, default) in [(0x1017_1470, 0xe58, 6.0), (0x1017_1474, 0xe68, 6.0), (0x1017_1478, 0xe78, 800.0)] {
        let v = mem.u32(opt + from);
        data.set(mem, at, if v != 0 { v as f32 } else { default });
    }
    let wear = mem.u32(def + 0x4f8).wrapping_add(mem.u32(def + 0x518).wrapping_mul(2)) / 3;
    for at in [0xc4, 0xd4, 0x18c, 0x19c, 0x254, 0x264, 0x31c, 0x32c] {
        mem.set_u32(state + at, wear);
    }
}

/// `0x1005e6b0` on wheel `k`: its definition's options checks and scaling, then its spring (`0x10059960`) and tyre
/// (`0x1005ab10`).
fn wheel_init(mem: &mut Mem, data: Data, c: Car, wheel: u32, k: u32, tables: &Tables) {
    let def = mem.u32(wheel + DEF);
    let opt = c.options + 800 * k;
    if !clamp_to(mem, def + 0x41c, opt, 0x1c9c) {
        mem.set_f32(def + 0x3fc, mem.f32(def + 0x40c) + mem.f32(def + 0x41c));
    }
    if !clamp_to(mem, def + 0x900, opt, 0x1c6c) {
        let angle = f64::from(mem.f32(def + 0x900));
        mem.set_f32(def + 0x3d0, angle.cos() as f32);
        mem.set_f32(def + 0x3d8, angle.sin() as f32);
    }
    clamp_to(mem, def + 0x23c, opt, 0x1c3c);
    let r = ratio(mem, c.car);
    let high = r * 60000.0;
    data.set(mem, 0x1017_2f5c, high);
    for at in [0x930, 0x940, 0x960, 0x970] {
        mem.set_f32(def + at, r * mem.f32(def + at));
    }
    let low = r * 28000.0;
    data.set(mem, 0x1017_2f58, low);
    data.set(mem, 0x1017_2f64, r * 800000.0);
    data.set(mem, 0x1017_2f60, 1.0 / (high - low));
    spring(mem, data, c, wheel, k);
    tyre(mem, data, c, wheel, k, tables);
}

/// `0x10059960` on wheel `k`'s spring (`wheel+0xec4`, its definition at `+4`).
fn spring(mem: &mut Mem, data: Data, c: Car, wheel: u32, k: u32) {
    let s = mem.u32(wheel + 0xec4 + 4);
    let opt = c.options + 0x18c * k;
    clamp_to(mem, s + 0x48, opt, 0x13fc);
    clamp_to(mem, s + 0x58, opt, 0x142c);
    clamp_to(mem, s + 0x78, opt, 0x145c);
    mem.set_f32(s + 0x68, mem.f32(s + 0x78) * 0.4);
    clamp_to(mem, s + 0x88, opt, 0x148c);
    clamp_to(mem, s + 0xb8, opt, 0x14bc);
    clamp_to(mem, s + 0xc8, opt, 0x14ec);
    clamp_to(mem, s + 0x108, opt, 0x151c);
    clamp_to(mem, s + 0xe8, opt, 0x154c);
    let wheel_def = mem.u32(wheel + DEF);
    let drive_def = mem.u32(mem.u32(wheel + 0x7f4) + 0x4b4);
    let setup = c.setup;
    let [bump, rebound, stop, stop_bump, stop_rebound] =
        if k < 2 { [0x510, 0x520, 0x550, 0x560, 0x570] } else { [0x530, 0x540, 0x580, 0x590, 0x5a0] }
            .map(|at| mem.f32(setup + at));
    mem.set_f32(s + 0xf8, 0.0);
    mem.set_f32(s + 0x118, 0.0);
    if mem.u32(drive_def + 0x640) & 0x10000 != 0 && bump > 0.0 && rebound > 0.0 {
        mem.set_f32(s + 0xf8, bump);
        mem.set_f32(s + 0x118, rebound);
        clamp_to(mem, s + 0xf8, opt, 0x154c);
        clamp_to(mem, s + 0x118, opt, 0x151c);
    }
    let o = c.options;
    let bounds = [0x10a8, 0x10b8, 0x10c8, 0x10d8, 0x10e8, 0x10f8].map(|at| mem.f32(o + at));
    if bounds.iter().all(|&b| b > 0.0) && stop > 0.0 && stop_bump > 0.0 && stop_rebound > 0.0 {
        let stop = clamped(stop, bounds[0], bounds[1]).unwrap_or(stop);
        let stop_bump = clamped(stop_bump, bounds[3], bounds[4]).unwrap_or(stop_bump);
        let stop_rebound = clamped(stop_rebound, bounds[3], bounds[4]).unwrap_or(stop_rebound);
        mem.set_f32(s + 0x98, stop);
        mem.set_f32(s + 0xd8, stop_bump);
        if stop_bump > 0.0 {
            mem.set_f32(wheel_def + 0x1fc, stop_rebound / stop_bump);
        }
    }
    let r = ratio(mem, c.car);
    data.set(mem, 0x1017_2ca8, r * 27000.0);
    data.set(mem, 0x1017_2cac, 5.555_555_6e-5 / r);
}

/// `0x1005ab10` on wheel `k`'s tyre (`wheel+0xf00`: `+0x10` its definition, `+0x14` the wheel's `+0x6b0`), with the
/// tyre's loaded copy (`TYRE_COPIES`).
fn tyre(mem: &mut Mem, data: Data, c: Car, wheel: u32, k: u32, tables: &Tables) {
    let t = mem.u32(wheel + 0xf00 + 0x10);
    let w = mem.u32(wheel + 0xf00 + 0x14);
    clamp_to(mem, w + 0x68, c.options + 0x3c * k, 0x1a2c);
    let r = ratio(mem, c.car);
    let copy = |mem: &Mem, at: u32| data.f32(mem, TYRE_COPIES + TYRE_COPY_SIZE * k + at);
    let scale = |mem: &mut Mem, at: u32, by: f32| mem.set_f32(at, by * mem.f32(at));
    let (hub, tread) = (copy(mem, 0x7c), copy(mem, 0x8c));
    if hub > 0.0 && tread > 0.0 {
        mem.set_f32(w + 0x4c, tread + hub);
        mem.set_f32(t + 0x7c, hub);
        mem.set_f32(t + 0x8c, tread);
    }
    for at in (0x1e4..=0x240).step_by(4) {
        scale(mem, t + at, r);
    }
    for at in [t + 0x4c, t + 0x5c, w + 0x50, w + 0x54] {
        scale(mem, at, r);
    }
    let fc = mem.f32(t + 0xfc);
    if fc > 0.0 {
        for at in [0x134, 0x138, 0x13c, 0x140] {
            scale(mem, t + at, fc);
        }
    }
    let g = mem.f32(t + 0x3c);
    if g <= 0.0 {
        mem.set_f32(t + 0x3c, 1.0);
    } else {
        for at in (0x204..=0x240).step_by(4) {
            scale(mem, t + at, g * g);
        }
    }
    for (from, to) in [(0x2c, 0x2c), (0x4c, 0x4c), (0x5c, 0x5c)] {
        if copy(mem, from) > 0.0 {
            mem.set_f32(t + to, copy(mem, from));
        }
    }
    let radius = copy(mem, 0xdc);
    if radius > 0.0 {
        let old = mem.f32(w + 0x50);
        let f = if old != 0.0 { radius / old } else { 0.0 };
        mem.set_f32(w + 0x50, radius);
        if f != 0.0 {
            scale(mem, w + 0x54, f);
        }
    }
    if copy(mem, 0x6c) != 0.0 {
        mem.set_f32(t + 0x6c, copy(mem, 0x6c));
    }
    if (copy(mem, 0x9c) - 0.005).abs() > EPSILON {
        mem.set_f32(t + 0x9c, copy(mem, 0x9c));
    }
    for (from, to) in [(0xac, 0xac), (0xbc, 0xbc), (0xcc, 0xcc)] {
        if copy(mem, from) > 0.0 {
            mem.set_f32(t + to, copy(mem, from));
            if from == 0xbc {
                mem.set_f32(w + 0x58, copy(mem, from));
            }
        }
    }
    if copy(mem, 0xec) > 0.02 {
        mem.set_f32(t + 0xec, copy(mem, 0xec));
    }

    let pi = std::f32::consts::PI;
    let f = |at| mem.f32(t + at);
    let inner = f(0x7c);
    let outer = inner + f(0x8c);
    let wall = f(0x9c) + f(0x9c);
    let width = f(0xac) * pi;
    let core = inner - wall;
    let tread = outer - f(0xec);
    let mean = (f(0xac) + f(0xcc)) * 0.5;
    let core = core * core;
    let area = (core - tread * tread) * pi;
    // NGP's SSE order (0x1005b06a): `width * f(0xbc)` is formed first, then the ring area times it, then 950. The
    // left-to-right product differs by 1 ulp on car slot 4 (ngp-gravel-206-23, wheel2+0x760 at step 800).
    let mass = (outer * outer - inner * inner) * (width * f(0xbc)) * 950.0;
    mem.set_f32(t + 0x11c, mass * 0.95);
    mem.set_f32(t + 0x110, mass * 0.05 * 0.125);
    let heat = (f32::from_bits(0x3d87_79a6) / (mean * outer)) * r; // 0.06615
    let carcass = ((inner * inner - core) * width + wall * area) * 1645.0;
    mem.set_f32(t + 0x128, carcass);
    for at in [0x154, 0x158, 0x15c, 0x160, 0x168, 0x164] {
        scale(mem, t + at, heat);
    }
    let kind = mem.u32(t + 0xc);
    let temps = tables.temperatures[usize::from(kind > 2)];
    let (offset, ambient) =
        (tables.temperature_offsets[kind.min(6) as usize], tables.tyre_ambient[kind.min(6) as usize]);
    for (i, at) in [0x184, 0x188, 0x18c, 0x190, 0x194, 0x198, 0x19c, 0x1a0].into_iter().enumerate() {
        mem.set_f32(t + at, temps[i] + offset);
    }
    mem.set_f32(w + 0x64, ambient);
    mem.set_f32(w + 0x6c, area * mean);
    gas_density(mem, w + 0x60);
    for at in [0x12c, 0x130, 0xd0, 0xdc, 0xe8, 0xf4, 0x100, 0x10c, 0x118, 0x124] {
        mem.set_f32(w + at, ambient);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_keeps_values_within_an_epsilon_of_a_bound() {
        assert_eq!(clamped(0.5, 0.0, 1.0), None);
        assert_eq!(clamped(1.0 + 5e-8, 0.0, 1.0), None);
        assert_eq!(clamped(2.0, 0.0, 1.0), Some(1.0));
        assert_eq!(clamped(-1.0, 0.0, 1.0), Some(0.0));
        assert_eq!(clamped(3.0, 0.0, 0.0), Some(0.0));
        assert_eq!(clamped(f32::NAN, 0.0, 1.0), Some(1.0));
    }
}
