//! The contact solves: how the tread, the carcass and the soil share the tyre's penetration into the surface. Four
//! cases, by whether the tread is loaded and whether the carcass is compressed, solve a quadratic each; a solve whose
//! result contradicts its case hands over to the case that fits (the cases call each other up to four times).
//!
//! Operation order and precision follow the original: each x87 step is `round24` on f64, and the SSE steps are f32.
//! `t` is the f32 quotient, `d` the x87 chain `t·(-0.5) - sqrt(...)`.

use super::layout::Indentation;
use crate::fpu::round_x87;

/// The constants of the quadratic's root, and the tolerance of the case tests.
const HALF_NEG: f64 = -0.5;
const QUARTER: f64 = 0.25;
const MINUS_ONE: f64 = -1.0;
const TOLERANCE: f32 = 0.00001;

/// The coefficients of a solve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coefficients {
    /// The carcass force is `(quadratic · d + linear) · d` for the tread deflection `d`.
    pub quadratic: f32,
    pub linear: f32,
    /// The tread's stiffness, N/m.
    pub tread_stiffness: f32,
    /// The soil's sink factor, the tread's contact area and the groove area (the solves do not read them).
    pub sink_factor: f32,
    pub tread_area: f32,
    pub groove_area: f32,
    /// The soil's stiffness under the groove area and under the tread area, N/m.
    pub groove_soil_stiffness: f32,
    pub tread_soil_stiffness: f32,
}

/// The values one solve reads and writes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    /// The soil's thickness, m.
    pub soil_thickness: f32,
    /// The penetration, the deflection and the other depths; the solve writes `deflection`, `sink_depth` and
    /// `carcass_compression`.
    pub indentation: Indentation,
    /// The normal force (not touched), and the carcass's and the tread's force, which the solve writes.
    pub normal_force: f32,
    pub carcass_force: f32,
    pub tread_force: f32,
    pub coefficients: Coefficients,
    /// The flags the last solve left, and this one leaves (by use: the tread is loaded, the carcass is compressed).
    pub tread_engaged: u32,
    pub carcass_engaged: u32,
}

/// The root `d = t·(-0.5) - sqrt(t²/4 - (-1/quadratic)·y)` of the quadratic, with the x87 rounding of each step; `y`
/// is the product term.
fn quadratic_root(t: f32, quadratic: f32, y: f64) -> f64 {
    let round24 = round_x87;
    let quarter = round24(round24(t as f64 * t as f64) * QUARTER);
    let neg_inv = round24(MINUS_ONE / quadratic as f64);
    let prod = round24(neg_inv * y);
    let root = round24(round24(quarter - prod).sqrt());
    round24(round24(t as f64 * HALF_NEG) - root)
}

/// Ends a solve that leaves the carcass force equal to the tread force: the carcass compression is 0.
fn end_equal_forces(c: &mut Contact, tread_engaged: u32, carcass_engaged: u32) {
    c.indentation.carcass_compression = 0.0;
    c.carcass_force = c.tread_force;
    c.tread_engaged = tread_engaged;
    c.carcass_engaged = carcass_engaged;
}

/// Ends a solve with the carcass force `(quadratic · d + linear) · d`.
fn end_with_carcass_force(c: &mut Contact, d: f32, tread_engaged: u32, carcass_engaged: u32) {
    let k = &c.coefficients;
    c.carcass_force = (k.quadratic * d + k.linear) * d;
    c.tread_engaged = tread_engaged;
    c.carcass_engaged = carcass_engaged;
}

/// Routes to the case by the two flags, with `iteration` one more than the caller's. Both set: the tread loaded and the
/// carcass compressed; only the tread: the tread alone; only the carcass; neither.
pub fn solve_contact(c: &mut Contact, tread_loaded: bool, carcass_compressed: bool, depth: i32) {
    let iteration = depth.wrapping_add(1);
    match (tread_loaded, carcass_compressed) {
        (true, true) => solve_both(c, iteration),
        (true, false) => solve_tread_only(c, iteration),
        (false, true) => solve_carcass_only(c, iteration),
        (false, false) => solve_neither(c, iteration),
    }
}

/// The tread loaded, the carcass not compressed. Ends when that holds again; otherwise hands over while `iteration <=
/// 3`, and ends otherwise. The soil is at its full depth.
pub fn solve_tread_only(c: &mut Contact, iteration: i32) {
    let round24 = round_x87;
    let soil_thickness = c.soil_thickness;
    let k = c.coefficients;
    let i = c.indentation;
    let neg_tread = 0.0 - k.tread_stiffness;
    let t = (k.linear - neg_tread) / k.quadratic;
    let load = (i.penetration - soil_thickness) * k.tread_stiffness;
    c.indentation.sink_depth = soil_thickness;
    let d = quadratic_root(t, k.quadratic, load as f64);
    let deflection = d as f32;
    c.indentation.deflection = deflection;
    let tread_force = neg_tread * deflection + load;
    c.indentation.carcass_compression = round24(round24(i.carcass_radius as f64 - i.hub_height as f64) - d) as f32;
    c.tread_force = tread_force;
    let tread_loaded = tread_force > -TOLERANCE;
    let carcass_compressed = c.indentation.carcass_compression > TOLERANCE;
    if tread_loaded && !carcass_compressed || (iteration as u32) > 3 {
        end_equal_forces(c, 1, 0);
    } else {
        solve_contact(c, tread_loaded, carcass_compressed, iteration);
    }
}

