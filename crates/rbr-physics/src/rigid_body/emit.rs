//! `CSolid::E43Vf90` (slot 36, 0x52bf30; `body_emit_a` in the notes): marks the two tweaks dirty, sets the active flag,
//! clears the armed and very-soft words, then runs the body's slot 18 and the host notify (`CEntity::E43Vf90`), and emits
//! the body's event. The calls outside the body go through `EmitHost`; the arithmetic is the asm's, in its order.

use super::host::HostUnavailable;
use super::layout::{Body, Vec4};
use crate::math::quat::Quat;

/// The event `E43Vf90` and `E43Vf94` emit (0x50 bytes): the kind at +0x00 ([`WAKE`] or [`SLEEP`]), the owner at +0x04,
/// the position at +0x10, the orientation at +0x20, then the position and the orientation again at +0x30 and +0x40. The
/// original copies the pose twice; the meaning of the second pair is open.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyEvent {
    pub kind: u32,
    pub owner: u32,
    pub position: Vec4,
    pub orientation: Quat,
    pub position_again: Vec4,
    pub orientation_again: Quat,
}

/// The event kinds: `E43Vf90`'s (the body becomes active) and `E43Vf94`'s (it leaves the active set; names are ours).
pub const WAKE: u32 = 1;
pub const SLEEP: u32 = 2;

/// The body's event of `kind`, from its owner word and pose.
pub fn event(body: &Body, kind: u32) -> BodyEvent {
    BodyEvent {
        kind,
        owner: body.state_owner,
        position: body.position,
        orientation: body.orientation,
        position_again: body.position,
        orientation_again: body.orientation,
    }
}

/// The calls `E43Vf90` makes outside the body.
pub trait EmitHost {
    /// The body's slot 18 on `body` (`CSolid::S18`, 0x529dc0, for every body built so far: its children's walk, then
    /// [`window_start`]). It leaves the pose and the owner as they are, so the event reads them before or after it alike.
    fn slot18(&mut self, body: &mut Body);
    /// `CEntity::E43Vf90` (0x519a60) on the body: the host's notify. The original's call checks the parent itself and
    /// calls its `WallRel` only when the body has none.
    fn entity_notify(&mut self);
    /// `CMultiSignal1<const CSigMrRBodyEvent&>::Emit` (0x51f7c0): the body's event.
    fn emit(&mut self, event: &BodyEvent) -> Result<(), HostUnavailable>;
    /// The whole `E43Vf90` on `body`, as `E51Wheel::Ctor` calls it (a direct call): [`emit_body_event`] by default.
    fn wake(&mut self, body: &mut Body) -> Result<(), HostUnavailable>
    where
        Self: Sized,
    {
        emit_body_event(body, self)
    }
}

/// One `E43Vf90` on the body (`layout::solid_wake` is the same on memory).
pub fn emit_body_event(body: &mut Body, host: &mut impl EmitHost) -> Result<(), HostUnavailable> {
    super::sleep::wake_up(body);
    host.slot18(body);
    host.entity_notify();
    host.emit(&event(body, WAKE))
}

/// `CSolid::S18` (0x529dc0, `CSolid::E43Vf48` in the C++) after its children's walk (`CEntity::E43Vf48`, 0x51a440): the
/// pose and the velocities become the window's start (`layout::solid_window_start` on memory).
pub fn window_start(body: &mut Body) {
    body.start_position = body.position;
    body.start_linear_velocity = body.linear_velocity;
    body.start_orientation = body.orientation;
    body.start_angular_velocity = body.angular_velocity;
}
