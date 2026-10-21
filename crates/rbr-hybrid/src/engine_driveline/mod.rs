//! Bindings of the engine and driveline ports (`rbr_physics::engine_driveline`, exe 0x504450-0x50ed70). The ABIs
//! are the original's (`harness/vector_specs/engine_driveline.py` and `harness/vcu_vectors.py`): the `CDrive`,
//! `CEngine` and `CDriveLine` methods are thiscall, the `E41_*` helpers stdcall. Their vector, CRT and `SetDirty`
//! callees are computed by the ports; `CCar::Get30`, the gear-change signal and `E12_SampleX` are host calls. eax is
//! given where the original returns a value; elsewhere it is a scratch value no caller reads, given as 0.

use crate::Binding;

mod curves;
mod engine_sim;
mod inputs;
mod workers;

use crate::entities::MB560;
use crate::rigid_body::WALKER;
pub(crate) use curves::row_value_at;
use curves::{
    blend_b, find_bracket, blend_tables_checked, blend_tables, clamp_mix, count_down, curve_eta, e12_sample_x, last, lerp_at, ring_average,
    row_value, set_drive_line, table_lerp, thunk_4f0, split_unit_fraction, unsigned_table_lerp,
};
use engine_sim::{
    abs_rpm_term, compliance, crank_tick, drive_setup, engine_state_speed, engine_torque, exhaust_blend,
    fuel_air_terms, intake_state_reset, link_worker, misfire_roll, options_gate_58, rev_gate, rpm_above_40, sample_big,
    set_cut_flags, set_options, set_scratch08, sim8610, sim8890, sim8cf0, sim8d90, sim_e970, sim_e9a0, sim_ebb0,
    sim_ee60, slot_roll, smooth_damper, solve_damper, stall_guard, torque_curve, turbo_energy_step, vec_lerp,
};
pub(crate) use engine_sim::{engine_running, engine_step};
pub(crate) use inputs::{clear_lanes, m6db0, m6dd0, set_two};
use inputs::{
    copy_row, drive_gate, drive_reset_part, drive_speed_key, emit_gear_change, emit_noise, get_engine_speed, get_gear,
    init_real_lerp, mat4_inverse_transposed, mat4_mul_vec4, mgr_b_entry, mgr_b_field_at, mgr_b_word8, nop_ret4,
    request_gear, rows_copy, set_brake_in, set_clutch_slip, set_diff_lock, set_throttle_in,
};
pub(crate) use workers::clutch_torque;
use workers::{
    axle_pair_stages, block_lane_step, block_pair_gate, clutch_flags, clutch_values, free_wheel_step, gear_band_guard,
    gear_logic, gear_speed_error, gear_trim, pair_axle_stage, pair_clutch_values, pair_latch, pair_slip_value,
    pair_worker, reset_pair, reset_wheels, set_lane_scale, signal_gear_change, step1e0, wheel_worker,
};

/// `CCar::Get30` (thiscall): the car's word at `+0x30`.
pub const CAR_GET30: u32 = 0x4e67f0;
/// `CMultiSignal1<const CSigMrRCarGearChange&>::Emit` (thiscall, the payload's address).
pub const EMIT_GEAR_CHANGE: u32 = 0x506f20;
/// The signal manager (`[0x893600]`) and its gear-change slot.
pub const SIGNALS: u32 = 0x893600;
pub const GEAR_CHANGE_SLOT: u32 = 0xea80;
/// `E12_SampleX` (thiscall, one float, st0), which `CDriveLine::Thunk4F0` tail-jumps to.
pub const E12_SAMPLE_X: u32 = 0x475ea0;
/// `E39_MatVec` (thiscall, the vector and the out pointer on the stack): the wheel worker's callee.
const MATVEC: u32 = 0x502400;
/// The car forwarder `CCar::Fwd18E20` (0x4e6650, thiscall: `dt` and the shaft speed on the stack, st0).
const FWD18E20: u32 = 0x4e6650;
/// Sim8E20's call of `E41EngineNoiseSignal::Emit` (0x50ef50, on the sound manager's +0x6050), which NGP retargets (to
/// 0x100ee000).
const EMIT_NOISE_SITE: u32 = 0x5191A8;
const GRAPH_PUSH: u32 = 0x494cf0; // CGraph1D::Push
pub(crate) const E01LOG: u32 = 0x43c8e0; // E01Log (cdecl)
const FLOOR_S32_FORMAT: u32 = 0x72dcb0; // "FloorS32: Invalid argument %f\n"

