//! The kernel inputs taken from a loaded `Frame`, and their results put back into it.

use super::frame::{Outside, Table};
use super::frame_types::{Frame, SCALE_238};
use super::kernels::SignInput;
use crate::spring_damper::AccumulationFactors;
use crate::tyre::force::{AccumulateInput, Accumulated};
use crate::tyre::friction::FrictionInput;
use crate::tyre::heat::{HeatInput, HeatOutput};
use crate::tyre::outputs::BlendPair;
use crate::tyre::solve::Contact;
use crate::tyre::step_inner::StepInput;

/// The `Accumulate` inputs from the frame.
pub fn accumulate_input(f: &Frame) -> AccumulateInput {
    let s = &f.scratch;
    AccumulateInput {
        blend_hi: s.contact_p[5],
        blend_lo: s.contact_p[6],
        tyre_factor: s.tyre_factor,
        tyre_factor_2: s.tyre_factor_2,
        scale_238: f32::from_bits(s.material[SCALE_238]),
        slip_x: s.slip_v[0],
        slip_y: s.slip_v[1],
        long_contact_vel: s.long_contact_vel,
        scale_300: s.scale_300,
        scale_3b0: s.scale_3b0,
        acc_504: s.acc_504,
        acc_508: s.acc_508,
        acc_50c: s.acc_50c,
        effective_radius: f.state.radius,
        lerp_weight: f.state.lerp_weight,
        setup_cc: f.setup.fcc,
    }
}

/// Stores the `Accumulate` results into the frame.
pub fn apply_accumulate(f: &mut Frame, out: &Accumulated) {
    let s = &mut f.scratch;
    s.force_a = out.force_a;
    s.force_b = out.force_b;
    s.zero_350 = out.zero_350;
    s.tyre_factor_2 = out.factor_314;
    s.force_sum = out.force_sum;
    s.force_t1 = out.force_t1;
    s.nh = out.nh;
    s.acc_504 = out.acc_504;
    s.acc_508 = out.acc_508;
    s.acc_50c = out.acc_50c;
}

/// The `Sign` inputs from the frame.
pub fn sign_input(f: &Frame) -> SignInput {
    let s = &f.scratch;
    SignInput {
        tyre_factor: s.tyre_factor,
        factor_298: s.factor_298,
        tyre_factor_2: s.tyre_factor_2,
        factor_3dc: s.f3dc,
        spin: s.spin,
    }
}

/// The `Friction` inputs from the frame; `k` is its stack word.
pub fn friction_input(f: &Frame, k: f32) -> FrictionInput {
    let (s, st) = (&f.scratch, &f.state);
    FrictionInput {
        gate: s.gate,
        state_flag: st.flag_0c,
        speed: s.speed,
        factor_314: s.tyre_factor_2,
        factor_1f8: s.factor_1f8,
        dir: s.contact_frame[0],
        rest: s.contact_frame[1],
        axis: s.contact_frame[2],
        sv: st.sv,
        k,
    }
}

/// The entry `i` of a table, or `Outside` when `i` is outside its window (the index is a state word, `u32`).
pub fn table_at(t: &Table, i: u32) -> Result<f32, Outside> {
    t.at(i as i32).ok_or(Outside { index: i as i32, len: t.count() })
}

