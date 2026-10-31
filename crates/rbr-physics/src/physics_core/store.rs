//! The physics core's store (`crate::store`): the words of the physics object, its state, the body list's nodes, the
//! collision events and the passenger that the step reads and writes. `Mem` implements it in `layout/mem_store.rs`,
//! which knows where each value lives.

use crate::physics_core::math::{BodyMotion, Vec4};
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
    /// The head of the body list and of the second list.
    fn bodies_head(&self, state: u32) -> u32;
    fn second_head(&self, state: u32) -> u32;
    fn node_next(&self, node: u32) -> u32;
    /// The function at the node's virtual slot `offset`.
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
    /// The drive of a car (the address the step's last call takes).
    fn player_drive(&self, car: u32) -> u32;

    // The passenger and its host.
    fn passenger_host(&self, passenger: u32) -> u32;
    fn passenger_step_host(&self, passenger: u32) -> u32;
    fn host_offset(&self, host: u32) -> [f32; 3];
    fn passenger_velocity(&self, passenger: u32) -> Vec4;
    /// The address of the passenger's velocity (the walk's out pointer).
    fn passenger_velocity_at(&self, passenger: u32) -> u32;
    /// The address of the host vector the pull's call leaves, which it returns.
    fn host_pull_at(&self, host: u32) -> u32;
    /// The host's two vectors the pull changes.
    fn host_pull_vectors(&self, host: u32) -> (Vec4, Vec4);
    fn store_host_pull_vectors(&mut self, host: u32, h40: Vec4, h90: Vec4);

    // The mounted body.
    /// The state word of a body's enable flag.
    fn mounted_enable(&self, body: u32) -> u32;
    fn mounted_car(&self, body: u32) -> u32;
    /// The body's NTweak base, its pose and the copy of its pose.
    fn mounted_base(&self, body: u32) -> u32;
    fn mounted_pose_at(&self, body: u32) -> u32;
    fn mounted_pose_copy_at(&self, body: u32) -> u32;
    /// The car's block the pose composes on (0 stays 0), and the car's pose.
    fn car_block(&self, car: u32) -> u32;
    fn car_pose(&self, car: u32) -> u32;
    fn mounted_owner(&self, body: u32) -> u32;
    fn set_mounted_dirty(&mut self, body: u32, value: u32);
    /// Copies the body's pose source to its target, as four words.
    fn copy_pose_source(&mut self, body: u32);

    // Signals: a `CMultiSignal1` and its connections.
    /// The address of the slot count (zero when there is none).
    fn signal_count_at(&self, signal: u32) -> u32;
    /// The guard word: set while an emission walks its connections.
    fn signal_guard(&self, signal: u32) -> u32;
    fn set_signal_guard(&mut self, signal: u32, value: u32);
    /// The head of a connection list (zero when empty).
    fn list_head(&self, list: u32) -> u32;
    /// The next link of a connection.
    fn link_next(&self, conn: u32) -> u32;
    /// The owner of a connection (the callback it notifies).
    fn conn_owner(&self, conn: u32) -> u32;
    /// The slot count the count pointer points at.
    fn signal_count(&self, count_at: u32) -> u32;
    /// The signal manager (`g_signalManager`).
    /// The body event and state signals of the manager.
    fn body_event_signal(&self) -> u32;
    fn body_state_signal(&self) -> u32;

    // The mounted body's motion.
    fn mounted_position(&self, body: u32) -> Vec4;
    fn set_mounted_position(&mut self, body: u32, value: Vec4);
    fn mounted_linear(&self, body: u32) -> Vec4;
    fn set_mounted_linear(&mut self, body: u32, value: Vec4);
    /// The velocity of the mount point, which the pull leaves, and its address.
    fn mount_velocity(&self, body: u32) -> Vec4;
    fn set_mount_velocity(&mut self, body: u32, value: Vec4);
    fn mount_velocity_at(&self, body: u32) -> u32;
    /// The pose of the mount on the car.
    fn mount_point(&self, car: u32) -> Vec4;
    fn mount_point_at(&self, car: u32) -> u32;
    /// The CDef's origin base address (null only for the sentinel CDef).
    fn origin_base(&self) -> u32;
    /// Copies the CDef's origin vector to `to`.
    fn copy_origin(&mut self, base: u32, to: u32);
    fn mounted_key(&self, body: u32) -> u32;
    fn mounted_event_sent(&self, body: u32) -> u32;
    fn set_mounted_event_sent(&mut self, body: u32, value: u32);
    /// Copies the body's rotation or position vector, four words, to `to`.
    fn copy_mounted_rotation(&mut self, body: u32, to: u32);
    fn copy_mounted_position(&mut self, body: u32, to: u32);
    /// The motion terms of the body the point velocity reads.
    fn body_motion(&self, body: u32) -> BodyMotion;

    // The force accumulation.
    /// Whether the accumulate's hook runs: the log flag 0x4000 in the CDef's word or the physics object's.
    fn accumulate_hook(&self) -> bool;
    /// The inverse mass of the body's object.
    fn inverse_mass(&self, body: u32) -> f32;
    /// The address of the walk's output in the accumulate's locals (`temp`: the gravity copy, then the output).
    fn accumulate_out(&self, temp: u32) -> u32;
    /// Copies the gravity vector (the CDef's) to `to`, as four words.
    fn copy_gravity(&mut self, to: u32);

    /// Copies the 16 bytes of the vector at `from` to `to` (as four words, so the bits are kept).
    fn copy_vec4(&mut self, from: u32, to: u32) {
        let words = [0, 4, 8, 12].map(|k| self.u32(from + k));
        for (k, value) in [0, 4, 8, 12].into_iter().zip(words) {
            self.set_u32(to + k, value);
        }
    }
}
