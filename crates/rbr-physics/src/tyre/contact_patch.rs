//! The contact patch: the sector of the tread in contact, the patch's geometry (its length and areas), and the solve of
//! how the tyre and the soil give way, which sets the normal force and the surface's friction coefficients.
//!
//! The x87 steps are `round24` on f64 and the SSE steps are f32. Every branch follows the original's `ja`/`jbe` order,
//! so a NaN takes the same path. The sector is `trunc_i32`.

use super::layout::{apply_contact, contact_input, Frame};
use super::solve::{solve_contact, Coefficients};
use crate::fpu::{round_x87, trunc_i32};

/// Twenty half turns, rad: added to the contact angle so that it is positive.
const ANGLE_OFFSET: f32 = 62.83185577392578;
/// Sectors per radian: eight sectors of a quarter of a half turn each.
const SECTORS_PER_RAD: f32 = 1.2732394933700562;
/// The smallest penetration and inclination sine that count, and the smallest sink factor.
const MIN_PENETRATION: f32 = 1e-4;
const MIN_SINK_FACTOR: f32 = 1e-10;
/// The soil's largest stiffness-to-area ratio for the solve, and the camber radius when the inclination is zero.
const MAX_AREA_RATIO: f32 = 5000000.0;
const FLAT_CAMBER_RADIUS: f32 = 10000000.0;
const QUARTER: f64 = 0.25;
const HALF_NEG: f64 = -0.5;
const MINUS_ONE: f64 = -1.0;

