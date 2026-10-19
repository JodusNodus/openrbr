//! `CScratch::RelaxStep(dt)` (0x5345a0) and `CScratch::TableStep(idx, x)` (0x534480): the tread and carcass
//! damper states relax towards their targets, and the table entry of the tyre's index moves. RelaxStep calls
//! TableStep after its own accumulators, with the `m_state+0x12c` value they leave.
//!
//! The order of each operation follows the asm, not `src/rbr/RBR_E49_TyreSim.cpp`. The x87 chains round at 24
//! bits (`round24`). A value the asm stores to memory as f32 and reloads is rounded to f32 here, too.
//! The damper helpers are in `crate::spring_damper`.
//!
//! Under NGP (`ngp`, `None` for 1.02) the chains round at 53 bits and the damper calls are NGP's (`ngp::damper`),
//! except RelaxStep's `DamperStep2`, which stays the exe's (at 53 bits, the same as NGP's `step2`).

use crate::fpu::round_x87;
use crate::math::abs_asm;
use crate::ngp::damper::{convection, inner_flux, step1 as ngp_step1, step2 as ngp_step2, Thermal};
use crate::spring_damper::{
    accumulate_a, force as damper_force, step1 as damper_step1, step2 as damper_step2, AccumulationFactors, ForceTerms,
};
use crate::Variant;

/// The inputs of `TableStep` for one table index (see `layout::load_table`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TableInput {
    /// `m_state+0xd0+12·idx` (`stf[0x34 + 3·idx]`): the table entry.
    pub v: f32,
    /// `this+0x360` (`f[0xD8]`): the speed.
    pub speed: f32,
    /// `m_state+0x12c` (`stf[0x4B]`): the second damper state.
    pub s4b: f32,
    /// `m_def+0x118` (`dff[0x46]`): the time constant.
    pub def_118: f32,
    /// The two `DamperAccumA` blocks: `m_def+0x110` (updates the table entry) and `m_def+0x11c` (updates `s4b`).
    pub s110: AccumulationFactors,
    pub s11c: AccumulationFactors,
}

/// What `TableStep` writes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TableOutput {
    /// `this+0x4f8` (`f[0x13E]`): `r·8`, where `r` is the first `DamperStep1`.
    pub f13e: f32,
    /// The table entry after `DamperAccumA`.
    pub v: f32,
    /// `m_state+0x12c` after `DamperAccumA`.
    pub s4b: f32,
}

/// 0x534480 `CScratch::TableStep`. Under NGP its `DamperStep1` and `DamperStep2` are NGP's `0x1004d3c0` and
/// `0x1004d380` (the calls at `0x5344eb`, `0x534531`): the scratch's factor in place of 0.0562, and the air temperature.
pub fn table_step(t: &TableInput, x: f32, ngp: Option<&Thermal>) -> TableOutput {
    let round24 = |v: f64| round_x87(v, Variant::of(ngp));
    // w = min(speed·0.1, 1), then the first term is DamperStep1(v, 298.15, (w·0.5 + 1)·30, 0.0562).
    let w = t.speed * 0.1;
    let w = if w > 1.0 { 1.0 } else { w };
    let c = ((w * 0.5) + 1.0) * 30.0;
    let r = match ngp {
        None => damper_step1(t.v, 298.14999, c, 0.0562, Variant::Stock102),
        Some(n) => f64::from(ngp_step1(t.v, 298.14999, c, n.factor(), n.temperature)),
    };
    // `r` is stored as f32 for the sum below, and `r·8` is rounded from the x87 value.
    let f13e = round24(r * 8.0) as f32;
    let r_f = r as f32;
    // r2 = DamperStep2((s4b - v)·999.99994, dff[0x46], 0.0562), stored as f32 (`fstp`).
    let a = (t.s4b - t.v) * 999.99994;
    let r2 = match ngp {
        None => damper_step2(a, t.def_118, 0.0562, Variant::Stock102),
        Some(n) => ngp_step2(a, t.def_118, n.factor()),
    } as f32;
    // The table entry takes -((r2 + r)·x) and s4b takes r2·x, each rounded in the x87 chain, then stored as f32.
    let x_table = (-round24(round24(r2 as f64 + r_f as f64) * x as f64)) as f32;
    let x_s4b = round24(r2 as f64 * x as f64) as f32;
    let mut v = t.v;
    accumulate_a(t.s110, x_table, &mut v);
    let mut s4b = t.s4b;
    accumulate_a(t.s11c, x_s4b, &mut s4b);
    TableOutput { f13e, v, s4b }
}

/// The inputs of `RelaxStep` (see `layout::load_relax`). `table` is the tyre index's `TableStep` input; its
/// `s4b` is the `m_state+0x12c` that the first accumulator of RelaxStep updates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RelaxInput {
    /// The time step `dt`.
    pub dt: f32,
    /// `m_state+0x130` (`stf[0x4C]`): the first damper state of the carcass.
    pub s130: f32,
    /// `m_state+0x64` (`stf[0x19]`).
    pub s19: f32,
    /// `m_def+0x7c` (`dff[0x1F]`), `m_def+0xac` (`dff[0x2B]`), `m_def+0x124` (`dff[0x49]`).
    pub dff_1f: f32,
    pub dff_2b: f32,
    pub dff_49: f32,
    /// `m_def+0x11c` (the first accumulator's `s`) and `m_def+0x128` (the second's).
    pub s11c: AccumulationFactors,
    pub s128: AccumulationFactors,
    /// `m_state+0x60..0x6c`: the state of `DamperForce` (`s0..s3`).
    pub force: ForceTerms,
    /// The table step, with `s4b` at `m_state+0x12c`.
    pub table: TableInput,
}

