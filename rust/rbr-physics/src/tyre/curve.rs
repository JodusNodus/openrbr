//! The tyre friction curve: `CScratch::TyreFrictionCurve` (0x5334d0) and its table interpolation `FloorS32`
//! (0x5325f0). The original is stdcall with 13 stack words. `args[0..9]` are the values, `args[9]` and
//! `args[10]` are the two pointers it writes, and `args[11]` (table) and `args[12]` (count) are read by
//! `floor_s32` only.
//!
//! The branches follow the asm: each comparison is the `ja`/`jbe` the original takes (`a > b` or `!(a > b)`),
//! so a NaN takes the same path. The names are ours: the original's strings and RTTI do not name these values.

use crate::fpu::{round_x87, trunc_i32, INT_INDEFINITE};
use crate::Variant;

/// `2^31`, the index's bound (f32, as the asm compares it), and `2^32`, the offset of a negative count.
const TWO_31: f32 = 2147483648.0;
const TWO_32: f64 = 4294967296.0;

/// The values of `TyreFrictionCurve`: `args[k]` is stack word `k` as an f32, and `count` is `args[12]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurveInput {
    pub args: [f32; 9],
    pub count: i32,
}

/// What `TyreFrictionCurve` writes through `args[9]` and `args[10]`, and returns in st0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurveOutput {
    pub value: f64,
    pub out9: f32,
    pub out10: f32,
}

/// The index `FloorS32` takes from `p`: `cvttss2si`, clamped as the original does. From `2^31` up it is
/// `i32::MAX`. Below `-2^31` and for NaN it is the integer indefinite (the original also logs, which has no
/// effect on the result).
fn index_of(p: f32) -> i32 {
    if p >= TWO_31 {
        i32::MAX
    } else if p >= -TWO_31 {
        trunc_i32(f64::from(p))
    } else {
        INT_INDEFINITE
    }
}

/// An int read by `fild` as unsigned: a negative value gets `2^32` added (rounded to 24 bits by the x87).
fn unsigned_f64(v: i32, variant: Variant) -> f64 {
    let round24 = |v: f64| round_x87(v, variant);
    if v < 0 {
        round24(f64::from(v) + TWO_32)
    } else {
        f64::from(v)
    }
}

/// 0x5325f0 `FloorS32`: the table `table(i)` (`a[i]`) interpolated at `p = x * n`, where `n` is read as unsigned.
/// It returns st0.
pub fn floor_s32(x: f32, n: i32, table: impl Fn(u32) -> f32, variant: Variant) -> f64 {
    let round24 = |v: f64| round_x87(v, variant);
    let p = round24(unsigned_f64(n, variant) * f64::from(x)) as f32;
    let k = (index_of(p) as u32).min(n as u32);
    let frac = round24(f64::from(p) - unsigned_f64(k as i32, variant));
    let low = round24(round24(1.0 - frac) * f64::from(table(k)));
    let high = round24(frac * f64::from(table(k + 1)));
    round24(low + high)
}

/// 0x5334d0 `CScratch::TyreFrictionCurve`.
pub fn friction_curve(x: &CurveInput, table: impl Fn(u32) -> f32, variant: Variant) -> CurveOutput {
    let round24 = |v: f64| round_x87(v, variant);
    let a = x.args;
    let x0 = a[3] * a[6] / a[5] * 1.2;
    // The larger of `x0` and `a[1]`, where `x0` is first tripled when it is below `a[1]`.
    let m = if x0 > a[1] {
        x0
    } else {
        let t = x0 * 3.0;
        if a[1] > t {
            t
        } else {
            a[1]
        }
    };
    if m > a[0] {
        let q = a[0] / m;
        let r = (m / (a[3] * a[6])) * a[5];
        let p = (3.0 - r * 2.0) + (r - 2.0) * q;
        let v = (p * (q * q) + q * r) * a[3];
        CurveOutput { value: f64::from(v), out9: (1.0 - q) * a[7] + q * a[8], out10: q }
    } else if a[2] > a[0] {
        let s = (a[0] - m) / (a[2] - m);
        let f = floor_s32(s, x.count, table, variant);
        let g = round24(round24(f64::from(a[3]) - f64::from(a[4])) * f);
        CurveOutput { value: round24(g + f64::from(a[4])), out9: round24(f * f64::from(a[8])) as f32, out10: s + 1.0 }
    } else {
        CurveOutput { value: f64::from(a[4]), out9: 0.0, out10: 2.0 }
    }
}