/// The `HeatStep` inputs from the frame: `a`, `b` and `c` are Step's arguments, `idx` is `this+0x3c0`. The table
/// entries are read at `idx` and the shape, mix and wear words at the indices the frame's searches hold.
pub fn heat_input(f: &Frame, a: f32, b: f32, c: f32) -> Result<HeatInput, Outside> {
    let (s, st, d) = (&f.scratch, &f.state, &f.setup);
    let idx = s.tyre_index as usize;
    let entry = st.table.get(idx).ok_or(Outside { index: idx as i32, len: st.table.len() })?;
    let s_mix = &st.seg_mix;
    let s_shape = &st.seg_shape;
    Ok(HeatInput {
        a,
        b,
        c,
        heat_source: s.heat_source,
        f_da: s.slip_v[0],
        f_dd: s.long_contact_vel,
        f_de: s.spin,
        f_f2: s.heat_f2,
        f_c4: s.f310,
        speed: s.speed,
        f_e2: s.f_e2,
        f_c2: s.f308,
        tab: entry.value,
        state_12c: st.table[7].b,
        state_130: st.state_130,
        state_b0: s_shape.frac,
        state_c8: s_mix.frac,
        mix_a: table_at(&d.t16c, s_mix.lo)?,
        mix_b: table_at(&d.t16c, s_mix.hi)?,
        shape_a: table_at(&d.t1c4, s_shape.lo)?,
        shape_b: table_at(&d.t1c4, s_shape.hi)?,
        def_118: d.def_118,
        damper: [
            AccumulationFactors { first: d.damper_110[0], second: d.damper_110[1] },
            AccumulationFactors { first: d.damper_11c[0], second: d.damper_11c[1] },
            AccumulationFactors { first: d.damper_128[0], second: d.damper_128[1] },
        ],
        wear: s.wear[idx],
    })
}

/// Stores the `HeatStep` results into the frame (the wear word of the index, the table entry, the damper words).
pub fn apply_heat(f: &mut Frame, out: &HeatOutput) {
    let idx = f.scratch.tyre_index as usize;
    let s = &mut f.scratch;
    s.heat_t = out.heat_t;
    s.heat_sum_ab = out.sum_ab;
    s.heat_t_inv = out.t_inv;
    s.heat_r = out.r;
    s.flow = out.flow;
    s.wear[idx] = out.wear;
    let st = &mut f.state;
    st.table[idx].value = out.tab;
    st.table[7].b = out.state_12c;
    st.state_130 = out.state_130;
}

/// The `StepInner` inputs of the scratch: its record is `this+0x20`, so the record's words are scratch words.
pub fn step_inner_input(f: &Frame) -> Result<StepInput, Outside> {
    let (s, st, d) = (&f.scratch, &f.state, &f.setup);
    let blend = |base: &Table| -> Result<BlendPair, Outside> {
        Ok(BlendPair { at_a: table_at(base, st.seg_heat.lo)?, at_b: table_at(base, st.seg_heat.hi)? })
    };
    Ok(StepInput {
        rec_340: s.speed,
        rec_348: s.slip_v[0],
        rec_34c: s.slip_v[1],
        rec_358: s.spin,
        rec_1c8: s.f_1e8,
        rec_1d8: s.factor_1f8,
        rec_2f4: s.tyre_factor_2,
        rec_36c: s.heat_flow_e3,
        rec_370: s.heat_flow_e4,
        rec_38c: s.f3ac,
        factor_30c: s.tyre_factor,
        this_340: s.contact_p[5],
        this_368: s.slip_v[0],
        this_36c: s.slip_v[1],
        radius: st.radius,
        def_3c: d.f3c,
        weight: st.seg_heat.frac,
        blend: [blend(&d.t244)?, blend(&d.t264)?, blend(&d.t284)?, blend(&d.t2a4)?],
    })
}

/// The `Step`-chain contact of the frame: `p` (seven words), `qb` (three, the first is `tyre_factor_2`) and `r`.
pub fn contact_input(f: &Frame, r: [f32; 8]) -> Contact {
    let (s, st) = (&f.scratch, &f.state);
    Contact {
        f96: s.f96,
        p: s.contact_p,
        qb: [s.tyre_factor_2, s.f318, s.f31c],
        r,
        state_70: st.contact_70,
        state_74: st.contact_74,
    }
}

/// Stores a solved contact back into the frame: `p`, `qb` and the state flags.
pub fn apply_contact(f: &mut Frame, c: &Contact) {
    let s = &mut f.scratch;
    s.contact_p = c.p;
    s.tyre_factor_2 = c.qb[0];
    s.f318 = c.qb[1];
    s.f31c = c.qb[2];
    f.state.contact_70 = c.state_70;
    f.state.contact_74 = c.state_74;
}
