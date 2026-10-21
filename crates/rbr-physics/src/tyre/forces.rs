//! The contact forces of one tyre: the carcass's spring forces from the lateral deflection and the torsion, the
//! normal force, and the torques they make, plus the rolling resistance torque.
//!
//! The sign and clamp tests follow the original (`x > 0` and `0 > x`, so a NaN takes the zero branch); the x87 steps
//! round each op to 24 bits.

use super::hold::{hold, HoldInput};
use super::layout::{scaled_sum, Frame};
use crate::fpu::round_x87;
use crate::math::vec::Vec4;
use crate::math::{abs_asm, sign};

/// The lower clamp of the lateral force's damping factor.
const MIN_DAMPING: f32 = 0.6;
/// How fast the damping factor falls with the squared spin slip, and the slope of the longitudinal damping.
const DAMPING_SLOPE: f32 = -5.0;
const LONGITUDINAL_SLOPE: f32 = 10.0;

/// What the rolling resistance torque reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RollingInput {
    /// The contact patch's length, m, and the surface's rolling resistance.
    pub patch_length: f32,
    pub rolling_resistance: f32,
    /// The normal force, N.
    pub normal_force: f32,
    /// The weight of the sliding model.
    pub sliding_weight: f32,
    /// The wheel's spin, rad/s.
    pub spin: f32,
}

/// The rolling resistance torque, N·m: `-(t · normal_force · sliding_weight · sgn(spin))` with `t = patch_length ·
/// rolling_resistance · 0.5` in f32 (the original rounds it there, before the x87 part). `sgn` is +1, -1 or 0.
/// x87 order: `((t · normal_force) · sliding_weight) · sgn`, each op rounded to 24 bits.
pub fn rolling_torque(input: RollingInput) -> f64 {
    let round24 = round_x87;
    let t = input.patch_length * input.rolling_resistance * 0.5;
    let sign = if input.spin > 0.0 {
        1.0
    } else if 0.0 > input.spin {
        -1.0
    } else {
        0.0
    };
    let product = round24(round24(t as f64 * input.normal_force as f64) * input.sliding_weight as f64);
    -round24(product * sign)
}

/// The forces on the wheel from the contact: returns the force and the torque. `dt` is the time step, and `torque_in`
/// the torque the caller holds (the contact's torques are added into it). The lateral force is `-deflection ·
/// lateral_stiffness · inclination_cos`, the torsion torque `-torsion · torsional_stiffness` and the longitudinal
/// force the torsion torque over the effective radius. Unless the sticking weight is zero the lateral and
/// longitudinal forces are damped, and the pneumatic trail is scaled by the sliding weight. The force is the sum of
/// the three forces on their axes; the torque is the torsion torque about the lateral axis, the overturning and
/// lateral-force torque about the rolling axis, and the pneumatic and hold torques about the normal axis.
pub fn solve_forces(f: &mut Frame, dt: f32, torque_in: Vec4) -> (Vec4, Vec4) {
    let round24 = round_x87;
    let (deflection, torsion, radius) = (f.state.lateral_deflection, f.state.torsion_angle, f.state.effective_radius);
    let (sticking, sliding, normal_force) =
        (f.scratch.sticking_weight, f.scratch.sliding_weight, f.scratch.normal_force);
    let frame = f.scratch.contact_frame;
    let [lateral_velocity, longitudinal_velocity, _] = f.scratch.contact_velocity;
    let mut lateral_force = 0.0 - deflection * f.scratch.lateral_stiffness * f.scratch.inclination_cos;
    let mut torsion_torque = 0.0 - torsion * f.scratch.torsional_stiffness;
    let mut longitudinal_force = torsion_torque / radius;
    // The damping runs unless the sticking weight is an equal, ordered zero.
    if sticking != 0.0 {
        if sign(lateral_force) == sign(lateral_velocity) {
            let a = abs_asm(f.scratch.spin_slip);
            let v = a * a * DAMPING_SLOPE + 1.0;
            let v = if MIN_DAMPING > v {
                MIN_DAMPING
            } else if v > 1.0 {
                1.0
            } else {
                v
            };
            lateral_force *= v * sticking + sliding;
        }
        if sign(torsion_torque) != sign(f.scratch.spin_slip) {
            let a = abs_asm(f.scratch.spin_slip);
            let d = -1.0 - a * LONGITUDINAL_SLOPE;
            let r = 1.0 / d;
            let y = 1.0 - (r + 1.0);
            let z = y * sticking + sliding;
            longitudinal_force *= z;
            torsion_torque *= z;
        }
        f.scratch.pneumatic_trail *= sliding;
    }
    // The force: the lateral axis scaled by the lateral force, then the rolling axis and the normal axis added.
    let mut force = frame.lateral.scale(lateral_force);
    force = scaled_sum(frame.rolling, longitudinal_force, force);
    f.scratch.longitudinal_force = longitudinal_force;
    f.scratch.lateral_force = lateral_force;
    f.scratch.torsion_torque = torsion_torque;
    force = scaled_sum(frame.normal, normal_force, force);
    let torque = scaled_sum(frame.lateral, f.scratch.extra_axle_torque + torsion_torque, torque_in);
    let rolling_axis_torque =
        (0.0 - radius * f.scratch.inclination_cos * lateral_force) + f.scratch.inclination_sin * radius * normal_force;
    let torque = scaled_sum(frame.rolling, rolling_axis_torque, torque);

    // The torque about the normal axis: the pneumatic trail's and the carcass's, then the hold's, which the sticking
    // weight scales. `trail_torque` is the x87 difference of the two products.
    let sgn = sign(longitudinal_velocity) as f64;
    let trail = round24(round24(round24(sgn * f.scratch.pneumatic_trail as f64) * lateral_force as f64));
    let offset = round24(round24(sliding as f64 * deflection as f64) * longitudinal_force as f64);
    let trail_torque = round24(trail - offset) as f32;
    let hold_out = hold(HoldInput {
        sticking_weight: sticking,
        active: f.state.active,
        speed: f.scratch.speed,
        normal_force,
        kinetic_friction: f.scratch.kinetic_friction,
        lateral: frame.lateral,
        rolling: frame.rolling,
        normal: frame.normal,
        direction: f.state.hold_direction,
        dt,
    });
    f.state.hold_direction = hold_out.direction;
    let torque = scaled_sum(frame.normal, round24(hold_out.torque + trail_torque as f64) as f32, torque);
    (force, torque)
}
