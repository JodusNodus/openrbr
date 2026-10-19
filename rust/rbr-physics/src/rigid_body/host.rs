//! The calls a body's step makes outside the body, which the ports take through these traits. The implementer
//! makes the call and changes whatever it changes; the port only computes the body's own fields.

use super::layout::{Body, Vec4};
use crate::mem::Mem;

/// The host has no implementation for a call the step makes (a listed gap, not a failure of the step). The step stops
/// at that call: the body's fields before it are written, the ones after it are not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostUnavailable;

/// The body's own calls the contact steps make: the slot-80 response to a point and the body's transform of a vector.
pub trait BodyCalls {
    /// The body's slot-80 response to a point: the three floats the step takes from it (`x, y, z`).
    fn response(&mut self, point: Vec4) -> Vec4;
    /// `E43PairWalk::MB560` on a vector, with the body as the matrix (its slot 12, `S30`, returns the body).
    fn transform(&mut self, src: Vec4) -> Vec4;
}

/// The cell registration a free body makes after its position moves (`CCellGrid::LookupTwin` at `(x, y)`, then
/// `CCar::AppendA980` with the link node at `link`, a body offset).
pub trait CellHost {
    fn register_cell(&mut self, link: u32, x: f32, y: f32) -> Result<(), HostUnavailable>;
}

/// The state refresh the window steps make last (the body's vtable slot 16, `CSolid::Vf40`). The slot reads the body
/// from memory, so the body is stored before the call.
pub trait StateHost {
    fn refresh_state(&mut self, body: &Body);
}

/// The entity call `E47_Vf54Helper` (0x52b6b0) makes first: `E43_EntityVf54Fwd` (0x519ca0) (name ours), which tail-jumps into the
/// body's `+0x5c` object's slot 1. That object's code is not ported, so its effects are the host's.
pub trait Vf54Host {
    fn entity_scratch_tail(&mut self) -> Result<(), HostUnavailable>;
}

/// The call `CSolid::E43Vf20` (0x529200) (name ours) tail-jumps to: `CEntity::E43Vf20` (0x51a3a0), the entity walk on the body.
pub trait Vf20Host {
    fn entity_walk_s20(&mut self, body: u32, arg: u32) -> u32;
}

/// The `CSolid` constructor (0x52d480; its state and base constructors are not ported): the solid, its definition, the
/// name and owner arguments, and the flags. `CDef::Vf50` passes zero flags; `E51Wheel::Ctor` passes its own with 0x20.
pub trait SolidCtorHost {
    fn solid_ctor(
        &mut self,
        mem: &mut Mem,
        solid: u32,
        def: u32,
        a: u32,
        b: u32,
        flags: u32,
    ) -> Result<(), HostUnavailable>;
}
