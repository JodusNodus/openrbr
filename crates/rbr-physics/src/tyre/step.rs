//! 0x5360c0 `CScratch::Step` (thiscall, 16 stack words, `ret 0x40`): the per-wheel tyre step. It frames the contact
//! (`Align`, `ContactPatch`), takes the slip and the weights, runs `Accumulate`, the table lookups, the contact
//! solves and the force, heat and output terms. The operation order and the branches follow the asm, not
//! `src/rbr/RBR_E49_TyreStep.cpp`: the clamps and the max picks are the asm's `jbe`/`ja` forms, so a NaN takes the
//! same branch. The x87 chains round each op to 24 bits (`round24`); the SSE parts are f32.
//!
//! The frame is loaded once by the caller and stored once after it. A table index the frame does not hold makes the
//! step return `Outside`, and the caller then runs the original.

use super::align;
use super::contact_patch;
use super::fill;
use super::force;
use super::forces;
use super::grip;
use super::heat;
use super::layout::{
    accumulate_input, apply_accumulate, apply_heat, copy_material, heat_input, sig, sign_input, table_at, Frame,
    OutputBlock, Outside, StepArgs, StepOut, Table,
};
use super::locate::locate;
use super::slip;
use super::solve_patch;
use super::write::{clamp01, write_outputs};
use crate::fpu::{precision, round_x87, Precision};
use crate::math::trig::{one_minus_x87_cos_f64, x87_cos};
use crate::math::vec::Vec4;
use crate::ngp::heat::TyreThermal;
use crate::ngp::inclination;
use crate::Variant;

/// The asm's constants: 1.0 (0x795e14), 0.0 (0x795e50), 10 (0x72b154), 0.1 (0x72b068), 0.9 (0x795e90), 0.4 (0x72f664)
/// and 0.6 (0x72f684). The f32 literals below are the same values. NGP repoints the output scale's 0.1 (`0x536703`)
/// to its 0.025 (`1014fbd8`).
const SPEED_KNEE: f32 = 10.0;
const SPEED_SCALE: f32 = 0.1;
const OUTPUT_SCALE: f32 = 0.025;
const NINE_TENTHS: f32 = 0.9;
const WEIGHT_NEAR: f32 = 0.4;
const WEIGHT_FAR: f32 = 0.6;

/// The no-contact path (`Align` or `ContactPatch` failed): the outputs are cleared and the contact flag is 0.
fn no_contact(f: &mut Frame, a: &StepArgs) -> StepOut {
    f.scratch.contact_flag = 0;
    f.scratch.signal = [0; 24];
    f.scratch.signal[sig::DEBUG_ID] = f.setup.debug_id;
    f.scratch.signal[sig::FLAG] = 0;
    f.state.vec_30 = a.vec_k;
    f.state.k40 = 0.0;
    f.state.k44 = 0.0;
    f.state.flag_0c = 0;
    StepOut { l: Vec4::default(), m: Vec4::default(), n: 0.0, o: None, block: None }
}

