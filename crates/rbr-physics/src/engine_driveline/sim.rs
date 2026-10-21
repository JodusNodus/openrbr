//! The engine's table kernels: a table lerp, a split of a unit fraction, a curve lookup and the blends of knot
//! tables. `tests/engine_driveline_vectors.rs` checks each against recorded calls of the original. The float
//! operations are f32: the x87 at 24-bit precision rounds each operation to f32, so a chain of them is the same as
//! the f32 operations in the same order. Under NGP the x87 runs at 53 bits, so the lerps' x87 chains are
//! [`fpu::round_x87`] steps and their results stay unrounded (`f64`).

use super::layout::{CurveKnot, EngineDef, EngineOptions, EngineState, UnitSplit};
use crate::fpu;

/// A linear lookup in `table` (knots spread evenly over `lo..hi`). The fraction
/// `f = (x - lo) * (n - 1) / (hi - lo)` picks the two knots around it. Below the first knot the first value
/// is returned, and from the last knot on (`n - 2 < i`) the last value. `table` must not be empty.
pub fn table_lerp(table: &[f32], x: f32, lo: f32, hi: f32) -> f64 {
    let n = table.len() as i32;
    // f32: (x - lo) * (n - 1), then divided by (hi - lo).
    let f = (x - lo) * (n - 1) as f32 / (hi - lo);
    // f >= 2^31 gives i32::MAX; NaN and f < -2^31 give i32::MIN, as the hardware conversion does.
    let i = if f >= 2147483648.0 { i32::MAX } else { fpu::trunc_i32(f as f64) };
    if n - 2 < i {
        return f64::from(table[table.len() - 1]);
    }
    if i < 0 {
        return f64::from(table[0]);
    }
    let i = i as usize;
    // The weight from the integer index, then the blend.
    let x87 = fpu::round_x87;
    let w = x87(f64::from(f) - f64::from(i as i32));
    let low = x87(x87(1.0 - w) * f64::from(table[i]));
    x87(low + x87(w * f64::from(table[i + 1])))
}

/// The split of a unit fraction `x` into weights. At 0 it is the "at zero" case. Up to 0.5 the weights are
/// `(0, 2x, 1 - 2x)`. Above 0.5 they are `(2x - 1, 2 - 2x, 0)`. `UnitSplit::store` writes the record.
pub fn split_unit_fraction(x: f32) -> UnitSplit {
    if x == 0.0 {
        return UnitSplit { at_zero: 1.0, w_low: 0.0, w_mid: 0.0, w_high: 0.0 };
    }
    let f1 = x * 2.0;
    if 0.5 < x {
        let f2 = 1.0 - (f1 - 1.0);
        return UnitSplit { at_zero: 0.0, w_low: f1 - 1.0, w_mid: f2, w_high: 0.0 };
    }
    UnitSplit { at_zero: 0.0, w_low: 0.0, w_mid: f1, w_high: 1.0 - f1 }
}

/// The value of a curve at `x`. Keys ascend. Below the first key the first value is
/// returned, and past the last key (`last < x`) the last value. Between, the knot pair around `x` is found and
/// blended: `f * value(k - 1) + (1 - f) * value(k)`, where `f = 1 - (x - key(k - 1)) / (key(k) - key(k - 1))`.
/// A NaN `x` takes the search. The curve needs at least two knots.
pub fn curve_eta(curve: &[CurveKnot], x: f32) -> f64 {
    let n = curve.len();
    if x < curve[0].key {
        return f64::from(curve[0].value);
    }
    let last = curve[n - 1];
    if last.key <= x && x != last.key {
        return f64::from(last.value);
    }
    // The first knot k >= 1 with `!(x > key)`. The search cannot run past the last knot: the check above excludes it.
    let k = (1..n).find(|&k| !(x > curve[k].key)).unwrap_or(n);
    let (lo, hi) = (curve[k - 1], curve[k]);
    let x87 = fpu::round_x87;
    let along = x87(f64::from(x) - f64::from(lo.key));
    let span = x87(f64::from(hi.key) - f64::from(lo.key));
    let f = x87(1.0 - x87(along / span));
    let upper = x87(x87(1.0 - f) * f64::from(hi.value));
    x87(upper + x87(f * f64::from(lo.value)))
}

/// The result of [`find_bracket`]: the knot pair around `x` and its weights. `hi` is the knot at or above
/// `x` (the search's index, or the last knot), `lo` = hi - 1. `w_lower` is `1 - t` and `w_upper` is `t`, where `t`
/// is the fraction of the way from the lower knot to the upper one, clamped to 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bracket {
    pub hi: i32,
    pub lo: i32,
    pub w_upper: f32,
    pub w_lower: f32,
}

