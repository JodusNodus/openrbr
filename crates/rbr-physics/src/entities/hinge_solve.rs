//! The hinge's angular acceleration about its axis: the torque on the body from the frame's acceleration, the hinge spring
//! (`SpringRate` towards `HingeDefaultAngle`) and damper (`Damping`), the slip and the limit springs, dry friction, and
//! the contact load, divided by `InertiaHingeAxis`, less the frame's angular acceleration along the axis. The calls it makes
//! (the pivot update, the two torque sums, the contact load, and for an unowned body the plain solid body's force
//! pass) are the caller's ([`SolveCalls`]).
//!
//! The term order is the original's source order where the source and the code agree, and the code's where they differ.

use crate::math::vec::Vec3;

/// The dry friction torque: it opposes the rate, and at rest it holds the hinge while the other torques are smaller.
/// Named by use (`sign(rate) * -0.3`).
const FRICTION_TORQUE: f32 = 0.3;

/// What the solve did with the body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    /// The body's link flag is set: the early return, nothing written.
    Linked,
    /// The body has no owner car: the plain solid body's force pass ran in place of the solve (writes nothing here).
    /// `result` is the pass's result.
    Unowned { result: u32 },
    /// The solve: the angular acceleration, rad/s^2, which the caller stores.
    Solved(f32),
}

/// What the solve reads before the pivot update: `InertiaHingeAxis` (0.1 when zero) and `InverseMass` (whose reciprocal
/// scales the inertial torque; 1 when zero).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntryTerms {
    pub inertia: f32,
    pub inverse_mass: f32,
}

/// What the solve reads after the pivot update.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolveTerms {
    /// The axes built from `vecHingeAxis_RF` and `vecZeroAxis_RF`, row-major.
    pub axes_rf: [[f32; 3]; 3],
    /// The hinge rotation ([`super::hinge_limits::rotation_block`]), row-major.
    pub hinge_rotation: [[f32; 3]; 3],
    /// The acceleration of the hinge point ([`super::hinge_pivot::Pivot`]), the second operand of the cross product.
    pub hinge_point_acceleration: [f32; 3],
    /// The inverse origin of the body-frame axes build: the weights of the matrix's columns.
    pub inverse_origin_bf: [f32; 3],
    /// `vecHingeAxis_RF`: the inertial torque is projected on it.
    pub hinge_axis_rf: [f32; 3],
    /// `vecHingeAxis_BF`: the contact load's torque is projected on it.
    pub hinge_axis_bf: [f32; 3],
    /// `vecHingePoint_BF`, negated for the contact force's lever arm.
    pub hinge_point_bf: [f32; 3],
    /// `HingeDefaultAngle`, `SpringRate` and `Damping`; the hinge angle (rad) and rate (rad/s); and the frame's angular
    /// acceleration along the axis ([`super::hinge_pivot::Pivot`]).
    pub default_angle: f32,
    pub spring_rate: f32,
    pub damping: f32,
    pub angle: f32,
    pub rate: f32,
    pub frame_angular_acceleration: f32,
}

/// The scratch the contact load writes: its force and torque vectors (16 bytes each), as handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContactHandles {
    pub force: u32,
    pub torque: u32,
}

/// The calls and reads of the solve: the link flag, the owner, the reads before and after the pivot update, and the
/// sub-calls.
pub trait SolveCalls {
    /// Whether the body's link flag is set: the early return.
    fn linked(&self, body: u32) -> bool;
    /// Whether the body has an owner car. An unowned body gets `force_pass` instead.
    fn owned(&self, body: u32) -> bool;
    /// The plain solid body's force pass on the body: for an unowned body. Its result.
    fn force_pass(&mut self, body: u32) -> u32;
    fn entry_terms(&self, body: u32) -> EntryTerms;
    /// Updates the body's pivot terms ([`super::hinge_pivot::pivot_terms`]).
    fn update_pivot(&mut self, body: u32);
    fn solve_terms(&self, body: u32) -> SolveTerms;
    /// The slip torque ([`super::hinge_slip::slip_torque`]).
    fn slip_torque(&mut self, body: u32) -> f32;
    /// The limit springs' torque sum ([`super::hinge_limits::limit_torque_sum`]).
    fn limit_torque_sum(&mut self, body: u32) -> f32;
    /// The contact load on the body (the solid body's contact query, with the constant 3.0): `Some((force, torque))` when
    /// it reports a hit.
    fn contact_load(&mut self, body: u32, handles: &ContactHandles) -> Option<([f32; 3], [f32; 3])>;
}