/// What `RelaxStep` writes. `table.s4b` is the `m_state+0x12c` value after both steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RelaxOutput {
    /// `m_state+0x130` after the second accumulator.
    pub s130: f32,
    /// `this+0x4fc` (`f[0x13F]`): the third `DamperStep1`, stored as f32.
    pub r3: f32,
    /// `m_state+0x60..0x6c` after `DamperForce`.
    pub force: ForceTerms,
    pub table: TableOutput,
}

/// 0x5345a0 `CScratch::RelaxStep`. Under NGP the rate 100 is NGP's per-car `10171478` (the operand at `0x5345cd`),
/// and the two `DamperStep1` calls are NGP's: the carcass flux `0x1004d550` (the call at `0x534612`, retargeted
/// relative to NGP's base) and the convection `0x1004d410` (`0x534638`).
pub fn relax_step(r: &RelaxInput, ngp: Option<&Thermal>) -> RelaxOutput {
    let round24 = |v: f64| round_x87(v, Variant::of(ngp));
    let rate = ngp.map_or(100.0, |n| f64::from(n.relax_rate));
    // r1 = DamperStep2((s130 - s12c)·100, dff[0x49], dff[0x2B]·dff[0x1F]·2π).
    let mut s12c = r.table.s4b;
    let a1 = round24(round24(r.s130 as f64 - s12c as f64) * rate) as f32;
    let c1 = (r.dff_2b * r.dff_1f) * 6.2831855;
    let r1 = match ngp {
        None => damper_step2(a1, r.dff_49, c1, Variant::Stock102),
        Some(_) => ngp_step2(a1, r.dff_49, c1),
    };
    // r2 = DamperStep1(s130, s19, 30, 0.54); r3 = DamperStep1(s130, 298.15, 20, 0.54).
    let (r2, r3) = match ngp {
        None => (
            damper_step1(r.s130, r.s19, 30.0, 0.54, Variant::Stock102),
            damper_step1(r.s130, 298.14999, 20.0, 0.54, Variant::Stock102),
        ),
        Some(n) => (
            f64::from(inner_flux(r.s130, r.s19, 30.0, n.shape())),
            f64::from(convection(r.s130, 298.14999, 20.0, &n.convection, n.temperature)),
        ),
    };
    let r3_f = r3 as f32;
    // Each product with dt is an x87 value stored as f32 (`fstp`). r1·dt and r2·dt are reused as f32 below.
    let r1_dt = round24(r1 * r.dt as f64) as f32;
    let r2_dt = round24(r2 * r.dt as f64) as f32;
    // The first accumulator takes -(r1·dt), the second ((r1·dt - r2·dt) - r3·dt).
    let x1 = 0.0 - r1_dt;
    let x2 = round24(round24(r1_dt as f64 - r2_dt as f64) - round24(r3_f as f64 * r.dt as f64)) as f32;
    accumulate_a(r.s11c, x1, &mut s12c);
    let mut s130 = r.s130;
    accumulate_a(r.s128, x2, &mut s130);
    let mut force = r.force;
    damper_force(&mut force, r2_dt, 742.85718, 287.0);
    let table = table_step(&TableInput { s4b: s12c, ..r.table }, r.dt * 8.0, ngp);
    RelaxOutput { s130, r3: r3_f, force, table }
}

/// The inputs of `BlendTable` for one pair of indices (see `layout::load_blend`): the bases and slopes of the two
/// entries, their wear factors, the def's `+0x2dc`, the two fractions, and the wear factor `u` of the tyre index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendInput {
    pub base_i: f32,
    pub base_j: f32,
    pub slope_i: f32,
    pub slope_j: f32,
    pub wear_i: f32,
    pub wear_j: f32,
    pub dff_2dc: f32,
    pub a: f32,
    pub b: f32,
    pub u: f32,
}

/// The two outputs of `BlendTable`, in the order the asm writes them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendOut {
    pub o1: f32,
    pub o2: f32,
}

/// 0x534e70 `CScratch::BlendTable`'s arithmetic. Each output is its base, plus the slope times the distance of its
/// fraction from `dff[0xB7]`, then scaled by its wear factor. The asm writes the outputs through the caller's pointers;
/// the caller stores them in that order (`layout::store_blend`).
pub fn blend_table(b: &BlendInput) -> BlendOut {
    let a = abs_asm(b.dff_2dc - b.a);
    let c = abs_asm(b.dff_2dc - b.b);
    let o1 = b.slope_i * a + b.base_i;
    let o2 = b.slope_j * c + b.base_j;
    BlendOut { o1: ((b.wear_i - 1.0) * b.u + 1.0) * o1, o2: ((b.wear_j - 1.0) * b.u + 1.0) * o2 }
}
