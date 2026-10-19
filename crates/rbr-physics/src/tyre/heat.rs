//! 0x534090 `CScratch::HeatStep(a, b, c)`: the tyre's heat and wear step. It forms the heat flows `e`, `n`, `h`
//! from the arguments, moves the table and the two damper blocks (`DamperAccumA`), and adds the wear term.
//! The branches and the rounding follow the asm, not `src/rbr/RBR_E49_TyreSim.cpp` (the C++ differs in
//! places, e.g. its `g <= 0.4` test and its NaN handling of the clamps). The NaN-sensitive comparisons are
//! written as the `jbe` they compile to, and each x87 chain rounds to 24 bits (`round24`). The SSE part is f32.
//!
//! The asm stores `e - n` as an f32 before the x87 expression that uses it.

use crate::fpu::round_x87;
use crate::ngp::damper::{step1 as ngp_step1, Thermal};
use crate::spring_damper::{accumulate_a, step1 as damper_step1, AccumulationFactors};
use crate::Variant;

/// The inputs of `HeatStep` (see `layout::load_heat`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeatInput {
    /// The arguments `a`, `b`, `c`.
    pub a: f32,
    pub b: f32,
    pub c: f32,
    /// `this+0x288` (`f[0xA2]`): the heat source; the step uses 3 times it.
    pub heat_source: f32,
    /// `this+0x368` (`f[0xDA]`) and `this+0x374` (`f[0xDD]`): the factors of `a` and `b`; their squares sum to `ss`.
    pub f_da: f32,
    pub f_dd: f32,
    /// `this+0x378` (`f[0xDE]`) and `this+0x3c8` (`f[0xF2]`): the factors of `k`.
    pub f_de: f32,
    pub f_f2: f32,
    /// `this+0x310` (`f[0xC4]`): the factor of the wear term.
    pub f_c4: f32,
    /// `this+0x360` (`f[0xD8]`): the speed.
    pub speed: f32,
    /// `this+0x388` (`f[0xE2]`): minus 1 is the contact factor, clamped to [0, 1].
    pub f_e2: f32,
    /// `this+0x308` (`f[0xC2]`): the last argument of `DamperStep1`.
    pub f_c2: f32,
    /// `m_state+0xd0+12·idx`: the heat table entry of the tyre's index `idx` (`this+0x3c0`).
    pub tab: f32,
    /// `m_state+0x12c` (`stf[0x4B]`) and `m_state+0x130` (`stf[0x4C]`): the two damper states.
    pub state_12c: f32,
    pub state_130: f32,
    /// `m_state+0xb0` (`stf[0x2C]`): the blend weight of `shape_a` and `shape_b`.
    pub state_b0: f32,
    /// `m_state+0xc8` (`stf[0x32]`): the blend weight of `mix_a` and `mix_b`.
    pub state_c8: f32,
    /// `m_def[0x5B + sti[0x30]]` and `m_def[0x5B + sti[0x31]]`: the mix factors of the spring argument.
    pub mix_a: f32,
    pub mix_b: f32,
    /// `m_def[0x71 + sti[0x2A]]` and `m_def[0x71 + sti[0x2B]]`: the shape factors of the contact term.
    pub shape_a: f32,
    pub shape_b: f32,
    /// `m_def+0x118` (`dff[0x46]`): the time constant.
    pub def_118: f32,
    /// The three `DamperAccumA` blocks `s = [s0, s1]`: `m_def+0x110`, `+0x11c`, `+0x128`.
    pub damper: [AccumulationFactors; 3],
    /// `m_wear[idx]`: the wear, capped at 1.
    pub wear: f32,
}

/// What `HeatStep` writes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeatOutput {
    /// `this+0x3c4` (`f[0xF1]`): `heat + 2`.
    pub heat_t: f32,
    /// `this+0x4e4` (`f[0x139]`): `k·0.5 + |b| + |a|`.
    pub sum_ab: f32,
    /// `this+0x4ec` (`f[0x13B]`): `1 / (heat_t / def118 + 1)`.
    pub t_inv: f32,
    /// `this+0x4e0` (`f[0x138]`): the damper term `(R·c)/c`, stored as f32.
    pub r: f32,
    /// `this+0x404` (`f[0x101]`): the heat flow, or 0 when it is below 0.05 or `ss` is at most 0.4.
    pub flow: f32,
    /// `m_state+0xd0+12·idx` after `DamperAccumA`.
    pub tab: f32,
    /// `m_state+0x12c` and `+0x130` after `DamperAccumA`.
    pub state_12c: f32,
    pub state_130: f32,
    /// `m_wear[idx]` after the wear term, capped at 1. Unchanged when `ss` is at most 0.4.
    pub wear: f32,
}

