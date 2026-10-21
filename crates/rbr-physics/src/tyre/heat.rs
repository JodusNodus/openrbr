//! The tyre's heat step: the frictional power becomes heat in the tread's surface layer, the tread under it and the
//! carcass; the surface loses heat to the road and the air; and the rubber wears.
//!
//! The branches and the rounding follow the original. The NaN-sensitive comparisons are written as the `jbe` they
//! compile to, and each x87 chain rounds to 24 bits (`round24`). The SSE part is f32. The original stores the surface
//! share minus the passed-down share as an f32 before the x87 expression that uses it.

use crate::fpu::round_x87;
use crate::ngp::heat::{heat_flow_to_air, TyreThermal};
use crate::spring_damper::{accumulate_a, AccumulationFactors};

/// The temperature above which the tread is hot, K (NGP's 40 °C; 1.02's is 60 °C).
const HOT_ABOVE: f32 = 40.0;

/// What the heat step reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeatInput {
    /// The lateral and the longitudinal force, N, and the time step, s.
    pub lateral_force: f32,
    pub longitudinal_force: f32,
    pub dt: f32,
    /// The surface's water factor; the step uses 3 times it.
    pub water_factor: f32,
    /// The lateral velocity and the longitudinal slip velocity of the contact point, m/s: each times its force is a
    /// frictional power.
    pub lateral_velocity: f32,
    pub long_slip_velocity: f32,
    /// The wheel's spin, rad/s, and the rolling resistance torque, N·m: their product is the third power.
    pub spin: f32,
    pub rolling_torque: f32,
    /// The contact pressure, Pa, scales the wear.
    pub contact_pressure: f32,
    /// The slip speed, m/s.
    pub speed: f32,
    /// The slip phase: 1 less than it, clamped to [0, 1], is how far the contact slides beyond the peak.
    pub slip_phase: f32,
    /// The area of the tread in contact, m².
    pub tread_contact_area: f32,
    /// The surface temperature of the sector, the temperature of the tread under it, and the carcass temperature, K.
    pub surface_temperature: f32,
    pub tread_temperature: f32,
    pub carcass_temperature: f32,
    /// The blend weights between the two entries of the wear search (at the surface temperature) and of the contact
    /// pressure search.
    pub wear_weight: f32,
    pub pressure_weight: f32,
    /// The contact conductance factors at the two contact pressure entries.
    pub conductance_a: f32,
    pub conductance_b: f32,
    /// The rubber wear rates at the two temperature entries.
    pub wear_rate_a: f32,
    pub wear_rate_b: f32,
    /// The tread's thermal conductivity.
    pub tread_conductivity: f32,
    /// The heat capacities (mass and specific heat) of the surface layer, the tread under it and the carcass.
    pub capacity: [AccumulationFactors; 3],
    /// The wear of the sector, 0 to 1.
    pub wear: f32,
}

/// What the heat step writes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeatOutput {
    /// `2 + 3 · water_factor`.
    pub wet_term: f32,
    /// The frictional power, W: `rolling · 0.5 + longitudinal + lateral`.
    pub frictional_power: f32,
    /// `1 / (wet_term / tread_conductivity + 1)`.
    pub wet_damping: f32,
    /// The heat the surface loses to the road and the air, W (the flow computed over the time step, divided by it).
    pub surface_heat_loss: f32,
    /// The abrasion rate, or 0 when it is below 0.05 or the squared lateral and longitudinal slip velocity is at most
    /// 0.4.
    pub abrasion_rate: f32,
    /// The temperatures after the step.
    pub surface_temperature: f32,
    pub tread_temperature: f32,
    pub carcass_temperature: f32,
    /// The sector's wear after the step, capped at 1; unchanged when the squared slip velocity is at most 0.4.
    pub wear: f32,
}

/// The factor of the surface's heat input: `(hot · -0.6 + 1) · wet_damping`. The original multiplies by the f32
/// constant `-0.60000002f` after promoting it to x87 precision.
fn surface_heat_factor(hot: f32, wet_damping: f32) -> f64 {
    let round = round_x87;
    round(round(round(hot as f64 * f64::from(-0.6_f32)) + 1.0) * wet_damping as f64)
}

