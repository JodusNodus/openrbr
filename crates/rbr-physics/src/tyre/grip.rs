//! Limiting the carcass's spring forces by the grip available: the sticking model's friction circle, and the sliding
//! model's friction force.

use crate::fpu::round_x87;

use super::layout::{Frame, StateWords};

/// Limits the `deflection` of a spring of `stiffness` to the `friction_force` that can hold it. The spring force is
/// `t = -stiffness · deflection`. Where it exceeds the friction force the deflection becomes `-friction_force /
/// stiffness` (0 when the stiffness is 0.001 or less); where it does not, the deflection is kept, or becomes 0 for a
/// spring force that points against the friction force. The branches follow the original, so NaN takes the same path.
pub fn clamp_deflection(friction_force: f32, stiffness: f32, deflection: f32) -> f32 {
    const MIN_STIFFNESS: f32 = 0.001;
    let (a, b, c) = (friction_force, stiffness, deflection);
    let t = 0.0 - b * c;
    if a > 0.0 {
        if a < t {
            if b > MIN_STIFFNESS {
                -1.0 / b * a
            } else {
                0.0
            }
        } else if 0.0 > t {
            0.0
        } else {
            c
        }
    } else if a > t {
        if b > MIN_STIFFNESS {
            -1.0 / b * a
        } else {
            0.0
        }
    } else if t > 0.0 {
        0.0
    } else {
        c
    }
}

/// What the friction circle reads.
#[derive(Clone, Copy)]
pub struct GripInput {
    /// The normal force, N.
    pub normal_force: f32,
    /// The static and the kinetic friction coefficients (the static one applies while the state is clamped).
    pub static_friction: f32,
    pub kinetic_friction: f32,
    /// The lateral and the torsional stiffness.
    pub lateral_stiffness: f32,
    pub torsional_stiffness: f32,
}

/// What the friction circle leaves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grip {
    /// The new lateral deflection, m, and torsion angle, rad.
    pub deflection: f32,
    pub torsion: f32,
    /// Whether the forces were clamped to the circle.
    pub clamped: u32,
    /// The sticking model's slip phases: 2 when clamped, else 0.
    pub phase_a: f32,
    pub phase_b: f32,
}

/// Clamps the carcass's lateral force `u = -deflection · lateral_stiffness` and longitudinal force `t = (-1 / R) ·
/// torsion · torsional_stiffness` to the friction circle of radius `normal_force · μ`, where μ is the static
/// coefficient while the state is clamped and the kinetic one otherwise. When `!(t² + u² > radius²)` the state's
/// deflection and torsion are kept (the same bits) and the phases are 0. Otherwise both are scaled by `f`: 0 unless
/// the squared force is above 1e-5 and the normal force above 0.001, else `normal_force · kinetic_friction` over the
/// force's length (each rounded at the x87 precision), at most 1 (a NaN stays). The phases are then 2. NaN compares as
/// the original's `!(a > b)`.
pub fn limit_to_friction_circle(state: &StateWords, g: GripInput) -> Grip {
    let round24 = round_x87;
    let (deflection, torsion, radius) = (state.lateral_deflection, state.torsion_angle, state.effective_radius);
    let limit = g.normal_force * if state.grip_clamped != 0 { g.static_friction } else { g.kinetic_friction };
    let u = 0.0 - deflection * g.lateral_stiffness;
    let t = -1.0 / radius * torsion * g.torsional_stiffness;
    let force_sq = t * t + u * u;
    let limit_sq = limit * limit;
    if !(force_sq > limit_sq) {
        return Grip { deflection, torsion, clamped: 0, phase_a: 0.0, phase_b: 0.0 };
    }
    let f = if !(force_sq > 1e-5) || !(g.normal_force > 0.001) {
        0.0
    } else {
        // The numerator and the root stay on the x87 stack; the store rounds the quotient to f32.
        let numerator = round24(g.normal_force as f64 * g.kinetic_friction as f64);
        let length = round24((force_sq as f64).sqrt());
        let ratio = round24(numerator / length) as f32;
        if ratio > 1.0 {
            1.0
        } else {
            ratio
        }
    };
    Grip { deflection: deflection * f, torsion: torsion * f, clamped: 1, phase_a: 2.0, phase_b: 2.0 }
}

/// [`limit_to_friction_circle`] on the frame when the sticking weight is positive: stores the sticking model's
/// deflection and torsion and phases, and the clamp flag. Otherwise both are 0 and the flag is cleared.
pub fn limit_sticking(f: &mut Frame) {
    if !(f.scratch.sticking_weight > 0.0) {
        f.scratch.sticking_deflection = 0.0;
        f.scratch.sticking_torsion = 0.0;
        f.state.grip_clamped = 0;
        return;
    }
    let s = &f.scratch;
    let g = GripInput {
        normal_force: s.normal_force,
        static_friction: s.static_friction,
        kinetic_friction: s.kinetic_friction,
        lateral_stiffness: s.lateral_stiffness,
        torsional_stiffness: s.torsional_stiffness,
    };
    let grip = limit_to_friction_circle(&f.state, g);
    f.scratch.sticking_deflection = grip.deflection;
    f.scratch.sticking_torsion = grip.torsion;
    f.state.grip_clamped = grip.clamped;
    f.scratch.stick_phase_a = grip.phase_a;
    f.scratch.stick_phase_b = grip.phase_b;
}
