//! The engine's kernels: the pedal input, the crank tick, the rev-limit gate, the misfire roll, the series
//! compliance, the exhaust blend, the stall guard and the torque curve. `tests/engine_driveline_vectors.rs` checks
//! each against recorded calls of the original.

use std::f32::consts::{FRAC_PI_2, PI};

use super::layout::{BlendKnot, EngineDef, EngineOptions, EngineState, Lane, LoadDef};
use super::sim::{blend_tables, blend_tables_checked, find_bracket};
use crate::fpu;
use crate::spring_damper::{self, BlendTerms, Relaxation, UpdateTerms};

/// The gas constant the dampers use (287 J/(kg K), dry air).
const AIR_GAS_CONSTANT: f32 = 287.0;
/// The pressure the dampers solve against (101300 Pa, one standard atmosphere).
pub const STANDARD_PRESSURE: f32 = 101_300.0;

/// The game's random generator: one linear congruential step of `seed`.
pub fn next_random(seed: u32) -> u32 {
    seed.wrapping_mul(0x0200_0081).wrapping_add(0xe623_f41d)
}

/// A random word as a draw in `0..=1` with five decimals.
fn unit_draw(random: u32) -> f32 {
    (random % 100_001) as f32 * 0.000_01
}

/// Sets the pedal's value (its bits, as the caller passes them) and its dirty flag.
pub fn set_pedal(pedal: &mut Lane, bits: u32) {
    pedal.set_bits(bits);
}

