//! The engine sim steps (`E43EngineSim`, `CEngine::Sim80D0`, the dampers and the turbo energy step).

use super::drive_objects::Lane;
use super::engine_calls::{
    abs_rpm_term, clamp_asm, compliance, exhaust_blend_at, merge_into, rev_gate, sim_ebb0, slot_roll, torque_curve,
};
use super::engine_objects::{EngineObjects, EngineOptions, EngineState, EngineThis, EngineTuning, LoadDef};

use crate::engine_driveline::store::EngineStore;
use crate::engine_driveline::{engine, sim};
use crate::math::le_or_unordered;
use crate::ntweak::set_dirty;
use crate::physics_core::layout::{PHYSICS_GLOBAL, SIGNAL_MANAGER};
use crate::spring_damper::{self, BlendTerms, UpdateTerms, WeightedValue};
use crate::{fpu, ngp, Variant};

/// `Sim8E20`'s cycle factor (0x73b6f4): 20.371832.
const SIM8E20_F10: f32 = 20.371_832;

/// `E43EngineSim::Sim8D90` (0x518d90, thiscall `(x, y)`, x87 return). The table entry `x` (`state+0x88+0x38 x`: the
/// key, then the value) takes the key `y` plus the definition's lane word for `x` (`+0x108+0x40 x`). When that key
/// is below the entry's old key, the entry is reset by the host first, and the change is taken from the reset value.
/// The host writes the entry, so the old value is read again after the call.
pub fn sim8d90<S: EngineStore>(
    host: &mut impl engine::E43Host,
    mem: &mut S,
    this: u32,
    x: u32,
    y: u32,
    variant: Variant,
) -> f64 {
    let objects = EngineObjects::at(mem, this);
    let at = objects.state + 0x88 + 0x38 * x;
    let key = engine::entry_key(mem.u32(objects.def + 0x108 + 0x40 * x), y);
    let mut extra = 0.0;
    if key < mem.u32(at) {
        let reset = exhaust_blend_at(mem, this, 0xff, at, variant);
        extra = fpu::round_x87(reset - f64::from(mem.f32(at + 4)), variant) as f32;
        host.intake_state_reset(this, x);
    }
    let blend = exhaust_blend_at(mem, this, key, at, variant);
    let old = mem.f32(at + 4);
    mem.set_u32(at, key);
    mem.set_f32(at + 4, blend as f32);
    engine::entry_change(blend, old, extra, variant)
}