const fn bind(addr: u32, name: &'static str, run: crate::BindingFn) -> Binding {
    Binding { addr, name, subsystem: "engine_driveline", run }
}

pub static BINDINGS: [Binding; 99] = [
    bind(0x501fa0, "E39_MgrB_Word8", mgr_b_word8),
    bind(0x501fb0, "E39_MgrB_Entry", mgr_b_entry),
    bind(0x502060, "E39_MgrB_FieldAt", mgr_b_field_at),
    bind(0x502400, "E39_MgrD_Mat4Vec4", mat4_mul_vec4),
    bind(0x502460, "E39_MgrD_CopyRow", copy_row),
    bind(0x502570, "E39_RowsCopy", rows_copy),
    bind(0x502c00, "E39_ClearLanes", clear_lanes),
    bind(0x503860, "E39_Nop3860", nop_ret4),
    bind(0x503870, "E39_Step3870", free_wheel_step),
    bind(0x5043b0, "CDrive::Nop3B0", nop_ret4),
    bind(0x5043c0, "CDrive::SetTwo", set_two),
    bind(0x504450, "CDrive::Set30", set_brake_in),
    bind(0x5044a0, "CDrive::Set44A0", set_throttle_in),
    bind(0x504510, "CDrive::Set4510", drive_reset_part),
    bind(0x504520, "CDrive::Get4520", get_engine_speed),
    bind(0x504530, "CDrive::Get4530", get_gear),
    bind(0x504540, "CDrive::Get4540", drive_speed_key),
    bind(0x504570, "CDrive::Set4570", set_diff_lock),
    bind(0x5047b0, "CDrive::Set47B0", request_gear),
    bind(0x504820, "CDrive::Set4820", set_clutch_slip),
    bind(0x504840, "CDrive::Test4840", drive_gate),
    bind(0x504990, "E39_Step4990", mat4_inverse_transposed),
    bind(0x505970, "CDrive::M5970", set_lane_scale),
    bind(0x505a10, "CDrive::M5A10", gear_speed_error),
    bind(0x505b00, "CDrive::M5B00", pair_clutch_values),
    bind(0x505be0, "CDrive::M5BE0", clutch_values),
    bind(0x505d50, "CDrive::M5D50", clutch_flags),
    bind(0x505f10, "CDrive::M5F10", pair_slip_value),
    bind(0x5060d0, "CDrive::M60D0", pair_latch),
    bind(0x506540, "CDrive::M6540", pair_axle_stage),
    bind(0x506630, "CDrive::M6630", axle_pair_stages),
    bind(0x506a00, "CDrive::M6A00", block_lane_step),
    bind(0x506b20, "CDrive::M6B20", block_pair_gate),
    bind(0x506db0, "CDrive::M6DB0", m6db0),
    bind(0x506dd0, "CDrive::M6DD0", m6dd0),
    bind(0x506f20, "CSigMrRCarGearChange emit", emit_gear_change),
    bind(0x509bc0, "CDrive::M9BC0", wheel_worker),
    bind(0x5096c0, "CDrive::M96C0", pair_worker),
    bind(0x507aa0, "CDrive::M7AA0", reset_pair),
    bind(0x503ca0, "E39_RatioCA0", ring_average),
    bind(0x503e00, "E39_TabE00", unsigned_table_lerp),
    bind(0x503f60, "E39_WorkerF60", count_down),
    bind(0x504060, "E39_Worker4060", row_value),
    bind(0x5079b0, "CDrive::M79B0", signal_gear_change),
    bind(0x507c10, "CDrive::M7C10", reset_wheels),
    bind(0x508570, "CDrive::M8570", gear_trim),
    bind(0x509120, "CDrive::M9120", gear_band_guard),
    bind(0x509210, "CDrive::M9210", gear_logic),
    bind(0x50a1e0, "CDrive::Step1E0", step1e0),
    bind(0x50a4f0, "CDriveLine::Thunk4F0", thunk_4f0),
    bind(E12_SAMPLE_X, "E12_SampleX", e12_sample_x),
    bind(0x50a650, "CDriveLine::BlendB", blend_b),
    bind(0x50a920, "CDriveLine::LerpAt", lerp_at),
    bind(0x50ad40, "CDriveLine::Last", last),
    bind(0x50aec0, "CDriveLine::SetDriveLine", set_drive_line),
    bind(0x50b0f0, "CDriveLine::ClampMix", clamp_mix),
    bind(0x50ced0, "E41_BlendX", blend_tables_checked),
    bind(0x50d090, "E41_BlendY", blend_tables),
    bind(0x50d220, "E41_BlendW", find_bracket),
    bind(0x50d310, "CEngine::Set48", set_options),
    bind(0x50d340, "CEngine::SetState4", set_cut_flags),
    bind(0x50d380, "CEngine::Get48_58", options_gate_58),
    bind(0x50d410, "CEngine::Sim410", abs_rpm_term),
    bind(0x50d580, "E41_TableLerp", table_lerp),
    bind(0x50d6f0, "E41_UnkD6F0", split_unit_fraction),
    bind(0x50d7a0, "E41_CurveEta", curve_eta),
    bind(0x50d870, "CEngine::SimD870", slot_roll),
    bind(0x50e060, "CEngine::SimE060", smooth_damper),
    bind(0x50e1f0, "CEngine::SimE1F0", solve_damper),
    bind(0x50e370, "CEngine::SimE370", crank_tick),
    bind(0x50e470, "CEngine::SimE470", rev_gate),
    bind(0x50e520, "CEngine::SimE520", misfire_roll),
    bind(0x50e660, "CEngine::SimE660", engine_torque),
    bind(0x50e970, "CEngine::SimE970", sim_e970),
    bind(0x50e980, "CEngine::SimE980", rpm_above_40),
    bind(0x50e9a0, "CEngine::SimE9A0", sim_e9a0),
    bind(0x50e9f0, "CEngine::SimE9F0", compliance),
    bind(0x50eb80, "CEngine::GetState188", engine_state_speed),
    bind(0x50eb90, "CEngine::SetScratch08", set_scratch08),
    bind(0x50ec40, "CEngine::SimEC40", exhaust_blend),
    bind(0x50ecf0, "CEngine::SimECF0", stall_guard),
    bind(0x50ed70, "CEngine::SimED70", torque_curve),
    bind(0x50ee60, "CEngine::SimEE60", sim_ee60),
    bind(0x50ef50, "E41EngineNoiseSignal::Emit", emit_noise),
    bind(0x50f010, "E41Plain::InitRealLerp", init_real_lerp),
    bind(0x5180d0, "E43EngineSim::Sim80D0", intake_state_reset),
    bind(0x517d30, "E43EngineSim::Sim7D30", turbo_energy_step),
    bind(0x518610, "E43EngineSim::Sim8610", sim8610),
    bind(0x518890, "E43EngineSim::Sim8890", sim8890),
    bind(0x518b20, "E43EngineSim::Sim8B20", fuel_air_terms),
    bind(0x518cf0, "E43EngineSim::Sim8CF0", sim8cf0),
    bind(0x518d90, "E43EngineSim::Sim8D90", sim8d90),
    bind(0x518e20, "E43EngineSim::Sim8E20", engine_step),
    bind(0x473c90, "E11_VecLerp", vec_lerp),
    bind(0x50b300, "CDriveLine::SampleBig", sample_big),
    bind(0x50ebb0, "CEngine::SimEBB0", sim_ebb0),
    bind(0x508770, "CDrive::M8770", drive_setup),
    bind(0x503ab0, "E39_Link3AB0", link_worker),
    bind(0x50d300, "E41Plain::NopD300", nop_ret4),
];

fn f(bits: u32) -> f32 {
    f32::from_bits(bits)
}
