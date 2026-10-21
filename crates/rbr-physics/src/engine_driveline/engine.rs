//! The engine's kernels (`CEngine`, `RBR_E41_Engine.cpp`): the pedal input, the crank tick, the rev-limit gate, the
//! misfire roll, the series compliance, the exhaust blend, the stall guard and the torque curve. Each is checked
//! against the original's calls in `tests/engine_driveline_vectors.rs`.

use super::layout::{BlendKnot, EngineDef, EngineOptions, EngineState, Lane, LoadDef};
use super::sim::{blend_w, blend_x, blend_y};
use crate::fpu;
use crate::spring_damper::{self, BlendTerms, Relaxation, UpdateTerms};
use crate::Variant;

/// The gas constant the dampers use (287 J/(kg K), dry air; the `0x438f8000` word of the calls).
const AIR_GAS_CONSTANT: f32 = 287.0;
/// The pressure the dampers solve against (101300 Pa, one standard atmosphere; `0x47c5da00`).
pub const STANDARD_PRESSURE: f32 = 101_300.0;

/// `CEngine::SetScratch08` (0x50eb90): the pedal's value (its bits, as the caller passes them) and its dirty flag.
pub fn set_pedal(pedal: &mut Lane, bits: u32) {
    pedal.set_bits(bits);
}

/// `CEngine::SimE370` (0x50e370): the crank advances by `a` (rad), wrapped to `0..4 pi` (the asm wraps once, by one
/// turn). The crank count is `trunc(angle * 20.37...)`, 256 ticks per turn. Returns the signed tick delta, wrapped
/// to +-128 (the sign follows `a`), and the angle that delta covers (`delta * 4 pi / 256`).
pub fn crank_tick(state: &mut EngineState, a: f32, variant: Variant) -> (i32, f32) {
    let turn = f32::from_bits(0x41490fdb); // 4 pi (0x73b6f8)
    let scale = f32::from_bits(0x41a2f983); // 256 / 4 pi (0x73b6f4)
    let step = f32::from_bits(0x3d490fdb); // 4 pi / 256 (0x73b6f0)
    let previous = state.crank_count;
    state.crank_count_prev = previous;
    state.crank_angle += a;
    let x = state.crank_angle;
    if x > turn {
        state.crank_angle = x - turn;
    } else if 0.0 > x {
        state.crank_angle = x + turn;
    }
    // The x87 product (`_ftol` truncates st0): at 24-bit precision it rounds to f32; at 53 (NGP) it stays exact, so a
    // product just under a tick doesn't round up to it.
    let scaled = fpu::round_x87(f64::from(state.crank_angle) * f64::from(scale), variant);
    state.crank_count = fpu::trunc_i32(scaled) as u32;
    let mut b = 0;
    if a > 0.0 {
        let v = state.crank_count;
        b = if v < previous { (v as i32) - (previous as i32) + 0x100 } else { (v as i32) - (previous as i32) };
    } else if 0.0 > a {
        let v = state.crank_count;
        b = if v > previous { (v as i32) - (previous as i32) - 0x100 } else { (v as i32) - (previous as i32) };
    }
    (b, b as f32 * step)
}

/// `CEngine::SimE470` (0x50e470): the rev-limit gate, from the speed (rad/s) and the pedal. Zero below the floor
/// rpm (500) or with the gate off. Otherwise a factor from the rpm above the start (3500) and the pedal: `1 - 10 x`
/// below -0.4, `1 + 0.2 x` above 0.4, else 1, with `x = pedal - clamp((rpm - start) * scale)`. `band` is the floor,
/// the start and the scale (`layout::EngineTuning::rev_gate`). The two x87 factors are returned in st0.
pub fn rev_gate(speed: f32, pedal: f32, gate: u32, band: [f32; 3], variant: Variant) -> f64 {
    let [floor, start, scale] = band;
    let rpm = speed * 9.549_296_4;
    if rpm < floor || gate == 0 {
        return 0.0;
    }
    let mut f2 = (rpm - start) * scale;
    // Not `contains`: a NaN fraction stays NaN, as the asm's two compares leave it.
    #[allow(clippy::manual_range_contains)]
    if f2 < 0.0 || 1.0 < f2 {
        f2 = if f2 < 0.0 { 0.0 } else { 1.0 };
    }
    let f2 = pedal - f2;
    let x87 = |v: f64| fpu::round_x87(v, variant);
    if f2 < -0.4 {
        return x87(1.0 - x87(f64::from(f2) * 10.0));
    }
    if 0.4 < f2 {
        return x87(x87(f64::from(f2) * f64::from(0.2f32)) + 1.0);
    }
    1.0
}

