//! The relaxation of the tyre's temperatures: the carcass, the tread under the surface, and the surface of one tread
//! sector at a time. Each step relaxes the carcass and the tread towards each other and loses heat to the inner air
//! (a gas state) and to the surroundings, then relaxes one sector's surface with the tread under it.
//!
//! The order of each operation follows the original. The x87 chains round at 24 bits (`round24`); under NGP at 53.
//! A value the original stores to memory as f32 and reloads is rounded to f32 here, too. The heat flows are NGP's
//! (`ngp::heat`).

use crate::fpu::round_x87;
use crate::ngp::heat::{carcass_to_inner_air, conduction, heat_flow_to_air, sidewall_convection, TyreThermal};
use crate::spring_damper::{accumulate_a, force as gas_force, AccumulationFactors, ForceTerms};

/// The distance the surface's temperature gradient to the tread under it spans: 1 mm (1000 per metre).
const SURFACE_GRADIENT: f32 = 999.99994;

/// What the sector step reads, for one tread sector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectorInput {
    /// The sector's surface temperature, K.
    pub surface_temperature: f32,
    /// The slip speed, m/s.
    pub speed: f32,
    /// The temperature of the tread under the surface, K.
    pub tread_temperature: f32,
    /// The tread's thermal conductivity.
    pub tread_conductivity: f32,
    /// The heat capacities of the surface layer and of the tread under it.
    pub surface_capacity: AccumulationFactors,
    pub tread_capacity: AccumulationFactors,
}

/// What the sector step writes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectorOutput {
    /// The heat the whole tread loses to the air, W: the sector's flow times 8 (a sector is stepped once in eight).
    pub tread_heat_loss: f32,
    /// The sector's surface temperature after the step.
    pub surface_temperature: f32,
    /// The temperature of the tread under the surface after the step.
    pub tread_temperature: f32,
}

/// Relaxes one sector for the time `elapsed` since it was last stepped. The sector loses heat to the air at the
/// speed's convection (`(min(speed · 0.1, 1) · 0.5 + 1) · 30` over a tread segment's area, against 25 °C) and
/// exchanges heat with the tread under it through 1 mm of rubber. NGP's heat flows use the scratch's tread segment area
/// and the air temperature.
pub fn relax_sector(t: &SectorInput, elapsed: f32, ngp: &TyreThermal) -> SectorOutput {
    let round24 = round_x87;
    let w = t.speed * 0.1;
    let w = if w > 1.0 { 1.0 } else { w };
    let coefficient = ((w * 0.5) + 1.0) * 30.0;
    let loss = f64::from(heat_flow_to_air(
        t.surface_temperature,
        298.14999,
        coefficient,
        ngp.shape.tread_segment_area(),
        ngp.air_temperature,
    ));
    // The loss is stored as f32 for the sum below, and the whole tread's loss is rounded from the x87 value.
    let tread_heat_loss = round24(loss * 8.0) as f32;
    let loss_f = loss as f32;
    let gradient = (t.tread_temperature - t.surface_temperature) * SURFACE_GRADIENT;
    let conducted = conduction(gradient, t.tread_conductivity, ngp.shape.tread_segment_area()) as f32;
    // The surface takes -((conducted + loss) · elapsed) and the tread takes conducted · elapsed, each rounded in the x87
    // chain, then stored as f32.
    let surface_input = (-round24(round24(conducted as f64 + loss_f as f64) * elapsed as f64)) as f32;
    let tread_input = round24(conducted as f64 * elapsed as f64) as f32;
    let mut surface_temperature = t.surface_temperature;
    accumulate_a(t.surface_capacity, surface_input, &mut surface_temperature);
    let mut tread_temperature = t.tread_temperature;
    accumulate_a(t.tread_capacity, tread_input, &mut tread_temperature);
    SectorOutput { tread_heat_loss, surface_temperature, tread_temperature }
}

