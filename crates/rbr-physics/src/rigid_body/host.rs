//! The calls a body's step makes outside the body, which the ports take through these traits. The implementer
//! makes the call and changes whatever it changes; the port only computes the body's own fields.

use super::layout::{Body, Vec4};

/// The host has no implementation for a call the step makes (a listed gap, not a failure of the step). The step stops
/// at that call: the body's fields before it are written, the ones after it are not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostUnavailable;

/// The body's own calls the contact steps make: the point response and the body's transform of a vector.
pub trait BodyCalls {
    /// The body's response to a point: the three floats the step takes from it (`x, y, z`).
    fn response(&mut self, point: Vec4) -> Vec4;
    /// The body's transform of a vector.
    fn transform(&mut self, src: Vec4) -> Vec4;
}

/// The cell registration a free body makes after its position moves: registers the link node at `(x, y)`.
pub trait CellHost {
    fn register_cell(&mut self, link: u32, x: f32, y: f32) -> Result<(), HostUnavailable>;
}

/// The state refresh the window steps make last. The refresh reads the body from memory, so the body is stored
/// before the call.
pub trait StateHost {
    fn refresh_state(&mut self, body: &Body);
}

/// The entity's scratch tail call, whose effects are the host's.
pub trait Vf54Host {
    fn entity_scratch_tail(&mut self) -> Result<(), HostUnavailable>;
}

/// The entity walk on the body.
pub trait Vf20Host {
    fn entity_walk_s20(&mut self, body: u32, arg: u32) -> u32;
}

pub use super::layout::SolidCtorHost;