/// `CEngine::SimE9F0` (0x50e9f0): the series compliance. Each of the four stages is interpolated between its low and
/// high ends by its fraction in the option block, and the combined value is `1 / (sum of 1 / stage)`. The fifth stage
/// joins the sum when the definition enables it. Sets the stages, the fifth (`extra_stage`) and the combined value
/// with its dirty flag.
pub fn compliance(def: &EngineDef, options: &EngineOptions, state: &mut EngineState) {
    // x87 order: (high - low) * t + low, per stage.
    let stage = |k: usize| (def.stage_high[k] - def.stage_low[k]) * options.stage_t[k] + def.stage_low[k];
    let s0 = stage(0);
    let s1 = stage(1);
    let s2 = stage(2);
    let s3 = stage(3);
    state.stage = [s0, s1, s2, s3];
    // The sum runs in the asm's order: 1/s0, then + 1/s1, + 1/s2, + 1/s3 (each step rounded to f32).
    let mut tot = 1.0 / s0;
    tot += 1.0 / s1;
    tot += 1.0 / s2;
    tot += 1.0 / s3;
    if def.enable != 0 {
        let extra = (def.extra_stage_high - def.extra_stage_low) * options.fifth_t + def.extra_stage_low;
        tot += 1.0 / extra;
        state.extra_stage = extra;
    }
    state.compliance_dirty = 1;
    state.compliance_inverse = 1.0 / tot;
}

/// The host calls `E43EngineSim::Sim8D90` makes: `Sim80D0` (0x5180d0, thiscall, `x`), which resets the table entry
/// `x` in memory. The port reads the entry again after the call.
pub trait E43Host {
    fn intake_state_reset(&mut self, this: u32, x: u32);
}

/// The calls `E43EngineSim::Sim8890` makes out of the port. `s30` is the car's slot 12 (`S30`, a virtual call on the car
/// that returns the object its walk takes). `mb560` is `E43PairWalk::MB560` on the walker, with the start `a` and the
/// object `b`; it leaves the transformed point, the four words of its output.
pub trait Sim8890Host {
    fn s30(&mut self, car: u32) -> u32;
    fn mb560(&mut self, a: u32, b: u32) -> [f32; 4];
}

/// The calls `E43EngineSim::Sim8E20` makes out of the port: `Sim8D90`'s reset (`E43Host`) and the car's walk
/// (`Sim8890Host`), and three sinks left to the host: `E41EngineNoiseSignal::Emit` (0x50ef50) on the engine's signal,
/// `CGraph1D::Push` (0x494cf0) on a graph, and `E01Log` (0x43c8e0) on the `FloorS32` error path.
pub trait EngineStepHost: E43Host + Sim8890Host {
    fn emit(&mut self, signal: u32);
    fn graph_push(&mut self, graph: u32, value: f32);
    fn floor_log(&mut self, value: f32);
}

/// The call `CEngine::Sim8CF0` makes out of the port: the emit (0x50ee90) on the global signal object, left to the
/// host. `arg` is the engine's `+0x184`, which the emit reads through its pointer.
pub trait Sim8CF0Host {
    /// Returns the eax the emit leaves.
    fn signal(&mut self, obj: u32, arg: u32) -> u32;
}

/// The lateral term of `Sim8890`: `k[0] * v[1] + k[1] * v[2] + k[2] * v[0]`, zero when that is negative (a NaN stays).
pub fn lateral_term(k: [f32; 3], v: [f32; 4]) -> f32 {
    let f = (k[0] * v[1] + k[1] * v[2]) + k[2] * v[0];
    if f < 0.0 {
        0.0
    } else {
        f
    }
}

/// The travel fraction of `Sim8890`: `(p - lo) / (hi - lo)`, clamped to `[0, 1]` (a NaN stays).
pub fn travel_fraction(p: f32, lo: f32, hi: f32) -> f32 {
    let f = (p - lo) / (hi - lo);
    if f < 0.0 {
        0.0
    } else if 1.0 < f {
        1.0
    } else {
        f
    }
}