/// What the relaxation reads. `sector` is the stepped sector's input; its tread temperature is the one the first
/// accumulator of the relaxation updates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RelaxInput {
    /// The time step, s.
    pub dt: f32,
    /// The carcass temperature, K, and the temperature of the air inside the tyre, K.
    pub carcass_temperature: f32,
    pub inner_air_temperature: f32,
    /// The carcass radius and the tread width, m, and the conductivity between the carcass and the tread.
    pub carcass_radius: f32,
    pub tread_width: f32,
    pub carcass_conductivity: f32,
    /// The heat capacities of the tread under the surface and of the carcass.
    pub tread_capacity: AccumulationFactors,
    pub carcass_capacity: AccumulationFactors,
    /// The air inside the tyre as a gas: `accumulated_force` is its temperature, K, and `response` its pressure, Pa.
    pub inner_air: ForceTerms,
    /// The stepped sector.
    pub sector: SectorInput,
}

/// What the relaxation writes. `sector.tread_temperature` is the temperature of the tread under the surface after
/// both steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RelaxOutput {
    /// The carcass temperature after the step.
    pub carcass_temperature: f32,
    /// The heat the carcass's sidewalls lose to the surroundings, W, stored as f32.
    pub sidewall_heat_loss: f32,
    pub inner_air: ForceTerms,
    pub sector: SectorOutput,
}

/// The relaxation of the temperatures. The temperature difference between the carcass and the tread spans 1 cm (NGP's
/// per-car relaxation rate, 100 per metre in 1.02); the heat flows are NGP's: the carcass to the inner air, and the
/// sidewalls to the surroundings with the speed's convection.
pub fn relax(r: &RelaxInput, ngp: &TyreThermal) -> RelaxOutput {
    let round24 = round_x87;
    let rate = f64::from(ngp.relax_rate);
    // The conduction through the tread band (width times the circumference) between the carcass and the tread under
    // the surface (negative while the carcass is the hotter).
    let mut tread_temperature = r.sector.tread_temperature;
    let gradient = round24(round24(r.carcass_temperature as f64 - tread_temperature as f64) * rate) as f32;
    let band_area = (r.tread_width * r.carcass_radius) * 6.2831855;
    let band_conduction = conduction(gradient, r.carcass_conductivity, band_area);
    // The carcass's flows to the inner air and to the surroundings.
    let (to_inner_air, to_ambient) = (
        f64::from(carcass_to_inner_air(r.carcass_temperature, r.inner_air_temperature, 30.0, &ngp.shape)),
        f64::from(sidewall_convection(
            r.carcass_temperature,
            298.14999,
            20.0,
            &ngp.shape,
            &ngp.airflow,
            ngp.air_temperature,
        )),
    );
    let sidewall_heat_loss = to_ambient as f32;
    // Each product with dt is an x87 value stored as f32. The band and inner air energies are reused as f32 below.
    let band_energy = round24(band_conduction * r.dt as f64) as f32;
    let inner_air_energy = round24(to_inner_air * r.dt as f64) as f32;
    // The tread takes minus the band's energy; the carcass takes the band's energy less what goes to the inner air and
    // the surroundings.
    let tread_input = 0.0 - band_energy;
    let carcass_input = round24(
        round24(band_energy as f64 - inner_air_energy as f64) - round24(sidewall_heat_loss as f64 * r.dt as f64),
    ) as f32;
    accumulate_a(r.tread_capacity, tread_input, &mut tread_temperature);
    let mut carcass_temperature = r.carcass_temperature;
    accumulate_a(r.carcass_capacity, carcass_input, &mut carcass_temperature);
    let mut inner_air = r.inner_air;
    gas_force(&mut inner_air, inner_air_energy, INNER_AIR_CAPACITY, GAS_CONSTANT);
    let sector = relax_sector(&SectorInput { tread_temperature, ..r.sector }, r.dt * 8.0, ngp);
    RelaxOutput { carcass_temperature, sidewall_heat_loss, inner_air, sector }
}

/// The inner air's heat capacity, J/K, and the gas constant of air, J/(kg·K).
const INNER_AIR_CAPACITY: f32 = 742.85718;
const GAS_CONSTANT: f32 = 287.0;
