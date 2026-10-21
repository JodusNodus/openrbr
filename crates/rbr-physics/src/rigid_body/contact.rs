//! `CSolid::BigA` (0x52b9f0): the contact worker. It measures the depth of the contact `e` (the point and normal at
//! `e+0x20`/`e+0x30`, the vector at `e+0x40`, two gains at `e+0x10`/`e+0x14`), finds the force from the body's
//! three-float response (the body's slot 80), and returns the impulse pair `a` and `b` (`b = e+0x40 x a`, `a` the
//! body's transform of the scaled normal). A hit on a body that takes the sample (the hook at `this+0x3cc`, flag 8) then
//! feeds the sample sink. The calls outside the body go through `ContactHost`; the arithmetic is the asm's, in its order
//! (`ComposeA`, the slot-80 response, the `crate::math::vec` length/scale/cross).

use super::host::{BodyCalls, HostUnavailable};
use super::layout::Vec4;

/// The contact record `e` (`BigA`'s fifth argument). Offsets live in `layout::contacts::load_contact`; the names
/// below are by use in the force formulas.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Contact {
    /// `e+0x04`: passed to the sample hook as it is.
    pub id: u32,
    /// `e+0x10`: the contact stiffness, N/m. Zero clamps the depth at -0.02 and selects the gravity-derived
    /// branch; above zero it is the force slope (`force = -depth * stiffness * damping`) and gates the reach
    /// correction.
    pub stiffness: f32,
    /// `e+0x14`: the capped branch's force-limit gain. Above zero it selects the capped force branch and caps
    /// the force at `force_cap * damping`.
    pub force_cap: f32,
    /// `e+0x18` and `e+0x1c`: the tangential correction's cap gains (neutral names: their meaning beyond the
    /// formula is open). Their mean, times the normal force, caps the tangential push.
    pub tangent_cap_0: f32,
    pub tangent_cap_1: f32,
    /// `e+0x20`: the point the depth is measured from.
    pub point: Vec4,
    /// `e+0x30`: the normal.
    pub normal: Vec4,
    /// `e+0x40`: the vector the matrix step takes, and the cross product's left operand.
    pub vector: Vec4,
}

/// What `BigA` gets besides the contact: the body's inverse mass (`def+0x16c`, `InverseMass`), the world's
/// `MaxTimeStep` (`CDef+0x1608c`, scales the tangential push), the caller's `range` (the vote passes 1.0; scales
/// the gravity-derived stiffness), and whether the body takes the contact sample (`this+0x3cc` non-null and
/// `this+0x60` bit 8).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Input {
    pub contact: Contact,
    pub inverse_mass: f32,
    pub max_step: f32,
    pub range: f32,
    pub sample: bool,
}

/// The calls `BigA` makes outside the body: the slot-80 response and the transform through `BodyCalls`.
pub trait ContactHost: BodyCalls {
    /// `E33Mat34::ComposeA` on the contact's vector: the 16-byte frame point the response is taken at.
    fn compose(&mut self, vector: Vec4) -> Vec4;
    /// `0x4e6000` on the body with the contact's vector: the two words it writes (the sample hook's first arguments).
    fn sample(&mut self) -> Result<(u32, u32), HostUnavailable>;
    /// The slot-2 virtual on the hook at `this+0x3cc` (its other arguments are the contact's vector and the frame point).
    fn sample_hook(&mut self, hook: &SampleHook) -> Result<(), HostUnavailable>;
}

/// The sample hook's arguments the contact step computes, in the original's order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampleHook {
    /// The two words `sample` wrote.
    pub first: u32,
    pub second: u32,
    /// The contact's word at `e+0x04`.
    pub id: u32,
    /// The length of the vector the transform took (the force along the normal and the push).
    pub length: f32,
    /// The length of the response's offset from the normal (the step's `len2`).
    pub reach: f32,
    /// The offset (the hook gets a pointer to it), divided by `reach` when that is above 0.0001. Lane `w` is the
    /// original's stack; the port's is 0.
    pub offset: Vec4,
}

/// The writes of one `BigA` call. `depth` is always written, `force` when the force is computed, `impulse` on a hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outcome {
    pub depth: f32,
    pub force: Option<f32>,
    pub impulse: Option<Impulse>,
}

/// The two vectors a hit writes: the transformed contact force (16 bytes) and its torque about the contact
/// vector (12 bytes). Named by use: the vote sums them as the force and the torque it applies through slots 28/29.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Impulse {
    pub force: Vec4,
    pub torque: Vec4,
}

/// The dot of `n` (lanes 1..3 weighted `y`, `z`, then `x`) with `v`: `(n.y*v.y + n.z*v.z) + n.x*v.x`.
fn dot_yzx(n: Vec4, v: Vec4) -> f32 {
    (n.y * v.y + n.z * v.z) + n.x * v.x
}

/// The contact step (`CSolid::BigA`). See the module doc. Returns the writes of the original's call; the hit flag is
/// `impulse.is_some()` (the original's `eax`).
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
    let l = host.response(frame);
    // The response's projection on the normal, and the damping factor `g` taken from it.
    let f4 = dot_yzx(n, l);
    let mut g = 1.0;
    if 0.001 < f4 {
        g = f4;
        let len = l.length3();
        if 1.0 < len {
            let t = l.scale(1.0 / len);
            g = dot_yzx(n, t) * f4;
        }
        if !(g > 0.4) {
            g = (0.4 - g) * 2.5;
        } else {
            g = 0.0;
        }
    }
    // The force: the first branch when the second gain is not above zero, the second otherwise.
    let mut f74;
    let mut force;
    if !(c.force_cap > 0.0) {
        f74 = (input.range * 9.81) / (input.inverse_mass * 0.03);
        force = 0.0 - depth * f74 * g;
        let limit = g / input.inverse_mass * 14.715;
        if force > limit {
            force = limit;
        }
    } else {
        f74 = c.stiffness;
        force = 0.0 - depth * f74 * g;
        let v6 = c.force_cap * g;
        if force > v6 {
            force = v6;
        }
        let limit = g / input.inverse_mass * 49.05;
        if force > limit {
            force = limit;
        }
    }
    if force == 0.0 {
        return Ok(Outcome { depth, force: Some(force), impulse: None });
    }
    // The scaled normal, then the frame's offset from the response, normalised, and pushed along the normal.
    let mut scaled = n.scale(force);
    let along = n.scale(f4);
    let offset = Vec4 { x: l.x - along.x, y: l.y - along.y, z: l.z - along.z, w: 0.0 };
    let len2 = offset.length3();
    let mut hook_offset = offset;
    if 0.0001 < len2 {
        let unit = offset.scale(1.0 / len2);
        hook_offset = unit;
        f74 = input.max_step * len2 * f74 * 3.0;
        let limit = force * ((c.tangent_cap_1 + c.tangent_cap_0) * 0.5);
        if f74 > limit {
            f74 = limit;
        }
        if 0.0 < c.stiffness && force > f74 {
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
            if by_reach > f74 {
                f74 = by_reach;
            }
        }
        let s = 0.0 - f74;
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
