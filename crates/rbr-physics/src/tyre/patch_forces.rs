//! The friction forces of one contact patch: the friction curve evaluated for the lateral and the longitudinal slip,
//! with the limits at the current load, and the pneumatic trail.
//!
//! Every branch takes the original's `ja`/`jbe` order (`a > b` or `!(a > b)`), so a NaN takes the same path.

use super::friction_curve::{friction_curve, CurveInput};
use super::friction_limits::{BlendPair, LimitsInput};
use super::slip::slip_directions;
use crate::fpu::round_x87;
use crate::ngp;

/// The weight of the sink depth per metre, and the pneumatic trail fractions it moves (`weight · slope + base`): from
/// 1/3 to 1/4 of the half patch length for the linear part, and from 0.05 to -0.1 for the sliding part.
const SINK_WEIGHT_PER_METRE: f32 = 33.333336;
const TRAIL_LINEAR_SLOPE: f32 = -0.083333343;
const TRAIL_LINEAR_BASE: f32 = 0.33333334;
const TRAIL_SLIDING_SLOPE: f32 = -0.15;
const TRAIL_SLIDING_BASE: f32 = 0.05;
/// The falloff table of the friction curve's falling part (the fraction of the drop left, from 1 at the peak to 0 at
/// the end of the curve). NGP's values for the second to fifth entries (0.95, 0.875, 0.775, 0.65; 1.02: 0.99, 0.96,
/// 0.87, 0.7). The curve reads an eleventh entry past the end for a NaN index: it is 0 here, and the result is NaN
/// either way.
const FALLOFF: [f32; 11] = [1.0, 0.95, 0.875, 0.775, 0.65, 0.5, 0.35, 0.2, 0.1, 0.05, 0.0];

/// What the patch forces read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatchInput {
    /// The slip speed, and the lateral and longitudinal velocities of the contact point, m/s.
    pub speed: f32,
    pub lateral_velocity: f32,
    pub longitudinal_velocity: f32,
    /// The wheel's spin, rad/s, and the effective radius, m.
    pub spin: f32,
    pub effective_radius: f32,
    /// The surface's static and kinetic friction, and the tyre's reference friction.
    pub static_friction: f32,
    pub kinetic_friction: f32,
    pub reference_friction: f32,
    /// The normal force, N, and the cornering and traction stiffness at that load.
    pub normal_force: f32,
    pub cornering_stiffness: f32,
    pub traction_stiffness: f32,
    /// The sine of the inclination.
    pub inclination_sin: f32,
    /// The contact patch's length, m, and how far the tyre sinks into the soil, m.
    pub patch_length: f32,
    pub sink_depth: f32,
    /// The weight between the two load entries and the tables there.
    pub weight: f32,
    pub tables: [BlendPair; 4],
}

/// The patch's forces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatchForces {
    /// The lateral and longitudinal friction forces, N.
    pub lateral_force: f32,
    pub longitudinal_force: f32,
    /// The pneumatic trail, m.
    pub pneumatic_trail: f32,
    /// The slip phases of the lateral and the longitudinal direction.
    pub lateral_phase: f32,
    pub longitudinal_phase: f32,
}

/// A slip measure `a` against the peak `c` and end `c3` slips: `a / c` up to the peak, 2 from `c3` on, and
/// `(a - c) / (c3 - c) + 1` between.
fn ratio(a: f32, c: f32, c3: f32) -> f32 {
    if c > a {
        a / c
    } else if !(c3 > a) {
        2.0
    } else {
        (a - c) / (c3 - c) + 1.0
    }
}