/// `E43EngineSim::Sim8E20` (0x518e20, thiscall `(a, b)`, st0): one engine step. The throttle `b` is clamped to
/// ±1151.9 (a NaN stays) and moves the state's `+0x184` and `+0x188`. The crank step (`crank_tick`) gives the cycle count
/// and the delta. With no cycle, `b` is the sum of the per-entry terms; with a cycle, `b` is the `Sim8D90` resets over
/// the entries divided by the delta. Then the misfire roll, the sound (`Emit`), the torque sum, the two averages at
/// `+0x210` and `+0x220`, the graphs (when the options' bit 0x200 is set), and `Sim8890` and `Sim8B20` on every tenth and
/// twentieth cycle. Returns the sum `g9 + b` the asm leaves in st0 (the reference C++ says `b`).
pub fn engine_step<S: EngineStore>(
    host: &mut impl engine::EngineStepHost,
    mem: &mut S,
    this: u32,
    a: f32,
    b_in: f32,
    tuning: &EngineTuning,
) -> f32 {
    let mut b = if -1_151.917_36 > b_in {
        -1_151.917_36
    } else if b_in > 1_151.917_36 {
        1_151.917_36
    } else {
        b_in
    };
    let objects = EngineObjects::at(mem, this);
    let (d, s) = (objects.def, objects.state);
    set_dirty(mem.mem_mut(), s + 0x228);
    mem.set_u32(s + 0x234, mem.u32(s + 0x234).wrapping_add(1));
    mem.set_f32(s + 0x184, b);
    mem.set_f32(s + 0x188, mem.f32(s + 0x184) * 0.3 + mem.f32(s + 0x188) * 0.7);
    mem.set_u32(s + 0x30, mem.u32(this + 0x20));
    mem.set_u32(s + 0x2c, mem.u32(s + 0x188));
    if mem.u32(d + 0x26f8) != 0 {
        turbo_energy_step(mem, this, a, tuning);
    }
    let (cycles, delta) = crank_tick(mem, this, a * b, tuning.variant);
    mem.set_f32(this + 0x10, delta);
    let count = mem.u32(d + 0x2698);
    b = 0.0;
    if cycles < 1 {
        // The entries of the definition (stride 0x40) and of the state (stride 0x38), each the blend of its two rows.
        let mut f13 = 0.0;
        let (mut offset, mut at) = (0, 0);
        for _ in 0..count {
            let mut u6 = mem.u32(d + 0x108 + offset).wrapping_add(mem.u32(s + 0x17c)) & 0xff;
            mem.set_u32(s + 0x88 + at, u6);
            if u6 == 0 {
                u6 = 1;
            }
            let row = d + u6 * 0x20;
            let zero = mem.f32(s + at + 0x90) == 0.0;
            let f11 = if zero {
                mem.f32(row + 0x204) * mem.f32(s + at + 0x9c)
                    + mem.f32(row + 0x200) * mem.f32(s + at + 0x98)
                    + mem.f32(s + at + 0x94) * mem.f32(row + 0x1fc)
            } else {
                mem.f32(row + 0x208)
            };
            let f12 = if zero {
                mem.f32(row + 0x1e4) * mem.f32(s + at + 0x9c)
                    + mem.f32(row + 0x1e0) * mem.f32(s + at + 0x98)
                    + mem.f32(s + at + 0x94) * mem.f32(row + 0x1dc)
            } else {
                mem.f32(row + 0x1e8)
            };
            let (k1, k2) = (mem.f32(s + at + 0xa4), mem.f32(s + at + 0xac));
            let (t14, t10, tc) = (mem.f32(s + at + 0xb0), mem.f32(s + at + 0xb4), mem.f32(s + at + 0xa8));
            f13 += (((mem.f32(row + 0x218) * k1 + f11) * k2
                + mem.f32(row + 0x210) * t14
                + mem.f32(row + 0x20c) * t10
                + mem.f32(row + 0x214) * tc)
                - ((mem.f32(row + 0x1f8) * k1 + f12) * k2
                    + mem.f32(row + 0x1f0) * t14
                    + mem.f32(row + 0x1ec) * t10
                    + mem.f32(row + 0x1f4) * tc))
                * SIM8E20_F10;
            b = f13;
            offset += 0x40;
            at += 0x38;
        }
    } else {
        let mut t18 = 0.0;
        for k in 0..count {
            let y = mem.u32(s + 0x17c);
            let g = sim8d90(host, mem, this, k, y, tuning.variant);
            t18 = fpu::round_x87(g + f64::from(t18), tuning.variant) as f32;
        }
        b = t18 / delta;
    }
    misfire_roll(mem, this, a);
    host.emit(s + 0x2c);
    let g9 = engine_torque(mem, this, a);
    let sum = g9 + b;
    let t8 = mem.f32(s + 0x184) * sum;
    let t4 = mem.f32(s + 0x210) * 0.95 + sum * 0.050_000_012;
    set_dirty(mem.mem_mut(), s + 0x204);
    mem.set_f32(s + 0x210, t4);
    let f10 = t8 * 0.050_000_012;
    let t4 = mem.f32(s + 0x220) * 0.95 + f10;
    set_dirty(mem.mem_mut(), s + 0x214);
    mem.set_f32(s + 0x220, t4);

    let g = mem.u32(PHYSICS_GLOBAL);
    if mem.u32(mem.u32(g + 0x24) + 0x160e0) & 0x200 != 0 {
        if mem.u32(g + 0x58) == 0 {
            let values = [mem.f32(s + 0x220) * 0.001_36, mem.f32(s + 0x210) * 0.737_463_2, mem.f32(s + 0x210)];
            for v in values {
                // The asm's three compares log an unordered value too, unlike the C++'s `(v < 0 || 2^31 <= v)` test.
                if v.is_nan() || v < -2_147_483_648.0 {
                    host.floor_log(v);
                }
            }
        }
        host.graph_push(mem.u32(s + 0x274), sum * f32::from_bits(0x3af4_fe91));
        host.graph_push(mem.u32(s + 0x278), mem.f32(s + 0x220) * 4.484_305e-6);
    }
    let cycle = mem.u32(s + 0x234);
    if cycle.is_multiple_of(10) {
        sim8890(host, mem, this, a * 10.0, tuning);
    }
    if cycle.is_multiple_of(20) {
        fuel_air_terms(mem, this, a * 20.0);
    }
    sum
}

