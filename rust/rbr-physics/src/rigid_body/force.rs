//! `CSolid::E43Vf64` (slot 25, 0x52cb20; `Vf25x` in the notes): the force pass. The entity walk (`CEntity::E43Vf64`,
//! 0x51a2c0) runs first. The contact vote (`Vf81x`, range 1.0) follows; on a hit its two vectors are applied through slots
//! 28 and 29 (the force and the torque). A body with flag 0x8 then runs `BigB`. The applies add to the accumulators, so the
//! body is read again after each of the two. Then the linear acceleration is
//! the force scaled by the inverse mass, and the angular one is the scratch torque times the local inverse inertia; both
//! go through the body's transform (`MB560`), and the force and the scratch are cleared. The calls outside the body go
//! through `ForceHost`; the arithmetic is the asm's, in its order.

use super::contact_list::{contact_list_step, ListHost};
use super::host::HostUnavailable;
use super::layout::{Body, Def, Vec4, BODY_FLAG_8};
use super::scratch::{FlushHost, FLUSH_BIT};

/// The calls `E43Vf64` makes outside the body, beyond `BigB`'s (`ListHost`, which includes the body's transform).
pub trait ForceHost: ListHost {
    /// `CEntity::E43Vf64` (0x51a2c0) on the body: the entity walk, the first call of the pass.
    fn entity_walk(&mut self);
    /// `CSolid::Vf81x` (0x52c510) on the body with range 1.0: the two vectors (the force, then the torque) on a hit.
    fn vote(&mut self) -> Result<Option<(Vec4, Vec4)>, HostUnavailable>;
    /// The body as the calls after the vote or `BigB` left it. Its applies write the body's accumulators (slots 28 and 29), which
    /// the port does not model, so the pass reads the body again.
    fn reread(&mut self) -> Body;
}

/// One force pass over the body. Returns `HostUnavailable` when the vote, its applies or `BigB` make a call the host does
/// not answer.
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

/// `CSolid::E43Vf70` (`Accum340`, slot 28, 0x529340): adds `pp * scratch_scale` to the force (`+0x370`), lane by lane, then
/// runs the flush when the gate has `FLUSH_BIT` (the same gate and call as `E43Vf74`). The asm adds `v*k` to the old lane.
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
