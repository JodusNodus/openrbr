//! Bindings of the car-body ports (`rbr_physics::car_body`, exe 0x4e4d10-0x4ea980, 0x4eb820, 0x4f3d20-0x4f3ec0). The ABIs
//! are the original's (thiscall `this` in ecx, the stack words `ret` pops). The car's and body's slots, the walks and the
//! compose calls are the original's: a binding calls them through the host, and a bound callee runs in Rust again.

use crate::{physics_core, Binding};

mod assist;
mod car_calls;
mod damage_ops;
mod members;
mod seed;
mod step;

use assist::{assist_8df0, lane_test, retire_a, vf158x};
use car_calls::{
    body_compose, drag_and_spin_damp, load_binary, min_pick, push_grid_edge, refresh_edge, sim7ad0, to_world,
    update_ffb_torque, world_compose, xform_8130,
};
use damage_ops::{
    add_damage_functor, apply_functors, cap_word, clear_head_gasket, clear_triple, clear_windshield,
    damage_impact_scale, damage_level_fractions, damage_ops_ctor, damage_ops_init_tables, damage_rand, damage_slot1,
    dmg_7b0, dmg_820, dmg_930, dmg_9a0, dmg_aa0, dmg_b20, dmg_bb0, dmg_cc30, dmg_d020, dmg_d300, drain_word,
    engine_gate, fail_brake_disc, fail_hub, fail_steering_rod, fail_suspension_arm, fail_water_pump, gearbox_wear,
    grow_cooling_wear, grow_intercooler_wear, grow_zone, ring_store, set_body_state, test_3b20, zero2f0, zone_scaled,
};
use members::{
    add_adc, body_to_world_pair, body_vf40x, body_vf80x, car_vf40x, car_vf_d8x, clear480, collide6000, emit_init,
    flow_m566640, flow_m5666b0, fwd18e20, fwd43c0, fwd6db0, fwd6dd0, get_15f8, get_30, get_a48, get_b0c, get_b1c,
    light_attach, motion_probe, reset_scratch_and_dispatch, set_a1c, set_brake_light, set_neutral_light, state_atb,
    state_vf44x, sub_block_init, subtract_points, to_quat, update_dampers, vf128x, vf24x,
};
use seed::{append_a980, replay_is_state2, update_a720, update_mrr, update_seed, EMIT_CAR_INIT};
use step::{
    emit, emit_signals, ghost_set_playback, respawn_car, run_countdown, s184, snap_9360, split_17c, step_aero,
    step_car, step_engine, vote_probe,
};

/// `CCar::UpdateSeed` (0x4e91f0), the car's control input: thiscall `(payload)`.
pub(crate) const UPDATE_SEED: u32 = 0x4E91F0;
/// `E37GridOps::AddEdge` (0x4f65b0).
pub(crate) const ADD_EDGE: u32 = 0x4F65B0;

const fn bind(addr: u32, name: &'static str, run: crate::BindingFn) -> Binding {
    Binding { addr, name, subsystem: "car_body", run }
}

