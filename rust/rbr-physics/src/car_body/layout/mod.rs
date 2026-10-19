//! Typed views of the car fields the batches read and write. Offsets are the original's (`notes/physics.md` "Car body",
//! `notes/physics/renames/car-body.csv`). The `load` and `store` functions below are the only code that knows them;
//! the ports work on the values. Each reads only its own words, so a window that maps part of the car still works.

mod body_points;
mod car;
mod car_calls;
mod car_parts;
mod construct;
mod damage_ctor;
mod damage_handler;

pub use body_points::{scatter_points, state_vf44x};
pub use car::{
    attach_lights, cell_at, damper_real_at, rate_b1c_at, scratch1690, state_address, steer_slew_at,
    steer_torque_sum_at, target_b0c_at, Real, ANGULAR_VELOCITY, DAMAGE_HANDLER, DIR_BACK, FLAG_50G, GRID_PUSHED,
    PARTIAL_CONTACT, RACK_RATE, SNAPSHOT_ON, SPEED, SPEED_PRE, STATE, STEER_INPUT, STEP_COUNT, T_OVERRIDE, VELOCITY,
    WALL_CONTACT, WALL_PENETRATION, WORLD,
};
pub use car_calls::{
    add_adc, assist_8df0, collide6000, lane_test, push_grid_edge, refresh_edge, reset_scratch_and_dispatch,
    respawn_car, retire_announce, run_countdown, set_a1c, set_brake_light, set_neutral_light, slot97, split_17c,
    update_ffb_torque, value_15f4_at, vf128x, vf158x, vote_probe, Collide6000Calls, Sampler54Calls, Slot97Calls,
    Vf128xCalls, Vf158xCalls, Vf180Calls, Vf98xCalls, DRIVE_LINE, SAMPLER_OPERAND,
};
pub use car_parts::{
    body_slot, drive_at, engine_at, passenger_at, scratch_at, wheel_list_at, wheel_slot, Seed, TimeKeeperDiff,
    FLOW_OBJECT, FRAME_COPY,
};
pub use construct::{
    body_def_after_parse, construct_body, construct_body_def, construct_car, construct_def, construct_light_def,
    construct_light_state, construct_lights, construct_options, construct_setup, construct_state, copy_setup, new_body,
    new_body_def, new_car, new_def, push_back_connection, BODY_DEF_AFTER_PARSE, BODY_DEF_SIZE, BODY_SIZE, CAR_SIZE,
    CURRENT_CAR, DEF_DANGEROUS_ACCELERATION, DEF_DRAG_COEFFICIENT, DEF_DRIVE, DEF_ENGINE, DEF_FRONTAL_AREA, DEF_SIZE,
    DEF_VCU,
};
pub use damage_ctor::{
    construct_damage_host, construct_damage_ops, damage_level_fractions, init_damage_handler_tables,
    init_damage_ops_tables, DamageOpsCalls, ResetPorts, DAMAGE_HOST_SIZE, DAMAGE_OPS_SIZE,
};
pub use damage_handler::{
    apply_functors, cap_word, clear_head_gasket, clear_triple, clear_windshield, damage_rand, damage_slot1, dmg_7b0,
    dmg_820, dmg_930, dmg_9a0, dmg_aa0, dmg_b20, dmg_bb0, dmg_cc30, dmg_d020, dmg_d300, drain_word, engine_gate,
    fail_brake_disc, fail_hub, fail_steering_rod, fail_suspension_arm, fail_water_pump, gearbox_wear,
    grow_cooling_wear, grow_intercooler_wear, grow_zone, impact_scale, ring_store, set_body_state, zero2f0,
    zone_scaled, DamageHitCalls, ImpactCalls,
};
