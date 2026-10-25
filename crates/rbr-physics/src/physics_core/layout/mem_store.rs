//! The Mem store of the physics core: `CoreStore` over the game's memory, with the offsets of the physics object, its
//! state, the body list's nodes, the collision events and the passenger.

use super::globals::{physics, physics_global, SIGNAL_MANAGER};
use super::prof_calls::{set_word64, word64, LOG_FLAGS_AT, LOG_HOOK_FLAG, PHYSICS_FLAGS_AT};
use crate::car_body::layout::CURRENT_CAR;
use crate::collision::layout::sort::event_get_94;
use crate::mem::Mem;
use crate::physics_core::contact::advanced_by_contact_step;
use crate::physics_core::layout::TELEMETRY_GLOBAL;
use crate::physics_core::math::{BodyMotion, Vec4};
use crate::physics_core::store::CoreStore;

// The physics object.
const PHYSICS_DEF_AT: u32 = 0x24;
const PHYSICS_STATE_AT: u32 = 0x20;
const PHYSICS_SCRATCH_AT: u32 = 0x28;
const CONTACT_HIT_AT: u32 = 0x1c;
const SINGLE_DELAY_AT: u32 = 0x5c;
const SINGLE_PENDING_AT: u32 = 0x60;
const HITS_AT: u32 = 0x4;
const HITS_LAST_AT: u32 = 0x8;
const HITS_MAXIMUM_AT: u32 = 0xc;
const PASS_MAXIMUM_AT: u32 = 0x14;
const PASS_COUNT_AT: u32 = 0x18;
// The CDef.
const STEP_FLAGS_AT: u32 = 0x160cc;
const COLLIDE_FLAGS_AT: u32 = 0x160e0;
const STEP_INTERVAL_AT: u32 = 0x1608c;
const GRAVITY_AT: u32 = 0x160b0;
/// The sentinel -0x160a0 in the CDef's place makes the original read address 0.
const NO_GRAVITY: u32 = 0x160a0u32.wrapping_neg();
// The state and its nodes.
const STATE_COUNTER_AT: u32 = 0x18;
const STATE_TIME_AT: u32 = 0x20;
const STATE_DIRTY_AT: u32 = 0x0c;
const STATE_BODIES_AT: u32 = 0x450;
const STATE_SECOND_AT: u32 = 0x480;
const NODE_NEXT_AT: u32 = 0x10;
// The collision events.
const EVENT_NEXT_AT: u32 = 0xc;
const SOLID_FLAGS_AT: u32 = 0x60;
// Globals.
const CAR_DRIVE_AT: u32 = 0x4b0;
const STEP_INDEX_GLOBAL: u32 = 0x0165_9b78;
const STEP_CALL_COUNT_GLOBAL: u32 = 0x0165_9b74;
// The passenger and its host.
const PASSENGER_HOST_AT: u32 = 0x14;
const STEP_HOST_AT: u32 = 0x40;
const VELOCITY_AT: u32 = 0x30;
const HOST_H40_AT: u32 = 0x40;
const HOST_H90_AT: u32 = 0x90;
const HOST_OFFSET_AT: u32 = 0x230;
// The mounted body.
const BODY_EVENT_SIGNAL: u32 = 0xec98;
const BODY_STATE_SIGNAL: u32 = 0xe438;
const MOUNTED_POSITION_AT: u32 = 0x140;
const MOUNTED_BASE_AT: u32 = 0x1d0;
const MOUNTED_COPY_AT: u32 = 0x330;
const CAR_BLOCK_AT: u32 = 0x200;
const CAR_POSE_AT: u32 = 0x220;
const MOUNTED_LINEAR_AT: u32 = 0x1c0;
const MOUNT_VELOCITY_AT: u32 = 0x230;
const MOUNT_POINT_AT: u32 = 0x220;
const ORIGIN_BASE_AT: u32 = 0x160a0;
const MOUNTED_KEY_AT: u32 = 0x30;
const MOUNTED_EVENT_SENT_AT: u32 = 0x584;
const MOUNTED_ENABLE_AT: u32 = 0x580;
const MOUNTED_DIRTY_AT: u32 = 0x1dc;
const MOUNTED_CAR_AT: u32 = 0x34;
const MOUNTED_OWNER_AT: u32 = 0x3d0;
const POSE_SOURCE_AT: u32 = 0x100;
const POSE_TARGET_AT: u32 = 0x2a0;

