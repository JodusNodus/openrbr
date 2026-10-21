//! Typed views of the memory the engine and drive ports read and write. `load` and `store` are the only code that
//! knows the original's offsets; the ports take and return the typed values.

mod construct;
mod curve;
mod drive_calls;
mod drive_objects;
mod engine_calls;
mod engine_objects;
mod engine_sim;
mod mem_store;
mod setup;
mod step;
mod tables;
mod worker;

pub use construct::{
    construct_drive, construct_drive_def, construct_drive_options, construct_drive_part, construct_drive_setup,
    construct_drive_state, construct_engine, construct_engine_def, construct_engine_options, construct_engine_scratch,
    construct_engine_setup, construct_engine_state, drive_reset_part, reset_sub78, state_init_c70,
};
pub use curve::{
    curve_input_at, curve_row_at, curve_state_at, hook_block_at, ratio_ring_at, with_gear, with_worker, CurveInput,
    CurveRow, CurveState, CurveTables, HookBlock,
};
pub use drive_calls::{
    clear_lanes, copy_row, drive_gate, m6db0, m6dd0, mat4_inverse_transposed, mat4_mul_vec4, mgr_b_field_at, rows_copy,
    sample_big, sample_x, set_diff_lock, set_two,
};
pub use drive_objects::{Drive, DriveDef, DriveObjects, DriveState, IntLane, Lane};
pub use engine_calls::{
    abs_rpm_term, compliance, engine_state_speed, exhaust_blend_at, init_real_lerp, options_gate_58, rev_gate,
    rpm_above_40, set_cut_flags, set_options, sim_e970, sim_e9a0, sim_ebb0, sim_ee60, slot_roll, torque_curve,
    ROLL_TABLE,
};
pub use engine_objects::{EngineDef, EngineObjects, EngineOptions, EngineState, EngineThis, EngineTuning, LoadDef};
pub use engine_sim::{
    crank_tick, engine_step, engine_torque, fuel_air_terms, intake_state_reset, misfire_roll, sim8610, sim8890,
    sim8cf0, sim8d90, smooth_damper, solve_damper, turbo_energy_step,
};
pub use setup::{drive_setup_into_def, link_worker, normalize_gear_rows, select_gear_rows};
pub use step::step1e0;
pub use tables::{BlendKnot, CurveKnot, UnitSplit};
pub use worker::{
    Axle, DriveWorker, GearBand, GearWork, LaneScale, RatioRing, Wheel, WheelBlock, WheelTerm, WorkerDef, WorkerObj,
    WorkerState, WorkerThis, CLUTCH_MATRIX_AT, LOC_AT, STEP_MATRIX_AT, STEP_VEC_AT,
};
