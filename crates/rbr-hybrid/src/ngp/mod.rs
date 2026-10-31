//! Bindings of NGP.dll's own functions (`rbr_physics::ngp`, `notes/ngp.md`). NGP.dll is relocated in a running game,
//! so its bindings are listed at file addresses (base `0x10000000`) and hooked at the snapshot's base, which the
//! harness passes at install ([`Scope`]; 0 for a 1.02 snapshot, where they aren't hooked).

mod address;
mod calls;
mod damper;
mod engine;
mod steps;
mod tuning;

pub(crate) use calls::Call;

pub(crate) use address::{at, is_file, is_ngp, only, trampolines};
pub use address::{base_in, file, file_at, original_for, runs_ngp, set_wall_clock, Scope};
pub(crate) use engine::{engine_wrapper, starter_step, OPERATOR_NEW};
pub(crate) use steps::{curve_term, CURVE_TERM};
pub(crate) use tuning::{
    clutch_scale, damage_tuning, damper_step, drag_factor, drive_split, engine_tuning, probe_tuning, step_slip,
    thermal, vcu_tuning, wheel_tuning,
};

use crate::{Binding, Ctx};

use damper::{convection, damper_solve, damper_step1, damper_step1_scratch, damper_step2, spring_step};
use engine::{starter_binding, ENGINE_WRAPPER, STARTER};
use steps::{
    atmosphere_update, axle_worker, compute_outputs, curve_term_binding, downforce_update, drive_line_sample,
    drive_wrapper, sample_slot, step_wrapper, tag_blend, wheel_latch, wheel_slip_value,
};

/// The pointer to NGP's settings object (`0x10013cd0` returns `*(*10174bb0)`).
pub(crate) const SETTINGS: u32 = 0x1017_4bb0;

/// NGP's per-car axle split (`rbr_physics::ngp::drive`).
pub(crate) const SPLIT: u32 = 0x1017_1a90;

pub(crate) fn f(word: u32) -> f32 {
    f32::from_bits(word)
}

/// The float at NGP.dll file address `addr` in its `.data` (as `10171ad8`, the air pressure).
pub(crate) fn data_f32(ctx: &Ctx, addr: u32) -> f32 {
    ctx.mem.f32(at(addr).expect("hooked only with NGP"))
}

pub static BINDINGS: [Binding; 21] = [
    crate::bind("ngp", 0x1001_5fc0, "NGP drive line sampler (SampleBig)", drive_line_sample),
    crate::bind("ngp", STARTER, "NGP starter (M6C90)", starter_binding),
    crate::bind("ngp", 0x1004_b5a0, "NGP atmosphere", atmosphere_update),
    crate::bind("ngp", 0x1004_7040, "NGP downforce", downforce_update),
    crate::bind("ngp", 0x1006_0230, "NGP DamperSolve (pressure limit)", damper_solve),
    crate::bind("ngp", 0x1004_d2f0, "NGP DamperStep1", damper_step1),
    crate::bind("ngp", 0x1004_d3c0, "NGP DamperStep1 (scratch factor)", damper_step1_scratch),
    crate::bind("ngp", 0x1004_d380, "NGP DamperStep2", damper_step2),
    crate::bind("ngp", 0x1005_b4a0, "NGP ComputeOutputs wrapper", compute_outputs),
    crate::bind("ngp", 0x1004_7300, "NGP CDrive::M93A0", crate::engine_driveline::clutch_torque),
    crate::bind("ngp", 0x1005_e8a0, "NGP E51Wheel::M3D230", tag_blend),
    crate::bind("ngp", 0x1004_58c0, "NGP CDrive::M5FE0", wheel_slip_value),
    crate::bind("ngp", 0x1004_5b80, "NGP CDrive::M6260", wheel_latch),
    crate::bind("ngp", 0x1004_df40, "NGP E44MatTable slot lookup", sample_slot),
    crate::bind("ngp", 0x1004_8d50, "NGP CDrive::M9BC0 wrapper (diffs)", drive_wrapper),
    crate::bind("ngp", 0x1004_87f0, "NGP axle worker (drive types 4 and 5)", axle_worker),
    crate::bind("ngp", 0x1004_d410, "NGP tyre convection (RelaxStep's DamperStep1)", convection),
    crate::bind("ngp", CURVE_TERM, "NGP E39_Hook4160 rewrite (curve term and heat)", curve_term_binding),
    crate::bind("ngp", ENGINE_WRAPPER, "NGP engine wrapper (Fwd18E20's jmp)", engine_wrapper),
    crate::bind("ngp", 0x1005_b660, "NGP CScratch::Step wrapper", step_wrapper),
    crate::bind("ngp", 0x1005_9f70, "NGP CSpringDamper::Step wrapper (bump stop)", spring_step),
];