/// The crank advances by `a` (rad), wrapped to `0..4 pi` (wrapped once, by one cycle). The crank count is
/// `trunc(angle * 20.37...)`, 256 ticks per cycle. Returns the signed tick delta, wrapped to +-128 (the sign follows
/// `a`), and the angle that delta covers (`delta * 4 pi / 256`).
pub fn crank_tick(state: &mut EngineState, a: f32) -> (i32, f32) {
    let turn = 4.0 * PI;
    let scale: f32 = 20.371_832; // 256 / 4 pi
    let step = PI / 64.0; // 4 pi / 256
    let previous = state.crank_count;
    state.crank_count_prev = previous;
    state.crank_angle += a;
    let x = state.crank_angle;
    if x > turn {
        state.crank_angle = x - turn;
    } else if 0.0 > x {
        state.crank_angle = x + turn;
    }
    // The x87 product, truncated: at 24-bit precision it rounds to f32; at 53 (NGP) it stays exact, so a
    // product just under a tick doesn't round up to it.
    let scaled = fpu::round_x87(f64::from(state.crank_angle) * f64::from(scale));
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

/// The rev-limit gate, from the speed (rad/s) and the pedal. Zero below the floor rpm (500) or with the gate off.
/// Otherwise a factor from the rpm above the start (3500) and the pedal: `1 - 10 x` below -0.4, `1 + 0.2 x` above
/// 0.4, else 1, with `x = pedal - clamp((rpm - start) * scale)`. `band` is the floor, the start and the scale
/// (`layout::EngineTuning::rev_gate`). The factor is the x87 result, not rounded to f32.
pub fn rev_gate(speed: f32, pedal: f32, gate: u32, band: [f32; 3]) -> f64 {
    let [floor, start, scale] = band;
    let rpm = speed * 9.549_296_4;
    if rpm < floor || gate == 0 {
        return 0.0;
    }
    let mut fraction = (rpm - start) * scale;
    // Not `contains`: a NaN fraction stays NaN, as its two compares leave it.
    #[allow(clippy::manual_range_contains)]
    if fraction < 0.0 || 1.0 < fraction {
        fraction = if fraction < 0.0 { 0.0 } else { 1.0 };
    }
    let x = pedal - fraction;
    let x87 = fpu::round_x87;
    if x < -0.4 {
        return x87(1.0 - x87(f64::from(x) * 10.0));
    }
    if 0.4 < x {
        return x87(x87(f64::from(x) * f64::from(0.2f32)) + 1.0);
    }
    1.0
}

/// The series compliance. Each of the four stages is interpolated between its low and high ends by its fraction in
/// the option block, and the combined value is `1 / (sum of 1 / stage)`. The fifth stage joins the sum when the
/// definition enables it. Sets the stages, the fifth (`extra_stage`) and the combined value with its dirty flag.
pub fn compliance(def: &EngineDef, options: &EngineOptions, state: &mut EngineState) {
    // Order: (high - low) * t + low, per stage.
    let stage = |k: usize| (def.stage_high[k] - def.stage_low[k]) * options.stage_t[k] + def.stage_low[k];
    let s0 = stage(0);
    let s1 = stage(1);
    let s2 = stage(2);
    let s3 = stage(3);
    state.stage = [s0, s1, s2, s3];
    // The sum runs in this order: 1/s0, then + 1/s1, + 1/s2, + 1/s3 (each step rounded to f32).
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

/// The call the intake entry update makes out of the port: the reset of table entry `x`, which works in memory. The
/// port reads the entry again after the call.
pub trait IntakeResetHost {
    fn intake_state_reset(&mut self, this: u32, x: u32);
}

/// The calls the engine's pose update makes out of the port. `walk_object` is the car's object the transform walks
/// from. `transform_point` walks from the start `a` through the object `b` and leaves the transformed point, four
/// words.
pub trait CarWalkHost {
    fn walk_object(&mut self, car: u32) -> u32;
    fn transform_point(&mut self, a: u32, b: u32) -> [f32; 4];
}

/// The calls the engine step makes out of the port: the intake reset ([`IntakeResetHost`]) and the car's walk
/// ([`CarWalkHost`]), and three sinks left to the host: the engine noise signal's emit, a graph's push, and the log
/// line on the floor conversion's error path.
pub trait EngineStepHost: IntakeResetHost + CarWalkHost {
    fn emit(&mut self, signal: u32);
    fn graph_push(&mut self, graph: u32, value: f32);
    fn floor_log(&mut self, value: f32);
}

/// The call the engine's signal update makes out of the port: the emit on the global signal object, left to the host.
/// `arg` is the engine's signed rate, which the emit reads through its pointer.
pub trait SignalHost {
    /// Returns the word the emit leaves for the caller.
    fn signal(&mut self, obj: u32, arg: u32) -> u32;
}

/// The lateral term of the pose update: `k[0] * v[1] + k[1] * v[2] + k[2] * v[0]`, zero when that is negative (a NaN stays).
pub fn lateral_term(k: [f32; 3], v: [f32; 4]) -> f32 {
    let f = (k[0] * v[1] + k[1] * v[2]) + k[2] * v[0];
    if f < 0.0 {
        0.0
    } else {
        f
    }
}

/// The travel fraction of the pose update: `(p - lo) / (hi - lo)`, clamped to `[0, 1]` (a NaN stays).
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

/// The key the intake entry update writes into an entry: the definition's lane word for the entry plus `y`, low byte.
pub fn entry_key(lane: u32, y: u32) -> u32 {
    lane.wrapping_add(y) & 0xff
}

/// What the intake entry update returns: the blend's change from the entry's old value (the blend is
/// [`exhaust_blend`]'s result; the change is stored), plus the reset's `extra` (0 when there is no reset).
pub fn entry_change(blend: f64, old: f32, extra: f32) -> f64 {
    let change = fpu::round_x87(blend - f64::from(old)) as f32;
    fpu::round_x87(f64::from(extra) + f64::from(change))
}

/// A bilinear blend of one exhaust row `t` with the eight inputs `s` (`s[2]` picks the first form: when it is 0, the
/// three-term sum of `t[0..3]` with `s[3..6]`; otherwise `t[3]`). That sum is in f32; the rest is an x87 chain whose
/// result is not rounded to f32.
pub fn exhaust_blend(t: &[f32; 8], s: &[f32; 12]) -> f64 {
    let f = if s[2] == 0.0 { t[2] * s[5] + t[1] * s[4] + s[3] * t[0] } else { t[3] };
    let x87 = fpu::round_x87;
    let product = |a: f32, b: f32| x87(f64::from(a) * f64::from(b));
    let head = x87(x87(product(t[7], s[7]) + f64::from(f)) * f64::from(s[9]));
    let head = x87(head + product(t[5], s[10]));
    let head = x87(head + product(t[4], s[11]));
    x87(head + product(t[6], s[8]))
}

/// The stall guard. True above `stall_speed` rad/s (40; `layout::EngineTuning`).
/// Below that, true when both gates (`first_gate`, `second_gate`) are at least 0.1 and at least three of the four stall terms
/// exceed 0.7.
pub fn stall_guard(speed: f32, options: &EngineOptions, stall_speed: f32) -> bool {
    if stall_speed < speed {
        return true;
    }
    if 0.1 <= options.second_gate && 0.1 <= options.first_gate {
        let high = options.stall_terms.iter().filter(|&&t| 0.7 < t).count();
        if high < 3 {
            return true;
        }
    }
    false
}

/// The torque curve at `x`, written to `q[7]`. The curve is 0 when `q[2]` is set (and `q[7]` is still stored).
/// Otherwise the product takes the second curve when `q[3]` is set, the third otherwise, scaled by `q[4]` or `q[5]`.
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

/// The misfire roll with the input `a` (the step's time). With the rate at 0 the output is cleared. When the state's
/// two counters allow a roll, the LCG (`seed`, the word the option block points at) steps, and a misfire fires when
/// the draw is below the rate (capped at 0.9) times the time term (capped at 1). A misfire returns its level
/// (`min(rate * 50, 1)`), which the caller stores as the engine's misfire level, and clears the counters. Otherwise
/// `a` is added to the time counter.
pub fn misfire_roll(state: &mut EngineState, seed: &mut u32, a: f32) -> Option<f32> {
    if state.misfire_rate == 0.0 {
        state.misfire_out = 0.0;
        return None;
    }
    if state.misfire_count <= state.misfire_time && state.misfire_time != state.misfire_count {
        let u = next_random(*seed);
        *seed = u;
        let draw = unit_draw(u);
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

/// Clamps a fraction to 0..1 with two compares: NaN is kept.
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

/// The torque at `a` (the step's time). Each of the four loads is its ratio to the definition's denominator
/// (`1 / abs_value` scales the loads; the ratio is clamped to 0..1), and the part of each that is below the span adds
/// to the torque, with the option words (scaled by 400) as the base. The threshold term adds above `threshold`. The
/// torque is stored in the state, and at speed above 100 its `a`-scaled step is added to `torque_accum`. The result
/// is negated, and a rate at or below zero takes the zero branch.
pub fn torque_sum(def: &LoadDef, options: &EngineOptions, state: &mut EngineState, a: f32) -> f32 {
    // The rate's magnitude, which the rate term below scales.
    let mut torque = state.signed_rate;
    if !(torque > 0.0) {
        torque = 0.0 - torque;
    }
    let inverse_abs = if !(state.abs_value > 0.00001) { 0.0 } else { 1.0 / state.abs_value };
    let mut load_sum = options.load_terms[0] * 400.0;
    if inverse_abs != 0.0 {
        let share = unit_fraction(state.load_a * inverse_abs / def.load_den[0]);
        load_sum += (1.0 - share) * def.load_span;
    }
    load_sum += options.load_terms[1] * 400.0;
    if inverse_abs != 0.0 {
        let share = unit_fraction(state.load_b * inverse_abs / def.load_den[1]);
        load_sum += (1.0 - share) * def.load_span;
    }
    load_sum += options.load_terms[2] * 400.0;
    if inverse_abs != 0.0 {
        let share = unit_fraction(state.load_c * inverse_abs / def.load_den[2]);
        load_sum += (1.0 - share) * def.load_span;
    }
    load_sum += options.load_terms[3] * 400.0;
    if inverse_abs != 0.0 {
        let share = unit_fraction(state.load_d * inverse_abs / def.load_den[3]);
        load_sum += (1.0 - share) * def.load_span;
    }
    let t = state.threshold_value;
    if def.threshold <= t && t != def.threshold {
        load_sum += (t - def.threshold) * def.threshold_slope;
    }
    torque = if !(state.signed_rate < 0.0) { torque * 0.0 } else { torque * 20.0 };
    torque += load_sum;
    state.torque = torque;
    let step = torque * a * state.speed;
    if 100.0 < state.speed {
        let step = if !(step > 0.0) { 0.0 - step } else { step };
        state.torque_accum += step;
    }
    let rate = state.signed_rate;
    if !(rate > 0.0) {
        let neg = if rate < 0.0 { -1.0 } else { 0.0 };
        return -(neg * torque);
    }
    -(1.0 * torque)
}

/// What the stall roll leaves for its caller: the roll's quotient when nothing hits, the slot's stall term when it
/// hits, the options block when the hit clamps the term to 1, and 0 when the gate is off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RollOutcome {
    Null,
    Quotient(u32),
    Slot,
    Options,
}

/// The stall roll for one slot. The slot's stall term (`option`, `stall_terms[i]`) and the slot's loads (`load_a`,
/// `load_b`) give a fraction `f` from the threshold (`threshold`, the state's threshold value) and the term; the
/// slot's global table entry `global` is scaled by `1 - f * f`. Then the LCG (`seed`) steps; a draw below `f * f`
/// raises the stall term to `draw + term + 0.01` (or to 1, reporting the options block). NGP's head gasket
/// durability `d` (from 2) scales the draw first: `draw * (1/d) + (1/d - 1) * 0.01` (`head_gasket`).
pub fn slot_roll(
    gate: u32,
    threshold: f32,
    load_a: f32,
    load_b: f32,
    option: &mut f32,
    global: &mut f32,
    seed: &mut u32,
    head_gasket: Option<[f32; 2]>,
) -> RollOutcome {
    if gate == 0 {
        return RollOutcome::Null;
    }
    let scaled_option = *option * 10.0;
    let option_fraction = unit_fraction(scaled_option);
    let threshold_fraction = unit_fraction((threshold - 600.0) * -0.005);
    let base = threshold_fraction * 344420.0 + 60_780.003_906_25;
    let f =
        unit_fraction(((load_a * 0.6 + 0.4) * load_b - base) / (((1.0 - option_fraction) * 100.0 + 4.0) * base - base));
    *global *= 1.0 - f * f;
    let u = next_random(*seed);
    *seed = u;
    let draw = unit_draw(u);
    let mut ret = RollOutcome::Quotient(u / 100_001);
    if draw < f * f && *option <= 1.0 && 1.0 != *option {
        let draw = head_gasket.map_or(draw, |[scale, offset]| draw * scale + offset);
        *option = draw + *option + 0.01;
        ret = RollOutcome::Slot;
        if 1.0 < *option {
            *option = 1.0;
            ret = RollOutcome::Options;
        }
    }
    ret
}

/// The knot keys of a blend table, for [`find_bracket`].
fn knot_keys(table: &[BlendKnot]) -> Vec<f32> {
    table.iter().map(|knot| knot.key).collect()
}

/// The smoothed damper step. [`find_bracket`] brackets `key` in the outer table, and [`blend_tables_checked`] blends
/// the two bracketing sub-tables at `ratio` with its weights. When the blend found a bracket, the
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
) -> f32 {
    let bracket = find_bracket(&knot_keys(table), key);
    let (hi, lo) = (&table[bracket.hi as usize].records, &table[bracket.lo as usize].records);
    let (f, g_blend, no_bracket) = blend_tables_checked(ratio, bracket.w_upper, bracket.w_lower, hi, lo);
    let no_bracket = no_bracket != 0;
    // x87 order: ((sqrt(ratio) - 1) / g + 1) * scale, then the scaled difference of the two damper scales.
    let x87 = fpu::round_x87;
    let root = spring_damper::sqrt4(ratio);
    let (scaled_ratio, g) = if !(ratio > 1.0) {
        (x87(root * f64::from(scale)) as f32, 0.0)
    } else {
        let shaped = x87(x87(x87(root - 1.0) / f64::from(g_blend)) + 1.0);
        let scaled_ratio = x87(shaped * f64::from(scale)) as f32;
        let scale_in = spring_damper::scale(scale) as f32;
        let difference = x87(f64::from(scale_in) - spring_damper::scale(scaled_ratio));
        let product = x87(difference * f64::from(f));
        (scaled_ratio, x87(product * f64::from(dt)) as f32)
    };
    let response = if no_bracket {
        0.0
    } else {
        // The update's four words are passed on as the combined source: its scale is the source's weight, its
        // numerator the value, its denominator the response and its response the scale.
        let mut update =
            UpdateTerms { scale: f * dt, numerator: scaled_ratio, denominator: block.response, response: 0.0 };
        spring_damper::update_a(&mut update, AIR_GAS_CONSTANT);
        let source = BlendTerms {
            weight: update.scale,
            value: update.numerator,
            response: update.denominator,
            scale: update.response,
        };
        spring_damper::combine(block, source, AIR_GAS_CONSTANT);
        f
    };
    smoothed.set(smoothed.value * 0.9 + response * 0.1);
    g
}

/// The solve damper step. [`find_bracket`] brackets `key` in the outer table, and [`blend_tables`] blends the two
/// sub-tables at `ratio`. The solve `block` (a `Relaxation`) advances with the input from the solved value and the
/// blend (`solved` is set to the solved value over `dt`). Returns the output: the difference of the averaged and the
/// `ratio` scales, times the solved value and the blend's second output. `limit` is the solve's pressure:
/// [`STANDARD_PRESSURE`] in 1.02, the air pressure under NGP.
#[allow(clippy::too_many_arguments)]
pub fn solve_damper(
    table: &[BlendKnot],
    key: f32,
    ratio: f32,
    dt: f32,
    block: &mut Relaxation,
    solved: &mut Lane,
    limit: f32,
) -> f32 {
    let bracket = find_bracket(&knot_keys(table), key);
    let (hi, lo) = (&table[bracket.hi as usize].records, &table[bracket.lo as usize].records);
    let (f, g) = blend_tables(ratio, bracket.w_upper, bracket.w_lower, hi, lo);
    // The block's accumulated word before the solve: the ratio's scale below uses it, and so does the average.
    let accumulated_before = block.accumulated;
    let x87 = fpu::round_x87;
    let scaled_ratio = if !(ratio > 1.0) {
        accumulated_before
    } else {
        x87(spring_damper::sqrt4(1.0 / ratio) * f64::from(accumulated_before)) as f32
    };
    // x87 order: (solved * 0.01 + f * 0.99) * dt.
    let input = (solved.value * 0.01 + f * 0.99) * dt;
    let answer = spring_damper::solve(block, input, limit, AIR_GAS_CONSTANT);
    solved.set(answer / dt);
    let average = (block.accumulated + accumulated_before) * 0.5;
    let scale_average = spring_damper::scale(average) as f32;
    let difference = x87(f64::from(scale_average) - spring_damper::scale(scaled_ratio));
    let product = x87(difference * f64::from(answer));
    x87(product * f64::from(g)) as f32
}

/// The angle of the clamped pedal cosine: `v` is the pedal and `floor` the definition's lower bound. The fraction of
/// `v` between `floor` and 1 is clamped to 0..=1 (a NaN keeps its value), then mapped back onto `floor..1` and scaled
/// by pi/2.
pub fn clamped_angle(v: f32, floor: f32) -> f32 {
    let span = 1.0 - floor;
    let mut fraction = (v - floor) / span;
    if 0.0 > fraction {
        fraction = 0.0;
    } else if fraction > 1.0 {
        fraction = 1.0;
    }
    (span * fraction + floor) * FRAC_PI_2
}

#[cfg(test)]
mod pose_tests {
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

#[cfg(test)]
mod constant_tests {
    use super::*;

    #[test]
    fn the_crank_constants_are_the_original_words() {
        assert_eq!((4.0 * PI).to_bits(), 0x41490fdb);
        assert_eq!(20.371_832_f32.to_bits(), 0x41a2f983);
        assert_eq!((PI / 64.0).to_bits(), 0x3d490fdb);
        assert_eq!(FRAC_PI_2.to_bits(), 1.5707964_f32.to_bits());
    }
}