/// The magnitude as `Sim80D0` takes it (`if (f <= 0) f = 0 - f`).
fn magnitude(v: f32) -> f32 {
    if v <= 0.0 {
        0.0 - v
    } else {
        v
    }
}

/// `CEngine::Sim80D0` (0x5180d0, thiscall `(i)`): the table entry `i` (`state+0x88+0x38 i`) is recomputed from the
/// damper, the torque curve and the rev gate, then slot `i` rolls (`slot_roll`). Returns that roll's eax. The asm takes
/// an unordered compare as true (`le_or_unordered`), which only matters for NaN.
pub fn intake_state_reset<S: EngineStore>(mem: &mut S, this: u32, i: u32, tuning: &EngineTuning) -> u32 {
    let objects = EngineObjects::at(mem, this);
    let (d, s, o) = (objects.def, objects.state, objects.options);
    let (air, variant) = (tuning.air, tuning.variant);
    let p = s + 0x88 + 0x38 * i;
    mem.set_u32(p, 0);
    mem.set_f32(p + 4, 0.0);
    sim_ebb0(mem, this);
    if mem.u32(d + 0x26f8) == 0 {
        Lane { dirty: 1, value: air.pressure }.store(mem, s + 0x1ac);
        Lane { dirty: 1, value: air.temperature }.store(mem, s + 0x19c);
        mem.set_f32(p + 0x28, air.pressure_115);
    } else {
        if mem.f32(p + 0x18) != 0.0 {
            let mut block = UpdateTerms {
                scale: mem.f32(p + 0x18),
                numerator: 0.0,
                denominator: mem.f32(p + 0x24) * 4.75,
                response: mem.f32(d + 0x2650),
            };
            spring_damper::update_c(&mut block, 287.0);
            // The original's four-word block is the same four words read as the blend terms.
            let source = BlendTerms {
                weight: block.scale,
                value: block.numerator,
                response: block.denominator,
                scale: block.response,
            };
            let mut target = EngineStore::load_blend_terms(mem, s + 0x70);
            spring_damper::combine(&mut target, source, 287.0, variant);
            EngineStore::store_blend_terms(mem, s + 0x70, target);
            mem.set_f32(s + 0x1dc, block.numerator);
        }
        mem.set_f32(p + 0x28, mem.f32(s + 0x78) * 1.15);
        Lane { dirty: 1, value: mem.f32(s + 0x68) }.store(mem, s + 0x1ac);
        Lane { dirty: 1, value: mem.f32(s + 0x64) }.store(mem, s + 0x19c);
    }
    mem.set_f32(p + 0x20, 0.0);
    mem.set_f32(p + 0x2c, air.pressure_0987);
    mem.set_f32(p + 0x1c, 0.0);

    let rate = mem.f32(s + 0x184);
    let f13 = sim::curve_eta(&mem.curve_knots(d + 0x2244, 0x10), rate, variant) as f32;
    let mut f9 = 1.0;
    if 0.01 < magnitude(rate) && 0.001 < f13 {
        let speed = magnitude(mem.f32(s + 0x188));
        // x87: the product and its root, stored.
        let root =
            fpu::round_x87(fpu::round_x87(f64::from(mem.f32(s + 0x1a8)) * 287.0, variant).sqrt(), variant) as f32;
        let x = ((mem.f32(d + 0x26b8) / speed) * mem.f32(d + 0x26d8) * mem.f32(this + 0x30) * root)
            / (mem.f32(d + 0x2650) * f13);
        f9 = sim::table_lerp(&mem.f32_table(d + 0x2654, 0x11), x, 0.0, 2.0, variant) as f32;
    }
    let fp9 = mem.f32(s + 0x1b8) * (f9 * f13);
    mem.set_f32(p + 0x24, fp9);
    Lane { dirty: 1, value: mem.f32(s + 0x1b8) * f9 }.store(mem, s + 0x1cc);

    let x87 = |v: f64| fpu::round_x87(v, variant);
    let f12 = x87(spring_damper::curve(f9 * f13, variant) * f64::from(mem.f32(d + 0x2650))) as f32;
    let f12 = if mem.u32(d + 0x26f8) == 0 {
        f12 * air.density
    } else {
        let pf = s + 0x60;
        let f12 = x87(x87(f64::from(f12) / f64::from(mem.f32(s + 0x6c))) * f64::from(mem.f32(pf))) as f32;
        spring_damper::layout::relax(mem, pf, f12, 287.0, variant);
        f12
    };
    mem.set_f32(p + 0x18, f12);
    let torque = if f12 == 0.0 {
        0.0
    } else {
        let q = (mem.f32(d + 0x2650) * fp9) / (f12 * 287.0);
        if q < 0.0 {
            0.0
        } else if 800.0 < q {
            800.0
        } else {
            q
        }
    };
    torque_curve(mem, this, torque, p);
    mem.set_f32(p + 0x1c, 0.0);
    mem.set_f32(s + 0x1e0, torque);

    let m = rev_gate(mem, this, tuning) as f32;
    let f10 = mem.f32(p + 0x18) * 0.1;
    let mut f11 = f10 * m;
    if 2.0 <= m {
        f11 += 0.0005;
    }
    let rng_at = mem.u32(objects.options);
    let seed = mem.u32(rng_at);
    let u6 = seed.wrapping_mul(0x0200_0081).wrapping_add(0xe623_f41d);
    mem.set_u32(rng_at, u6);

    let f8 = (fp9 - 500.0) * 0.000_232_558_14;
    let f9 = if f8 < 0.0 {
        0.0
    } else if 1.0 < f8 {
        1.0
    } else {
        f8
    };
    let f8 = mem.f32(o + 0x10 * i + 8) * 1.428_571_5;
    // The asm compares the draw here: it reads the slot the draw went to (`[esp+0x14]`), since the `pop ebp` before the
    // compare moves the frame by 4 bytes. It takes the `set` branch when a compare is unordered (NaN), as
    // `le_or_unordered` does.
    let draw = (u6 % 100_001) as f32 * 0.000_01;
    let set = mem.u32(s + 0x168 + 4 * i) == 0
        || le_or_unordered(draw, (1.0 - f9 * 0.995) + f8 * f8)
        || le_or_unordered(mem.f32(s + 0x188), tuning.misfire_speed)
        || mem.f32(o + 0x10 * i + 0x10) == 0.0
        || le_or_unordered(m, 0.5)
        || mem.u32(o + 0x74) == 0;
    if set {
        mem.set_f32(p + 0x8, 1.0);
        mem.set_f32(p + 0xc, 0.0);
        mem.set_f32(p + 0x10, 0.0);
        mem.set_f32(p + 0x14, 0.0);
        if i < 4 {
            mem.set_u32(s + 0x4c + 4 * i, 0);
        }
    } else {
        sim::unk_d6f0(0.95).store(mem, p);
        if i < 4 {
            mem.set_u32(s + 0x4c + 4 * i, 1);
        }
        mem.set_f32(s + 0x238, f10 * tuning.misfire_heat + mem.f32(s + 0x238));
        if le_or_unordered(m, 1.0) {
            return slot_roll(mem, this, i);
        }
        f11 -= f10;
    }
    mem.set_f32(s + 0x10, f11 + mem.f32(s + 0x10));
    slot_roll(mem, this, i)
}