/// The heat step; `ngp` is NGP's state. Its temperature threshold is 40 (1.02: 60), its heat flow to the air is NGP's
/// (against the air temperature), and its x87 chains run at 53 bits.
pub fn heat_step(x: HeatInput, ngp: &TyreThermal) -> HeatOutput {
    let round24 = round_x87;
    let abs = |v: f32| if v > 0.0 { v } else { 0.0 - v };
    let wet_boost = x.water_factor * 3.0;
    let wet_term = wet_boost + 2.0;
    let lateral_power = abs(x.lateral_velocity * x.lateral_force);
    let longitudinal_power = abs(x.longitudinal_force * x.long_slip_velocity);
    let rolling_power = abs(x.rolling_torque * x.spin);
    let frictional_power = ((rolling_power * 0.5) + longitudinal_power) + lateral_power;
    // The heat of the step splits: 70 % goes near the surface, of which 10 % passes down; the rest goes deeper.
    let heat = frictional_power * x.dt;
    let near_surface = heat * 0.7;
    let deeper = heat - near_surface;
    let passed_down = near_surface * 0.1;
    let surface_kept = near_surface - passed_down;
    let returned = passed_down * 0.2;
    let carcass_input = deeper * 0.4;
    let tread_input = (passed_down - returned) + deeper * 0.6;
    // How hot the tread is: the mean of the surface and the tread temperatures above 40 °C over 80 K, clamped to
    // [0, 1] (NaN passes through, as in the original).
    let hot = ((((x.tread_temperature + x.surface_temperature) * 0.5) - 273.15) - HOT_ABOVE) * 0.0125;
    let hot = if 0.0 > hot {
        0.0
    } else if hot > 1.0 {
        1.0
    } else {
        hot
    };
    let wet_damping = 1.0 / (wet_term / x.tread_conductivity + 1.0);

    // The heat transfer coefficient and the surface's flow to the air (x87 chains at 24 bits).
    let conductance = round24(
        round24(round24(1.0 - x.pressure_weight as f64) * x.conductance_a as f64)
            + round24(x.conductance_b as f64 * x.pressure_weight as f64),
    );
    let hot_factor = round24(round24(hot as f64 * 4500.0) + 1500.0);
    let coefficient = round24(round24(conductance * hot_factor) * round24(wet_boost as f64 + 1.0));
    let loss = f64::from(heat_flow_to_air(
        x.surface_temperature,
        303.0,
        coefficient as f32,
        x.tread_contact_area,
        ngp.air_temperature,
    ));
    let loss_energy = round24(loss * x.dt as f64);
    let surface_heat_loss = round24(loss_energy / x.dt as f64) as f32;

    // The surface's input: ((hot · -0.6 + 1) · wet_damping · surface_kept + returned) - the loss, as f32.
    let heat_factor = surface_heat_factor(hot, wet_damping);
    let surface_input =
        round24(round24(round24(heat_factor * surface_kept as f64) + returned as f64) - loss_energy) as f32;

    let mut surface_temperature = x.surface_temperature;
    accumulate_a(x.capacity[0], surface_input, &mut surface_temperature);
    let mut tread_temperature = x.tread_temperature;
    accumulate_a(x.capacity[1], tread_input, &mut tread_temperature);
    let mut carcass_temperature = x.carcass_temperature;
    accumulate_a(x.capacity[2], carcass_input, &mut carcass_temperature);

    let mut out = HeatOutput {
        wet_term,
        frictional_power,
        wet_damping,
        surface_heat_loss,
        abrasion_rate: 0.0,
        surface_temperature,
        tread_temperature,
        carcass_temperature,
        wear: x.wear,
    };
    let slip_sq = (x.lateral_velocity * x.lateral_velocity) + (x.long_slip_velocity * x.long_slip_velocity);
    if !(slip_sq > 0.4) {
        return out;
    }
    // The wear needs how far the contact slides beyond the peak (clamped to [0, 1]), the rubber's wear rate at the
    // surface temperature, and the slip length (x87 at 24 bits).
    let slip_length = round24((slip_sq as f64).sqrt()) as f32;
    let beyond_peak = x.slip_phase - 1.0;
    let beyond_peak = if 0.0 > beyond_peak {
        0.0
    } else if beyond_peak > 1.0 {
        1.0
    } else {
        beyond_peak
    };
    let wear_rate = (1.0 - x.wear_weight) * x.wear_rate_a + x.wear_rate_b * x.wear_weight;
    let abrasion = (x.contact_pressure * beyond_peak) * wear_rate;
    let worn = (abrasion * slip_length) * x.dt;
    let slip_share = if x.speed > slip_length { slip_length / x.speed } else { 1.0 };
    let abrasion_rate = (abrasion * slip_share) * 7.0e-6;
    out.abrasion_rate = if 0.05 > abrasion_rate { 0.0 } else { abrasion_rate };
    let wear = worn * 1.8e-8 + x.wear;
    out.wear = if wear > 1.0 { 1.0 } else { wear };
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpu::Precision;

    #[test]
    fn surface_heat_factor_promotes_the_original_f32_constant() {
        let _pc24 = Precision::Pc24.scope();
        assert_eq!((surface_heat_factor(0.75, 1.0) as f32).to_bits(), 0x3f0c_cccc);
    }
}
