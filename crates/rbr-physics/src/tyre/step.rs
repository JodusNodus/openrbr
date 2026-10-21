//! The per-wheel tyre step: it frames the contact, takes the slip and the weights of the sticking and the sliding
//! model, the soil's drag, the table lookups of the tyre type, the contact patch's forces, the heat and the outputs.
//! The operation order and the branches follow the original: the clamps and the max picks are `jbe`/`ja` forms, so a
//! NaN takes the same branch. The x87 chains round each op to 24 bits (`round24`); the SSE parts are f32.
//!
//! The frame is loaded once by the caller and stored once after it. A table index the frame does not hold makes the
//! step return `Outside`, and the caller then runs the original.

use super::contact_frame;
use super::contact_patch;
use super::forces;
use super::grip;
use super::heat;
use super::layout::{
    apply_heat, apply_soil, copy_material, heat_input, rolling_input, sig, soil_input, table_at, Frame, Outside,
    StepArgs, StepOut, Table,
};
use super::locate::locate;
use super::noise_signal::fill_noise_signal;
use super::output_block::{clamp01, write_output_block, OutputBlock};
use super::sliding;
use super::slip;
use super::soil::soil_drag;
use crate::fpu::{precision, round_x87, Precision};
use crate::math::trig::{one_minus_x87_cos_f64, x87_cos};
use crate::math::vec::Vec4;
use crate::ngp::heat::TyreThermal;
use crate::ngp::inclination;

/// The speed (m/s) above which the torsional stiffness stops being reduced, over the 10 m/s it takes, and the factors of
/// the reduction (0.1 at rest, 1 from 20 m/s on). NGP's output scale is 0.025 (1.02: 0.1).
const SPEED_KNEE: f32 = 10.0;
const SPEED_SCALE: f32 = 0.1;
const OUTPUT_SCALE: f32 = 0.025;
const NINE_TENTHS: f32 = 0.9;
/// The weights of the two temperature searches in the temperature grip: the tread's (0.4) and the surface's (0.6).
const WEIGHT_NEAR: f32 = 0.4;
const WEIGHT_FAR: f32 = 0.6;

/// The no-contact path (the contact frame or the contact patch failed): the outputs are cleared and the contact flag is 0.
fn no_contact(f: &mut Frame, a: &StepArgs) -> StepOut {
    f.scratch.in_contact = 0;
    f.scratch.signal = [0; 24];
    f.scratch.signal[sig::DEBUG_ID] = f.setup.debug_id;
    f.scratch.signal[sig::FLAG] = 0;
    f.state.last_point = a.contact_point;
    f.state.lateral_deflection = 0.0;
    f.state.torsion_angle = 0.0;
    f.state.active = 0;
    StepOut { force: Vec4::default(), torque: Vec4::default(), reaction_torque: 0.0, sticking_load: None, block: None }
}