/// `CEngine::SimE060` (0x50e060, thiscall): the smoothed damper step (`engine::smooth_damper`) on the blend table at
/// `table` and the block at `block_at` (name ours). `abc` is the key, the ratio and the scale; `dt` is the step. Returns the `g`.
pub fn smooth_damper<S: EngineStore>(
    mem: &mut S,
    this: u32,
    abc: [f32; 3],
    table: u32,
    dt: f32,
    block_at: u32,
    variant: Variant,
) -> f32 {
    let objects = EngineObjects::at(mem, this);
    let table = mem.blend_table(table);
    let mut block = EngineStore::load_blend_terms(mem, block_at);
    let mut state = EngineState::load(mem, objects.state);
    let g = engine::smooth_damper(&table, abc[0], abc[1], abc[2], dt, &mut block, &mut state.smoothed, variant);
    EngineStore::store_blend_terms(mem, block_at, block);
    state.store(mem, objects.state);
    g
}

/// `CEngine::SimE1F0` (0x50e1f0, thiscall): the solved damper step (`engine::solve_damper`) on the table at `table`
/// and the relaxation block at `block_at` (name ours). `ab` is the key and the ratio; `dt` is the step. Returns the output. NGP
/// retargets its `DamperSolve` (0x50e2f8) to the wrapper that solves against the air pressure.
pub fn solve_damper<S: EngineStore>(
    mem: &mut S,
    this: u32,
    ab: [f32; 2],
    table: u32,
    dt: f32,
    block_at: u32,
    tuning: &EngineTuning,
) -> f32 {
    let objects = EngineObjects::at(mem, this);
    let table = mem.blend_table(table);
    let mut block = EngineStore::load_relaxation(mem, block_at);
    let mut state = EngineState::load(mem, objects.state);
    let v = engine::solve_damper(
        &table,
        ab[0],
        ab[1],
        dt,
        &mut block,
        &mut state.solved,
        tuning.air.pressure,
        tuning.variant,
    );
    EngineStore::store_relaxation(mem, block_at, block);
    state.store(mem, objects.state);
    v
}