/// Works out the contact patch on the frame. Returns the carcass force when the patch is solved (the normal force is
/// set to it), and `None` when the penetration is under 1e-4 m (the writes before that test stay).
pub fn contact_patch(f: &mut Frame) -> Option<f32> {
    let round24 = round_x87;
    // The sector of the tread in contact: `trunc((angle + 20π) · 4/π) & 7`.
    let angle =
        round24(round24(f64::from(f.scratch.contact_angle) + f64::from(ANGLE_OFFSET)) * f64::from(SECTORS_PER_RAD));
    let sector = (trunc_i32(angle) & 7) as u32;
    f.scratch.sector = sector;
    f.state.sector = sector;
    f.scratch.sector_copy = sector;

    // The tread left on the sector: `(1 - wear) · tread_depth`.
    let wear = f.scratch.wear[sector as usize];
    let tread_remaining = (1.0 - wear) * f.setup.tread_depth;
    f.scratch.tread_remaining = tread_remaining;

    // The lateral deflection as a fraction of the sidewall height: `sqrt(deflection² / sidewall²)` when the sidewall is
    // the larger, else 1.
    let deflection_sq = f.state.lateral_deflection * f.state.lateral_deflection;
    let sidewall_sq = f.setup.sidewall_height * f.setup.sidewall_height;
    let deflection_ratio = if sidewall_sq > deflection_sq {
        round24(round24(f64::from(deflection_sq) / f64::from(sidewall_sq)).sqrt()) as f32
    } else {
        1.0
    };
    f.scratch.lateral_deflection_ratio = deflection_ratio;

    // The camber radius: the hub height over the inclination's sine while that is above 1e-4 in magnitude.
    let hub_height = f.scratch.hub_height;
    let sin = f.scratch.inclination_sin;
    let abs_sin = if sin > 0.0 { sin } else { 0.0 - sin };
    let camber_radius = if abs_sin > MIN_PENETRATION { hub_height / sin } else { FLAT_CAMBER_RADIUS };
    f.scratch.camber_radius = camber_radius;

    // The outer radius and the penetration of it below the surface. The early exit is a penetration under 1e-4 m.
    let carcass_radius = f.setup.carcass_radius;
    let outer_radius = tread_remaining + carcass_radius;
    let penetration = outer_radius - hub_height;
    if MIN_PENETRATION > penetration {
        return None;
    }

    // The patch's half length `sqrt((2 · outer_radius - penetration) · penetration)` (x87), and the areas.
    let half_length = round24(
        round24(
            round24(round24(f64::from(outer_radius) + f64::from(outer_radius)) - f64::from(penetration))
                * f64::from(penetration),
        )
        .sqrt(),
    ) as f32;
    f.scratch.patch_length = half_length + half_length;
    let carcass_width = f.setup.carcass_width;
    let contact_area = half_length * carcass_width * 2.0;
    let tread_area = f.setup.tread_coverage * contact_area;
    let groove_area = contact_area - tread_area;
    f.scratch.tread_contact_area = tread_area;

    // The soil solve applies when the sink factor and the soil thickness are positive and the contact area is not too
    // large against the sink factor.
    let sink_factor = f.scratch.sink_factor;
    let soil_thickness = f.scratch.soil_thickness;
    let solve_soil =
        sink_factor > MIN_SINK_FACTOR && soil_thickness > 0.0 && MAX_AREA_RATIO > contact_area / sink_factor;

    // The coefficients of the force-deflection quadratic of the patch, and the tread's stiffness.
    let pressure_stiffness = f.setup.pressure_ratio * f.state.air_pressure;
    let inv = 1.0 / half_length;
    let chord_slope = inv * penetration;
    let chord_lever = chord_slope * hub_height;
    let chord_excess = chord_lever - half_length;
    let quadratic_shape = chord_excess / (penetration * penetration);
    let linear_shape = chord_lever - quadratic_shape * penetration * 2.0;
    let pressure_linear = linear_shape * pressure_stiffness * carcass_width * 2.0;
    let linear = pressure_linear + f.state.vertical_stiffness;
    let quadratic = quadratic_shape * pressure_stiffness * carcass_width * 2.0;
    let tread_stiffness = f.state.tread_elasticity * tread_area / (f.setup.carcass_thickness + tread_remaining);

    if !solve_soil {
        // The carcass alone: the deflection is the root `(-0.5 · u2) - sqrt(...)` and the force `(quadratic · d + linear)
        // · d` (x87).
        let u1 = round24(f64::from(linear) + f64::from(tread_stiffness));
        let u2 = round24(u1 / f64::from(quadratic));
        let u3 = round24(HALF_NEG * u2);
        let v1 = round24(round24(u2 * u2) * QUARTER);
        let v2 = round24(f64::from(tread_stiffness) * f64::from(penetration));
        let v3 = round24(MINUS_ONE / f64::from(quadratic));
        let v5 = round24(v1 - round24(v2 * v3));
        let deflection = round24(u3 - round24(v5.sqrt()));
        let force =
            round24(round24(round24(f64::from(quadratic) * deflection) + f64::from(linear)) * deflection) as f32;
        f.state.tread_engaged = 1;
        f.state.carcass_engaged = 0;
        f.scratch.normal_force = force;
        f.scratch.carcass_force = force;
        f.scratch.tread_force = force;
        f.scratch.indentation.deflection = deflection as f32;
        f.scratch.indentation.sink_depth = 0.0;
        f.scratch.indentation.carcass_compression = 0.0;
        return Some(force);
    }

    // The solve: the indentation set here, then the case picked by the flags the last solve left.
    f.scratch.indentation.penetration = penetration;
    f.scratch.indentation.carcass_radius = carcass_radius;
    f.scratch.indentation.hub_height = hub_height;
    f.scratch.indentation.tread_remaining = tread_remaining;
    let inv_sink = 1.0 / sink_factor;
    let coefficients = Coefficients {
        quadratic,
        linear,
        tread_stiffness,
        sink_factor,
        tread_area,
        groove_area,
        groove_soil_stiffness: inv_sink * groove_area,
        tread_soil_stiffness: inv_sink * tread_area,
    };
    let mut contact = contact_input(f, coefficients);
    let (tread_loaded, carcass_compressed) = (contact.tread_engaged != 0, contact.carcass_engaged != 0);
    solve_contact(&mut contact, tread_loaded, carcass_compressed, 0);
    apply_contact(f, &contact);

    // The surface's friction coefficients blend between the soil's (nothing sunk through) and the hard ground's (the
    // soil's full thickness): `static_soil · (1 - t) + static · t` with `t = sink_depth / soil_thickness`.
    let t = f.scratch.indentation.sink_depth / soil_thickness;
    let one_minus = 1.0 - t;
    let static_friction = f.scratch.static_friction_soil * one_minus + f.scratch.static_friction * t;
    let kinetic_friction = f.scratch.kinetic_friction_soil * one_minus + f.scratch.kinetic_friction * t;
    f.scratch.static_friction_dirty = 1;
    f.scratch.static_friction = static_friction;
    f.scratch.kinetic_friction_dirty = 1;
    f.scratch.kinetic_friction = kinetic_friction;
    let force = f.scratch.carcass_force;
    f.scratch.normal_force = force;
    Some(force)
}
