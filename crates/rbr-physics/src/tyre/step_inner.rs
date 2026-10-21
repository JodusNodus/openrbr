//! 0x535310 `CScratch::StepInner`: one contact step of a tyre. It runs `SolveDirs` on the record's slip words, blends
//! the tables with `ComputeOutputs`, clamps two direction components, and calls `TyreFrictionCurve` twice: once for
//! each component. The original is thiscall with `ret 0x18`: `args[0]` is the record, `args[1]` a 3-float output,
//! `args[2]` a 4-float vector it zeroes, and `args[3]` to `args[5]` are float outputs.
//!
//! The frame's locals are named by what they hold (`E-0x..` in the asm). The prologue builds a 12-word table at
//! `E-0x2c`, which the two `TyreFrictionCurve` calls read. Every branch takes the asm's `ja`/`jbe` order (`a > b` or
//! `!(a > b)`), so a NaN takes the same path. Names are ours.

use super::curve::{friction_curve, CurveInput};
use super::outputs::{BlendPair, ComputeInput};
use super::slip::solve_dirs;
use crate::fpu::round_x87;
use crate::ngp;

/// The table the prologue stores at `E-0x2c`. Its twelfth word is the return address in the original; `friction_curve`
/// reads it only for a NaN index, and the result is NaN either way, so it is 0 here. NGP repoints words 1 to 4 (the
/// operands at `0x535317`, `0x535337`, `0x535346`, `0x53536a`) to its `1014fbc8..d4` (1.02: 0.99, 0.96, 0.87, 0.7).
const TABLE: [f32; 11] = [1.0, 0.95, 0.875, 0.775, 0.65, 0.5, 0.35, 0.2, 0.1, 0.05, 0.0];

/// The inputs of `StepInner`. The record words are read at `args[0]` (`rec_*`); `this_*` are the CScratch words,
/// `radius` is `m_state+0x4c`, `def_3c` is `m_def+0x3c`, and `weight` and `blend` are the `ComputeOutputs` tables.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepInput {
    pub rec_340: f32,
    pub rec_348: f32,
    pub rec_34c: f32,
    pub rec_358: f32,
    pub rec_1c8: f32,
    pub rec_1d8: f32,
    pub rec_2f4: f32,
    pub rec_36c: f32,
    pub rec_370: f32,
    pub rec_38c: f32,
    /// `this+0x30c`.
    pub factor_30c: f32,
    /// `this+0x340`.
    pub this_340: f32,
    /// `this+0x368` and `this+0x36c`: the two words `SolveDirs` takes.
    pub this_368: f32,
    pub this_36c: f32,
    pub radius: f32,
    pub def_3c: f32,
    pub weight: f32,
    pub blend: [BlendPair; 4],
}

/// What `StepInner` writes: `args[1]` (three floats), `args[3]`, `args[4]` and `args[5]`. `args[2]` is zeroed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepOutput {
    pub vec3: [f32; 3],
    pub arg3: f32,
    pub arg4: f32,
    pub arg5: f32,
}

/// `(a, c, c3)` of the directions: `a / c` while `c > a`, 2 when `c3 <= a`, and `(a - c) / (c3 - c) + 1` otherwise.
fn ratio(a: f32, c: f32, c3: f32) -> f32 {
    if c > a {
        a / c
    } else if !(c3 > a) {
        2.0
    } else {
        (a - c) / (c3 - c) + 1.0
    }
}

