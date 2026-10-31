//! Marks the two tweaks dirty, sets the active flag, clears the armed and very-soft words, then runs the body's
//! walk and the host notify, and emits the body's event. The calls outside the body go through `EmitHost`; the
//! arithmetic is in operation order.

use super::host::HostUnavailable;
use super::layout::{Body, Vec4};
use crate::math::quat::Quat;

/// The wake/sleep event: the kind ([`WAKE`] or [`SLEEP`]), the owner, the position, the orientation, then the position
/// and the orientation again. The pose is copied twice; the meaning of the second pair is open.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyEvent {
    pub kind: u32,
    pub owner: u32,
    pub position: Vec4,
    pub orientation: Quat,
    pub position_again: Vec4,
    pub orientation_again: Quat,
}

/// The event kinds: wake (the body becomes active) and sleep (it leaves the active set).
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

/// The calls the wake makes outside the body.
pub trait EmitHost {
    /// The body's walk on `body` (its children's walk, then the window start). It leaves the pose and the owner as
    /// they are, so the event reads them before or after it alike.
    fn slot18(&mut self, body: &mut Body);
    /// The host's notify on the body. The call checks the parent itself and notifies only when the body has none.
    fn entity_notify(&mut self);
    /// The body's event.
    fn emit(&mut self, event: &BodyEvent) -> Result<(), HostUnavailable>;
    /// The whole wake on `body`, as the wheel constructor calls it (a direct call): [`emit_body_event`] by default.
    fn wake(&mut self, body: &mut Body) -> Result<(), HostUnavailable>
    where
        Self: Sized,
    {
        emit_body_event(body, self)
    }
}

/// One wake on the body (the layout's wake is the same on memory).
pub fn emit_body_event(body: &mut Body, host: &mut impl EmitHost) -> Result<(), HostUnavailable> {
    super::sleep::wake_up(body);
    host.slot18(body);
    host.entity_notify();
    host.emit(&event(body, WAKE))
}

/// After its children's walk: the pose and the velocities become the window's start.
pub fn window_start(body: &mut Body) {
    body.start_position = body.position;
    body.start_linear_velocity = body.linear_velocity;
    body.start_orientation = body.orientation;
    body.start_angular_velocity = body.angular_velocity;
}
