//! The force pass: the entity walk runs first, then the contact vote (range 1.0); on a hit its two vectors are
//! applied as the force and the torque. A flagged body then runs the contact list step. The applies add to the
//! accumulators, so the body is read again after each of the two. Then the linear acceleration is the force scaled
//! by the inverse mass, and the angular one is the scratch torque times the local inverse inertia; both go through
//! the body's transform, and the force and the scratch are cleared. The calls outside the body go through
//! `ForceHost`; the arithmetic is in operation order.

use super::contact_list::{contact_list_step, ListHost};
use super::host::HostUnavailable;
use super::layout::{Body, Def, Vec4, BODY_FLAG_8};
use super::scratch::{FlushHost, FLUSH_BIT};

/// The calls the force pass makes outside the body, beyond the contact list step's (which includes the body's
/// transform).
pub trait ForceHost: ListHost {
    /// The entity walk, the first call of the pass.
    fn entity_walk(&mut self);
    /// The contact vote with range 1.0: the two vectors (the force, then the torque) on a hit.
    fn vote(&mut self) -> Result<Option<(Vec4, Vec4)>, HostUnavailable>;
    /// The body as the calls after the vote or the contact list left it. Its applies write the body's accumulators,
    /// which the port does not model, so the pass reads the body again.
    fn reread(&mut self) -> Body;
}

/// One force pass over the body. Returns `HostUnavailable` when the vote, its applies or the contact list step make
/// a call the host does not answer.
pub fn force_pass(body: &mut Body, def: &Def, host: &mut impl ForceHost) -> Result<(), HostUnavailable> {
    host.entity_walk();
    if let Some((force, torque)) = host.vote()? {
        host.apply_force(force)?;
        host.apply_torque(torque)?;
        *body = host.reread();
    }
    if body.body_flags & BODY_FLAG_8 != 0 {
        contact_list_step(body, host)?;
        *body = host.reread();
    }
    body.linear_accel = host.transform(body.force.scale(def.inverse_mass));
    body.force = Vec4::default();
    body.angular_accel = host.transform(body.scratch.mul_lanes(def.local_inverse_inertia));
    body.scratch = Vec4::default();
    Ok(())
}

/// Adds `pp * scratch_scale` to the force, lane by lane, then runs the flush when the gate has `FLUSH_BIT` (the same
/// gate and call as the scratch accumulator). The sum adds `v*k` to the old lane.
pub fn accumulate_force(
    body: &mut Body,
    pp: Vec4,
    gate: u32,
    host: &mut impl FlushHost,
) -> Result<(), HostUnavailable> {
    body.force = body.force.add_scaled(pp, body.scratch_scale);
    if gate & FLUSH_BIT != 0 {
        host.flush()?;
    }
    Ok(())
}
