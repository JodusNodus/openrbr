//! NGP's engine wrapper. Over-revving past 1.05 times the rev limit wears the engine's health; below 0.101 the engine
//! dies (the alive flag clears and NGP emits the engine fire, which the binding does). While the engine lives and
//! runs, [`torque`] adds NGP's engine braking and idle terms to 1.02's torque; the result is divided by 1.05
//! ([`output`]). All SSE, in doubles where the original uses them.

use super::sign;
use crate::fpu::round_x87;

/// The bit of NGP's engine mode word (the car's ini) that adds the engine braking term.
pub const BRAKING_MODE: u32 = 2;
/// The bit that runs the idle model, when the idle table is made.
pub const IDLE_MODEL_MODE: u32 = 8;

/// The rev limit in rad/s from the limit in rpm (`rpm * pi / 30`).
pub fn rev_limit(rpm: f32) -> f32 {
    rpm * 0.104_719_76
}

/// The health after the step: it loses `0.05 dt` while `|speed|` is above 1.05 times the rev limit.
pub fn worn_health(health: f32, speed: f32, rev_limit: f32, dt: f32) -> f32 {
    if speed.abs() > rev_limit * 1.05 {
        health - dt * 0.05
    } else {
        health
    }
}

/// Whether the health is low enough to kill the engine (`0.101 > health`).
pub fn is_fatal(health: f32) -> bool {
    0.101 > health
}

/// `max(a, b)` and `min(a, b)` with `b` when they are unordered (SSE's `maxsd`, `minsd`, `minss`).
fn max_sd(a: f64, b: f64) -> f64 {
    if a > b {
        a
    } else {
        b
    }
}

fn min_sd(a: f64, b: f64) -> f64 {
    if a < b {
        a
    } else {
        b
    }
}

fn min_ss(a: f32, b: f32) -> f32 {
    if a < b {
        a
    } else {
        b
    }
}

/// The engine's damage state, as [`damage_factor`] reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EngineDamage {
    /// Four part conditions, 1 intact: each below 0.7 adds `clamp((1 - x / 0.7) * 0.25, 0, 0.25)` to the sum.
    pub parts: [f32; 4],
    /// Four wear terms (named by use) that scale the sum by `1 - ((1 - health) 0.05 + damage_a 0.1 + damage_b 0.1 +
    /// (1 - condition) 0.1)`; `health` is the engine's health (see [`worn_health`]).
    pub damage_a: f32,
    pub health: f32,
    pub condition: f32,
    pub damage_b: f32,
}

/// The damage factor of the engine's damage state.
pub fn damage_factor(d: &EngineDamage) -> f32 {
    let term = |x: f32| {
        let x = f64::from(x);
        (!(x > 0.7)).then(|| min_sd(0.25, max_sd(0.0, (1.0 - x / 0.7) * 0.25)))
    };
    let mut sum = 0.0;
    if let Some(t) = term(d.parts[0]) {
        sum = t + 0.0;
    }
    for &x in &d.parts[1..] {
        if let Some(t) = term(x) {
            sum += t;
        }
    }
    let [damage_a, health, condition, damage_b] = [d.damage_a, d.health, d.condition, d.damage_b].map(f64::from);
    let loss = (1.0 - health) * 0.05 + damage_a * 0.1 + damage_b * 0.1 + (1.0 - condition) * 0.1;
    ((1.0 - loss) * sum) as f32
}

/// The idle table on `n` knots: `y(0)` below `x(0)`, `y(n - 1)` above `x(n - 1)`, else the lerp in the segment found
/// by a binary search (one before the first `x` not below `v`, at least the first). `x(k)` and `y(k)` are the doubles
/// at knot slot `k` of each vector, read as the original reads them: with fewer than two knots it reads past the
/// knots (one knot lerps towards the double after it; none compares the doubles just after and before the empty
/// vector, and lerps over the two after).
pub fn idle_table_lookup(n: usize, x: impl Fn(isize) -> f64, y: impl Fn(isize) -> f64, v: f64) -> f64 {
    let last = n as isize - 1;
    if x(0) > v {
        return y(0);
    }
    if v > x(last) {
        return y(last);
    }
    let (mut first, mut count) = (0, n as isize);
    while count > 0 {
        let half = count / 2;
        if v > x(first + half) {
            first += half + 1;
            count -= half + 1;
        } else {
            count = half;
        }
    }
    let i = (first - 1).max(0);
    let t = (v - x(i)) / (x(i + 1) - x(i));
    (1.0 - t) * y(i) + t * y(i + 1)
}

