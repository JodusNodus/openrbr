//! The model inputs taken from a loaded `Frame`, and their results put back into it.

use super::frame::{Outside, Table};
use super::frame_types::{Frame, SOIL_DENSITY};
use crate::spring_damper::HeatCapacity;
use crate::tyre::forces::RollingInput;
use crate::tyre::friction_limits::BlendPair;
use crate::tyre::heat::{HeatInput, HeatOutput};
use crate::tyre::hold::HoldInput;
use crate::tyre::patch_forces::PatchInput;
use crate::tyre::soil::{SoilDrag, SoilInput};
use crate::tyre::solve::{Coefficients, Contact};

/// The soil's drag inputs from the frame.
pub fn soil_input(f: &Frame) -> SoilInput {
    let s = &f.scratch;
    SoilInput {
        sink_depth: s.indentation.sink_depth,
        carcass_compression: s.indentation.carcass_compression,
        tread_coverage: f.state.tread_coverage,
        carcass_width: f.setup.carcass_width,
        patch_length: s.patch_length,
        normal_force: s.normal_force,
        soil_density: f32::from_bits(s.material[SOIL_DENSITY]),
        lateral_velocity: s.contact_velocity[0],
        longitudinal_velocity: s.contact_velocity[1],
        long_slip_velocity: s.long_slip_velocity,
        lateral_deflection_ratio: s.lateral_deflection_ratio,
        inclination_cos: s.inclination_cos,
        effective_radius: f.state.effective_radius,
        smoothed_soil_drag: s.smoothed_soil_drag,
        smoothed_drag_weight: s.smoothed_drag_weight,
        smoothed_furrow_ratio: s.smoothed_furrow_ratio,
    }
}

/// Stores the soil's drag into the frame (the normal force gains the drag).
pub fn apply_soil(f: &mut Frame, out: &SoilDrag) {
    let s = &mut f.scratch;
    s.soil_flow_opposed = out.flow_opposed;
    s.soil_flow_aligned = out.flow_aligned;
    s.longitudinal_soil_force = out.longitudinal_force;
    s.normal_force = out.normal_force;
    s.soil_drag = out.drag;
    s.lateral_soil_flow = out.lateral_flow;
    s.lateral_soil_force = out.lateral_force;
    s.smoothed_soil_drag = out.smoothed_soil_drag;
    s.smoothed_drag_weight = out.smoothed_drag_weight;
    s.smoothed_furrow_ratio = out.smoothed_furrow_ratio;
}

/// The rolling resistance torque inputs from the frame.
pub fn rolling_input(f: &Frame) -> RollingInput {
    let s = &f.scratch;
    RollingInput {
        patch_length: s.patch_length,
        rolling_resistance: s.rolling_resistance,
        normal_force: s.normal_force,
        sliding_weight: s.sliding_weight,
        spin: s.spin,
    }
}

/// The hold's inputs from the frame; `dt` is the time step.
pub fn hold_input(f: &Frame, dt: f32) -> HoldInput {
    let (s, st) = (&f.scratch, &f.state);
    HoldInput {
        sticking_weight: s.sticking_weight,
        active: st.active,
        speed: s.speed,
        normal_force: s.normal_force,
        kinetic_friction: s.kinetic_friction,
        lateral: s.contact_frame.lateral,
        rolling: s.contact_frame.rolling,
        normal: s.contact_frame.normal,
        direction: st.hold_direction,
        dt,
    }
}

/// The entry `i` of a table, or `Outside` when `i` is outside its window (the index is a state word, `u32`).
pub fn table_at(t: &Table, i: u32) -> Result<f32, Outside> {
    t.at(i as i32).ok_or(Outside { index: i as i32, len: t.count() })
}