/// Both. Ends when both hold again; otherwise hands over while `iteration <= 3`, and ends otherwise.
pub fn solve_both(c: &mut Contact, iteration: i32) {
    let round24 = round_x87;
    let soil_thickness = c.soil_thickness;
    let k = c.coefficients;
    let i = c.indentation;
    let neg_tread = 0.0 - k.tread_stiffness;
    let neg_groove = 0.0 - k.groove_soil_stiffness;
    let t = ((k.linear - neg_tread) - neg_groove) / k.quadratic;
    let squash = i.carcass_radius - i.hub_height;
    let load = (i.penetration - soil_thickness) * k.tread_stiffness;
    c.indentation.sink_depth = soil_thickness;
    let inner = round24(round24(squash as f64 * k.groove_soil_stiffness as f64) + load as f64);
    let d = quadratic_root(t, k.quadratic, inner);
    let deflection = d as f32;
    let compression = squash - deflection;
    c.indentation.deflection = deflection;
    c.indentation.carcass_compression = compression;
    let tread_force = deflection * neg_tread + load;
    c.tread_force = tread_force;
    let tread_loaded = tread_force > -TOLERANCE;
    let carcass_compressed = c.indentation.carcass_compression > -TOLERANCE;
    if tread_loaded && carcass_compressed || (iteration as u32) > 3 {
        end_with_carcass_force(c, deflection, 1, 1);
    } else {
        solve_contact(c, tread_loaded, carcass_compressed, iteration);
    }
}

/// Neither. Ends when neither holds again; otherwise hands over while `iteration < 4`, and ends otherwise. The sink
/// depth follows from the deflection.
pub fn solve_neither(c: &mut Contact, iteration: i32) {
    let round24 = round_x87;
    let soil_thickness = c.soil_thickness;
    let k = c.coefficients;
    let i = c.indentation;
    let series = 1.0 / (1.0 / k.tread_soil_stiffness + 1.0 / k.tread_stiffness);
    let load = i.penetration * series;
    let neg_series = 0.0 - series;
    let t = (k.linear - neg_series) / k.quadratic;
    let d = quadratic_root(t, k.quadratic, load as f64);
    let deflection = d as f32;
    c.indentation.deflection = deflection;
    let tread_force = neg_series * deflection + load;
    c.indentation.carcass_compression = round24(round24(i.carcass_radius as f64 - i.hub_height as f64) - d) as f32;
    c.tread_force = tread_force;
    let sink = i.penetration - deflection - tread_force / k.tread_stiffness;
    c.indentation.sink_depth = sink;
    let tread_loaded = sink - soil_thickness > TOLERANCE;
    let carcass_compressed = c.indentation.carcass_compression > TOLERANCE;
    if !tread_loaded && !carcass_compressed || (iteration as u32) >= 4 {
        end_equal_forces(c, 0, 0);
    } else {
        solve_contact(c, tread_loaded, carcass_compressed, iteration);
    }
}

/// Only the carcass. Ends when that holds again; otherwise hands over while `iteration < 4`, and ends otherwise.
pub fn solve_carcass_only(c: &mut Contact, iteration: i32) {
    let round24 = round_x87;
    let soil_thickness = c.soil_thickness;
    let k = c.coefficients;
    let i = c.indentation;
    let series = 1.0 / (1.0 / k.tread_soil_stiffness + 1.0 / k.tread_stiffness);
    let load = i.penetration * series;
    let neg_series = 0.0 - series;
    let neg_groove = 0.0 - k.groove_soil_stiffness;
    let t = ((k.linear - neg_series) - neg_groove) / k.quadratic;
    let squash = i.carcass_radius - i.hub_height;
    let inner = round24(round24(squash as f64 * k.groove_soil_stiffness as f64) + load as f64);
    let d = quadratic_root(t, k.quadratic, inner);
    let deflection = d as f32;
    let compression = squash - deflection;
    c.indentation.deflection = deflection;
    c.indentation.carcass_compression = compression;
    let tread_force = deflection * neg_series + load;
    c.tread_force = tread_force;
    let sink = i.penetration - deflection - tread_force / k.tread_stiffness;
    c.indentation.sink_depth = sink;
    let tread_loaded = sink - soil_thickness > TOLERANCE;
    let carcass_compressed = c.indentation.carcass_compression > -TOLERANCE;
    if !tread_loaded && carcass_compressed || (iteration as u32) >= 4 {
        end_with_carcass_force(c, deflection, 0, 1);
    } else {
        solve_contact(c, tread_loaded, carcass_compressed, iteration);
    }
}