/// Solves the hinge: the outcome; the solved value is the angular acceleration the caller stores.
pub fn hinge_solve<C: SolveCalls>(calls: &mut C, body: u32, handles: &ContactHandles) -> Outcome {
    if calls.linked(body) {
        return Outcome::Linked;
    }
    if !calls.owned(body) {
        return Outcome::Unowned { result: calls.force_pass(body) };
    }
    let entry = calls.entry_terms(body);
    // zero takes the default (a NaN is kept: the compare is unordered)
    let inertia = if entry.inertia == 0.0 { 0.1 } else { entry.inertia };
    // an equal compare takes 1.0; a NaN divides
    let mass = if entry.inverse_mass != 0.0 { 1.0 / entry.inverse_mass } else { 1.0 };

    calls.update_pivot(body);
    let t = calls.solve_terms(body);

    // d = rotation * axes_rf, each element in the source's term order
    let (r, dm) = (t.hinge_rotation, t.axes_rf);
    let d = [
        dm[1][0] * r[0][1] + r[0][2] * dm[2][0] + r[0][0] * dm[0][0],
        dm[1][1] * r[0][1] + r[0][2] * dm[2][1] + r[0][0] * dm[0][1],
        dm[1][2] * r[0][1] + r[0][2] * dm[2][2] + r[0][0] * dm[0][2],
        r[1][2] * dm[2][0] + r[1][0] * dm[0][0] + r[1][1] * dm[1][0],
        r[1][0] * dm[0][1] + r[1][1] * dm[1][1] + r[1][2] * dm[2][1],
        r[1][2] * dm[2][2] + dm[0][2] * r[1][0] + r[1][1] * dm[1][2],
        dm[1][0] * r[2][1] + r[2][2] * dm[2][0] + r[2][0] * dm[0][0],
        r[2][0] * dm[0][1] + dm[1][1] * r[2][1] + r[2][2] * dm[2][1],
        r[2][2] * dm[2][2] + r[2][0] * dm[0][2] + dm[1][2] * r[2][1],
    ];
    // SSE lanes: arm[i] = (d[i]*c0 + d[3+i]*c1) + d[6+i]*c2
    let c = t.inverse_origin_bf;
    let arm = [0, 1, 2].map(|i| (d[i] * c[0] + d[3 + i] * c[1]) + d[6 + i] * c[2]);
    let cr = cross(arm, t.hinge_point_acceleration);
    let axis = t.hinge_axis_rf;
    let inertial = (cr[1] * axis[1] + cr[2] * axis[2]) + cr[0] * axis[0];
    let inertial = inertial * mass;

    // `fchs` negates the torque (the source writes 0 - x; the two differ only in a zero's sign)
    let mut torque = -inertial;
    torque += (t.default_angle - t.angle) * t.spring_rate;
    torque -= t.damping * t.rate;
    torque += calls.slip_torque(body);
    torque += calls.limit_torque_sum(body);

    // 1 above zero, -1 below, 0 at zero or NaN
    let direction = if t.rate > 0.0 {
        1.0_f32
    } else if 0.0 > t.rate {
        -1.0
    } else {
        0.0
    };
    let mut net = direction * -FRICTION_TORQUE;
    let mut held = false;
    if t.rate == 0.0 {
        let torque_magnitude = if torque <= 0.0 { 0.0 - torque } else { torque };
        let friction_magnitude = if net <= 0.0 { 0.0 - net } else { net };
        if friction_magnitude > torque_magnitude {
            net = 0.0;
            held = true;
        }
    }
    if !held {
        net += torque;
    }

    if let Some((force, contact_torque)) = calls.contact_load(body, handles) {
        let lever = t.hinge_point_bf.map(|v| -v);
        let moment = cross(lever, force);
        let w = t.hinge_axis_bf;
        net += w[2] * (contact_torque[2] + moment[2])
            + w[1] * (contact_torque[1] + moment[1])
            + w[0] * (contact_torque[0] + moment[0]);
    }
    Outcome::Solved(net / inertia - t.frame_angular_acceleration)
}

/// `a x b`, via the shared `math::vec` copy (same lane order as the original).
pub(super) fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    let r = Vec3 { x: a[0], y: a[1], z: a[2] }.cross3(Vec3 { x: b[0], y: b[1], z: b[2] });
    [r.x, r.y, r.z]
}