/// 0x5360c0 `CScratch::Step`. Its outputs are the windows the vector test compares (`tests/tyre_vectors.rs`).
///
/// `ngp` is NGP's state. The x87 chains (here and in the callees) run at 53 bits, the inclination wrappers
/// (`ngp::inclination`) scale the blend pairs and heat lerps before `Accumulate` and the heat factor around `HeatStep`
/// (NGP's `Step` wrapper restores the def's tables afterwards: they are not stored here), `e3` scales `+0x1f8` in
/// place of the second `+0x1e8` (`0x5365cb`, `0x5365ec`; 1.02 scales `+0x1e8` twice), and the output scale is 0.025.
pub fn step(f: &mut Frame, a: &StepArgs, ngp: &TyreThermal) -> Result<StepOut, Outside> {
    let variant = Variant::Ngp778;
    let round24 = |v: f64| round_x87(v, variant);
    if f.state.flag_2c == 0 {
        f.state.dirty_28 = 1;
        f.state.flag_2c = 1;
    }
    if a.pair_copy {
        f.scratch.pair = a.pair;
    }
    copy_material(f, &a.material);
    f.scratch.k_vec = a.vec_k;
    f.scratch.spin = a.spin;
    f.scratch.arg_c = a.c;
    f.scratch.arg_d = a.d;
    f.state.seg_blend = locate(f.state.seg_blend, &f.setup.t134, f.state.s68)?;

    // The blend of the table entries at the two indices `seg_blend.lo` and `.hi`, weighted by `seg_blend.frac`.
    let sb = f.state.seg_blend;
    let blend = (1.0 - sb.frac) * table_at(&f.setup.t144, sb.lo)? + table_at(&f.setup.t144, sb.hi)? * sb.frac;
    f.scratch.blend = blend;
    f.scratch.f394 = f.setup.f4c * blend;
    f.scratch.f398 = f.setup.f5c * blend;

    if !align::align(f, a.align_a, a.dir) || contact_patch::contact_patch(f, variant).is_none() {
        return Ok(no_contact(f, a));
    }

    f.scratch.contact_flag = 1;
    slip::apply_compute_slip(f, [a.out_a.x, a.out_a.y, a.out_a.z], variant);
    // The weight of the speed above 10 (`(v - 10) · 0.1`, clamped to [0, 1]) scales `f398`.
    let w = clamp01((f.scratch.speed - SPEED_KNEE) * SPEED_SCALE);
    f.scratch.f398 *= w * NINE_TENTHS + 0.1;
    f.scratch.f3cc = 0.0;
    f.scratch.f3d0 = 0.0;
    f.scratch.f3d4 = 0.0;
    let (k, heat_lerps) = inclination::grip_scale(inclination::degrees(f.scratch.f3ac));
    let d = &mut f.setup;
    d.t244.scale(k, &mut d.t264);
    if heat_lerps {
        d.t204.scale(k, &mut d.t224);
    }
    let acc = force::accumulate(accumulate_input(f));
    apply_accumulate(f, &acc);

    // The total of the three weights (`force_sum + f3cc`, then `+ f31c`) divides the three ratios; the asm's order.
    let total = (f.scratch.force_sum + f.scratch.f3cc) + f.scratch.f31c;
    f.scratch.f310 = total / f.scratch.f308;
    f.scratch.ratio_324 = total / f.scratch.tyre_factor_2;
    f.scratch.ratio_328 = f.scratch.f31c / total;
    f.state.seg_mix = locate(f.state.seg_mix, &f.setup.t154, f.scratch.f310)?;
    f.state.seg_heat = locate(f.state.seg_heat, &f.setup.t1e4, f.scratch.tyre_factor_2)?;

    // The heat-flow terms: the table lerps at the indices `seg_heat.lo` and `.hi` (fraction `.frac`), times `blend`.
    let sh = f.state.seg_heat;
    let lerp = |table: &Table| -> Result<f32, Outside> {
        Ok((1.0 - sh.frac) * table_at(table, sh.lo)? + table_at(table, sh.hi)? * sh.frac)
    };
    let heat_e3 = lerp(&f.setup.t204)? * blend;
    let heat_e4 = lerp(&f.setup.t224)? * blend;
    f.scratch.heat_flow_e3 = heat_e3;
    f.scratch.heat_flow_e4 = heat_e4;

    // The x87 part: `stf[0x10] += dir·outA · dt + (1 - F[0xEC]) · -((1 - cos) · F[0xC1])`, rounded to 24 bits.
    let angle = (f.scratch.slip_v[1] / f.scratch.r304) * a.dt;
    // `fsubr` takes `fcos`'s extended result: NGP's 53-bit precision keeps its low bits.
    let one_minus_cos = match precision(variant) {
        Precision::Pc24 => 1.0 - x87_cos(angle),
        Precision::Pc53 => one_minus_x87_cos_f64(angle),
    };
    let neg = -round24(round24(one_minus_cos) * f64::from(f.scratch.r304));
    let dir = [a.dir.x, a.dir.y, a.dir.z];
    let out = [a.out_a.x, a.out_a.y, a.out_a.z];
    // Each product is rounded before the sum, as the x87 does: `(d1·o1 + d2·o2) + d0·o0`.
    let product = |k: usize| round24(f64::from(dir[k]) * f64::from(out[k]));
    let d12 = round24(product(1) + product(2));
    let dot = round24(product(0) + d12);
    let scaled = round24(dot * f64::from(a.dt));
    let rest = round24(round24(1.0 - f64::from(f.scratch.scale_3b0)) * neg);
    let force_x = round24(scaled + rest) + f64::from(f.state.k40);
    f.state.k40 = round24(force_x) as f32;
    f.state.k44 -= f.scratch.slip_ratio_term * a.dt;

    let idx = f.scratch.tyre_index;
    let table_x = f.state.table.get(idx as usize).ok_or(Outside { index: idx as i32, len: f.state.table.len() })?.value;
    f.state.seg_shape = locate(f.state.seg_shape, &f.setup.t184, table_x)?;
    f.state.seg_shape2 = locate(f.state.seg_shape2, &f.setup.t184, f.state.table[7].b)?;

    // The three-term weight `e3` of the two table lerps, then the wear factor when `heat_source` is above 0.
    let (ss, ss2) = (f.state.seg_shape, f.state.seg_shape2);
    let d = |index: u32| table_at(&f.setup.t1a4, index);
    let (fb0, fbc) = (ss.frac, ss2.frac);
    let mut e3 = (((1.0 - fbc) * d(ss2.lo)? * WEIGHT_NEAR + (1.0 - fb0) * d(ss.lo)? * WEIGHT_FAR)
        + (d(ss.hi)? * fb0) * WEIGHT_FAR)
        + (d(ss2.hi)? * fbc) * WEIGHT_NEAR;
    if 0.0 < f.scratch.heat_source {
        let wear = f.scratch.wear[idx as usize];
        e3 *= (((f.setup.f310 - 1.0) * f.scratch.heat_source + 1.0) - 1.0) * wear + 1.0;
    }

    // `F[0x7A]` takes `e3` twice (the C++ and the asm both do), each store after a dirty flag on `+0x1dc`.
    f.scratch.f_1e8 *= e3;
    f.scratch.dirty_1e4 = 1;
    f.scratch.factor_1f8 *= e3;
    f.scratch.dirty_1e4 = 1;
    f.scratch.e3 = e3;

    slip::split_slip(f);
    solve_patch::solve_patch(f)?;
    grip::solve_pair(f, variant);
    f.state.k40 = f.scratch.f3fc * f.scratch.gate + f.scratch.f3e8 * f.scratch.f3dc;
    f.state.k44 = f.scratch.f400 * f.scratch.gate + f.scratch.f3ec * f.scratch.f3dc;

    // `w` and `e2` are the maxes of two weight pairs, picked as the asm does: `x > y` takes `x`, else `y`
    // (a NaN gives `y`).
    let w = if f.scratch.f3e0 > f.scratch.f3e4 { f.scratch.f3e0 } else { f.scratch.f3e4 };
    let e2 = if f.scratch.f3f4 > f.scratch.f3f8 { f.scratch.f3f4 } else { f.scratch.f3f8 };
    f.scratch.f_e2 = f.scratch.f3dc * w + f.scratch.gate * e2;
    let (l, m) = forces::solve_forces(f, a.dt, a.out_m, variant);

    // `Sign` (0x531ba0) is the x87 term; the float store takes its f32 rounding and `n` adds `F[0xE9]` at 24 bits.
    let sign = force::signed_term(sign_input(f), variant);
    f.scratch.heat_f2 = sign as f32;
    let n = round24(sign + f64::from(f.scratch.f3a4)) as f32;
    let o = f.scratch.tyre_factor_2 * OUTPUT_SCALE * f.scratch.gate;

    let heat_f2 = f.scratch.heat_f2;
    // `0x1005bc50`, with the contact flag set (this path set it).
    let k = inclination::heat_scale(inclination::degrees(f.scratch.f3ac), f.setup.f10c);
    f.scratch.ngp_4e8 = (heat_f2 * f.scratch.spin).abs() * k;
    f.scratch.heat_f2 = k * heat_f2;
    let heat_in = heat_input(f, f.scratch.f39c, f.scratch.f3a0, a.dt)?;
    apply_heat(f, &heat::heat_step(heat_in, ngp));
    f.scratch.heat_f2 = heat_f2;
    let block: OutputBlock = write_outputs(f);
    fill::fill_outputs(f, a.out_a, a.pair, a.dt);
    f.state.flag_0c = 1;
    Ok(StepOut { l, m, n, o: Some(o), block: Some(block) })
}
