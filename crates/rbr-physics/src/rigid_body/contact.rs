//! Contact response for a penetrating rigid body. The contact depth and the body's response determine a normal
//! force, a capped tangential correction and the resulting torque. Bodies that collect contact samples also receive
//! the force's magnitude and reach. The operation and rounding order follows the reference.

use super::host::{BodyCalls, HostUnavailable};
use super::layout::Vec4;

/// A contact and its force coefficients, named by their use in the formulas.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Contact {
    /// Identifier passed through to the sample hook.
    pub id: u32,
    /// The contact stiffness, N/m. Zero clamps the depth at -0.02 and selects the gravity-derived
    /// branch; above zero it is the force slope (`force = -depth * stiffness * damping`) and gates the reach
    /// correction.
    pub stiffness: f32,
    /// The capped branch's force-limit gain. Above zero it selects the capped force branch and caps
    /// the force at `force_cap * damping`.
    pub force_cap: f32,
    /// The tangential correction's cap gains, named by use; their meaning beyond the formula is open.
    /// Their mean, times the normal force, caps the tangential push.
    pub tangent_cap_0: f32,
    pub tangent_cap_1: f32,
    /// The point the depth is measured from.
    pub point: Vec4,
    /// The contact normal.
    pub normal: Vec4,
    /// The vector transformed for the response, and the torque cross product's left operand.
    pub vector: Vec4,
}

/// Contact-response inputs. The maximum physics time step scales the tangential push; `range` scales the
/// gravity-derived stiffness and is 1.0 in the contact-list vote. `sample` enables the contact-sample callback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Input {
    pub contact: Contact,
    pub inverse_mass: f32,
    pub max_step: f32,
    pub range: f32,
    pub sample: bool,
}

/// Body transforms, response and optional sample callbacks used by the contact solver.
pub trait ContactHost: BodyCalls {
    /// Transforms the contact vector to the frame point where the response is measured.
    fn compose(&mut self, vector: Vec4) -> Vec4;
    /// Reads the two metadata words passed through to the contact-sample callback.
    fn sample(&mut self) -> Result<(u32, u32), HostUnavailable>;
    /// Delivers the contact sample to the body's listener.
    fn sample_hook(&mut self, hook: &SampleHook) -> Result<(), HostUnavailable>;
}

/// The sample hook's arguments the contact step computes, in the original's order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampleHook {
    /// The two words `sample` wrote.
    pub first: u32,
    pub second: u32,
    /// The contact identifier.
    pub id: u32,
    /// The length of the vector the transform took (the force along the normal and the push).
    pub length: f32,
    /// The length of the response's offset from the normal (the step's `len2`).
    pub reach: f32,
    /// The offset (the hook gets a pointer to it), divided by `reach` when that is above 0.0001. Lane `w` is the
    /// original's stack; the port's is 0.
    pub offset: Vec4,
}

/// The result of one contact response. `depth` is always written, `force` when the force is computed, `impulse` on a hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outcome {
    pub depth: f32,
    pub force: Option<f32>,
    pub impulse: Option<Impulse>,
}

/// The two vectors a hit writes: the transformed contact force (16 bytes) and its torque about the contact
/// vector (12 bytes). Named by use: the contact vote sums and applies them as force and torque.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Impulse {
    pub force: Vec4,
    pub torque: Vec4,
}

/// The dot of `n` (lanes 1..3 weighted `y`, `z`, then `x`) with `v`: `(n.y*v.y + n.z*v.z) + n.x*v.x`.
fn dot_yzx(n: Vec4, v: Vec4) -> f32 {
    (n.y * v.y + n.z * v.z) + n.x * v.x
}

/// Computes the contact response. A hit has `impulse.is_some()`; zero force and nonpenetrating contacts return
/// without generating a force/torque pair or emitting a contact sample.
pub fn contact_step(input: &Input, host: &mut impl ContactHost) -> Result<Outcome, HostUnavailable> {
    let c = &input.contact;
    let n = c.normal;
    let p = c.point;
    let frame = host.compose(c.vector);
    // The depth: the normal's projection of the frame point from `point`.
    let mut depth = {
        let dz = frame.z - p.z;
        let dy = frame.y - p.y;
        let dx = frame.x - p.x;
        (n.z * dz + n.y * dy) + dx * n.x
    };
    if !(depth < 0.0) {
        return Ok(Outcome { depth, force: None, impulse: None });
    }
    if c.stiffness == 0.0 && depth < -0.02 {
        depth = -0.02;
    }
    let response = host.response(frame);
    // Project the response onto the normal to obtain the damping factor.
    let normal_response = dot_yzx(n, response);
    let mut damping = 1.0;
    if 0.001 < normal_response {
        damping = normal_response;
        let len = response.length3();
        if 1.0 < len {
            let t = response.scale(1.0 / len);
            damping = dot_yzx(n, t) * normal_response;
        }
        if !(damping > 0.4) {
            damping = (0.4 - damping) * 2.5;
        } else {
            damping = 0.0;
        }
    }
    // The force: the first branch when the second gain is not above zero, the second otherwise.
    let stiffness;
    let mut force;
    if !(c.force_cap > 0.0) {
        stiffness = (input.range * 9.81) / (input.inverse_mass * 0.03);
        force = 0.0 - depth * stiffness * damping;
        let limit = damping / input.inverse_mass * 14.715;
        if force > limit {
            force = limit;
        }
    } else {
        stiffness = c.stiffness;
        force = 0.0 - depth * stiffness * damping;
        let contact_limit = c.force_cap * damping;
        if force > contact_limit {
            force = contact_limit;
        }
        let limit = damping / input.inverse_mass * 49.05;
        if force > limit {
            force = limit;
        }
    }
    if force == 0.0 {
        return Ok(Outcome { depth, force: Some(force), impulse: None });
    }
    // The scaled normal, then the frame's offset from the response, normalised, and pushed along the normal.
    let mut scaled = n.scale(force);
    let along = n.scale(normal_response);
    let offset = Vec4 { x: response.x - along.x, y: response.y - along.y, z: response.z - along.z, w: 0.0 };
    let len2 = offset.length3();
    let mut hook_offset = offset;
    if 0.0001 < len2 {
        let unit = offset.scale(1.0 / len2);
        hook_offset = unit;
        let mut tangent_force = input.max_step * len2 * stiffness * 3.0;
        let limit = force * ((c.tangent_cap_1 + c.tangent_cap_0) * 0.5);
        if tangent_force > limit {
            tangent_force = limit;
        }
        if 0.0 < c.stiffness && force > tangent_force {
            // The reach fraction, clamped to [0, 1] (a NaN passes through, as the asm's compares do).
            let t = len2 * 0.2;
            let t = if t < 0.0 {
                0.0
            } else if 1.0 < t {
                1.0
            } else {
                t
            };
            let by_reach = force * t;
            if by_reach > tangent_force {
                tangent_force = by_reach;
            }
        }
        let s = 0.0 - tangent_force;
        scaled = Vec4 {
            x: unit.x * s + scaled.x,
            y: unit.y * s + scaled.y,
            z: unit.z * s + scaled.z,
            w: unit.w * s + scaled.w,
        };
    }
    let force_vec = host.transform(scaled);
    let torque = c.vector.cross3(force_vec);
    if input.sample {
        let (first, second) = host.sample()?;
        host.sample_hook(&SampleHook {
            first,
            second,
            id: c.id,
            length: scaled.length3(),
            reach: len2,
            offset: hook_offset,
        })?;
    }
    Ok(Outcome { depth, force: Some(force), impulse: Some(Impulse { force: force_vec, torque }) })
}
