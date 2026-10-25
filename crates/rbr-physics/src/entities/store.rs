//! The entities' store (`crate::store`): the layout's load and store groups. `Mem` implements it in `layout/`.

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

pub trait EntityStore: Raw {
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