/// 0x535310 `CScratch::StepInner`. Its `ComputeOutputs` is NGP's wrapper (`ngp::outputs`), and its x87 chains (and
/// those of its callees) run at 53 bits.
pub fn step_inner(x: &StepInput) -> StepOutput {
    let round24 = round_x87;
    let table = |i: u32| TABLE.get(i as usize).copied().unwrap_or(0.0);
    // SolveDirs: the first two slip directions. Its third output is overwritten by the sign below, so it is unused.
    let [p1, p2, _] = solve_dirs(x.rec_358, x.this_368, x.this_36c, x.radius);
    let out = ngp::outputs::compute_outputs(&ComputeInput {
        weight: x.weight,
        table_244: x.blend[0],
        table_264: x.blend[1],
        table_284: x.blend[2],
        table_2a4: x.blend[3],
        scale_1: p1,
        scale_2: p2,
        scale_4: x.rec_1c8 * x.def_3c,
        scale_5: x.rec_1d8 * x.def_3c,
        raw: [x.rec_36c.to_bits(), x.rec_370.to_bits()],
    });

    // The x component: clamped to [-1, 1], then split into its sign and magnitude.
    let x0 = x.rec_348 / x.rec_340 - x.rec_38c * 0.1;
    let x_dir = if -1.0 > x0 {
        -1.0
    } else if x0 > 1.0 {
        1.0
    } else {
        x0
    };
    let neg_x = 0.0 > x_dir;
    let ax = if neg_x { 0.0 - x_dir } else { x_dir };

    // The y component: zero when the record's `+0x34c` is within 0.001 of zero (or NaN).
    let y = if x.rec_34c > 0.001 || -0.001 > x.rec_34c { (0.0 - x.radius * x.rec_358) / x.rec_34c - 1.0 } else { 0.0 };
    let neg_y = 0.0 > y;
    let ay = if neg_y { 0.0 - y } else { y };
    let sign_34c = if 0.0 > x.rec_34c { -1.0 } else { 1.0 };

    // The ratios of the two components to the blended C and D; the third of each clamps at 3 * C or 3 * D.
    let (c, c3, d, d3) = (out.out13, out.out14, out.out15, out.out16);
    let rho = ratio(ax, c, c3);
    let yv = ratio(ay, d, if neg_y { 1.0 } else { d3 });

    // The norm of (yv, rho). Above 0.01 it rescales C, D, and two products.
    let sq = round24(round24(f64::from(yv) * f64::from(yv)) + round24(f64::from(rho) * f64::from(rho)));
    let norm = round24(sq.sqrt()) as f32;
    let (mut c_new, mut d_new) = (c, d);
    let (mut out9, mut out11) = (out.out9, out.out11);
    if norm > 0.01 {
        let inv = 1.0 / norm;
        let k = inv * yv;
        let ir = inv * rho;
        c_new = c3 - (c3 - c) * ir;
        d_new = d3 - (d3 - d) * k;
        let b = k * out.out11;
        let a9 = ir * out.out9;
        out9 = if out.out10 > a9 { out.out10 } else { a9 };
        out11 = if out.out12 > b { out.out12 } else { b };
    }

    // `W` is the clamp of `this+0x340 * 100/3` to [0, 1]. `F1` and `H` are two blends of it.
    let z = x.this_340 * 33.333336;
    let w = if 0.0 > z {
        0.0
    } else if z > 1.0 {
        1.0
    } else {
        z
    };
    let f1 = round24(round24(f64::from(w) * f64::from(-0.083333343_f32)) + f64::from(0.33333334_f32)) as f32;
    let h = round24(round24(f64::from(w) * f64::from(-0.15_f32)) + f64::from(0.05_f32)) as f32;

    // TyreFrictionCurve for the x component. Its output scales the record's factor.
    let curve1 = friction_curve(
        &CurveInput { args: [ax, c_new, c3, out9, out.out10, f32::from_bits(out.raw[0]), x.rec_2f4, f1, h], count: 10 },
        table,
    );
    let v1 = curve1.value as f32;
    let v1s = if neg_x { 0.0 - v1 } else { v1 };
    let w1 = curve1.out9;
    let arg3 = x.factor_30c * w1 * 0.5;

    // TyreFrictionCurve for the y component. Its third input is D3, or 1 when the component is negative.
    let curve2 = friction_curve(
        &CurveInput {
            args: [
                ay,
                d_new,
                if neg_y { 1.0 } else { d3 },
                out11,
                out.out12,
                f32::from_bits(out.raw[1]),
                x.rec_2f4,
                f1,
                h,
            ],
            count: 10,
        },
        table,
    );
    let v2 = curve2.value as f32;
    let v2s = if neg_y { -v2 } else { v2 };

    StepOutput {
        vec3: [0.0 - v1s * x.rec_2f4, v2s * sign_34c * x.rec_2f4, 0.0],
        arg3,
        arg4: curve1.out10,
        arg5: curve2.out10,
    }
}
