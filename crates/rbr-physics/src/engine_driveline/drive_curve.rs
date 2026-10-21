//! The drive's curve workers: the unsigned table lerp, the counter step and the row's interpolation. Each is a pure
//! function of its typed inputs; the rbr-hybrid bindings load and store the words. Their x87 chains round each
//! operation to the thread's precision (24 bits in 1.02, 53 under NGP); the lerps return the unrounded result and the
//! counters truncate it.

use super::layout::{CurveRow, CurveState, CurveTables};
use crate::fpu::round_x87;
use crate::math::abs_asm;
use crate::math::ftol::ftol2;

/// The scale [`count_down`] applies to the value.
const COUNT_SCALE: f32 = -99999997952.0;

/// A word read as unsigned, as the x87 loads it: the signed word (exact), plus `2^32` when it is negative.
fn unsigned(v: i32) -> f64 {
    if v < 0 {
        round_x87(f64::from(v) + 4294967296.0)
    } else {
        f64::from(v)
    }
}

/// The table lerp with unsigned operands. The fraction is `(a-b)*n/(c-b)`, its integer part the
/// segment `i` and the result `(1-u)*table[i] + u*table[i+1]` with `u` the fraction's rest. The callers keep `b <= a < c`,
/// so the segment is inside the table; a segment outside it panics.
pub fn unsigned_table_lerp(table: &[f32], a: i32, b: i32, c: i32, n: i32) -> f64 {
    let x87 = round_x87;
    let span = x87(unsigned(a.wrapping_sub(b)) * unsigned(n));
    let t = x87(span / unsigned(c.wrapping_sub(b)));
    let i = ftol2(t) as u32 as i32;
    let u = x87(t - unsigned(i));
    let i = i as usize;
    let low = x87(x87(1.0 - u) * f64::from(table[i]));
    x87(low + x87(u * f64::from(table[i + 1])))
}

/// Counts the state's counter down by the row's scaled value and returns `true`. At or past the
/// row's limit it returns `false` and leaves the counter alone. The value is the table `c` at the position: `c[0]` below
/// the lower break, the lerp to the upper break, and `c[9]` from there on.
pub fn count_down(
    dt: f32,
    row: &CurveRow,
    state: &mut CurveState,
    tables: &CurveTables,
    block_gain: f32,
    s: f32,
) -> bool {
    let counter = state.counter.value as u32;
    if counter >= row.limit {
        return false;
    }
    let pos = state.pos.value as u32;
    let v = if pos < row.breaks[3] {
        tables.c[0]
    } else if pos < row.breaks[1] {
        unsigned_table_lerp(&tables.c, pos as i32, row.breaks[3] as i32, row.breaks[1] as i32, 9) as f32
    } else {
        tables.c[9]
    };
    let x = row.rate * dt;
    let w = abs_asm(state.slope);
    // The x87 chain, in this order, truncated.
    let k = ftol2(
        [block_gain, s, 0.5, row.gain, x, v, COUNT_SCALE]
            .iter()
            .fold(f64::from(w), |p, &m| round_x87(p * f64::from(m))),
    ) as u32;
    state.counter.set(counter.wrapping_sub(k) as i32);
    true
}

/// Runs [`count_down`] on the row. If it returns `false`, the row's last value is the result.
/// Else the result interpolates the row's values over the position: `values[0]` below the first break, the lerp of
/// `values[0]` and `values[1]` by the table `a` up to the middle break, the lerp of `values[2]` and `values[1]` by the table
/// `b` up to the upper break, and `values[2]` from there on.
pub fn row_value(
    dt: f32,
    row: &CurveRow,
    state: &mut CurveState,
    tables: &CurveTables,
    block_gain: f32,
    s: f32,
) -> f64 {
    if !count_down(dt, row, state, tables, block_gain, s) {
        return f64::from(row.values[3]);
    }
    // x87: `(hi - lo) * t + lo` on the lerp's unrounded result.
    let blend = |hi: f32, lo: f32, t: f64| {
        let x87 = round_x87;
        x87(x87(x87(f64::from(hi) - f64::from(lo)) * t) + f64::from(lo))
    };
    let pos = state.pos.value as u32;
    if pos < row.breaks[0] {
        return f64::from(row.values[0]);
    }
    if pos < row.breaks[2] {
        let t = unsigned_table_lerp(&tables.a, pos as i32, row.breaks[0] as i32, row.breaks[2] as i32, 9);
        return blend(row.values[1], row.values[0], t);
    }
    if pos < row.breaks[1] {
        let t = unsigned_table_lerp(&tables.b, pos as i32, row.breaks[2] as i32, row.breaks[1] as i32, 9);
        return blend(row.values[1], row.values[2], t);
    }
    f64::from(row.values[2])
}
