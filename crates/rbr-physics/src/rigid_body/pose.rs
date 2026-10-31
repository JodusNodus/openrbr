//! The body's pose step: the orientation turns by the angular velocity over `a` (`q <- normalize((w*a/2, 1) * q)`),
//! the position moves along the linear velocity (`p <- p + v*a`), and the body is registered in its cell (root
//! bodies) or its position goes to the parent frame (child bodies). A contact sample is taken when the step flags
//! have bit 3 set. The calls into other code go through `PoseHost`, in order; their effects on the body are the body
//! fields the host can't see (see the README).

use super::host::{CellHost, HostUnavailable};
use super::layout::{Body, ParentFrame, Vec4, CELL_LINK};
use crate::math::quat::{mul, normalize, Quat};

/// The calls the pose step makes outside the body: the parent transform, the cell registration (`CellHost`) and the
/// contact sample.
pub trait PoseHost: CellHost {
    /// Child bodies: the position in the parent frame. The result is used only by the contact sample.
    fn to_parent_frame(&mut self, local: Vec4) -> Vec4;
    /// The drive-line sample at `at` with the body's current sample: returns the new sample.
    fn sample_big(&mut self, at: Vec4, value: f32) -> f32;
}

/// One pose step of `a` seconds. `update_position` and `update_orientation` are the caller's flags (`b`, `c`).
/// `parent` is the parent frame, `None` for a free body.
pub fn pose_step(
    body: &mut Body,
    a: f32,
    update_position: bool,
    update_orientation: bool,
    parent: Option<ParentFrame>,
    host: &mut impl PoseHost,
) -> Result<(), HostUnavailable> {
    if a <= 0.0 {
        if a < 0.0 {
            body.step_flags |= 1;
        }
        return Ok(());
    }
    body.step_flags &= !1;
    if update_orientation {
        let t = a * 0.5;
        let half_turn = Quat {
            x: body.angular_velocity.x * t,
            y: body.angular_velocity.y * t,
            z: body.angular_velocity.z * t,
            w: 1.0,
        };
        body.orientation = normalize(mul(half_turn, body.orientation));
    }
    // The position's new value, the last write of this step. Its four lanes come from the linear velocity: lane * a,
    // then + the position.
    let next = if update_position {
        let v = body.linear_velocity;
        Vec4 {
            x: v.x * a + body.position.x,
            y: v.y * a + body.position.y,
            z: v.z * a + body.position.z,
            w: v.w * a + body.position.w,
        }
    } else {
        body.position
    };
    // The point the cell lookup and the sample use: the new position, or its parent-frame image.
    let at = match parent {
        None => {
            host.register_cell(CELL_LINK, next.x, next.y)?;
            next
        }
        Some(frame) if frame.updates_child => host.to_parent_frame(next),
        // The original reads an uninitialised stack word here (unverified; see the README).
        Some(_) => next,
    };
    if body.body_flags & 8 != 0 {
        body.sampled = host.sample_big(at, body.sampled);
        body.sampled_dirty = 1;
    }
    body.position = next;
    Ok(())
}