/// `E43EngineSim::Sim7D30` (0x517d30, thiscall `(x)`): the damper and load stage. Two `DamperSolve`s, the `SimE060`
/// and `SimE1F0` steps (their `g` outputs feed the energy term), then the rate `+0x5c` is the square root of the
/// energy over the definition's inertia (x87), clamped to 0..20943.951, or 0 when the energy is not positive. Returns eax:
/// the definition's pointer when the energy is positive, the state's otherwise. The C++ and asm agree; NaN inputs
/// take the other branch here where the original's unordered compares do (not reproduced).
pub fn turbo_energy_step<S: EngineStore>(mem: &mut S, this: u32, x: f32, tuning: &EngineTuning) -> u32 {
    let (air, variant) = (tuning.air, tuning.variant);
    let pressure = air.pressure;
    let objects = EngineObjects::at(mem, this);
    let (d, s, o) = (objects.def, objects.state, objects.options);
    let mut t4 = mem.f32(d + 0x18) * mem.f32(s + 0x5c) * mem.f32(s + 0x5c) * 0.5;
    let f5 = mem.f32(d + 0x48);
    let f4 = mem.f32(s + 0x68) - mem.f32(s + 0x1d8);
    let (lc, t8) = if f4 <= f5 || mem.f32(s + 0x68) <= pressure {
        (0.0, 0.0)
    } else {
        let lc = if f4 <= mem.f32(d + 0x58) {
            ((f4 - f5) * mem.f32(d + 0x68)) / (mem.f32(d + 0x58) - f5)
        } else {
            mem.f32(d + 0x68)
        };
        (lc, (mem.f32(s + 0x68) - pressure) * lc * 0.05)
    };
    let gate = mem.f32(o + 0x64);
    let f5 = if gate == 0.0 || mem.f32(s + 0x68) <= pressure {
        0.0
    } else {
        mem.f32(d + 0x2718) * gate * (mem.f32(s + 0x68) - pressure) * 0.05
    };
    // NGP's `DamperSolve` wrapper (0x10060230) solves against the air pressure instead of the pushed 101300.
    spring_damper::layout::solve(mem, s + 0x60, (t8 + f5) * x, pressure, 287.0, variant);
    mem.set_f32(s + 0x38, mem.f32(s + 0x68) * air.inv_pressure);
    mem.set_u32(s + 0x34, mem.u32(s + 0x5c));
    mem.set_f32(s + 0x44, t8);
    mem.set_f32(s + 0x48, lc);

    let eta = sim::curve_eta(&mem.curve_knots(d + 0x2444, 0x10), mem.f32(s + 0x184), variant);
    let f4 = eta as f32 - tuning.band[0];
    let lc_top = fpu::round_x87(eta + f64::from(tuning.band[1]), variant) as f32;
    // NGP's code patch at 0x517ec6 reads the band's level from `+0x78` instead of `+0x68`.
    let level = match variant {
        Variant::Stock102 => mem.f32(s + 0x68),
        Variant::Ngp778 => mem.f32(s + 0x78),
    };
    let f5 = if level <= f4 || mem.f32(s + 0x78) <= pressure {
        0.0
    } else {
        let ratio =
            if level <= lc_top { ((level - f4) * mem.f32(d + 0x98)) / (lc_top - f4) } else { mem.f32(d + 0x98) };
        (mem.f32(s + 0x78) - pressure) * ratio * 0.05
    };
    spring_damper::layout::solve(mem, s + 0x70, f5 * x, pressure, 287.0, variant);

    let rate = mem.f32(s + 0x5c);
    let lc = smooth_damper(
        mem,
        this,
        [rate, mem.f32(s + 0x68) * air.inv_pressure, 293.140_01],
        d + 0x9c,
        x,
        s + 0x60,
        variant,
    );
    let t8 = solve_damper(mem, this, [rate, mem.f32(s + 0x78) * air.inv_pressure], d + 0xa4, x, s + 0x70, tuning);
    let rate = mem.f32(s + 0x5c);
    t4 += (t8 + lc) - (mem.f32(d + 0xc8) * rate * rate + mem.f32(d + 0xb8) * rate) * x;
    if 0.0 < t4 {
        // x87: the quotient and the root round to the precision control (53 bits under NGP).
        let half = fpu::round_x87(f64::from(t4) / f64::from(mem.f32(d + 0x18)), variant);
        let root = fpu::round_x87((half + half).sqrt(), variant) as f32;
        let root = if root < 0.0 {
            0.0
        } else if 20_943.951 < root {
            20_943.951
        } else {
            root
        };
        mem.set_f32(s + 0x5c, root);
        return d;
    }
    mem.set_u32(s + 0x5c, 0);
    s
}