/// The heat step's inputs from the frame: the forces and the time step are the step's arguments. The sector's
/// surface temperature and wear are read at the sector, the weights and entries at the segments the frame's searches
/// hold.
pub fn heat_input(f: &Frame, lateral_force: f32, longitudinal_force: f32, dt: f32) -> Result<HeatInput, Outside> {
    let (s, st, d) = (&f.scratch, &f.state, &f.setup);
    let idx = s.sector as usize;
    let surface_temperature =
        *st.surface_temperature.get(idx).ok_or(Outside { index: idx as i32, len: st.surface_temperature.len() })?;
    let pressure = &st.contact_pressure_segment;
    let temperature = &st.surface_temperature_segment;
    Ok(HeatInput {
        lateral_force,
        longitudinal_force,
        dt,
        water_factor: s.water_factor,
        lateral_velocity: s.contact_velocity[0],
        long_slip_velocity: s.long_slip_velocity,
        spin: s.spin,
        rolling_torque: s.rolling_torque,
        contact_pressure: s.contact_pressure,
        speed: s.speed,
        slip_phase: s.slip_phase,
        tread_contact_area: s.tread_contact_area,
        surface_temperature,
        tread_temperature: st.tread_temperature,
        carcass_temperature: st.carcass_temperature,
        wear_weight: temperature.frac,
        pressure_weight: pressure.frac,
        conductance_a: table_at(&d.contact_conductance, pressure.lo)?,
        conductance_b: table_at(&d.contact_conductance, pressure.hi)?,
        wear_rate_a: table_at(&d.rubber_wear_by_temperature, temperature.lo)?,
        wear_rate_b: table_at(&d.rubber_wear_by_temperature, temperature.hi)?,
        tread_conductivity: d.tread_conductivity,
        capacity: [
            HeatCapacity { mass: d.surface_capacity[0], specific_heat: d.surface_capacity[1] },
            HeatCapacity { mass: d.tread_capacity[0], specific_heat: d.tread_capacity[1] },
            HeatCapacity { mass: d.carcass_capacity[0], specific_heat: d.carcass_capacity[1] },
        ],
        wear: s.wear[idx],
    })
}

/// Stores the heat step's results into the frame (the sector's wear and surface temperature, the tread and carcass
/// temperatures).
pub fn apply_heat(f: &mut Frame, out: &HeatOutput) {
    let idx = f.scratch.sector as usize;
    let s = &mut f.scratch;
    s.wet_term = out.wet_term;
    s.frictional_power = out.frictional_power;
    s.wet_damping = out.wet_damping;
    s.surface_heat_loss = out.surface_heat_loss;
    s.abrasion_rate = out.abrasion_rate;
    s.wear[idx] = out.wear;
    let st = &mut f.state;
    st.surface_temperature[idx] = out.surface_temperature;
    st.tread_temperature = out.tread_temperature;
    st.carcass_temperature = out.carcass_temperature;
}

/// The patch forces' inputs from the frame. (The original reads the record at `this+0x20`, so its words are scratch
/// words.)
pub fn patch_input(f: &Frame) -> Result<PatchInput, Outside> {
    let (s, st, d) = (&f.scratch, &f.state, &f.setup);
    let blend = |base: &Table| -> Result<BlendPair, Outside> {
        Ok(BlendPair { at_a: table_at(base, st.load_segment.lo)?, at_b: table_at(base, st.load_segment.hi)? })
    };
    Ok(PatchInput {
        speed: s.speed,
        lateral_velocity: s.contact_velocity[0],
        longitudinal_velocity: s.contact_velocity[1],
        spin: s.spin,
        effective_radius: st.effective_radius,
        static_friction: s.static_friction,
        kinetic_friction: s.kinetic_friction,
        reference_friction: d.reference_friction,
        normal_force: s.normal_force,
        cornering_stiffness: s.cornering_stiffness,
        traction_stiffness: s.traction_stiffness,
        inclination_sin: s.inclination_sin,
        patch_length: s.patch_length,
        sink_depth: s.indentation.sink_depth,
        weight: st.load_segment.frac,
        tables: [
            blend(&d.cornering_friction_by_load)?,
            blend(&d.traction_friction_by_load)?,
            blend(&d.cornering_peak_slip_by_load)?,
            blend(&d.traction_peak_slip_by_load)?,
        ],
    })
}

/// The contact solve's inputs from the frame: the indentation, the forces and the engaged flags, with `coefficients`.
pub fn contact_input(f: &Frame, coefficients: Coefficients) -> Contact {
    let (s, st) = (&f.scratch, &f.state);
    Contact {
        soil_thickness: s.soil_thickness,
        indentation: s.indentation,
        normal_force: s.normal_force,
        carcass_force: s.carcass_force,
        tread_force: s.tread_force,
        coefficients,
        tread_engaged: st.tread_engaged,
        carcass_engaged: st.carcass_engaged,
    }
}

/// Stores a solved contact back into the frame: the indentation, the forces and the engaged flags.
pub fn apply_contact(f: &mut Frame, c: &Contact) {
    let s = &mut f.scratch;
    s.indentation = c.indentation;
    s.normal_force = c.normal_force;
    s.carcass_force = c.carcass_force;
    s.tread_force = c.tread_force;
    f.state.tread_engaged = c.tread_engaged;
    f.state.carcass_engaged = c.carcass_engaged;
}