/// The key `Sim8D90` writes into an entry: the definition's lane word for the entry plus `y`, low byte.
pub fn entry_key(lane: u32, y: u32) -> u32 {
    lane.wrapping_add(y) & 0xff
}

/// What `Sim8D90` returns: the blend's change from the entry's old value (the blend is `SimEC40`'s st0; the change is
/// stored), plus the reset's `extra` (0 when there is no reset), in st0.
pub fn entry_change(blend: f64, old: f32, extra: f32, variant: Variant) -> f64 {
    let change = fpu::round_x87(blend - f64::from(old), variant) as f32;
    fpu::round_x87(f64::from(extra) + f64::from(change), variant)
}

/// `CEngine::SimEC40` (0x50ec40): a bilinear blend of one exhaust row `t` with the eight inputs `s` (`s[2]` picks
/// the first form: when it is 0, the three-term sum of `t[0..3]` with `s[3..6]`; otherwise `t[3]`). That sum is SSE;
/// the rest is an x87 chain, returned in st0.
pub fn exhaust_blend(t: &[f32; 8], s: &[f32; 12], variant: Variant) -> f64 {
    let f = if s[2] == 0.0 { t[2] * s[5] + t[1] * s[4] + s[3] * t[0] } else { t[3] };
    let x87 = |v: f64| fpu::round_x87(v, variant);
    let product = |a: f32, b: f32| x87(f64::from(a) * f64::from(b));
    let head = x87(x87(product(t[7], s[7]) + f64::from(f)) * f64::from(s[9]));
    let head = x87(head + product(t[5], s[10]));
    let head = x87(head + product(t[4], s[11]));
    x87(head + product(t[6], s[8]))
}

/// `CEngine::SimECF0` (0x50ecf0): the stall guard. True above `stall_speed` rad/s (40; `layout::EngineTuning`).
/// Below that, true when both gates (`gate_58`, `gate_5c`) are at least 0.1 and at least three of the four stall terms
/// exceed 0.7.
pub fn stall_guard(speed: f32, options: &EngineOptions, stall_speed: f32) -> bool {
    if stall_speed < speed {
        return true;
    }
    if 0.1 <= options.gate_5c && 0.1 <= options.gate_58 {
        let high = options.stall_terms.iter().filter(|&&t| 0.7 < t).count();
        if high < 3 {
            return true;
        }
    }
    false
}

/// `CEngine::SimED70` (0x50ed70): the torque curve at `x`, written to `q[7]`. The curve is 0 when `q[2]` is set:
/// the asm stores 0 in that case too, so the store is not inside the C++ `if`. Otherwise the product takes the second
/// curve when `q[3]` is set, the third otherwise, scaled by `q[4]` or `q[5]`.
pub fn torque_curve(def: &EngineDef, x: f32, q: &mut [f32; 8]) {
    let mut f3 = 0.0;
    if q[2] == 0.0 {
        let x2 = x * x;
        // The coefficients are constant, linear, quadratic: (quadratic * x2 + linear * x) + constant.
        let poly = |c: &[f32; 3]| (c[2] * x2 + c[1] * x) + c[0];
        f3 = poly(&def.torque_a) * def.torque_a_scale * q[4];
        if q[3] != 0.0 {
            f3 += poly(&def.torque_b) * def.torque_b_scale * q[3];
        } else {
            f3 += poly(&def.torque_c) * def.torque_c_scale * q[5];
        }
    }
    q[7] = f3;
}

/// `CEngine::SimE520` (0x50e520): the misfire roll with the input `a` (the step's time). With the rate at 0 the
/// output is cleared. When the state's two counters allow a roll, the LCG (`seed`, the word the option block points
/// at) steps, and a misfire fires when the draw is below the rate (capped at 0.9) times the time term (capped at 1).
/// A misfire returns its level (`min(rate * 50, 1)`), which the caller stores at `this+0x194`, and clears the
/// counters. Otherwise `a` is added to the time counter.
pub fn misfire_roll(state: &mut EngineState, seed: &mut u32, a: f32) -> Option<f32> {
    if state.misfire_rate == 0.0 {
        state.misfire_out = 0.0;
        return None;
    }
    if state.misfire_count <= state.misfire_time && state.misfire_time != state.misfire_count {
        let u = seed.wrapping_mul(0x0200_0081).wrapping_add(0xe623_f41d);
        *seed = u;
        let draw = (u % 100_001) as f32 * 0.000_01;
        let mut time = state.misfire_time * 40.0;
        if 1.0 < time {
            time = 1.0;
        }
        let mut rate = state.misfire_rate * 999.999_94;
        if 0.9 < rate {
            rate = 0.9;
        }
        state.misfire_count += 0.1;
        if draw < rate * time {
            let mut level = state.misfire_rate * 50.0;
            if 1.0 < level {
                level = 1.0;
            }
            state.misfire_out = level;
            state.misfire_time = 0.0;
            state.misfire_rate = 0.0;
            state.misfire_count = 0.0;
            return Some(level);
        }
    }
    state.misfire_time += a;
    state.misfire_out = 0.0;
    None
}