/// The patch forces. The limits are NGP's wrapper (`ngp::outputs`), and the x87 chains (and those of the callees) run
/// at 53 bits.
pub fn patch_forces(x: &PatchInput) -> PatchForces {
    let round24 = round_x87;
    let falloff = |i: u32| FALLOFF.get(i as usize).copied().unwrap_or(0.0);
    // The first two slip directions. The third is overwritten by the sign below, so it is unused.
    let [lateral_share, longitudinal_share, _] =
        slip_directions(x.spin, x.lateral_velocity, x.longitudinal_velocity, x.effective_radius);
    let limits = ngp::outputs::friction_limits(&LimitsInput {
        weight: x.weight,
        cornering_friction: x.tables[0],
        traction_friction: x.tables[1],
        cornering_peak_slip: x.tables[2],
        traction_peak_slip: x.tables[3],
        lateral_share,
        longitudinal_share,
        static_scale: x.static_friction * x.reference_friction,
        kinetic_scale: x.kinetic_friction * x.reference_friction,
        cornering_stiffness: x.cornering_stiffness,
        traction_stiffness: x.traction_stiffness,
    });

    // The lateral slip: the lateral velocity over the speed, less the inclination's share, clamped to [-1, 1] and split
    // into its sign and magnitude.
    let x0 = x.lateral_velocity / x.speed - x.inclination_sin * 0.1;
    let lateral_slip = if -1.0 > x0 {
        -1.0
    } else if x0 > 1.0 {
        1.0
    } else {
        x0
    };
    let lateral_negative = 0.0 > lateral_slip;
    let lateral_abs = if lateral_negative { 0.0 - lateral_slip } else { lateral_slip };

    // The longitudinal slip: zero when the longitudinal velocity is within 0.001 of zero (or NaN).
    let y = if x.longitudinal_velocity > 0.001 || -0.001 > x.longitudinal_velocity {
        (0.0 - x.effective_radius * x.spin) / x.longitudinal_velocity - 1.0
    } else {
        0.0
    };
    let longitudinal_negative = 0.0 > y;
    let longitudinal_abs = if longitudinal_negative { 0.0 - y } else { y };
    let longitudinal_sign = if 0.0 > x.longitudinal_velocity { -1.0 } else { 1.0 };

    // The two slips against the peak and end slips of their limits.
    let (lateral_peak_slip, lateral_end_slip) = (limits.lateral_peak_slip, limits.lateral_end_slip);
    let (longitudinal_peak_slip, longitudinal_end_slip) = (limits.longitudinal_peak_slip, limits.longitudinal_end_slip);
    let lateral_ratio = ratio(lateral_abs, lateral_peak_slip, lateral_end_slip);
    let longitudinal_end = if longitudinal_negative { 1.0 } else { longitudinal_end_slip };
    let longitudinal_ratio = ratio(longitudinal_abs, longitudinal_peak_slip, longitudinal_end);

    // The norm of the two ratios. Above 0.01 it pulls the peak slips towards their end slips, and lowers the peak
    // frictions (not below the kinetic ones) by the share of the other direction.
    let sq = round24(
        round24(f64::from(longitudinal_ratio) * f64::from(longitudinal_ratio))
            + round24(f64::from(lateral_ratio) * f64::from(lateral_ratio)),
    );
    let norm = round24(sq.sqrt()) as f32;
    let (mut lateral_peak_slip_now, mut longitudinal_peak_slip_now) = (lateral_peak_slip, longitudinal_peak_slip);
    let (mut lateral_peak, mut longitudinal_peak) = (limits.lateral_peak, limits.longitudinal_peak);
    if norm > 0.01 {
        let inv = 1.0 / norm;
        let longitudinal_part = inv * longitudinal_ratio;
        let lateral_part = inv * lateral_ratio;
        lateral_peak_slip_now = lateral_end_slip - (lateral_end_slip - lateral_peak_slip) * lateral_part;
        longitudinal_peak_slip_now =
            longitudinal_end_slip - (longitudinal_end_slip - longitudinal_peak_slip) * longitudinal_part;
        let longitudinal_reduced = longitudinal_part * limits.longitudinal_peak;
        let lateral_reduced = lateral_part * limits.lateral_peak;
        lateral_peak = if limits.lateral_kinetic > lateral_reduced { limits.lateral_kinetic } else { lateral_reduced };
        longitudinal_peak = if limits.longitudinal_kinetic > longitudinal_reduced {
            limits.longitudinal_kinetic
        } else {
            longitudinal_reduced
        };
    }

    // The weight of the sink depth (100/3 per metre, clamped to [0, 1]) blends the trail fractions.
    let z = x.sink_depth * SINK_WEIGHT_PER_METRE;
    let sink_weight = if 0.0 > z {
        0.0
    } else if z > 1.0 {
        1.0
    } else {
        z
    };
    let trail_linear =
        round24(round24(f64::from(sink_weight) * f64::from(TRAIL_LINEAR_SLOPE)) + f64::from(TRAIL_LINEAR_BASE)) as f32;
    let trail_sliding =
        round24(round24(f64::from(sink_weight) * f64::from(TRAIL_SLIDING_SLOPE)) + f64::from(TRAIL_SLIDING_BASE))
            as f32;

    // The friction curve of the lateral slip; its trail fraction gives the pneumatic trail.
    let lateral = friction_curve(
        &CurveInput {
            slip: lateral_abs,
            peak_slip: lateral_peak_slip_now,
            end_slip: lateral_end_slip,
            peak_friction: lateral_peak,
            slide_friction: limits.lateral_kinetic,
            stiffness: limits.cornering_stiffness,
            load: x.normal_force,
            trail_linear,
            trail_sliding,
            count: 10,
        },
        falloff,
    );
    let lateral_friction = lateral.friction as f32;
    let lateral_signed = if lateral_negative { 0.0 - lateral_friction } else { lateral_friction };
    let pneumatic_trail = x.patch_length * lateral.trail_fraction * 0.5;

    // The longitudinal slip. Its end slip is the limit's, or 1 when the slip is negative.
    let longitudinal = friction_curve(
        &CurveInput {
            slip: longitudinal_abs,
            peak_slip: longitudinal_peak_slip_now,
            end_slip: longitudinal_end,
            peak_friction: longitudinal_peak,
            slide_friction: limits.longitudinal_kinetic,
            stiffness: limits.traction_stiffness,
            load: x.normal_force,
            trail_linear,
            trail_sliding,
            count: 10,
        },
        falloff,
    );
    let longitudinal_friction = longitudinal.friction as f32;
    let longitudinal_signed = if longitudinal_negative { -longitudinal_friction } else { longitudinal_friction };

    PatchForces {
        lateral_force: 0.0 - lateral_signed * x.normal_force,
        longitudinal_force: longitudinal_signed * longitudinal_sign * x.normal_force,
        pneumatic_trail,
        lateral_phase: lateral.phase,
        longitudinal_phase: longitudinal.phase,
    }
}
