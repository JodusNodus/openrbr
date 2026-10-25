//! The entities' store (`crate::store`): the layout's load and store groups. `Mem` implements it in `layout/`.

use super::frame::EntityFrame;
use super::ground::Contact;
use super::hinge_frame_walks::FrameWalkInput;
use super::hinge_limits::{HingeState, LimitSpring, RotationBlock};
use super::hinge_offset::OffsetInput;
use super::hinge_pivot::{Pivot, PivotInput};
use super::hinge_pose::{PoseHandles, PoseState};
use super::hinge_refresh::{RefreshHandles, RefreshState};
use super::hinge_signals::SignalInput;
use super::hinge_slip::SlipInput;
use super::hinge_solve::{EntryTerms, SolveTerms};
use super::hinge_stops::Limits;
use super::transform::Transform;
use super::walk::Node;
use crate::math::vec::Vec3;
use crate::store::Raw;

/// What `SimF950` reads of the body before it advances the hinge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdvanceInput {
    /// The solve's angular acceleration (`SimEAD0`'s value).
    pub solved: f32,
    pub rate: f32,
    pub angle: f32,
}

/// The state payload `SimF950` emits: the body's key, orientation and position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatePayload {
    pub key: u32,
    pub orientation: [f32; 4],
    pub position: [f32; 4],
}

/// What the material table's indexed lookup finds at `index` (`sample_indexed` samples the slot).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotLookup {
    /// The table's own address (+0x800) is 0.
    NoTable,
    OutOfRange,
    /// The slot's address.
    At(u32),
}

/// The hinged body's words (`CHingedRigidBody`).
pub trait HingeStore: Raw {
    // The hinged body (`CHingedRigidBody`), at `this`.
    /// The link pointer and the definition pointer.
    fn hinge_link(&self, this: u32) -> u32;
    fn hinge_definition(&self, this: u32) -> u32;
    /// Whether `*link != 0` (`SimEAD0` returns at once), and whether the body has an owner car.
    fn hinge_linked(&self, this: u32) -> bool;
    fn hinge_owned(&self, this: u32) -> bool;

    /// The definition's limit block `idx` (any index), and the two limits of the body at `this`.
    fn load_hinge_limit(&self, def: u32, idx: u32) -> LimitSpring;
    fn load_hinge_limits(&self, def: u32) -> [LimitSpring; 2];
    fn load_hinge_state(&self, this: u32) -> HingeState;
    /// The limit block `idx` of the body's definition, and the body's limit state word `idx` (any index).
    fn body_limit(&self, this: u32, idx: u32) -> LimitSpring;
    fn body_limits(&self, this: u32) -> [LimitSpring; 2];
    fn limit_position(&self, this: u32, idx: u32) -> f32;
    fn store_limit_position(&mut self, this: u32, idx: u32, value: f32);
    fn store_limit_positions(&mut self, this: u32, values: [f32; 2]);
    fn angle_rate(&self, this: u32) -> (f32, f32);
    fn hinge_angle(&self, this: u32) -> f32;
    fn store_rotation_block(&mut self, this: u32, block: &RotationBlock);

    fn slip_input(&self, this: u32) -> SlipInput;
    /// The car state's word that `SimCFF0` reads (`state` is the state object, from `slip_input`).
    fn car_state_c00(&self, state: u32) -> f32;
    fn pivot_input(&self, this: u32) -> PivotInput;
    fn store_pivot(&mut self, this: u32, pivot: &Pivot);
    /// `SimE1D0`'s eax: the definition's settle vector when its gate address is nonzero, else 0.
    fn settle_axis(&self, this: u32) -> u32;
    fn signal_input(&self, this: u32) -> SignalInput;
    fn solve_entry_terms(&self, this: u32) -> EntryTerms;
    fn solve_terms(&self, this: u32) -> SolveTerms;
    fn store_solved(&mut self, this: u32, value: f32);
    fn db40_input(&self, this: u32) -> Option<FrameWalkInput>;
    fn d860_input(&self, this: u32) -> OffsetInput;
    fn efe0_state(&self, this: u32) -> RefreshState;
    fn store_efe0_state(&mut self, this: u32, state: &RefreshState);
    fn efe0_frame(&self, this: u32, d860_out: u32) -> RefreshHandles;
    fn d160_frame(&self, this: u32) -> PoseHandles;
    fn d160_words(&self, this: u32) -> PoseState;
    fn store_d160_words(&mut self, this: u32, state: &PoseState);

