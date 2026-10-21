//! `CSolid::E43Vf6C` (slot 27, 0x52cc30; `Vf27x` in the notes): the integrator pass. The angular velocity takes
//! `angular_accel * dt` and the linear velocity `linear_accel * dt`. A body with flag 0x8 clears a velocity below the
//! threshold (the linear one first; the angular one only when the linear one was cleared). Then the pose step (`VfC8`),
//! the acceleration estimates and their filter, the history copy, the entity walk, the sleep step (`Vf24x`) and the
//! state signal. The asm decides the lane order: each lane is one `mulps`/`addps` step, and the estimate's lane 3 keeps
//! its old value.

use super::host::HostUnavailable;
use super::layout::{Body, Def, ParentFrame, Vec4, BODY_FLAG_8};
use super::pose::{pose_step, PoseHost};
use super::sleep;
use crate::math::quat::Quat;
use crate::math::sse;

/// Squared speed under which a flagged body's velocity is cleared: NGP's 4e-7 (`1014fbf0`), where NGP repoints both
/// reads (`0x52cd05`, `0x52cd4e`) of 1.02's 1e-5 (`0x795e8c`).
const CLEAR_SPEED_SQ: f32 = f32::from_bits(0x34D6_BF94);

/// The filter's weights (the floats at 0x72f428 and 0x72f610).
const FILTER_OLD: f32 = 0.2;
const FILTER_NEW: f32 = 0.8;

/// The calls `E43Vf6C` makes outside the body, beyond the pose step's (`PoseHost`).
pub trait IntegrateHost: PoseHost {
    /// `CEntity::E43Vf6C` (0x51a330) on the body with the step length: the entity walk, whose state refresh (the
    /// body's slot 16) the host runs. The callee reads the body, so the host stores `body` first and returns the body
    /// as the walk leaves it.
    fn entity_walk(&mut self, body: &Body, dt: f32) -> Body;
    /// The rest of the body's slot 37 after a sleep trip (`CSolid::E43Vf94`, 0x52c130). The in-object part is
    /// `sleep::deactivate`, which the step makes itself. As for `entity_walk`, the host stores `body` first and returns
    /// the body after the call.
    fn sleep_trip(&mut self, body: &Body) -> Result<Body, HostUnavailable>;
    /// Emits the body's state signal (`CSigMrRBodyState`), the step's last call.
    fn emit_state(&mut self, state: &StateSignal) -> Result<(), HostUnavailable>;
}

/// The state signal's payload, in the original's stack buffer: `owner` at +0x00, `position` at +0x10 and `orientation` at
/// +0x20 (the asm copies the position to +0x10: the C++ names that offset +0x14).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StateSignal {
    pub owner: u32,
    pub position: Vec4,
    pub orientation: Quat,
}

/// One integrator pass over `dt` seconds. `def` is the body's definition (the sleep step reads it) and `parent` its
/// parent frame, `None` for a free body.
pub fn integrate(
    body: &mut Body,
    def: &Def,
    dt: f32,
    parent: Option<ParentFrame>,
    host: &mut impl IntegrateHost,
) -> Result<(), HostUnavailable> {
    body.angular_velocity = add_scaled_sse(body.angular_velocity, body.angular_accel, dt);
    body.linear_velocity = add_scaled_sse(body.linear_velocity, body.linear_accel, dt);
    let (update_position, update_orientation) = clear_slow(body, CLEAR_SPEED_SQ);
    pose_step(body, dt, update_position, update_orientation, parent, host)?;

    // The estimates divide by `dt` as `1.0f / dt` (the asm's divss), then every lane is scaled.
    let inv = 1.0 / dt;
    body.accel_linear_est = estimate(body.linear_velocity, body.prev_linear_velocity, body.accel_linear_est, inv);
    body.accel_angular_est = estimate(body.angular_velocity, body.prev_angular_velocity, body.accel_angular_est, inv);
    body.accel_linear_filt = filter(body.accel_linear_filt, body.accel_linear_est);
    body.accel_angular_filt = filter(body.accel_angular_filt, body.accel_angular_est);

    body.prev_angular_velocity = body.angular_velocity;
    body.prev_linear_velocity = body.linear_velocity;
    *body = host.entity_walk(body, dt);

    if sleep::step(body, def, dt) {
        sleep::deactivate(body);
        *body = host.sleep_trip(body)?;
    }
    host.emit_state(&StateSignal { owner: body.state_owner, position: body.position, orientation: body.orientation })
}

/// `v + a * dt` lanewise as the asm's `mulps` then `addps` (the product the first operand): lane w carries the
/// original's stack residue, which reaches infinities, so `inf + -inf` must be the instruction's negative NaN.
fn add_scaled_sse(v: Vec4, a: Vec4, dt: f32) -> Vec4 {
    Vec4 {
        x: sse::add(sse::mul(a.x, dt), v.x),
        y: sse::add(sse::mul(a.y, dt), v.y),
        z: sse::add(sse::mul(a.z, dt), v.z),
        w: sse::add(sse::mul(a.w, dt), v.w),
    }
}

/// The flag-8 clears. Returns the pose step's flags `(update position, update orientation)`: a cleared linear velocity
/// stops the position update, a cleared angular velocity stops the orientation update.
fn clear_slow(body: &mut Body, threshold: f32) -> (bool, bool) {
    if body.body_flags & BODY_FLAG_8 == 0 || !(squared_speed(body.linear_velocity) < threshold) {
        return (true, true);
    }
    body.linear_velocity = Vec4::default();
    if !(squared_speed(body.angular_velocity) < threshold) {
        return (false, true);
    }
    body.angular_velocity = Vec4::default();
    (false, false)
}

/// `z*z + y*y + x*x` in the asm's order.
pub(super) fn squared_speed(v: Vec4) -> f32 {
    (v.z * v.z + v.y * v.y) + v.x * v.x
}

/// The acceleration estimate `(v - prev) * inv`. The asm subtracts lanes x, y and z only, so lane w keeps the old
/// estimate's value before the scale.
fn estimate(v: Vec4, prev: Vec4, old: Vec4, inv: f32) -> Vec4 {
    Vec4 { x: (v.x - prev.x) * inv, y: (v.y - prev.y) * inv, z: (v.z - prev.z) * inv, w: old.w * inv }
}

/// The filter `0.2 * old + 0.8 * estimate`, lanewise.
fn filter(old: Vec4, est: Vec4) -> Vec4 {
    Vec4 {
        x: old.x * FILTER_OLD + est.x * FILTER_NEW,
        y: old.y * FILTER_OLD + est.y * FILTER_NEW,
        z: old.z * FILTER_OLD + est.z * FILTER_NEW,
        w: old.w * FILTER_OLD + est.w * FILTER_NEW,
    }
}
