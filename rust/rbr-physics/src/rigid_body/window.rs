//! The window steps: `CSolid::VfEx` (0x52d110) takes the body to the end of its window, `CSolid::VfDx` (0x52cfd0)
//! takes it back to a fraction `a` of the window. `progress` (`+0x290`) is the fraction already taken. Both move the
//! orientation and the position, register a free body in its cell, then refresh the state through the host.

use super::host::{CellHost, HostUnavailable, StateHost};
use super::layout::{Body, Vec4, CELL_LINK};
use crate::math::quat::{mul, normalize, Quat};

/// `v * a + s * d`, the lane arithmetic of the blends below (each product rounds before the sum).
fn blend(v: f32, a: f32, s: f32, d: f32) -> f32 {
    v * a + s * d
}

/// `VfEx`: advances the body by the rest of its window, `a` seconds of the window's time. The move takes
/// `s = (1 - progress) * a` of the velocity; the turn takes half of `s` (the asm halves it for the quaternion only).
pub fn advance_rest(body: &mut Body, a: f32, host: &mut (impl CellHost + StateHost)) -> Result<(), HostUnavailable> {
    if body.progress >= 1.0 {
        return Ok(());
    }
    let s = (1.0 - body.progress) * a;
    let w = body.angular_velocity;
    // The asm's 0.5 (the float at 0x795e18 of RichardBurnsRally_SSE.exe) halves `s` for the turn only.
    let h = s * 0.5;
    let half_turn = Quat { x: w.x * h, y: w.y * h, z: w.z * h, w: 1.0 };
    body.orientation = normalize(mul(half_turn, body.orientation));
    let v = body.linear_velocity;
    let p = body.position;
    body.position = Vec4 { x: v.x * s + p.x, y: v.y * s + p.y, z: v.z * s + p.z, w: v.w * s + p.w };
    if body.parent == 0 {
        host.register_cell(CELL_LINK, body.position.x, body.position.y)?;
    }
    host.refresh_state(body);
    body.progress = 1.0;
    Ok(())
}

/// `VfDx`: moves the body back to fraction `a` of its window. The orientation and the position blend the current
/// values (scaled by `a`) with the window's start values (`start_orientation`, `start_position`, scaled by `1 - a`).
pub fn back_step(body: &mut Body, a: f32, host: &mut (impl CellHost + StateHost)) -> Result<(), HostUnavailable> {
    if a >= body.progress {
        return Ok(());
    }
    let d = 1.0 - a;
    let (o, so) = (body.orientation, body.start_orientation);
    body.orientation = normalize(Quat {
        x: blend(o.x, a, so.x, d),
        y: blend(o.y, a, so.y, d),
        z: blend(o.z, a, so.z, d),
        w: blend(o.w, a, so.w, d),
    });
    let (p, sp) = (body.position, body.start_position);
    body.position = Vec4 {
        x: blend(p.x, a, sp.x, d),
        y: blend(p.y, a, sp.y, d),
        z: blend(p.z, a, sp.z, d),
        w: blend(p.w, a, sp.w, d),
    };
    if body.parent == 0 {
        host.register_cell(CELL_LINK, body.position.x, body.position.y)?;
    }
    host.refresh_state(body);
    body.progress = a;
    Ok(())
}
