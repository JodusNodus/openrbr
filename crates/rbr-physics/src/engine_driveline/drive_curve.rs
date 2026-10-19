//! The E39 curve workers (`RBR_E39_DriveF.cpp`, 0x503e00-0x504160): the table lerp `TabE00`, the counter step `WorkerF60`
//! and the row's interpolation `Worker4060`. Each is a pure function of its typed inputs; the rbr-hybrid bindings load and
//! store the words. Their x87 chains round each operation to the variant's precision (24 bits in 1.02, 53 under NGP); the
//! lerps return st0 and the counters truncate st0.

use super::layout::{CurveInput, CurveRow, CurveState, CurveTables, HookBlock};
use crate::fpu::round_x87;
use crate::math::abs_asm;
use crate::math::ftol::ftol2;
use crate::Variant;

/// The original's constant at 0x73a130 that `WorkerF60` scales the value by.
const COUNT_SCALE: f32 = -99999997952.0;

/// A word read as unsigned, as the x87 loads it: `fild` of the signed word (exact), plus `2^32` when it is negative.
fn unsigned(v: i32, variant: Variant) -> f64 {
    if v < 0 {
        round_x87(f64::from(v) + 4294967296.0, variant)
    } else {
        f64::from(v)
    }
}

/// `E39_TabE00` (0x503e00): the table lerp with unsigned operands. The fraction is `(a-b)*n/(c-b)`, its integer part the
/// segment `i` and the result `(1-u)*table[i] + u*table[i+1]` with `u` the fraction's rest. The callers keep `b <= a < c`,
/// so the segment is inside the table; a segment outside it panics.
pub fn unsigned_table_lerp(table: &[f32], a: i32, b: i32, c: i32, n: i32, variant: Variant) -> f64 {
    let x87 = |v: f64| round_x87(v, variant);
    let span = x87(unsigned(a.wrapping_sub(b), variant) * unsigned(n, variant));
    let t = x87(span / unsigned(c.wrapping_sub(b), variant));
    let i = ftol2(t) as u32 as i32;
    let u = x87(t - unsigned(i, variant));
    let i = i as usize;
    let low = x87(x87(1.0 - u) * f64::from(table[i]));
    x87(low + x87(u * f64::from(table[i + 1])))
}

/// `E39_WorkerF60` (0x503f60): counts the state's counter down by the row's scaled value and returns `true`. At or past the
/// row's limit it returns `false` and leaves the counter alone. The value is the table `c` at the position: `c[0]` below
/// the lower break, the lerp to the upper break, and `c[9]` from there on.
pub fn count_down(
    dt: f32,
    row: &CurveRow,
    state: &mut CurveState,
    tables: &CurveTables,
    block_gain: f32,
    s: f32,
    variant: Variant,
) -> bool {
    let counter = state.counter.value as u32;
    if counter >= row.limit {
        return false;
    }
    let pos = state.pos.value as u32;
    let v = if pos < row.breaks[3] {
        tables.c[0]
    } else if pos < row.breaks[1] {
        unsigned_table_lerp(&tables.c, pos as i32, row.breaks[3] as i32, row.breaks[1] as i32, 9, variant) as f32
    } else {
        tables.c[9]
    };
    let x = row.rate * dt;
    let w = abs_asm(state.slope);
    // The asm's x87 chain, in its order, truncated from st0.
    let k = ftol2(
        [block_gain, s, 0.5, row.gain, x, v, COUNT_SCALE]
            .iter()
            .fold(f64::from(w), |p, &m| round_x87(p * f64::from(m), variant)),
    ) as u32;
    state.counter.set(counter.wrapping_sub(k) as i32);
    true
}

/// `E39_Worker4060` (0x504060): runs `count_down` on the row. If it returns `false`, the row's last value is the result.
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
    variant: Variant,
) -> f64 {
    if !count_down(dt, row, state, tables, block_gain, s, variant) {
        return f64::from(row.values[3]);
    }
    // x87: `(hi - lo) * t + lo` on the lerp's st0.
    let blend = |hi: f32, lo: f32, t: f64| {
        let x87 = |v: f64| round_x87(v, variant);
        x87(x87(x87(f64::from(hi) - f64::from(lo)) * t) + f64::from(lo))
    };
    let pos = state.pos.value as u32;
    if pos < row.breaks[0] {
        return f64::from(row.values[0]);
    }
    if pos < row.breaks[2] {
        let t = unsigned_table_lerp(&tables.a, pos as i32, row.breaks[0] as i32, row.breaks[2] as i32, 9, variant);
        return blend(row.values[1], row.values[0], t);
    }
    if pos < row.breaks[1] {
        let t = unsigned_table_lerp(&tables.b, pos as i32, row.breaks[2] as i32, row.breaks[1] as i32, 9, variant);
        return blend(row.values[1], row.values[2], t);
    }
    f64::from(row.values[2])
}

/// `E39_Hook4160` (0x504160): the car's speed term. The term is `e = e_scale * x.w60 + term_gain * x.w70`, zero when the
/// gate is clear. `row_value` runs on `w = dt_scale * e`, giving `r`, and the value is `aval = gain * r * w`. The position
/// and the lane each count down by `-1e6 * num / den` (`num` the speed and position terms, `den` the block's
/// denominators) and are then set to 500000000. The asm writes the counted words before the final ones, so they are
/// set here too.
pub fn wheel_curve_term(
    gate: u32,
    block: &HookBlock,
    row: &CurveRow,
    tables: &CurveTables,
    state: &mut CurveState,
    input: &CurveInput,
    s: f32,
    variant: Variant,
) -> f32 {
    let x87 = |v: f64| round_x87(v, variant);
    let e = block.e_scale * input.w60 + state.term_gain * input.w70;
    let dt = if gate != 0 { e } else { 0.0 };
    let w = block.dt_scale * dt;
    let r = row_value(w, row, state, tables, block.gain, s, variant) as f32;
    let aval = block.gain * r * w;
    // `fild` (plus 2^32), times 1e-6 (`0x795e70`), stored.
    let micro = |v: i32| x87(unsigned(v, variant) * f64::from(1.0e-6f32)) as f32;
    let d = micro(state.lane.value);
    let r2 = micro(state.pos.value);
    // SSE: p, then x1 (the position term), y and x2 (the lane's terms).
    let p = block.p_scale * 2.0;
    let x1 = (d - r2) * block.x1_scale * p;
    let y = (293.15 - d) * p * 3.0;
    let x2 = (293.15 - r2) * p * 2.0;
    // x87 from here, each count truncated from st0.
    let mul = |a: f64, b: f32| x87(a * f64::from(b));
    let num = mul(mul(f64::from(abs_asm(state.slope)), block.w_scale), aval);
    let num = mul(x87(x87(num + f64::from(x2)) + f64::from(x1)), s);
    let den = mul(mul(f64::from(block.den_a), block.den_b), 0.04);
    let k1 = ftol2(mul(x87(num / den), -1.0e6)) as u32;
    state.pos.set((state.pos.value as u32).wrapping_sub(k1) as i32);
    let num2 = mul(x87(f64::from(y) - f64::from(x1)), s);
    let den2 = mul(mul(f64::from(block.den_b), block.den_a), 0.96);
    let k2 = ftol2(mul(x87(num2 / den2), -1.0e6)) as u32;
    state.lane.set((state.lane.value as u32).wrapping_sub(k2) as i32);
    state.pos.set(500_000_000);
    state.lane.set(500_000_000);
    aval
}
