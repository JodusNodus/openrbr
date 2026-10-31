//! NGP's per-car tuning words in its `.data`, read the way the ports tune themselves.

use rbr_physics::engine_driveline::layout::EngineTuning;
use rbr_physics::mem::Mem;
use rbr_physics::ngp::atmosphere::Atmosphere;
use rbr_physics::ngp::heat::{Airflow, TyreShape, TyreThermal};
use rbr_physics::tyre::layout::{
    DEF_CARCASS_RADIUS, DEF_CARCASS_THICKNESS, DEF_DEBUG_ID, DEF_SIDEWALL_HEIGHT, DEF_TREAD_DEPTH, DEF_TREAD_WIDTH,
};
use rbr_physics::wheel::layout::{ProbeTuning, WheelTuning};

use crate::{Ctx, Decline};

use super::address::{at, only, FILE_BASE};
use super::{data_f32, SPLIT};

/// NGP's per-car load thresholds of the wheel step (`10172f58`, `10172f5c`, `10172f60`; `wheel_step`).
pub(crate) fn step_slip(ctx: &Ctx) -> Result<[f32; 3], Decline> {
    only()?;
    Ok([0x1017_2f58, 0x1017_2f5c, 0x1017_2f60].map(|a| data_f32(ctx, a)))
}

/// The drag factor `CCar::DragAndSpinDamp` reads: NGP's `1017147c` under NGP, else 1.02's 0.5.
pub(crate) fn drag_factor(ctx: &Ctx) -> f32 {
    match at(FILE_BASE) {
        Some(_) => data_f32(ctx, 0x1017_147c),
        None => rbr_physics::car_body::drag::DRAG_FACTOR,
    }
}

/// The scale of `Step1E0`'s clutch input: NGP's per-car `10171aa4` under NGP, else 1.02's 900.
pub(crate) fn clutch_scale(ctx: &Ctx) -> f32 {
    match at(FILE_BASE) {
        Some(_) => data_f32(ctx, 0x1017_1aa4),
        None => rbr_physics::engine_driveline::drive_step::CLUTCH_SCALE,
    }
}

/// Per-car damage thresholds. The boot supplies the gate directly; snapshots retain it in the patched immediate
/// at 0x4f13a7. The impact threshold is NGP's data word 0x1017130c.
pub(crate) fn damage_tuning(ctx: &Ctx) -> rbr_physics::car_body::damage::DamageTuning {
    if at(FILE_BASE).is_none() {
        return rbr_physics::car_body::damage::DamageTuning::STOCK;
    }
    rbr_physics::car_body::damage::DamageTuning {
        engine_gate_limit: ctx.mem.ngp_runtime().map_or_else(|| ctx.mem.f32(0x004f_13a7), |r| r.damage_gate_limit()),
        emit_impact: data_f32(ctx, 0x1017_130c),
    }
}

/// The VCU's shift cooldown, launch rpm and launch clutch band: NGP's per-car `10172db0`, `10172db4`, `10171ae4`,
/// `10172338` and `101767c4` under NGP, else 1.02's constants.
pub(crate) fn vcu_tuning(ctx: &Ctx) -> rbr_physics::vcu::layout::Tuning {
    if at(FILE_BASE).is_none() {
        return rbr_physics::vcu::layout::Tuning::STOCK;
    }
    let d = |a| data_f32(ctx, a);
    rbr_physics::vcu::layout::Tuning {
        cooldown: d(0x1017_2db0),
        launch_rpm: d(0x1017_2db4),
        clutch_from: d(0x1017_1ae4),
        clutch_to: d(0x1017_2338),
        clutch_slope: d(0x1017_67c4),
        cut_margin: d(0x1017_1ab4),
    }
}

