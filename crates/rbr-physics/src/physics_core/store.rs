//! The physics core's store (`crate::store`): the words of the physics object, its state, the body list's nodes, the
//! collision events and the passenger that the step reads and writes. `Mem` implements it in `layout/mem_store.rs`,
//! which knows where each value lives.

use crate::physics_core::math::Vec4;
use crate::store::Raw;

pub trait CoreStore: Raw {
    // The physics object (`CPhysics`) at `this`, and its CDef.
    /// The CDef's step flags (the global physics object's): bit 0x10 collide window, 0x20 penetration pass.
    fn step_flags(&self) -> u32;
    /// The CDef flags the contact loop reads (through the other physics global).
    fn collide_flags(&self) -> u32;
    /// The gravity the passenger's walk starts from (three floats in the CDef), or None for the sentinel pointer.
    fn gravity_point(&self) -> Option<[f32; 3]>;
    /// The base step time (the CDef's), s.
    fn step_interval(&self, this: u32) -> f32;
    fn single_step_pending(&self, this: u32) -> u32;
    fn single_step_delay(&self, this: u32) -> u32;
    fn set_single_step_delay(&mut self, this: u32, value: u32);
    /// The state (`CState`) of the physics object (its owner of the body lists).
    fn physics_state(&self, this: u32) -> u32;
    fn set_contact_hit(&mut self, this: u32, hits: u32);
    /// The solver, in the physics object's scratch.
    fn solver(&self, this: u32) -> u32;
    /// The head of the collision event list the scratch holds.
    fn scratch_events(&self, this: u32) -> u32;
    /// The contact loop's counters: passes, the pass maximum, and the solver's hits (last and maximum).
    fn pass_count(&self, this: u32) -> u32;
    fn set_pass_count(&mut self, this: u32, value: u32);
    fn pass_maximum(&self, this: u32) -> u32;
    fn set_pass_maximum(&mut self, this: u32, value: u32);
    fn set_hits(&mut self, this: u32, value: u32);
    fn set_hits_last(&mut self, this: u32, value: u32);
    fn hits_maximum(&self, this: u32) -> u32;
    fn set_hits_maximum(&mut self, this: u32, value: u32);

    // The state and the nodes of its lists.
    fn step_counter(&self, state: u32) -> u32;
    fn set_step_counter(&mut self, state: u32, value: u32);
    /// Marks the state's NTweak base dirty.
    fn mark_state_dirty(&mut self, state: u32);
    /// The head of the body list (+0x450) and of the second list (+0x480).
    fn bodies_head(&self, state: u32) -> u32;
    fn second_head(&self, state: u32) -> u32;
    fn node_next(&self, node: u32) -> u32;
    /// The function at the node's vtable byte offset.
    fn node_slot(&self, node: u32, offset: u32) -> u32;
    fn elapsed(&self, state: u32) -> f64;
    fn set_elapsed(&mut self, state: u32, value: f64);

    // The collision events and their solids.
    fn event_next(&self, event: u32) -> u32;
    fn event_solid(&self, event: u32, at: u32) -> u32;
    fn event_penetration(&self, event: u32) -> f32;
    /// Whether the solid carries the flag the contact step moves it by.
    fn solid_advanced(&self, solid: u32) -> bool;

    // Globals.
    fn telemetry(&self) -> u32;
    fn set_step_index(&mut self, step: u32);
    fn count_step_call(&mut self);
    fn player_car(&self) -> u32;

    // The passenger and its host.
    fn passenger_host(&self, passenger: u32) -> u32;
    fn passenger_step_host(&self, passenger: u32) -> u32;
    fn host_offset(&self, host: u32) -> [f32; 3];
    fn passenger_velocity(&self, passenger: u32) -> Vec4;
    /// The address of the passenger's velocity (the walk's out pointer).
    fn passenger_velocity_at(&self, passenger: u32) -> u32;
    /// The address of the host vector the pull's call leaves (its +0x90), which the original returns.
    fn host_pull_at(&self, host: u32) -> u32;
    /// The host's two vectors the pull changes (+0x40, +0x90).
    fn host_pull_vectors(&self, host: u32) -> (Vec4, Vec4);
    fn store_host_pull_vectors(&mut self, host: u32, h40: Vec4, h90: Vec4);

    // The mounted body.
    /// The state word of a body's enable flag (the pointer at +0x580 points at it).
    fn mounted_enable(&self, body: u32) -> u32;
    fn mounted_car(&self, body: u32) -> u32;
    fn mounted_owner(&self, body: u32) -> u32;
    fn set_mounted_dirty(&mut self, body: u32, value: u32);
    /// Copies the 16 bytes of the body's pose source (+0x100) to its target (+0x2a0), as four words.
    fn copy_pose_source(&mut self, body: u32);
}