impl CoreStore for Mem {
    fn step_flags(&self) -> u32 {
        let def = self.u32(physics_global(self) + PHYSICS_DEF_AT);
        self.u32(def + STEP_FLAGS_AT)
    }

    fn collide_flags(&self) -> u32 {
        self.u32(self.u32(physics(self) + PHYSICS_DEF_AT) + COLLIDE_FLAGS_AT)
    }

    fn gravity_point(&self) -> Option<[f32; 3]> {
        let def = self.u32(physics_global(self) + PHYSICS_DEF_AT);
        if def == NO_GRAVITY {
            return None;
        }
        let at = def + GRAVITY_AT;
        Some([0, 4, 8].map(|k| self.f32(at + k)))
    }

    fn step_interval(&self, this: u32) -> f32 {
        self.f32(self.u32(this + PHYSICS_DEF_AT) + STEP_INTERVAL_AT)
    }

    fn single_step_pending(&self, this: u32) -> u32 {
        self.u32(this + SINGLE_PENDING_AT)
    }

    fn single_step_delay(&self, this: u32) -> u32 {
        self.u32(this + SINGLE_DELAY_AT)
    }

    fn set_single_step_delay(&mut self, this: u32, value: u32) {
        self.set_u32(this + SINGLE_DELAY_AT, value);
    }

    fn physics_state(&self, this: u32) -> u32 {
        self.u32(this + PHYSICS_STATE_AT)
    }

    fn set_contact_hit(&mut self, this: u32, hits: u32) {
        self.set_u32(this + CONTACT_HIT_AT, hits);
    }

    fn solver(&self, this: u32) -> u32 {
        self.u32(this + PHYSICS_SCRATCH_AT) + 0x10
    }

    fn scratch_events(&self, this: u32) -> u32 {
        self.u32(self.u32(this + PHYSICS_SCRATCH_AT) + 0x14)
    }

    fn pass_count(&self, this: u32) -> u32 {
        self.u32(this + PASS_COUNT_AT)
    }

    fn set_pass_count(&mut self, this: u32, value: u32) {
        self.set_u32(this + PASS_COUNT_AT, value);
    }

    fn pass_maximum(&self, this: u32) -> u32 {
        self.u32(this + PASS_MAXIMUM_AT)
    }

    fn set_pass_maximum(&mut self, this: u32, value: u32) {
        self.set_u32(this + PASS_MAXIMUM_AT, value);
    }

    fn set_hits(&mut self, this: u32, value: u32) {
        self.set_u32(this + HITS_AT, value);
    }

    fn set_hits_last(&mut self, this: u32, value: u32) {
        self.set_u32(this + HITS_LAST_AT, value);
    }

    fn hits_maximum(&self, this: u32) -> u32 {
        self.u32(this + HITS_MAXIMUM_AT)
    }

    fn set_hits_maximum(&mut self, this: u32, value: u32) {
        self.set_u32(this + HITS_MAXIMUM_AT, value);
    }

    fn step_counter(&self, state: u32) -> u32 {
        self.u32(state + STATE_COUNTER_AT)
    }

    fn set_step_counter(&mut self, state: u32, value: u32) {
        self.set_u32(state + STATE_COUNTER_AT, value);
    }

    fn mark_state_dirty(&mut self, state: u32) {
        crate::ntweak::layout::set_dirty(self, state + STATE_DIRTY_AT);
    }

    fn bodies_head(&self, state: u32) -> u32 {
        self.u32(state + STATE_BODIES_AT)
    }

    fn second_head(&self, state: u32) -> u32 {
        self.u32(state + STATE_SECOND_AT)
    }

    fn node_next(&self, node: u32) -> u32 {
        self.u32(node + NODE_NEXT_AT)
    }

    fn node_slot(&self, node: u32, offset: u32) -> u32 {
        self.u32(self.u32(node) + offset)
    }

    fn elapsed(&self, state: u32) -> f64 {
        f64::from_bits(word64(self, state + STATE_TIME_AT))
    }

    fn set_elapsed(&mut self, state: u32, value: f64) {
        set_word64(self, state + STATE_TIME_AT, value.to_bits());
    }