/// One step of the wheel's tyre. Its outputs are the windows the vector test compares (`tests/tyre_vectors.rs`).
///
/// `ngp` is NGP's state. The x87 chains (here and in the callees) run at 53 bits, the inclination wrappers
/// (`ngp::inclination`) scale the blend pairs and heat lerps before the soil's drag and the heat factor around the heat
/// step (NGP's wrapper restores the setup's tables afterwards: they are not stored here), the temperature grip scales
/// the kinetic friction in place of the static one a second time (1.02 scales the static one twice), and the output
/// scale is 0.025.
pub fn step(f: &mut Frame, a: &StepArgs, ngp: &TyreThermal) -> Result<StepOut, Outside> {
    let round24 = round_x87;
    if f.state.started == 0 {
        f.state.started_dirty = 1;
        f.state.started = 1;
    }
    if a.copy_debug_words {
        f.scratch.debug_words = a.debug_words;
    }
    copy_material(f, &a.material);
    f.scratch.contact_point = a.contact_point;
    f.scratch.spin = a.spin;
    f.scratch.hub_height = a.hub_height;
    f.scratch.slip_area_scale = a.slip_area_scale;
    f.state.pressure_segment = locate(f.state.pressure_segment, &f.setup.pressure_axis, f.state.air_pressure)?;

    // The stiffness factor of the air pressure: the table entries at the two segment ends, blended.
    let sb = f.state.pressure_segment;
    let stiffness_factor = (1.0 - sb.frac) * table_at(&f.setup.stiffness_factor_by_pressure, sb.lo)?
        + table_at(&f.setup.stiffness_factor_by_pressure, sb.hi)? * sb.frac;
    f.scratch.stiffness_factor = stiffness_factor;
    f.scratch.lateral_stiffness = f.setup.carcass_stiffness_lateral * stiffness_factor;
    f.scratch.torsional_stiffness = f.setup.carcass_stiffness_torsional * stiffness_factor;

    if !contact_frame::build(f, a.normal, a.axle) || contact_patch::contact_patch(f).is_none() {
        return Ok(no_contact(f, a));
    }

    f.scratch.in_contact = 1;
    slip::update_slip(f, [a.velocity.x, a.velocity.y, a.velocity.z]);
    // The weight of the speed above 10 (`(v - 10) · 0.1`, clamped to [0, 1]) scales the torsional stiffness.
    let w = clamp01((f.scratch.speed - SPEED_KNEE) * SPEED_SCALE);
    f.scratch.torsional_stiffness *= w * NINE_TENTHS + 0.1;
    f.scratch.extra_force = 0.0;
    f.scratch.extra_axle_torque = 0.0;
    f.scratch.extra_term = 0.0;
    let (k, heat_lerps) = inclination::grip_scale(inclination::degrees(f.scratch.inclination_sin));
    let d = &mut f.setup;
    d.cornering_friction_by_load.scale(k, &mut d.traction_friction_by_load);
    if heat_lerps {
        d.cornering_stiffness_by_load.scale(k, &mut d.traction_stiffness_by_load);
    }
    let soil = soil_drag(soil_input(f));
    apply_soil(f, &soil);

    // The total of the three forces (`soil_drag + extra_force`, then `+ tread_force`) divides the three ratios, in this order.
    let total = (f.scratch.soil_drag + f.scratch.extra_force) + f.scratch.tread_force;
    f.scratch.contact_pressure = total / f.scratch.tread_contact_area;
    f.scratch.total_force_ratio = total / f.scratch.normal_force;
    f.scratch.tread_force_share = f.scratch.tread_force / total;
    f.state.contact_pressure_segment =
        locate(f.state.contact_pressure_segment, &f.setup.contact_pressure_axis, f.scratch.contact_pressure)?;
    f.state.load_segment = locate(f.state.load_segment, &f.setup.load_axis, f.scratch.normal_force)?;

    // The stiffnesses at the load: the table entries at the two segment ends, blended, times the stiffness factor.
    let sh = f.state.load_segment;
    let lerp = |table: &Table| -> Result<f32, Outside> {
        Ok((1.0 - sh.frac) * table_at(table, sh.lo)? + table_at(table, sh.hi)? * sh.frac)
    };
    let cornering_stiffness = lerp(&f.setup.cornering_stiffness_by_load)? * stiffness_factor;
    let traction_stiffness = lerp(&f.setup.traction_stiffness_by_load)? * stiffness_factor;
    f.scratch.cornering_stiffness = cornering_stiffness;
    f.scratch.traction_stiffness = traction_stiffness;

    // The x87 part: the lateral deflection grows by `axle·velocity · dt + (1 - cos inclination) · -((1 - cos) · camber
    // radius)`, rounded to 24 bits.
    let angle = (f.scratch.contact_velocity[1] / f.scratch.camber_radius) * a.dt;
    // The subtraction takes the cosine's extended result: NGP's 53-bit precision keeps its low bits.
    let one_minus_cos = match precision() {
        Precision::Pc24 => 1.0 - x87_cos(angle),
        Precision::Pc53 => one_minus_x87_cos_f64(angle),
    };
    let neg = -round24(round24(one_minus_cos) * f64::from(f.scratch.camber_radius));
    let axle = [a.axle.x, a.axle.y, a.axle.z];
    let velocity = [a.velocity.x, a.velocity.y, a.velocity.z];
    // Each product is rounded before the sum, as the x87 does: `(d1·o1 + d2·o2) + d0·o0`.
    let product = |k: usize| round24(f64::from(axle[k]) * f64::from(velocity[k]));
    let d12 = round24(product(1) + product(2));
    let dot = round24(product(0) + d12);
    let scaled = round24(dot * f64::from(a.dt));
    let rest = round24(round24(1.0 - f64::from(f.scratch.inclination_cos)) * neg);
    let growth = round24(scaled + rest) + f64::from(f.state.lateral_deflection);
    f.state.lateral_deflection = round24(growth) as f32;
    f.state.torsion_angle -= f.scratch.spin_slip * a.dt;

    // The temperature searches: the sector's surface temperature and the tread temperature.
    let idx = f.scratch.sector;
    let surface_temperature = *f
        .state
        .surface_temperature
        .get(idx as usize)
        .ok_or(Outside { index: idx as i32, len: f.state.surface_temperature.len() })?;
    f.state.surface_temperature_segment =
        locate(f.state.surface_temperature_segment, &f.setup.temperature_axis, surface_temperature)?;
    f.state.tread_temperature_segment =
        locate(f.state.tread_temperature_segment, &f.setup.temperature_axis, f.state.tread_temperature)?;

    // The temperature grip: the rubber friction at the two temperatures, weighted 0.6 for the surface and 0.4 for the
    // tread; then the wear factor when the surface is wet.
    let (ss, ss2) = (f.state.surface_temperature_segment, f.state.tread_temperature_segment);
    let d = |index: u32| table_at(&f.setup.rubber_friction_by_temperature, index);
    let (surface_weight, tread_weight) = (ss.frac, ss2.frac);
    let mut temperature_grip = (((1.0 - tread_weight) * d(ss2.lo)? * WEIGHT_NEAR
        + (1.0 - surface_weight) * d(ss.lo)? * WEIGHT_FAR)
        + (d(ss.hi)? * surface_weight) * WEIGHT_FAR)
        + (d(ss2.hi)? * tread_weight) * WEIGHT_NEAR;
    if 0.0 < f.scratch.water_factor {
        let wear = f.scratch.wear[idx as usize];
        temperature_grip *= (((f.setup.wet_wear_factor - 1.0) * f.scratch.water_factor + 1.0) - 1.0) * wear + 1.0;
    }

    // The friction takes the temperature grip twice (the original does), each store after a dirty flag.
    f.scratch.static_friction *= temperature_grip;
    f.scratch.static_friction_dirty = 1;
    f.scratch.kinetic_friction *= temperature_grip;
    f.scratch.static_friction_dirty = 1;
    f.scratch.temperature_grip = temperature_grip;

    slip::weigh_models(f);
    sliding::solve_sliding(f)?;
    grip::limit_sticking(f);
    f.state.lateral_deflection = f.scratch.sticking_deflection * f.scratch.sticking_weight
        + f.scratch.sliding_deflection * f.scratch.sliding_weight;
    f.state.torsion_angle =
        f.scratch.sticking_torsion * f.scratch.sticking_weight + f.scratch.sliding_torsion * f.scratch.sliding_weight;

    // The slip phase: the larger phase of each model, weighted. `x > y` takes `x`, else `y` (a NaN gives `y`).
    let sliding_phase = if f.scratch.lateral_phase > f.scratch.longitudinal_phase {
        f.scratch.lateral_phase
    } else {
        f.scratch.longitudinal_phase
    };
    let sticking_phase = if f.scratch.stick_phase_a > f.scratch.stick_phase_b {
        f.scratch.stick_phase_a
    } else {
        f.scratch.stick_phase_b
    };
    f.scratch.slip_phase = f.scratch.sliding_weight * sliding_phase + f.scratch.sticking_weight * sticking_phase;
    let (force, torque) = forces::solve_forces(f, a.dt, a.torque_in);

    // The rolling resistance torque is the x87 term; the float store takes its f32 rounding and the reaction torque
    // adds the torsion torque at 24 bits.
    let rolling = forces::rolling_torque(rolling_input(f));
    f.scratch.rolling_torque = rolling as f32;
    let reaction_torque = round24(rolling + f64::from(f.scratch.torsion_torque)) as f32;
    let sticking_load = f.scratch.normal_force * OUTPUT_SCALE * f.scratch.sticking_weight;

    let rolling_torque = f.scratch.rolling_torque;
    // NGP's heat power, with the contact flag set (this path set it).
    let k = inclination::heat_scale(inclination::degrees(f.scratch.inclination_sin), f.setup.inclination_heat_scale);
    f.scratch.ngp_heat_power = (rolling_torque * f.scratch.spin).abs() * k;
    f.scratch.rolling_torque = k * rolling_torque;
    let heat_in = heat_input(f, f.scratch.lateral_force, f.scratch.longitudinal_force, a.dt)?;
    apply_heat(f, &heat::heat_step(heat_in, ngp));
    f.scratch.rolling_torque = rolling_torque;
    let block: OutputBlock = write_output_block(f);
    fill_noise_signal(f, a.velocity, a.debug_words, a.dt);
    f.state.active = 1;
    Ok(StepOut { force, torque, reaction_torque, sticking_load: Some(sticking_load), block: Some(block) })
}