/// The engine stage's air and tuning words: NGP's `.data` under NGP, else 1.02's constants.
pub(crate) fn engine_tuning(ctx: &Ctx) -> EngineTuning {
    if at(FILE_BASE).is_none() {
        return EngineTuning::STOCK;
    }
    let d = |a| data_f32(ctx, a);
    EngineTuning {
        air: Atmosphere {
            temperature: d(0x1017_1adc),
            pressure: d(0x1017_1ad8),
            density: d(0x1017_1ae0),
            inv_pressure: d(0x1017_67c8),
            pressure_115: d(0x1017_67cc),
            pressure_0987: d(0x1017_67d0),
        },
        band: [d(0x1017_1ac4), d(0x1017_1ac8)],
        rev_gate: [d(0x1017_1aa8), d(0x1017_1aac), d(0x1017_1ab0)],
        misfire_speed: d(0x1017_1ab8),
        misfire_heat: d(0x1017_1ac0),
        stall_speed: d(0x1017_1abc),
    }
}

/// The tyre's heat-exchanging dimensions from its definition `def` (`*(scratch+0x10)`).
pub(crate) fn tyre_shape(m: &Mem, def: u32) -> TyreShape {
    TyreShape {
        carcass_radius: m.f32(def + DEF_CARCASS_RADIUS),
        tread_depth: m.f32(def + DEF_TREAD_DEPTH),
        carcass_thickness: m.f32(def + DEF_CARCASS_THICKNESS),
        tread_width: m.f32(def + DEF_TREAD_WIDTH),
        sidewall_height: m.f32(def + DEF_SIDEWALL_HEIGHT),
    }
}

/// What the tyre thermal steps of `scratch` read; they decline on a 1.02 snapshot.
pub(crate) fn thermal(ctx: &Ctx, scratch: u32) -> Result<TyreThermal, Decline> {
    only()?;
    let m = &ctx.mem;
    let (def, wheel) = (m.u32(scratch + 0x10), m.u32(scratch + 0x410));
    Ok(TyreThermal {
        air_temperature: data_f32(ctx, 0x1017_1adc),
        relax_rate: data_f32(ctx, 0x1017_1478),
        shape: tyre_shape(m, def),
        airflow: Airflow {
            // NGP reads the definition's `DebugId` word as the tyre's kind.
            kind: m.u32(def + DEF_DEBUG_ID),
            spin: m.f32(wheel + 0xea0),
            car_speed: m.f32(m.u32(wheel + 0x7f4) + 0x15f8),
            boost: (m.u32(scratch + 0x480) != 0).then(|| m.f32(scratch + 0x1c8)),
            factors: [data_f32(ctx, 0x1017_1470), data_f32(ctx, 0x1017_1474)],
        },
    })
}

/// NGP's edits to 1.02's `CSpringDamper::Step` (0x52e090) with its per-car fade words; declines on a 1.02 snapshot.
pub(crate) fn damper_step(mem: &Mem) -> Result<rbr_physics::spring_damper::NgpStep, Decline> {
    let word =
        |addr| at(addr).map(|a| mem.f32(a)).ok_or(Decline::Unsupported("the port is NGP's; the snapshot runs 1.02"));
    Ok(rbr_physics::spring_damper::NgpStep { fade_threshold: word(0x1017_2ca8)?, fade_scale: word(0x1017_2cac)? })
}

/// NGP's axle split for `M9BC0`'s leaves (declines on 1.02): under NGP `M9BC0` runs only from NGP's trampoline, and its
/// calls of `M5FE0` and `M6260` go to NGP's rewrites.
pub(crate) fn drive_split(ctx: &Ctx) -> Result<f32, Decline> {
    only()?;
    Ok(data_f32(ctx, SPLIT))
}

/// NGP's per-car wheel constants (`10171300`, `10171304`, `10171308`; the last NGP also writes into the ground
/// probe's push at `0x53daa6`).
pub(crate) fn wheel_tuning(ctx: &Ctx) -> WheelTuning {
    WheelTuning {
        fill: data_f32(ctx, 0x1017_1300),
        weight: data_f32(ctx, 0x1017_1304),
        pose: data_f32(ctx, 0x1017_1308),
    }
}

/// The ground probe's tuning under NGP: the selected collision system and the per-car wheel constants.
pub(crate) fn probe_tuning(ctx: &Ctx) -> ProbeTuning {
    ProbeTuning {
        collision: rbr_physics::stage::layout::CollisionQuery::WheelProbe.system(&ctx.mem),
        wheel: wheel_tuning(ctx),
    }
}
