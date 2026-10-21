//! The wheel's axes and how damage moves them. The wheel axle starts at the setup's axis inclination and tilts with the
//! hub damage; a damaged hub also makes the axle wobble as the wheel turns. The top mount shifts along the setup's
//! deformation axes with the damage it has taken. The ports take the values `layout::load_*` read and return the values
//! `layout::store_*` write. Operation order is the original's (its x87 products are f32 products at 24-bit precision).

use crate::math::trig::{x87_cos, x87_sin};
use crate::math::vec::{Vec3, Vec4};

/// The sine and cosine the axis functions take, as the f64 the x87 leaves in its top register (the wobble multiplies them
/// unrounded). The port answers them with [`X87Trig`] (`math::trig`); the tests can answer with the original's results to
/// retain the hardware's last-bit approximation (`notes/physics.md` "x87 and float rules").
pub trait Trig {
    fn sin(&mut self, angle: f32) -> f64;
    fn cos(&mut self, angle: f32) -> f64;
}

/// The sine and cosine as `math::trig` computes them.
pub struct X87Trig;

impl Trig for X87Trig {
    fn sin(&mut self, angle: f32) -> f64 {
        x87_sin(angle)
    }

    fn cos(&mut self, angle: f32) -> f64 {
        x87_cos(angle)
    }
}

/// The inputs of [`wobbled_axle`]: the hub damage in [-1, 1], the inclined axle and its side vector, the wheel's angle
/// of rotation (rad) and the fourth lane of the previous axle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxleWobbleIn {
    pub hub_damage: f32,
    pub axle: Vec4,
    pub side: Vec4,
    pub wheel_angle: f32,
    pub previous_w: f32,
}

/// The inputs of [`inclined_axle`]: the hub damage, the setup's `AxisInclinationDamageAngle` (rad at full damage) and
/// `WheelAxisInclination` (rad).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Inclination {
    pub hub_damage: f32,
    pub damage_angle: f32,
    pub axis_inclination: f32,
}

/// The axle direction of the inclined wheel and the unit vector at right angles to it in the horizontal plane (three
/// lanes each; the fourth lanes are not written).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InclinedAxle {
    pub axle: Vec3,
    pub side: Vec3,
}

/// The inputs of [`deformed_top_mount`]: the damage of the top mount along the two axes (0 to 1), the setup's
/// `TopMountMaxDeformUp` and `TopMountMaxDeformSide`, and the mount position with the two deformation axes
/// (`vecTopMountDeformUpAxis`, `vecTopMountDeformSideAxis`) in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TopMountDeformIn {
    pub up_damage: f32,
    pub side_damage: f32,
    pub max_up: f32,
    pub max_side: f32,
    pub up_axis: Vec4,
    pub mount: Vec4,
    pub side_axis: Vec4,
}

/// The inputs of [`apply_hub_torque`]: the torque, the setup's `HubDamageTorqueThreshold` and `HubDamageMaxTorque`, the
/// hub damage so far and the inclination inputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HubDamageIn {
    pub torque: f32,
    pub threshold: f32,
    pub max_torque: f32,
    pub hub_damage: f32,
    pub inclination: Inclination,
}

/// What [`apply_hub_torque`] changes: the new hub damage and the axle that follows from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HubDamageUpdate {
    pub hub_damage: f32,
    pub axle: InclinedAxle,
}

/// The largest tilt of the wobbling axle at full hub damage (named by use: the wobble's scale).
const WOBBLE_TILT: f32 = 0.07;

/// Clamps to [-1, 1] with the original's compares: a NaN passes through.
fn clamp_unit(x: f32) -> f32 {
    if x < -1.0 {
        -1.0
    } else if x > 1.0 {
        1.0
    } else {
        x
    }
}

/// The wheel axle with the wobble of a damaged hub. With the hub damage nonzero (NaN too), `tilt = 0.07 * clamp(2 *
/// damage, -1, 1)`; the wheel angle gives `s = sin` and `c = cos`. The axle is the inclined axle plus its side vector
/// scaled by `c * tilt` (x, z only), with `-(s * tilt)` as its y, normalised (the fourth lane is kept from the previous
/// axle, then scaled). With no damage the axle is the inclined axle.
pub fn wobbled_axle(trig: &mut impl Trig, input: &AxleWobbleIn) -> Vec4 {
    let damage = input.hub_damage;
    if damage == 0.0 {
        return input.axle;
    }
    let tilt = clamp_unit(damage * 2.0) * WOBBLE_TILT;
    // The x87 keeps the sine and cosine unrounded: each product is rounded once, when it is stored.
    let s = trig.sin(input.wheel_angle);
    let c = trig.cos(input.wheel_angle);
    let side = input.side.scale((c * f64::from(tilt)) as f32);
    let out = Vec4 {
        x: input.axle.x + side.x,
        y: 0.0 - ((s * f64::from(tilt)) as f32),
        z: input.axle.z + side.z,
        w: input.previous_w,
    };
    out.normalize()
}

/// The inclined axle and its side vector: the inclination angle is `damage * damage_angle + axis_inclination` (the
/// product, then the sum, each at 24 bits). The axle is `(cos, 0, sin)` of it and the side vector `(-sin, 0, cos)`.
pub fn inclined_axle(trig: &mut impl Trig, input: &Inclination) -> InclinedAxle {
    let a = input.hub_damage * input.damage_angle + input.axis_inclination;
    let s = trig.sin(a) as f32;
    let c = trig.cos(a) as f32;
    InclinedAxle { axle: Vec3 { x: c, y: 0.0, z: s }, side: Vec3 { x: -s, y: 0.0, z: c } }
}

/// The top mount moved along the setup's deformation axes by the damage it took: `up_axis * (max_up * up_damage) +
/// mount`, then `side_axis * (max_side * side_damage)` plus that (each lane: the product, then the sum).
pub fn deformed_top_mount(input: &TopMountDeformIn) -> Vec4 {
    let up = input.max_up * input.up_damage;
    let moved = input.up_axis.scale(up) + input.mount;
    let side = input.max_side * input.side_damage;
    input.side_axis.scale(side) + moved
}

/// A torque on the hub. Above the setup's damage threshold the hub damage moves by `-(torque - threshold) / (max_torque -
/// threshold)` (f32 steps), is clamped to [-1, 1], and the axle is inclined by it. Below the threshold nothing changes
/// and the result is `None`.
pub fn apply_hub_torque(trig: &mut impl Trig, input: &HubDamageIn) -> Option<HubDamageUpdate> {
    if !(input.torque > input.threshold) {
        return None;
    }
    let step = (input.torque - input.threshold) / (input.max_torque - input.threshold);
    let hub_damage = clamp_unit((0.0 - step) + input.hub_damage);
    let axle = inclined_axle(trig, &Inclination { hub_damage, ..input.inclination });
    Some(HubDamageUpdate { hub_damage, axle })
}