pub static BINDINGS: [Binding; 116] = [
    bind(0x4e58f0, "CCar::Clear480", clear480),
    bind(0x4e6000, "CCar::Collide6000", collide6000),
    bind(0x4e6dd0, "CCar::Vf98x", run_countdown),
    bind(0x4e5530, "CCar::Split17C", split_17c),
    bind(0x4e54a0, "CCar::Sampler54A0", vote_probe),
    bind(0x4e5270, "CCar::S184", s184),
    bind(0x4e9660, "CCar::Vf180", respawn_car),
    bind(0x4e8830, "CSigRespawn::Emit", emit::<0x738778>),
    bind(0x492e90, "CGhostCarManager::SetPlayback", ghost_set_playback),
    bind(0x4e6100, "CCar::Vf128x", vf128x),
    bind(0x4eaf10, "CCar::VfD8x", car_vf_d8x),
    bind(0x4e5c60, "CCar::Vf5cx", physics_core::dispatch_vf5c),
    bind(0x4e6600, "CCar::Fwd6DB0", fwd6db0),
    bind(0x4e6610, "CCar::Fwd6DD0", fwd6dd0),
    bind(0x4e6620, "CCar::Fwd43C0", fwd43c0),
    bind(0x4e67f0, "CCar::Get30", get_30),
    bind(0x4e6770, "CCar::Get15F8", get_15f8),
    bind(0x5666b0, "E60Flow::M5666b0", flow_m5666b0),
    bind(0x566640, "E60Flow::M566640", flow_m566640),
    bind(0x4e6650, "CCar::Fwd18E20", fwd18e20),
    bind(0x4e7fb0, "CCar::Vf40x", car_vf40x),
    bind(0x4eb7b0, "CCarBody::Vf80x", body_vf80x),
    bind(0x4eb960, "CCarBody::Vf40x", body_vf40x),
    bind(0x4f1e40, "CCarLightSystem::Set1e40", set_brake_light),
    bind(0x4f1ed0, "CCarLightSystem::Set1ed0", set_neutral_light),
    bind(0x4e4d10, "E34Quat::ToQuat", to_quat),
    bind(0x4e58e0, "CCar::Vf24x", vf24x),
    bind(0x4e65b0, "CCar::StateAtb", state_atb),
    bind(0x4e65e0, "CCar::GetB0C", get_b0c),
    bind(0x4e65f0, "CCar::GetB1C", get_b1c),
    bind(0x4e6660, "CCar::AddADC", add_adc),
    bind(0x4e66a0, "CCar::SetA1C", set_a1c),
    bind(0x4e66d0, "CCar::Damp66D0", update_dampers),
    bind(0x4e6730, "CCar::GetA48", get_a48),
    bind(0x4eb820, "CCarBody::Vf9Cx", subtract_points),
    bind(0x4eb6a0, "CCarBody::CState::Vf44x", state_vf44x),
    bind(0x4f3e80, "NTweak::CE36SubBlock::Init", sub_block_init),
    bind(0x4f3d20, "NTweak::CFloat3::LoadBinary", load_binary),
    bind(0x4f3ec0, "E36Triple::MinPick", min_pick),
    bind(0x4e68e0, "CCar::ToWorld", to_world),
    bind(0x4e6950, "CCar::WorldCompose", world_compose),
    bind(0x4e6a30, "CCar::BodyCompose", body_compose),
    bind(0x4e8130, "CCar::Xform8130", xform_8130),
    bind(0x4e7270, "CCar::PushGridEdge", push_grid_edge),
    bind(0x4e72d0, "CCar::RefreshEdge", refresh_edge),
    bind(0x4e7730, "CCar::MotionProbe", motion_probe),
    bind(0x4e7f90, "CCar::Vf54x", reset_scratch_and_dispatch),
    bind(0x4e7ad0, "CCar::Sim7AD0", sim7ad0),
    bind(0x4e7dc0, "CCar::Sim7DC0", drag_and_spin_damp),
    bind(0x4e7ff0, "CCar::UpdateFfbTorque", update_ffb_torque),
    bind(0x4e8b30, "CSigCarPosition emit", emit::<0x738950>),
    bind(0x4e88f0, "CSigCarStandStill emit", emit::<0x7387e8>),
    bind(0x4e8bf0, "CSigMrRCarState emit", emit::<0x7389c8>),
    bind(0x4e9020, "CCar::Vf64x", step_aero),
    bind(0x4ea300, "CCar::Vf6cx", step_car),
    bind(0x4e9930, "CCar::StepEngine", step_engine),
    bind(0x4e9af0, "CCar::Update9AF0", emit_signals),
    bind(0x4ea980, "CCar::AppendA980", append_a980),
    bind(0x4ea720, "CCar::UpdateA720", update_a720),
    bind(0x4e9c70, "CCar::UpdateMrr", update_mrr),
    bind(0x4e91f0, "CCar::UpdateSeed", update_seed),
    bind(0x4e7910, "CCar::Test7910", lane_test),
    bind(0x4e8df0, "CCar::Assist8DF0", assist_8df0),
    bind(0x4e5ee0, "CCar::Vf158x", vf158x),
    bind(0x4e8fa0, "E34_RetireA", retire_a),
    bind(0x4e8770, "CSigRetireCar emit", emit::<0x738708>),
    bind(0x4e89b0, "CSigOutsideAssistance emit", emit::<0x738860>),
    bind(0x5665f0, "E60Flow::M5665f0", flow_m566640),
    bind(0x499900, "E19IsState2", replay_is_state2),
    bind(0x4ec0d0, "CCarDamageHandler::Test3B20", test_3b20),
    bind(0x4ec140, "CCarDamageHandler::ImpactScale", damage_impact_scale),
    bind(0x4f12a0, "E36DamageOps::Slot1", damage_slot1),
    bind(0x4edee0, "CCarDamageHandler::ZoneScaled", zone_scaled),
    bind(0x4ec560, "CCarDamageHandler::ApplyFunctors", apply_functors),
    bind(0x4ef680, "E36DamageOps::EngineGate", engine_gate),
    bind(0x4ef5f0, "E36DamageOps::RingStore", ring_store),
    bind(0x4ebd70, "E35_Rand", damage_rand),
    bind(0x4ef1d0, "CSigCarDamage emit", emit::<0x7390a8>),
    bind(0x4ef290, "CSigEngineFire emit", emit::<0x739118>),
    bind(0x4ae0d0, "CSigCarDamageSpeech emit", emit::<0x731a88>),
    bind(0x4ef410, "E36DamageOps::Ctor", damage_ops_ctor),
    bind(0x4ef720, "E36DamageOps::AddFunctor", add_damage_functor),
    bind(0x4ef910, "E36DamageOps::InitTables", damage_ops_init_tables),
    bind(0x4f1000, "E36DamageOps::Slot4", damage_level_fractions),
    bind(0x4ec660, "CCarDamageHandler::Dmg660", gearbox_wear),
    bind(0x4ec720, "CCarDamageHandler::Dmg720", grow_cooling_wear),
    bind(0x4eca00, "CCarDamageHandler::DmgA00", grow_zone),
    bind(0x4ecf20, "CCarDamageHandler::DmgCF20", clear_triple),
    bind(0x4ecfa0, "CCarDamageHandler::DmgCFA0", clear_head_gasket),
    bind(0x4ecfe0, "CCarDamageHandler::DmgCFE0", clear_windshield),
    bind(0x4ec9a0, "CCarDamageHandler::Dmg9A0", dmg_9a0),
    bind(0x4ec7b0, "CCarDamageHandler::Dmg7B0", dmg_7b0),
    bind(0x4ecaa0, "CCarDamageHandler::DmgAA0", dmg_aa0),
    bind(0x4ecb20, "CCarDamageHandler::DmgB20", dmg_b20),
    bind(0x4ec890, "CCarDamageHandler::Dmg890", drain_word),
    bind(0x4ec930, "CCarDamageHandler::Dmg930", dmg_930),
    bind(0x4ec820, "CCarDamageHandler::Dmg820", dmg_820),
    bind(0x4eceb0, "CCarDamageHandler::DmgCEB0", cap_word),
    bind(0x4ecbb0, "CCarDamageHandler::DmgBB0", dmg_bb0),
    bind(0x4ecc30, "CCarDamageHandler::DmgCC30", dmg_cc30),
    bind(0x4eccb0, "CCarDamageHandler::DmgCCB0", fail_suspension_arm),
    bind(0x4ecd30, "CCarDamageHandler::DmgCD30", fail_steering_rod),
    bind(0x4ecdb0, "CCarDamageHandler::DmgCDB0", fail_hub),
    bind(0x4ece30, "CCarDamageHandler::DmgCE30", fail_brake_disc),
    bind(0x4ed020, "CCarDamageHandler::DmgD020", dmg_d020),
    bind(0x4ed0f0, "CCarDamageHandler::DmgD0F0", fail_water_pump),
    bind(0x4ed160, "CCarDamageHandler::DmgD160", grow_intercooler_wear),
    bind(0x4ed200, "CCarDamageHandler::DmgD200", set_body_state),
    bind(0x4ed2f0, "NPhysics::E35_Zero2F0", zero2f0),
    bind(0x4ed300, "CCarDamageHandler::DmgD300", dmg_d300),
    bind(0x4ef350, "CMultiSignal1<CSigCarDamageHandlerInit>::Emit", emit_init),
    bind(EMIT_CAR_INIT, "CMultiSignal1<CSigMrRCarInit>::Emit", emit_init),
    bind(0x4f1e00, "CCarLightSystem::Attach", light_attach),
    bind(0x4e69c0, "CCar::BodyToWorldPair", body_to_world_pair),
    bind(0x4e9360, "CCar::Snap9360", snap_9360),
    bind(0x4e85b0, "CSigCarStateAnimatedReplay emit", emit::<0x7385f8>),
];
