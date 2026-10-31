//! The rigid body's store (`crate::store`): the layout's load and store groups. `Mem` implements it in
//! `layout/mem_store.rs`.

use super::contact::Input;
use super::layout::{Body, Def, ParentFrame, Vec4};
use super::rows::RowFrame;
use crate::math::quat::Quat;
use crate::math::vec::Vec3;
use crate::physics_core::math::Mat;
use crate::store::Raw;

/// The body's position, angular velocity and linear velocity: what the velocity of a point on it takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub position: Vec3,
    pub angular_velocity: Vec3,
    pub linear_velocity: Vec3,
}

/// The state's rotation inputs: the orientation, the gate and the scale diagonal (`refresh_rotation`'s arguments).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StateRotation {
    pub orientation: Quat,
    pub gate: f32,
    pub diagonal: [f32; 3],
}

/// The hook's pair: its third lane, the two words and the 0.5 the lane is compared with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HookPair {
    pub third: f32,
    pub half: f32,
    pub first: u32,
    pub second: u32,
}

/// A ring's wrap flag, index and count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingHeader {
    pub wraps: bool,
    pub index: u32,
    pub count: u32,
}

/// `this` is the body, `definition` its definition, `state` its solid state.
pub trait RigidStore: Raw {
    /// Reads the body at `this`. The memory must cover the whole object.
    fn body(&self, this: u32) -> Body;
    fn store_body(&mut self, this: u32, body: &Body);
    /// The sleep and mass settings of the body's definition.
    fn definition(&self, this: u32) -> Def;
    /// The parent a body is attached to, at the non-null pointer `parent`.
    fn parent_frame(&self, parent: u32) -> ParentFrame;
    /// Whether the definition's very-soft word is nonzero.
    fn def_very_soft(&self, this: u32) -> bool;
    fn inverse_mass(&self, this: u32) -> f32;

    /// A 16-byte row (or vector) at `at`, and its store: all four lanes, or the first three.
    fn row(&self, at: u32) -> Vec4;
    fn store_row(&mut self, at: u32, row: Vec4);
    fn store_row3(&mut self, at: u32, row: Vec4);
    fn vec3(&self, at: u32) -> Vec3;
    fn store_vec3(&mut self, at: u32, value: Vec3);

    /// The matrix and origin the row loops transform by (the origin's `w` is zero).
    fn row_frame(&self, this: u32) -> RowFrame;
    /// The body's matrix words (nine).
    fn matrix_words(&self, this: u32) -> [f32; 9];
    /// The body's matrix as the row scaling takes it.
    fn matrix(&self, this: u32) -> Mat;
    fn position(&self, this: u32) -> Vec4;
    fn motion(&self, this: u32) -> Motion;
    fn linear_velocity(&self, this: u32) -> Vec4;
    fn store_linear_velocity(&mut self, this: u32, value: Vec4);
    /// The address of the body's linear velocity (what the clamp returns).
    fn linear_velocity_at(&self, this: u32) -> u32;
    /// The body's solid state.
    fn state_at(&self, this: u32) -> u32;
    /// The orientation and the position (zero `w`) at the window's start.
    fn start_orientation(&self, this: u32) -> Quat;
    fn start_offset(&self, this: u32) -> Vec4;

    /// The two limits the velocity clamp reads from the physics settings.
    fn impulse_limits(&self) -> (f32, f32);

    /// The contact worker's inputs: the contact at `contact`, the body's inverse mass, the world's time step and
    /// the sample test.
    fn contact_input(&self, this: u32, contact: u32, range: f32) -> Input;
    /// The contact list's count, `None` when the body has no list.
    fn contact_count(&self, this: u32) -> Option<u32>;
    /// The hook and the ring tail of the body, zero when absent.
    fn hook(&self, this: u32) -> u32;
    fn ring_tail(&self, this: u32) -> u32;
    /// The pair the impulse tail gives the hook: the word the hook takes is the first when the pair's third lane
    /// passes the physics half, else the second.
    fn hook_pair(&self, pair: u32) -> HookPair;
    fn ring_header(&self, ring: u32) -> RingHeader;
    /// Writes the entry at `index`: its three words and the vector at `src`.
    fn store_ring_entry(&mut self, ring: u32, index: u32, words: [u32; 3], src: u32);
    fn store_ring_cursor(&mut self, ring: u32, index: u32, count: u32);

    fn scratch_scale(&self, this: u32) -> f32;
    fn scratch_accum(&self, this: u32) -> Vec4;
    fn store_scratch_accum(&mut self, this: u32, value: Vec4);

    fn state_rotation(&self, state: u32) -> StateRotation;
    /// The state's refreshed matrix (nine words).
    fn store_state_matrix(&mut self, state: u32, matrix: [f32; 9]);
}
