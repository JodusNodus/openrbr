//! NGP's engine wrapper `0x1004b8e0`, the target of `CCar::Fwd18E20`'s `jmp` (`0x4e6650`), thiscall on the engine
//! (`car+0x690`) with `(dt, speed)`. Over-revving past 1.05 times the rev limit (`*(car+0x850)+0x188`, rpm) wears the
//! engine's health (`*(engine+0x48)+0x58`); below 0.101 the engine dies: its alive flag (`+0x74`) clears and NGP
//! emits the engine fire (the binding). Then the atmosphere and 1.02's `Sim8E20` run, and while the engine lives and
//! runs (`SimECF0`) [`torque`] adds NGP's engine braking and idle terms to `Sim8E20`'s value. The result is divided
//! by 1.05. All SSE, in doubles where the asm uses them.

use super::sign;
use crate::fpu::round_x87;
use crate::Variant;

/// The rev limit in rad/s: the rpm times `pi / 30` (`10148a5c`).
pub fn limit(rpm: f32) -> f32 {
    rpm * 0.104_719_76
}

/// The health after the step: it loses `0.05 dt` while `|speed|` is above 1.05 times the limit.
pub fn health_step(health: f32, speed: f32, limit: f32, dt: f32) -> f32 {
    if speed.abs() > limit * 1.05 {
        health - dt * 0.05
    } else {
        health
    }
}

/// Whether the health is low enough to kill the engine (`0.101 > health`).
pub fn dies(health: f32) -> bool {
    0.101 > health
}

/// `maxsd a, b` / `minsd a, b` / `minss a, b`: `b` when they are unordered.
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

/// `0x1004c740`: the damage factor of the engine's health object. `w` holds its `+0x8`, `+0x18`, `+0x28`, `+0x38`
/// (each adds `clamp((1 - x / 0.7) * 0.25, 0, 0.25)` when at most 0.7), then `+0x54`, `+0x58`, `+0x5c` and `+0x60`,
/// which scale the sum by `1 - ((1 - h58) 0.05 + h54 0.1 + h60 0.1 + (1 - h5c) 0.1)`.
pub fn damage_factor(w: &[f32; 8]) -> f32 {
    let term = |x: f32| {
        let x = f64::from(x);
        (!(x > 0.7)).then(|| min_sd(0.25, max_sd(0.0, (1.0 - x / 0.7) * 0.25)))
    };
    let mut sum = 0.0;
    if let Some(t) = term(w[0]) {
        sum = t + 0.0;
    }
    for &x in &w[1..4] {
        if let Some(t) = term(x) {
            sum += t;
        }
    }
    let [h54, h58, h5c, h60] = [w[4], w[5], w[6], w[7]].map(f64::from);
    let loss = (1.0 - h58) * 0.05 + h54 * 0.1 + h60 * 0.1 + (1.0 - h5c) * 0.1;
    ((1.0 - loss) * sum) as f32
}

/// `0x1004a700`, the table's slot 5, on `n` knots: `y(0)` below `x(0)`, `y(n - 1)` above `x(n - 1)`, else the lerp
/// in the segment `0x1004cc70` finds (one before the first `x` not below `v`, at least the first). `x(k)` and `y(k)`
/// are the doubles at knot slot `k` of each vector, read as the original reads them: with fewer than two knots it reads
/// past the knots (one knot lerps towards the double after it; none compares the doubles just after and before the
/// empty vector, and lerps over the two after).
pub fn table_lookup(n: usize, x: impl Fn(isize) -> f64, y: impl Fn(isize) -> f64, v: f64) -> f64 {
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

/// What [`torque`] reads (names ours).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TorqueIn {
    pub speed: f32,
    /// [`limit`] of the car's rev limit.
    pub limit: f32,
    /// The engine's `+0x20`.
    pub blend: f32,
    /// `*(car+0x854)+0x64`: above 0 the idle term takes the def's `+0x283c` (0.5 when not above 0).
    pub w854_64: f32,
    pub idle_scale: f32,
    /// The def's `+0x26f8` is set: the stall term is halved.
    pub half_stall: bool,
    /// NGP's mode word (`101767d4`): 8 runs the idle model when the table (`101767e8`) is made, else 2 adds the
    /// braking term to `Sim8E20`'s value.
    pub flags: u32,
    pub has_table: bool,
    /// NGP's per-car words: the braking factor (`1017234c`), the gain (`10171aa0`), the stall speed (`10171a9c`)
    /// and the idle band (`10171a98`), and the air density (`10171ae0`).
    pub braking: f32,
    pub gain: f32,
    pub stall_speed: f32,
    pub idle_band: f32,
    pub density: f32,
    /// `*(engine+8)+0x270`.
    pub friction: f32,
    /// The health object's words for [`damage_factor`].
    pub damage: [f32; 8],
}

/// The wrapper's value while the engine lives and runs, from `Sim8E20`'s `r`. `history` drops the oldest speed of
/// NGP's ring (`101767b0`), appends `speed` and says whether none is above the stall speed; `table` is the idle
/// table's lookup (rounded to f32).
pub fn torque(i: &TorqueIn, r: f32, history: impl FnOnce() -> bool, table: impl FnOnce(f64) -> f32) -> f32 {
    let dir = sign(i.speed) as f32;
    let x = dir * min_ss(i.limit, i.speed.abs());
    let braking = x / i.limit * -i.braking * i.gain;
    if i.flags & 8 == 0 || !i.has_table {
        return if i.flags & 2 != 0 { (1.0 - i.blend) * braking + r } else { r };
    }
    let stalled = history();
    if stalled && i.stall_speed > i.speed {
        let v = -dir * 0.1 * i.gain;
        return if i.half_stall { v * 0.5 } else { v };
    }
    let t = table(f64::from(x));
    let m = damage_factor(&i.damage) * (i.density / 1.204_071_9);
    let mut v = m * t * i.gain;
    if i.w854_64 > 0.0 {
        v *= if i.idle_scale > 0.0 { i.idle_scale } else { 0.5 };
    }
    v -= dir * i.friction;
    if i.idle_band > i.speed {
        if min_ss(104.719_76, i.idle_band * 0.95) > i.speed {
            v = min_ss(m * 0.05 * i.gain, v);
        }
        v + braking
    } else {
        if i.speed > i.limit {
            v = 0.0;
        }
        (1.0 - i.blend) * braking + v * i.blend
    }
}

/// The wrapper's st0: the value over 1.05, an x87 `fdiv` at the control word's precision (24 bits in the load).
pub fn result(v: f32) -> f64 {
    round_x87(f64::from(v) / f64::from(1.05f32), Variant::Ngp778)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`table_lookup`] on knot slots `-1..`, `xs[0]` and `ys[0]` being slot -1.
    fn lookup(n: usize, xs: &[f64], ys: &[f64], v: f64) -> f64 {
        table_lookup(n, |k| xs[(k + 1) as usize], |k| ys[(k + 1) as usize], v)
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
        assert_eq!(damage_factor(&[1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0]), 0.0);
        assert_eq!(damage_factor(&[0.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0]), 0.25);
    }
}