fn table_factor(g: f32, t_inv: f32, variant: Variant) -> f64 {
    // The original multiplies by the f32 constant `-0.60000002f` after promoting it to x87 precision.
    let round = |v: f64| round_x87(v, variant);
    round(round(round(g as f64 * f64::from(-0.6_f32)) + 1.0) * t_inv as f64)
}

/// 0x534090 `CScratch::HeatStep`; `ngp` is NGP's state, `None` for 1.02. Under NGP its temperature threshold is 40
/// (NGP's `1014fbc0`, 60 in 1.02), its `DamperStep1` is NGP's (`0x1004d2f0`, SSE, against the air temperature; the call
/// at `0x53428d`), and its x87 chains run at 53 bits.
pub fn heat_step(x: HeatInput, ngp: Option<&Thermal>) -> HeatOutput {
    let variant = Variant::of(ngp);
    let round24 = |v: f64| round_x87(v, variant);
    let hot = if ngp.is_some() { 40.0 } else { 60.0 };
    let abs = |v: f32| if v > 0.0 { v } else { 0.0 - v };
    let heat = x.heat_source * 3.0;
    let heat_t = heat + 2.0;
    let a = abs(x.f_da * x.a);
    let b = abs(x.b * x.f_dd);
    let k = abs(x.f_f2 * x.f_de);
    let sum_ab = ((k * 0.5) + b) + a;
    // Heat flows: e, n = e·0.1, q = n·0.2, h1 = h - e, m = h1·0.4, and h1·0.6 is the last part of `p`.
    let h = sum_ab * x.c;
    let e = h * 0.7;
    let h1 = h - e;
    let n = e * 0.1;
    let e_minus_n = e - n;
    let q = n * 0.2;
    let m = h1 * 0.4;
    let p = (n - q) + h1 * 0.6;
    // g: the temperature term, clamped to [0, 1] (NaN passes through, as in the asm).
    let g = ((((x.state_12c + x.tab) * 0.5) - 273.15) - hot) * 0.0125;
    let g = if 0.0 > g {
        0.0
    } else if g > 1.0 {
        1.0
    } else {
        g
    };
    let t_inv = 1.0 / (heat_t / x.def_118 + 1.0);

    // The spring argument and the DamperStep1 term (x87 chains at 24 bits).
    let mix = round24(
        round24(round24(1.0 - x.state_c8 as f64) * x.mix_a as f64) + round24(x.mix_b as f64 * x.state_c8 as f64),
    );
    let factor = round24(round24(g as f64 * 4500.0) + 1500.0);
    let arg = round24(round24(mix * factor) * round24(heat as f64 + 1.0));
    let r_step = match ngp {
        None => damper_step1(x.tab, 303.0, arg as f32, x.f_c2, Variant::Stock102),
        Some(n) => f64::from(ngp_step1(x.tab, 303.0, arg as f32, x.f_c2, n.temperature)),
    };
    let r_times_c = round24(r_step * x.c as f64);
    let r = round24(r_times_c / x.c as f64) as f32;

    // The table input: ((g·-0.6 + 1)·t·(e - n) + q) - R·c, as f32.
    let factor_t = table_factor(g, t_inv, variant);
    let table_in = round24(round24(round24(factor_t * e_minus_n as f64) + q as f64) - r_times_c) as f32;

    let mut tab = x.tab;
    accumulate_a(x.damper[0], table_in, &mut tab);
    let mut state_12c = x.state_12c;
    accumulate_a(x.damper[1], p, &mut state_12c);
    let mut state_130 = x.state_130;
    accumulate_a(x.damper[2], m, &mut state_130);

    let mut out = HeatOutput { heat_t, sum_ab, t_inv, r, flow: 0.0, tab, state_12c, state_130, wear: x.wear };
    let ss = (x.f_da * x.f_da) + (x.f_dd * x.f_dd);
    if !(ss > 0.4) {
        return out;
    }
    // The wear term needs the contact factor (clamped to [0, 1]), the blend `w`, and `sqrt(ss)` (x87 at 24 bits).
    let root = round24((ss as f64).sqrt()) as f32;
    let d = x.f_e2 - 1.0;
    let d = if 0.0 > d {
        0.0
    } else if d > 1.0 {
        1.0
    } else {
        d
    };
    let w = (1.0 - x.state_b0) * x.shape_a + x.shape_b * x.state_b0;
    let contact = (x.f_c4 * d) * w;
    let z = (contact * root) * x.c;
    let f4 = if x.speed > root { root / x.speed } else { 1.0 };
    let flow = (contact * f4) * 7.0e-6;
    out.flow = if 0.05 > flow { 0.0 } else { flow };
    let wear = z * 1.8e-8 + x.wear;
    out.wear = if wear > 1.0 { 1.0 } else { wear };
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_factor_promotes_the_original_f32_constant() {
        assert_eq!((table_factor(0.75, 1.0, Variant::Stock102) as f32).to_bits(), 0x3f0c_cccc);
    }
}