/// `E43EngineSim::Sim8610` (0x518610, thiscall `(x)`, ret 4): the first damper step of the brake-side term. It moves the two
/// slots of the state (`+0x1c`, `+0x24`) by at most the rate and the limits, merges the two weighted values into them
/// (`DamperMerge` twice), and adds the step to `this+0x58`. Its lerp is `InitRealLerp` (0x50f010): `(b - a) * c + a`
/// with `a = def+0x27ac`, `b = def+0x27bc` and `c` the options' `1 - v54` rounded to f32 (the x87 store). Returns the
/// `eax` the original leaves: the options block when the gate is off, else the state.
pub fn sim8610<S: EngineStore>(mem: &mut S, this: u32, x: f32) -> u32 {
    let objects = EngineObjects::at(mem, this);
    let (d, s, o) = (objects.def, objects.state, objects.options);
    if mem.u32(o + 0x48) == 0 {
        return o;
    }
    let s1c = mem.f32(s + 0x1c);
    let mut t = mem.f32(d + 0x2784) * x;
    if t > s1c {
        t = s1c;
    }
    let max = mem.f32(d + 0x2764);
    if t > max {
        t = max;
    }
    let (m1_value, m2_value) = (mem.f32(s + 0x20), mem.f32(s + 0x28));
    mem.set_f32(s + 0x1c, s1c - t);
    let f4 = t - (max - mem.f32(s + 0x24));
    // Unordered compares take the `0 <= f4` branch, so a NaN keeps `f4` (the asm's `jbe`).
    let mut f3 = if f4 < 0.0 {
        0.0
    } else if f4 > t {
        t
    } else {
        f4
    };
    mem.set_f32(s + 0x24, mem.f32(s + 0x24) - f3);
    let v54 = mem.f32(o + 0x54);
    let a = mem.f32(d + 0x27ac);
    let q = (mem.f32(d + 0x27bc) - a) * (1.0 - v54) + a;
    if v54 == 0.0 || !(mem.f32(s + 0x1c) > q) {
        mem.set_f32(s + 0x40, 0.0);
    } else {
        let v = clamp_asm(v54, 1.0);
        mem.set_f32(s + 0x40, (v / x) * f3);
        f3 *= 1.0 - v * 0.1;
    }
    merge_into(mem, WeightedValue { weight: t, value: m1_value }, s + 0x24);
    merge_into(mem, WeightedValue { weight: f3, value: m2_value }, s + 0x1c);
    mem.set_f32(s + 0x24, clamp_asm(mem.f32(s + 0x24), max));
    mem.set_f32(s + 0x1c, clamp_asm(mem.f32(s + 0x1c), mem.f32(d + 0x279c)));
    let total = mem.f32(this + 0x58) + t;
    set_dirty(mem.mem_mut(), this + 0x4c);
    mem.set_f32(this + 0x58, total);
    s
}

/// `CEngine::SimE370` (0x50e370, thiscall `(a)`): the crank step at time `a`. Returns the cycle count and the delta the
/// original stores through its two out pointers.
pub fn crank_tick<S: EngineStore>(mem: &mut S, this: u32, a: f32, variant: Variant) -> (i32, f32) {
    let objects = EngineObjects::at(mem, this);
    let mut state = EngineState::load(mem, objects.state);
    let out = engine::crank_tick(&mut state, a, variant);
    state.store(mem, objects.state);
    out
}

