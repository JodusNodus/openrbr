//! Force terms of the tyre step: `CScratch::Sign` (0x531ba0), `TyreClamp` (0x5322e0) and
//! `CScratch::Accumulate` (0x5338a0).

use crate::fpu::round_x87;
use crate::Variant;

use super::layout::SignInput;

/// `0.1` and `0.9`: the decay constants of the accumulators in `Accumulate`.
const K01: f32 = 0.1;
const K09: f32 = 0.9;

/// 0x531ba0 `CScratch::Sign`: a signed force term, `-(t · f314 · f3dc · sgn(spin))`, with
/// `t = f30c · f298 · 0.5` in f32 (the asm rounds it there, before the x87 part). `sgn` is +1, -1 or 0.
/// The original's name (`Sign`) is misleading: the result is a force term, not a sign.
/// x87 order: `((t · f314) · f3dc) · sgn`, each op rounded to 24 bits.
pub fn signed_term(input: SignInput, variant: Variant) -> f64 {
    let round24 = |v: f64| round_x87(v, variant);
    let t = input.tyre_factor * input.factor_298 * 0.5;
    let sign = if input.spin > 0.0 {
        1.0
    } else if 0.0 > input.spin {
        -1.0
    } else {
        0.0
    };
    let product = round24(round24(t as f64 * input.tyre_factor_2 as f64) * input.factor_3dc as f64);
    -round24(product * sign)
}

/// 0x5322e0 `TyreClamp`: clamps `a` against `t = 0 - b·c`, with the `0.001` gate on `b`. The branches
/// follow the asm, so NaN takes the same path.
pub fn tyre_clamp(a: f32, b: f32, c: f32) -> f32 {
    const GATE: f32 = 0.001;
    let t = 0.0 - b * c;
    if a > 0.0 {
        if a < t {
            if b > GATE {
                -1.0 / b * a
            } else {
                0.0
            }
        } else if 0.0 > t {
            0.0
        } else {
            c
        }
    } else if a > t {
        if b > GATE {
            -1.0 / b * a
        } else {
            0.0
        }
    } else if t > 0.0 {
        0.0
    } else {
        c
    }
}

/// The scratch and state values `Accumulate` reads (see `layout::load_accumulate`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AccumulateInput {
    /// `this+0x340` and `this+0x344`: the two ends of the lerp `(hi - lo)·w + lo` (`hi`, `lo`).
    pub blend_hi: f32,
    pub blend_lo: f32,
    /// `this+0x30c`: the tyre factor, multiplies `blend_hi` in `p4`.
    pub tyre_factor: f32,
    /// `this+0x314`: the second tyre factor, added to the output at `+0x314`.
    pub tyre_factor_2: f32,
    /// `this+0x238`: unknown meaning (a multiplier of `t1` and `q`).
    pub scale_238: f32,
    /// `this+0x368`, `this+0x36c`, `this+0x374`: the slip components and the longitudinal contact velocity.
    pub slip_x: f32,
    pub slip_y: f32,
    pub long_contact_vel: f32,
    /// `this+0x300`, `this+0x3b0`: unknown meaning (`g` and `k4` use them).
    pub scale_300: f32,
    pub scale_3b0: f32,
    /// `this+0x504`, `this+0x508`, `this+0x50c`: the three accumulators, decayed by 0.9.
    pub acc_504: f32,
    pub acc_508: f32,
    pub acc_50c: f32,
    /// The state's `R` (`m_state+0x4c`) and lerp weight (`m_state+0x58`).
    pub effective_radius: f32,
    pub lerp_weight: f32,
    /// The tyre setup's `+0xcc` (the value the original reads as `d33`).
    pub setup_cc: f32,
}

/// What `Accumulate` writes, by its scratch offset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Accumulated {
    /// `+0x35c`: `|z|·p` when `w < 0`, else 0.
    pub force_a: f32,
    /// `+0x358`: `|z|·p` when `w >= 0`, else 0.
    pub force_b: f32,
    /// `+0x350`: always `±0`: the original multiplies by 0.0 (rbr-recomp `notes/physics/tyre.md` S4).
    pub zero_350: f32,
    /// `+0x314`: `force_sum + tyre_factor_2`.
    pub factor_314: f32,
    /// `+0x348`: the force sum `s1`.
    pub force_sum: f32,
    /// `+0x354`: `t1`.
    pub force_t1: f32,
    /// `+0x34c`: `n · h`.
    pub nh: f32,
    /// `+0x504`, `+0x508`, `+0x50c`: the accumulators after the decay.
    pub acc_504: f32,
    pub acc_508: f32,
    pub acc_50c: f32,
}

/// 0x5338a0 `CScratch::Accumulate`: the tyre force and the heat accumulation. All SSE f32, in the asm's
/// operation order (MSVC keeps the `* 0.0` terms, which stay in place: `q·y·0.0` and `e1vz·0.0`).
pub fn accumulate(x: AccumulateInput) -> Accumulated {
    let abs = |v: f32| if v > 0.0 { v } else { 0.0 - v };
    let p4 = x.tyre_factor * x.blend_hi;
    let p = ((x.blend_hi - x.blend_lo) * x.lerp_weight + x.blend_lo) * x.setup_cc;
    let t1 = abs(x.slip_x) * p4;
    let x5 = x.scale_238 * t1;
    let w = x.long_contact_vel * x.slip_y;
    let q = abs(x.slip_y) * p * x.scale_238;
    let z = x.long_contact_vel;
    let (force_a, force_b) = if 0.0 > w { (abs(z) * p, 0.0) } else { (0.0, abs(z) * p) };
    let v = x.slip_x * x5;
    let vz = q * x.slip_y * 0.0;
    let h = 0.0 - v;
    let g = 1.0 - p4 / (x.effective_radius * 2.0);
    let k4 = (1.0 - x.scale_300) * x.scale_3b0;
    let m = 1.0 - k4;
    let n = 1.0 - m;
    let e1 = 1.0 - g;
    let nh = n * h;
    let e1vz = e1 * vz;
    let zero_350 = e1vz * 0.0;
    let force_sum = abs(h) * m + abs(vz) * g;
    Accumulated {
        force_a,
        force_b,
        zero_350,
        factor_314: force_sum + x.tyre_factor_2,
        force_sum,
        force_t1: t1,
        nh,
        acc_504: x.acc_504 * K09 + force_sum * K01,
        acc_508: m * K01 + x.acc_508 * K09,
        acc_50c: x.acc_50c * K09 + g * K01,
    }
}