/// Clamps a fraction to 0..1 as the asm's compares do: NaN is kept.
#[allow(clippy::manual_range_contains)]
fn unit_fraction(f: f32) -> f32 {
    if f < 0.0 || 1.0 < f {
        if f < 0.0 {
            0.0
        } else {
            1.0
        }
    } else {
        f
    }
}

/// `CEngine::SimE660` (0x50e660): the torque at `a` (the step's time). Each of the four loads is its ratio to the
/// definition's denominator (`1 / abs_value` scales the loads; the ratio is clamped to 0..1), and the part of each
/// that is below the span adds to the torque, with the option words (scaled by 400) as the base. The threshold
/// term adds above `threshold`. The torque is stored at `+0x270`, and at speed above 100 its `a`-scaled step is
/// accumulated at `+0x238`. The result is negated, and a rate at or below zero takes the zero branch.
pub fn torque_sum(def: &LoadDef, options: &EngineOptions, state: &mut EngineState, a: f32) -> f32 {
    let mut f6 = state.signed_rate;
    if !(f6 > 0.0) {
        f6 = 0.0 - f6;
    }
    let f4 = if !(state.abs_value > 0.00001) { 0.0 } else { 1.0 / state.abs_value };
    let mut f5 = options.load_terms[0] * 400.0;
    if f4 != 0.0 {
        let f3 = unit_fraction(state.load_bc * f4 / def.load_den[0]);
        f5 += (1.0 - f3) * def.load_span;
    }
    f5 += options.load_terms[1] * 400.0;
    if f4 != 0.0 {
        let f3 = unit_fraction(state.load_f4 * f4 / def.load_den[1]);
        f5 += (1.0 - f3) * def.load_span;
    }
    f5 += options.load_terms[2] * 400.0;
    if f4 != 0.0 {
        let f3 = unit_fraction(state.load_12c * f4 / def.load_den[2]);
        f5 += (1.0 - f3) * def.load_span;
    }
    f5 += options.load_terms[3] * 400.0;
    if f4 != 0.0 {
        let f3 = unit_fraction(state.load_164 * f4 / def.load_den[3]);
        f5 += (1.0 - f3) * def.load_span;
    }
    let t = state.threshold_value;
    if def.threshold <= t && t != def.threshold {
        f5 += (t - def.threshold) * def.threshold_slope;
    }
    f6 = if !(state.signed_rate < 0.0) { f6 * 0.0 } else { f6 * 20.0 };
    f6 += f5;
    state.torque = f6;
    let step = f6 * a * state.speed;
    if 100.0 < state.speed {
        let step = if !(step > 0.0) { 0.0 - step } else { step };
        state.torque_accum += step;
    }
    let rate = state.signed_rate;
    if !(rate > 0.0) {
        let neg = if rate < 0.0 { -1.0 } else { 0.0 };
        return -(neg * f6);
    }
    -(1.0 * f6)
}

/// What `CEngine::SimD870` leaves in eax: the roll's quotient when nothing hits, the slot's stall term when it hits,
/// the options block (`this+0x48`) when the hit clamps the term to 1, and 0 when the gate is off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RollRet {
    Null,
    Quotient(u32),
    Slot,
    Options,
}