    fn event_next(&self, event: u32) -> u32 {
        self.u32(event + EVENT_NEXT_AT)
    }

    fn event_solid(&self, event: u32, at: u32) -> u32 {
        self.u32(event + at)
    }

    fn event_penetration(&self, event: u32) -> f32 {
        event_get_94(self, event)
    }

    fn solid_advanced(&self, solid: u32) -> bool {
        advanced_by_contact_step(self.u8(solid + SOLID_FLAGS_AT))
    }

    fn telemetry(&self) -> u32 {
        self.u32(TELEMETRY_GLOBAL)
    }

    fn set_step_index(&mut self, step: u32) {
        self.set_u32(STEP_INDEX_GLOBAL, step);
    }

    fn count_step_call(&mut self) {
        let calls = self.u32(STEP_CALL_COUNT_GLOBAL).wrapping_add(1);
        self.set_u32(STEP_CALL_COUNT_GLOBAL, calls);
    }

    fn player_car(&self) -> u32 {
        self.u32(CURRENT_CAR)
    }

    fn player_drive(&self, car: u32) -> u32 {
        car + CAR_DRIVE_AT
    }

    fn passenger_host(&self, passenger: u32) -> u32 {
        self.u32(passenger + PASSENGER_HOST_AT)
    }

    fn passenger_step_host(&self, passenger: u32) -> u32 {
        self.u32(passenger + STEP_HOST_AT)
    }

    fn host_offset(&self, host: u32) -> [f32; 3] {
        [0, 4, 8].map(|k| self.f32(host + HOST_OFFSET_AT + k))
    }

    fn passenger_velocity(&self, passenger: u32) -> Vec4 {
        Vec4::load(self, passenger + VELOCITY_AT)
    }

    fn passenger_velocity_at(&self, passenger: u32) -> u32 {
        passenger + VELOCITY_AT
    }

    fn host_pull_at(&self, host: u32) -> u32 {
        host + HOST_H90_AT
    }

    fn host_pull_vectors(&self, host: u32) -> (Vec4, Vec4) {
        (Vec4::load(self, host + HOST_H40_AT), Vec4::load(self, host + HOST_H90_AT))
    }

    fn store_host_pull_vectors(&mut self, host: u32, h40: Vec4, h90: Vec4) {
        h40.store(self, host + HOST_H40_AT);
        h90.store(self, host + HOST_H90_AT);
    }

    fn mounted_enable(&self, body: u32) -> u32 {
        self.u32(self.u32(body + MOUNTED_ENABLE_AT))
    }

    fn mounted_car(&self, body: u32) -> u32 {
        self.u32(body + MOUNTED_CAR_AT)
    }

    fn mounted_base(&self, body: u32) -> u32 {
        body + MOUNTED_BASE_AT
    }

    fn mounted_pose_at(&self, body: u32) -> u32 {
        body + MOUNTED_POSITION_AT
    }

    fn mounted_pose_copy_at(&self, body: u32) -> u32 {
        body + MOUNTED_COPY_AT
    }

    fn car_block(&self, car: u32) -> u32 {
        match car.wrapping_add(CAR_BLOCK_AT) {
            0 => 0,
            block => block + 0x10,
        }
    }

    fn car_pose(&self, car: u32) -> u32 {
        car.wrapping_add(CAR_POSE_AT)
    }

    fn mounted_owner(&self, body: u32) -> u32 {
        self.u32(body + MOUNTED_OWNER_AT)
    }

    fn set_mounted_dirty(&mut self, body: u32, value: u32) {
        self.set_u32(body + MOUNTED_DIRTY_AT, value);
    }

    fn copy_pose_source(&mut self, body: u32) {
        let words = [0, 4, 8, 12].map(|k| self.u32(body + POSE_SOURCE_AT + k));
        for (k, value) in [0, 4, 8, 12].into_iter().zip(words) {
            self.set_u32(body + POSE_TARGET_AT + k, value);
        }
    }

    fn signal_count_at(&self, signal: u32) -> u32 {
        self.u32(signal + 4)
    }

    fn signal_guard(&self, signal: u32) -> u32 {
        self.u32(signal)
    }