/// `CEngine::SimE520` (0x50e520, thiscall `(a)`): the misfire roll at `a` (`engine::misfire_roll`) (name ours). The LCG word is
/// the options block's, and a misfire sets the engine's level.
pub fn misfire_roll<S: EngineStore>(mem: &mut S, this: u32, a: f32) {
    let objects = EngineObjects::at(mem, this);
    let seed_at = mem.u32(objects.options);
    let mut seed = mem.u32(seed_at);
    let mut state = EngineState::load(mem, objects.state);
    let level = engine::misfire_roll(&mut state, &mut seed, a);
    mem.set_u32(seed_at, seed);
    state.store(mem, objects.state);
    if let Some(level) = level {
        let mut fields = EngineThis::load(mem, this);
        fields.misfire_level = level;
        fields.store(mem, this);
    }
}

/// `CEngine::SimE660` (0x50e660, thiscall `(a)`, st0): the torque sum at `a`.
pub fn engine_torque<S: EngineStore>(mem: &mut S, this: u32, a: f32) -> f32 {
    let objects = EngineObjects::at(mem, this);
    let def = LoadDef::load(mem, objects.def);
    let options = EngineOptions::load(mem, objects.options);
    let mut state = EngineState::load(mem, objects.state);
    let st0 = engine::torque_sum(&def, &options, &mut state, a);
    state.store(mem, objects.state);
    st0
}

/// `E43EngineSim::Sim8890` (0x518890, thiscall `(x)`, void): the car's damper step. The first spring-damper step moves
/// the state's `+0x24` and the definition's accumulator, and `DamperAccumA` takes the rest of `+0x238` into `+0x23c`.
/// The second step moves `+0x1c` from the car's walk: the object from slot 12 (`S30`) and the point `MB560` transforms
/// it to (the host calls). Then `Sim8610` runs when `+0x28` is above the definition's `+0x2774`, both slots are clamped,
/// and the engine's four outputs (`+0x198`, `+0x64`, `+0x68`, `+0x6c`) take the reciprocal of `x` times their terms.
/// Returns the state, the `eax` the original leaves.
pub fn sim8890<S: EngineStore>(
    host: &mut impl engine::Sim8890Host,
    mem: &mut S,
    this: u32,
    x: f32,
    tuning: &EngineTuning,
) -> u32 {
    let objects = EngineObjects::at(mem, this);
    let (d, s) = (objects.def, objects.state);
    let variant = tuning.variant;
    let x87 = |v: f64| fpu::round_x87(v, variant);
    let mut t30 = 0.0;
    let mut t2c = 0.0;
    if 0.001 < mem.f32(s + 0x24) {
        let g = spring_damper::step1(
            mem.f32(s + 0x23c),
            mem.f32(s + 0x28),
            mem.f32(d + 0x2734),
            mem.f32(d + 0x2744),
            variant,
        );
        t30 = x87(g * x as f64) as f32;
        spring_damper::layout::accumulate_b(mem, d + 0x2748, t30, s + 0x24);
    }
    spring_damper::layout::accumulate_a(mem, d + 0x271c, mem.f32(s + 0x238) - t30, s + 0x23c);
    if 0.001 < mem.f32(s + 0x1c) {
        let car = mem.u32(s + 0xc);
        let start = if car.wrapping_add(0x1b0) == 0 { 0 } else { car + 0x1c0 };
        let u = host.s30(car);
        let v = host.mb560(start, u);
        let f8 = engine::lateral_term([mem.f32(d + 0x2804), mem.f32(d + 0x2808), mem.f32(d + 0x2800)], v);
        let f9 = engine::travel_fraction(mem.f32(s + 0x1c), mem.f32(d + 0x27ac), mem.f32(d + 0x27bc));
        // NGP retargets this call (0x518a32) to its `DamperStep1` with the air temperature (0x1004d2f0).
        let (a, c, k) = (mem.f32(s + 0x20), mem.f32(d + 0x27ec) * f8 + mem.f32(d + 0x27dc), f9 * mem.f32(d + 0x27cc));
        let g = match variant {
            Variant::Stock102 => spring_damper::step1(a, 293.140_01, c, k, variant),
            Variant::Ngp778 => f64::from(ngp::damper::step1(a, 293.140_01, c, k, tuning.air.temperature)),
        };
        t2c = x87(g * x as f64) as f32;
        spring_damper::layout::accumulate_b(mem, d + 0x2748, -t2c, s + 0x1c);
    }
    if mem.f32(d + 0x2774) < mem.f32(s + 0x28) {
        sim8610(mem, this, x);
    }
    let g1 = spring_damper::layout::clamp_value(mem, d + 0x2748, s + 0x1c);
    let inv = 1.0 / x;
    let g2 = spring_damper::layout::clamp_value(mem, d + 0x2748, s + 0x24);
    let sum = x87(g2 as f64 + g1 as f64);
    mem.set_f32(this + 0x198, x87(sum * inv as f64) as f32);
    mem.set_f32(this + 0x68, inv * mem.f32(s + 0x238));
    mem.set_f32(this + 0x64, inv * t2c);
    mem.set_f32(this + 0x6c, inv * t30);
    mem.set_f32(s + 0x238, 0.0);
    s
}