/// The bracket of `x` in the ascending `keys` (at least two). The search starts at knot 1 and stops at the first
/// knot with `x <= key`, or at the last one (`count - 1`) when none takes x. A NaN `x` stops at the first knot.
pub fn find_bracket(keys: &[f32], x: f32) -> Bracket {
    let count = keys.len();
    let mut hi = 1;
    if count > 2 {
        loop {
            if x <= keys[hi] || x.is_nan() {
                break;
            }
            hi += 1;
            if hi >= count - 1 {
                break;
            }
        }
    }
    let lo = hi - 1;
    let g = 1.0 - (x - keys[lo]) / (keys[hi] - keys[lo]);
    let h = 1.0 - g;
    // Clamped: a fraction below 0 gives all weight to the lower knot, above 1 all of it to the upper one. The
    // `h <= 1` test is true for NaN, so a NaN fraction is stored as it is (both weights NaN).
    let (w_upper, w_lower) = if h.is_nan() || h <= 1.0 {
        if h < 0.0 {
            (0.0, 1.0)
        } else {
            (h, g)
        }
    } else {
        (1.0, 0.0)
    };
    Bracket { hi: hi as i32, lo: lo as i32, w_upper, w_lower }
}

/// The absolute rpm term. The value is `abs_scale * speed`, made non-negative (`0 - t` when `!(t > 0)`, so NaN and
/// zero take that path too). It is stored with the dirty flag
/// set. The result is the value when the option gate is set and the speed is above zero, else 0.
pub fn abs_rpm_term(def: &EngineDef, state: &mut EngineState, options: &EngineOptions) -> f32 {
    let t = def.abs_scale * state.speed;
    let t = if !(t > 0.0) { 0.0 - t } else { t };
    state.abs_value_dirty = 1;
    state.abs_value = t;
    if options.gate_58 != 0.0 && 0.0 < state.speed {
        t
    } else {
        0.0
    }
}

/// One record of a blend table: the key and two values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendRec {
    pub key: f32,
    pub y1: f32,
    pub y2: f32,
}

/// The combined key of record `k` of the two tables (the search's comparison: `b * d + c * e`).
fn blend_key(b: &[BlendRec], c: &[BlendRec], k: usize, d: f32, e: f32) -> f32 {
    b[k].key * d + c[k].key * e
}

/// The blend of two tables of the same count, at the combined key `a`. The search walks the
/// records from the first one (`b * d + c * e` of each key) while the key is above `a` (`!(key > a)` stops it, so
/// NaN stops it at once), or to the last one. The bracket's records `k - 1` and `k` are mixed with the weight `w` (1
/// at the lower record's end, clamped to 0..1, NaN to 0). Returns `(f, g, h)` with `h = 1` when there is no bracket
/// (one record, or `a` is at or below the first key), and the first record's values are the result then.
pub fn blend_tables_checked(a: f32, d: f32, e: f32, b: &[BlendRec], c: &[BlendRec]) -> (f32, f32, u32) {
    let count = b.len();
    if count != 1 {
        let mut k = 0;
        let mut key = blend_key(b, c, 0, d, e);
        let mut below = 0.0;
        loop {
            let here = key;
            if !(here > a) {
                break;
            }
            k += 1;
            key = blend_key(b, c, k, d, e);
            below = here;
            if k >= count - 1 {
                break;
            }
        }
        if k != 0 {
            let mut w = 1.0 - (a - below) / (key - below);
            w = if 0.0 <= w {
                if 1.0 < w {
                    1.0
                } else {
                    w
                }
            } else {
                0.0
            };
            let rest = 1.0 - w;
            let (lo, hi) = (b[k - 1], b[k]);
            let (lo_c, hi_c) = (c[k - 1], c[k]);
            let f = (lo_c.y1 * e + lo.y1 * d) * w + (hi_c.y1 * e + hi.y1 * d) * rest;
            let g = (hi_c.y2 * e + hi.y2 * d) * rest + (lo_c.y2 * e + lo.y2 * d) * w;
            return (f, g, 0);
        }
    }
    let (b0, c0) = (b[0], c[0]);
    (b0.y1 * d + c0.y1 * e, b0.y2 * d + c0.y2 * e, 1)
}

/// [`blend_tables_checked`] with the search's test `!(a > key)` (NaN stops it) and the weight clamped as
/// `!(w < 0)` (NaN kept). With no bracket it returns `(0, 0.5)`, the weight's start value.
pub fn blend_tables(a: f32, d: f32, e: f32, b: &[BlendRec], c: &[BlendRec]) -> (f32, f32) {
    let count = b.len();
    let mut w = 0.5;
    if count != 1 {
        let mut k = 0;
        let mut key = blend_key(b, c, 0, d, e);
        let mut below = 0.0;
        loop {
            let here = key;
            if !(a > here) {
                break;
            }
            k += 1;
            key = blend_key(b, c, k, d, e);
            below = here;
            if k >= count - 1 {
                break;
            }
        }
        if k != 0 {
            w = 1.0 - (a - below) / (key - below);
            w = if !(w < 0.0) {
                if 1.0 < w {
                    1.0
                } else {
                    w
                }
            } else {
                0.0
            };
            let rest = 1.0 - w;
            let (lo, hi) = (b[k - 1], b[k]);
            let (lo_c, hi_c) = (c[k - 1], c[k]);
            let f = (hi_c.y1 * e + hi.y1 * d) * rest + (lo_c.y1 * e + lo.y1 * d) * w;
            let g = (hi_c.y2 * e + hi.y2 * d) * rest + (lo_c.y2 * e + lo.y2 * d) * w;
            return (f, g);
        }
    }
    (0.0, w)
}