/// `CEngine::SimD870` (0x50d870) for slot `i`: the roll. The slot's stall term (`option`, `stall_terms[i]`) and the
/// slot's loads (`load_a`, `load_b`) give a fraction `f` from the threshold (`threshold`, state `+0x23c`) and the
/// term; the table entry `global` (`0x7ca668 + 4 i`) is scaled by `1 - f * f`. Then the LCG (`seed`) steps; a draw
/// below `f * f` raises the stall term to `draw + term + 0.01` (or to 1, reporting the options block). NGP's head
/// gasket durability `d` (from 2) scales the draw first: `draw * (1/d) + (1/d - 1) * 0.01` (`head_gasket`).
pub fn slot_roll(
    gate: u32,
    threshold: f32,
    load_a: f32,
    load_b: f32,
    option: &mut f32,
    global: &mut f32,
    seed: &mut u32,
    head_gasket: Option<[f32; 2]>,
) -> RollRet {
    if gate == 0 {
        return RollRet::Null;
    }
    let f7 = *option * 10.0;
    let f8 = unit_fraction(f7);
    let g8 = unit_fraction((threshold - 600.0) * -0.005);
    let base = g8 * 344420.0 + 60_780.003_906_25;
    let f = unit_fraction(((load_a * 0.6 + 0.4) * load_b - base) / (((1.0 - f8) * 100.0 + 4.0) * base - base));
    *global *= 1.0 - f * f;
    let u = seed.wrapping_mul(0x0200_0081).wrapping_add(0xe623_f41d);
    *seed = u;
    let draw = (u % 100_001) as f32 * 0.000_01;
    let mut ret = RollRet::Quotient(u / 100_001);
    if draw < f * f && *option <= 1.0 && 1.0 != *option {
        let draw = head_gasket.map_or(draw, |[scale, offset]| draw * scale + offset);
        *option = draw + *option + 0.01;
        ret = RollRet::Slot;
        if 1.0 < *option {
            *option = 1.0;
            ret = RollRet::Options;
        }
    }
    ret
}

/// The knot keys of a blend table, for `blend_w`.
fn knot_keys(table: &[BlendKnot]) -> Vec<f32> {
    table.iter().map(|knot| knot.key).collect()
}

/// `CEngine::SimE060` (0x50e060): the smoothed damper step. `BlendW` brackets `key` in the outer table, and `BlendX`
/// blends the two bracketing sub-tables at `ratio` with the weights of `BlendW`. When the blend found a bracket, the
/// damper `block` (four words) takes an update with `dt`, and its response is the output. Otherwise the output is 0.
/// The `smoothed` value is the 0.9/0.1 low-pass of that output. Returns the `g` output of the call: the scaled
/// difference of `scale` and `ratio`, zero when `ratio` is at most 1.
pub fn smooth_damper(
    table: &[BlendKnot],
    key: f32,
    ratio: f32,
    scale: f32,
    dt: f32,
    block: &mut BlendTerms,
    smoothed: &mut Lane,
    variant: Variant,
) -> f32 {
    let bracket = blend_w(&knot_keys(table), key);
    let (hi, lo) = (&table[bracket.hi as usize].records, &table[bracket.lo as usize].records);
    let (f, g_blend, no_bracket) = blend_x(ratio, bracket.w_upper, bracket.w_lower, hi, lo);
    let no_bracket = no_bracket != 0;
    // x87 order: ((sqrt(ratio) - 1) / g + 1) * scale, then the scaled difference of the two `DamperScale`s.
    let x87 = |x: f64| fpu::round_x87(x, variant);
    let root = spring_damper::sqrt4(ratio, variant);
    let (scaled_ratio, g) = if !(ratio > 1.0) {
        (x87(root * f64::from(scale)) as f32, 0.0)
    } else {
        let shaped = x87(x87(x87(root - 1.0) / f64::from(g_blend)) + 1.0);
        let scaled_ratio = x87(shaped * f64::from(scale)) as f32;
        let scale_in = spring_damper::scale(scale, variant) as f32;
        let difference = x87(f64::from(scale_in) - spring_damper::scale(scaled_ratio, variant));
        let product = x87(difference * f64::from(f));
        (scaled_ratio, x87(product * f64::from(dt)) as f32)
    };
    let response = if no_bracket {
        0.0
    } else {
        // The original passes the update block to `DamperCombine` as its four words: the update's scale is the
        // source's weight, its numerator the value, its denominator the response and its response the scale.
        let mut update =
            UpdateTerms { scale: f * dt, numerator: scaled_ratio, denominator: block.response, response: 0.0 };
        spring_damper::update_a(&mut update, AIR_GAS_CONSTANT);
        let source = BlendTerms {
            weight: update.scale,
            value: update.numerator,
            response: update.denominator,
            scale: update.response,
        };
        spring_damper::combine(block, source, AIR_GAS_CONSTANT, variant);
        f
    };
    smoothed.set(smoothed.value * 0.9 + response * 0.1);
    g
}