/// `E43EngineSim::Sim8B20` (0x518b20, thiscall `(x)`, ret 4): the fuel and air terms from the abs-rpm term `Sim410`, and
/// the throttle at `this+0x78` (name ours). Returns the options block, the `eax` the original leaves.
pub fn fuel_air_terms<S: EngineStore>(mem: &mut S, this: u32, x: f32) -> u32 {
    let objects = EngineObjects::at(mem, this);
    let (d, s, o) = (objects.def, objects.state, objects.options);
    let f5 = abs_rpm_term(mem, this);
    let f12 = 1.0 - mem.f32(o + 0x60);
    let f10 = mem.f32(o + 0x5c) * f5;
    compliance(mem, this);
    let p = mem.f32(s + 0x26c) * f10;
    set_dirty(mem.mem_mut(), s + 0x250);
    mem.set_f32(s + 0x25c, p);
    mem.set_f32(s + 0xbc, p / mem.f32(s + 0xb8));
    let t1 = mem.f32(s + 0xbc) * mem.f32(o + 0xc) + (f5 - f12 * f10);
    let f2 = p / mem.f32(s + 0xf0);
    mem.set_f32(s + 0xf4, f2);
    let t2 = f2 * mem.f32(o + 0x1c) + t1;
    let f3 = p / mem.f32(s + 0x128);
    mem.set_f32(s + 0x12c, f3);
    let t3 = f3 * mem.f32(o + 0x2c) + t2;
    let f4 = p / mem.f32(s + 0x160);
    mem.set_f32(s + 0x164, f4);
    let mut f11 = f4 * mem.f32(o + 0x3c) + t3;
    if mem.u32(d + 0x26f8) != 0 {
        let f5b = p / mem.f32(s + 0x80);
        mem.set_f32(s + 0x84, f5b);
        f11 += f5b * mem.f32(o + 0x70);
    }
    if 0.0 < f11 {
        let v = mem.f32(o + 0x58) - (f11 * x) / mem.f32(d + 0x281c);
        mem.set_f32(o + 0x58, clamp_asm(v, 1.0));
    }
    mem.set_f32(this + 0x78, f5);
    o
}

/// `E43EngineSim::Sim8CF0` (0x518cf0, thiscall, no arguments): `+0x184` becomes 0.7 times the state's float at `+0x200`
/// plus 0.3 times itself, and `+0x190` the options block's `+8`, `+0x18`, `+0x28` and `+0x38` summed, halved and clamped
/// to [0, 1]. Then the emit (0x50ee90) runs on the global signal object (`[0x893600] + 0x3cb8`) with `+0x184` as its
/// argument, and `+0x194` is cleared. Returns the eax the emit leaves.
pub fn sim8cf0<S: EngineStore>(host: &mut impl engine::Sim8CF0Host, mem: &mut S, this: u32) -> u32 {
    let state = mem.u32(this + 8);
    let options = mem.u32(this + 0x48);
    let blend = mem.f32(state + 0x200) * 0.7 + mem.f32(this + 0x184) * 0.3;
    mem.set_f32(this + 0x184, blend);
    let sum = mem.f32(options + 8) + mem.f32(options + 0x18) + mem.f32(options + 0x28) + mem.f32(options + 0x38);
    mem.set_f32(this + 0x190, clamp_asm(sum * 0.5, 1.0));
    let eax = host.signal(mem.u32(SIGNAL_MANAGER) + 0x3cb8, this + 0x184);
    mem.set_f32(this + 0x194, 0.0);
    eax
}