    /// The end stops' inputs, and the stores of the window taken and of the reflected rate.
    fn limit_inputs(&self, this: u32) -> Limits;
    fn store_limit_window(&mut self, this: u32, window: [u32; 3]);
    fn store_limit_rate(&mut self, this: u32, rate: f32);

    /// `SimF950`'s reads and stores.
    fn advance_input(&self, this: u32) -> AdvanceInput;
    fn store_rate(&mut self, this: u32, rate: f32);
    fn store_angle(&mut self, this: u32, angle: f32);
    /// The definition's axis vector, which the rate scales.
    fn rate_axis(&self, this: u32) -> u32;
    /// `E09Vec4::Clear` on the body's linear velocity.
    fn clear_linear_velocity(&mut self, this: u32);
    fn state_payload(&self, this: u32) -> StatePayload;
    /// Writes the payload into the emit's slot `sig`: the key, then the orientation, then the position (which overwrites
    /// the orientation's first lane).
    fn store_state_payload(&mut self, sig: u32, payload: &StatePayload);
    /// The state signal the body emits on.
    fn state_signal(&self) -> u32;
}

/// The entity layer: the walker marks, the ground probe's record, nodes, frames, transforms, the dummy object, the
/// exclusion sets and the car's body slots.
pub trait EntityStore: HingeStore {
    /// The root identity object (`g_7cbb00`) and its walker count (+8).
    fn identity(&self) -> u32;
    fn walker_count(&self, walker: u32) -> u32;
    /// `CEntity::E43_InitBD0`'s stores into walker mark `mark` (0 or 1): the walker, then its serial.
    fn store_walker(&mut self, this: u32, mark: u32, walker: u32);
    fn store_serial(&mut self, this: u32, mark: u32, serial: u32);
    /// `CSolid::E43ClearAcc`: the force and then the torque accumulator cleared. Returns the torque accumulator's address.
    fn clear_accumulators(&mut self, this: u32) -> u32;

    /// The collision systems of the ground probe and of the entity collisions.
    fn probe_host(&self) -> u32;
    /// The host's 13 floats at `at`.
    fn probe_output(&self, at: u32) -> [f32; 13];
    /// The record `CGround::VfE8` fills on a hit, after the vector copy: the second vector, the point, two cleared words,
    /// the depth and the owner.
    fn store_probe_contact(&mut self, record: u32, contact: &Contact);
    /// The first pass's edge count of the contact grid.
    fn grid_count(&self, grid: u32) -> u32;

    fn load_node(&self, at: u32) -> Node;
    fn store_pair(&mut self, node: u32, first: u32, second: u32) -> u32;
    fn load_entity_frame(&self, at: u32) -> EntityFrame;
    fn load_transform(&self, at: u32) -> Transform;
    /// The slot `CEntity::Vf120` jumps to.
    fn vf120_target(&self, entity: u32) -> u32;
    fn material_slot(&self, table: u32, index: u8) -> SlotLookup;

    /// The flags object and its flag byte, as `CEntity::E43VfD4` reads them.
    fn entity_flags(&self) -> (u32, u8);
    /// The dummy's box centre and half extents.
    fn dummy_extents(&self, this: u32) -> (Vec3, Vec3);
    /// The dummy's span group, span segment and post segment.
    fn dummy_group(&self, this: u32) -> u32;
    fn dummy_span(&self, this: u32) -> u32;
    fn dummy_post(&self, this: u32) -> u32;
    /// Sets the hit flag of the segment at `segment`.
    fn mark_segment_hit(&mut self, segment: u32);
    /// Four floats at `at`, and their store.
    fn vec4(&self, at: u32) -> [f32; 4];
    fn store_vec4(&mut self, at: u32, value: [f32; 4]);
    fn store_vec3(&mut self, at: u32, value: Vec3);

    /// The entity's exclusion set (the array's address, 0 for none) and its store. The array holds `[count, members...]`.
    fn exclusion_set(&self, entity: u32) -> u32;
    fn store_exclusion_set(&mut self, entity: u32, set: u32);
    fn exclusion_count(&self, set: u32) -> u32;
    fn store_exclusion_count(&mut self, set: u32, count: u32);
    fn exclusion_member(&self, set: u32, index: u32) -> u32;
    fn store_exclusion_member(&mut self, set: u32, index: u32, member: u32);

    /// The car's body slot `index` (`CCar::Clear480`'s four tags) and its store.
    fn car_body_slot(&self, car: u32, index: u32) -> u32;
    fn store_car_body_slot(&mut self, car: u32, index: u32, body: u32);
}