/// What [`torque`] reads.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TorqueIn {
    /// The engine's speed, rad/s.
    pub speed: f32,
    /// [`rev_limit`] of the car's rev limit.
    pub rev_limit: f32,
    pub throttle: f32,
    /// The VCU's shift cooldown: above 0 the idle term takes `shift_idle_scale`.
    pub shift_cooldown: f32,
    /// The engine definition's idle scale while shifting (0.5 when not above 0).
    pub shift_idle_scale: f32,
    /// The engine definition's `enable` word is set (1.02's fifth torque stage): the stall term is halved.
    pub stage_enabled: bool,
    /// NGP's engine mode word: [`IDLE_MODEL_MODE`] runs the idle model when the idle table is made, else
    /// [`BRAKING_MODE`] adds the braking term to 1.02's torque.
    pub modes: u32,
    pub has_idle_table: bool,
    /// NGP's per-car words: the engine braking factor (the ini's percentage), the max torque, the stall speed and the
    /// idle band (rad/s), and the air density.
    pub braking: f32,
    pub max_torque: f32,
    pub stall_speed: f32,
    pub idle_band: f32,
    pub density: f32,
    /// The engine's friction torque, subtracted against its direction of rotation.
    pub friction_torque: f32,
    pub damage: EngineDamage,
}

/// The wrapper's torque while the engine lives and runs, from 1.02's `base_torque`. `stalled_history` drops the oldest
/// speed of NGP's speed history, appends `speed` and says whether none is above the stall speed; `idle_table` is the
/// idle table's lookup (rounded to f32).
pub fn torque(
    i: &TorqueIn,
    base_torque: f32,
    stalled_history: impl FnOnce() -> bool,
    idle_table: impl FnOnce(f64) -> f32,
) -> f32 {
    let direction = sign(i.speed) as f32;
    let x = direction * min_ss(i.rev_limit, i.speed.abs());
    let braking = x / i.rev_limit * -i.braking * i.max_torque;
    if i.modes & IDLE_MODEL_MODE == 0 || !i.has_idle_table {
        return if i.modes & BRAKING_MODE != 0 { (1.0 - i.throttle) * braking + base_torque } else { base_torque };
    }
    let stalled = stalled_history();
    if stalled && i.stall_speed > i.speed {
        let v = -direction * 0.1 * i.max_torque;
        return if i.stage_enabled { v * 0.5 } else { v };
    }
    let table = idle_table(f64::from(x));
    let m = damage_factor(&i.damage) * (i.density / 1.204_071_9);
    let mut v = m * table * i.max_torque;
    if i.shift_cooldown > 0.0 {
        v *= if i.shift_idle_scale > 0.0 { i.shift_idle_scale } else { 0.5 };
    }
    v -= direction * i.friction_torque;
    if i.idle_band > i.speed {
        if min_ss(104.719_76, i.idle_band * 0.95) > i.speed {
            v = min_ss(m * 0.05 * i.max_torque, v);
        }
        v + braking
    } else {
        if i.speed > i.rev_limit {
            v = 0.0;
        }
        (1.0 - i.throttle) * braking + v * i.throttle
    }
}

/// The wrapper's result: the torque over 1.05, an x87 division at the control word's precision (24 bits in the load).
pub fn output(v: f32) -> f64 {
    round_x87(f64::from(v) / f64::from(1.05f32))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`idle_table_lookup`] on knot slots `-1..`, `xs[0]` and `ys[0]` being slot -1.
    fn lookup(n: usize, xs: &[f64], ys: &[f64], v: f64) -> f64 {
        idle_table_lookup(n, |k| xs[(k + 1) as usize], |k| ys[(k + 1) as usize], v)
    }

    #[test]
    fn table_lerps_inside_and_clamps_outside() {
        let (x, y) = ([9.0, 0.0, 10.0, 20.0], [9.0, 1.0, 3.0, 7.0]);
        assert_eq!(lookup(3, &x, &y, -1.0), 1.0);
        assert_eq!(lookup(3, &x, &y, 25.0), 7.0);
        assert_eq!(lookup(3, &x, &y, 5.0), 2.0);
        assert_eq!(lookup(3, &x, &y, 10.0), 3.0);
        assert_eq!(lookup(3, &x, &y, 15.0), 5.0);
        assert_eq!(lookup(3, &x, &y, 0.0), 1.0);
    }

    #[test]
    fn short_tables_read_past_their_knots() {
        // One knot at its own speed lerps towards the double after it.
        assert_eq!(lookup(1, &[0.0, 5.0, 7.0], &[0.0, 2.0, 4.0], 5.0), 2.0);
        assert!(lookup(1, &[0.0, 5.0, 5.0], &[0.0, 2.0, 4.0], 5.0).is_nan());
        // No knots: the double after is the first x, the one before the last, the two after the segment.
        let (x, y) = ([3.0, 1.0, 5.0], [6.0, 8.0, 10.0]);
        assert_eq!(lookup(0, &x, &y, 0.0), 8.0);
        assert_eq!(lookup(0, &x, &y, 4.0), 6.0);
        assert_eq!(lookup(0, &x, &y, 2.0), 8.5);
    }

    #[test]
    fn healthy_engine_has_no_damage_term() {
        let intact = EngineDamage { parts: [1.0; 4], health: 1.0, condition: 1.0, ..Default::default() };
        assert_eq!(damage_factor(&intact), 0.0);
        assert_eq!(damage_factor(&EngineDamage { parts: [0.0, 1.0, 1.0, 1.0], ..intact }), 0.25);
    }
}