/// `CEngine::SimE1F0` (0x50e1f0): the solve damper step. `BlendW` brackets `key` in the outer table, and `BlendY` blends
/// the two sub-tables at `ratio`. The solve `block` (a `Relaxation`) advances with the input from the solved value and
/// the blend (`solved`, the state's CReal, is set to the solved value over `dt`). Returns the output: the difference of
/// the averaged and the `ratio` scales, times the solved value and the blend's second output. `limit` is the solve's
/// pressure: [`STANDARD_PRESSURE`] in 1.02, the air pressure under NGP (its `DamperSolve` wrapper `0x10060230`).
#[allow(clippy::too_many_arguments)]
pub fn solve_damper(
    table: &[BlendKnot],
    key: f32,
    ratio: f32,
    dt: f32,
    block: &mut Relaxation,
    solved: &mut Lane,
    limit: f32,
    variant: Variant,
) -> f32 {
    let bracket = blend_w(&knot_keys(table), key);
    let (hi, lo) = (&table[bracket.hi as usize].records, &table[bracket.lo as usize].records);
    let (f, g) = blend_y(ratio, bracket.w_upper, bracket.w_lower, hi, lo);
    // The block's accumulated word before the solve: the ratio's scale below uses it, and so does the average.
    let accumulated_before = block.accumulated;
    let x87 = |x: f64| fpu::round_x87(x, variant);
    let scaled_ratio = if !(ratio > 1.0) {
        accumulated_before
    } else {
        x87(spring_damper::sqrt4(1.0 / ratio, variant) * f64::from(accumulated_before)) as f32
    };
    // x87 order: (solved * 0.01 + f * 0.99) * dt.
    let input = (solved.value * 0.01 + f * 0.99) * dt;
    let answer = spring_damper::solve(block, input, limit, AIR_GAS_CONSTANT, variant);
    solved.set(answer / dt);
    let average = (block.accumulated + accumulated_before) * 0.5;
    let scale_average = spring_damper::scale(average, variant) as f32;
    let difference = x87(f64::from(scale_average) - spring_damper::scale(scaled_ratio, variant));
    let product = x87(difference * f64::from(answer));
    x87(product * f64::from(g)) as f32
}

/// `CEngine::SimEBB0` (0x50ebb0): the angle of its clamped cosine. `v` is the engine's value (`+0x20`) and `floor` the
/// definition's `+0x2708`. The fraction of `v` between `floor` and 1 is clamped to 0..=1 (a NaN keeps its value), then
/// mapped back onto `floor..1` and scaled by pi/2.
pub fn ebb0_angle(v: f32, floor: f32) -> f32 {
    let span = 1.0 - floor;
    let mut fraction = (v - floor) / span;
    if 0.0 > fraction {
        fraction = 0.0;
    } else if fraction > 1.0 {
        fraction = 1.0;
    }
    (span * fraction + floor) * 1.5707964
}

#[cfg(test)]
mod sim8890_tests {
    use super::*;

    #[test]
    fn lateral_term_clamps_negative_and_keeps_nan() {
        assert_eq!(lateral_term([1.0, 0.0, 0.0], [0.0, -2.0, 0.0, 0.0]), 0.0);
        assert_eq!(lateral_term([1.0, 1.0, 1.0], [1.0, 2.0, 3.0, 0.0]), 6.0);
        assert!(lateral_term([f32::NAN, 0.0, 0.0], [1.0, 1.0, 1.0, 0.0]).is_nan());
    }

    #[test]
    fn travel_fraction_clamps_to_unit_and_keeps_nan() {
        assert_eq!(travel_fraction(0.0, 1.0, 3.0), 0.0);
        assert_eq!(travel_fraction(2.0, 1.0, 3.0), 0.5);
        assert_eq!(travel_fraction(9.0, 1.0, 3.0), 1.0);
        assert_eq!(travel_fraction(-9.0, 1.0, 3.0), 0.0);
        assert!(travel_fraction(f32::NAN, 1.0, 3.0).is_nan());
    }
}