    fn set_signal_guard(&mut self, signal: u32, value: u32) {
        self.set_u32(signal, value);
    }

    fn list_head(&self, list: u32) -> u32 {
        self.u32(list)
    }

    fn link_next(&self, conn: u32) -> u32 {
        self.u32(conn + 4)
    }

    fn conn_owner(&self, conn: u32) -> u32 {
        self.u32(conn + 0x10)
    }

    fn signal_count(&self, count_at: u32) -> u32 {
        self.u32(count_at)
    }

    fn body_event_signal(&self) -> u32 {
        self.u32(SIGNAL_MANAGER) + BODY_EVENT_SIGNAL
    }

    fn body_state_signal(&self) -> u32 {
        self.u32(SIGNAL_MANAGER) + BODY_STATE_SIGNAL
    }

    fn mounted_position(&self, body: u32) -> Vec4 {
        Vec4::load(self, body + MOUNTED_POSITION_AT)
    }

    fn set_mounted_position(&mut self, body: u32, value: Vec4) {
        value.store(self, body + MOUNTED_POSITION_AT);
    }

    fn mounted_linear(&self, body: u32) -> Vec4 {
        Vec4::load(self, body + MOUNTED_LINEAR_AT)
    }

    fn set_mounted_linear(&mut self, body: u32, value: Vec4) {
        value.store(self, body + MOUNTED_LINEAR_AT);
    }

    fn mount_velocity(&self, body: u32) -> Vec4 {
        Vec4::load(self, body + MOUNT_VELOCITY_AT)
    }

    fn set_mount_velocity(&mut self, body: u32, value: Vec4) {
        value.store(self, body + MOUNT_VELOCITY_AT);
    }

    fn mount_velocity_at(&self, body: u32) -> u32 {
        body + MOUNT_VELOCITY_AT
    }

    fn mount_point(&self, car: u32) -> Vec4 {
        Vec4::load(self, car + MOUNT_POINT_AT)
    }

    fn mount_point_at(&self, car: u32) -> u32 {
        car + MOUNT_POINT_AT
    }

    fn copy_origin(&mut self, base: u32, to: u32) {
        CoreStore::copy_vec4(self, base + 0x10, to);
    }

    fn origin_base(&self) -> u32 {
        self.u32(physics_global(self) + PHYSICS_DEF_AT).wrapping_add(ORIGIN_BASE_AT)
    }

    fn mounted_key(&self, body: u32) -> u32 {
        self.u32(body + MOUNTED_KEY_AT)
    }

    fn mounted_event_sent(&self, body: u32) -> u32 {
        self.u32(body + MOUNTED_EVENT_SENT_AT)
    }

    fn set_mounted_event_sent(&mut self, body: u32, value: u32) {
        self.set_u32(body + MOUNTED_EVENT_SENT_AT, value);
    }

    fn copy_mounted_rotation(&mut self, body: u32, to: u32) {
        CoreStore::copy_vec4(self, body + POSE_SOURCE_AT, to);
    }

    fn copy_mounted_position(&mut self, body: u32, to: u32) {
        CoreStore::copy_vec4(self, body + MOUNTED_POSITION_AT, to);
    }

    fn body_motion(&self, body: u32) -> BodyMotion {
        BodyMotion {
            position: Vec4::load(self, body + MOUNTED_POSITION_AT),
            angular: Vec4::load(self, body + 0x190),
            lever: Vec4::load(self, body + 0x260),
            base: Vec4::load(self, body + 0x250),
        }
    }

    fn accumulate_hook(&self) -> bool {
        let physics = physics_global(self);
        let log_flags = self.u32(self.u32(physics + PHYSICS_DEF_AT) + LOG_FLAGS_AT);
        (log_flags | self.u32(physics + PHYSICS_FLAGS_AT)) & LOG_HOOK_FLAG != 0
    }

    fn accumulate_out(&self, temp: u32) -> u32 {
        temp + 0x10
    }

    fn inverse_mass(&self, body: u32) -> f32 {
        self.f32(self.u32(body + 0x34) + 0x16c)
    }

    fn copy_gravity(&mut self, to: u32) {
        let def = self.u32(physics_global(self) + PHYSICS_DEF_AT);
        CoreStore::copy_vec4(self, def + GRAVITY_AT, to);
    }
}
