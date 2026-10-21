//! NGP's atmosphere: temperature, pressure and density at the car's height, recomputed every engine step
//! (`notes/ngp.md`, "Atmosphere"). The engine and heat models read them.

/// The air at one height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    /// K.
    pub temperature: f32,
    /// Pa.
    pub pressure: f32,
    /// kg/m^3.
    pub density: f32,
    /// 1 / pressure.
    pub inv_pressure: f32,
    /// 1.15 pressure (named by the factor).
    pub pressure_115: f32,
    /// 0.9871668 pressure (named by the factor).
    pub pressure_0987: f32,
}

/// The temperature at height 0, K (20 °C).
const GROUND_TEMPERATURE: f32 = 293.15;
/// The temperature's lapse rate, K/m. NGP's temperature copy of 0.0065 is one ulp below its pressure copy.
const TEMPERATURE_LAPSE: f32 = 0.0064999997;
/// The pressure formula's lapse rate, K/m.
const PRESSURE_LAPSE: f32 = 0.0065;
/// The pressure formula's reference temperature, K (the standard atmosphere's 15 °C).
const REFERENCE_TEMPERATURE: f32 = 288.15;
/// The barometric exponent 5.255, as the f32 widened to double.
const BAROMETRIC_EXPONENT: f64 = 5.255_f32 as f64;
/// The pressure at height 0, Pa.
const GROUND_PRESSURE: f32 = 101_300.0;
/// The specific gas constant of air, J/(kg K).
const AIR_GAS_CONSTANT: f32 = 287.0;
const PRESSURE_FACTOR_115: f32 = 1.15;
const PRESSURE_FACTOR_0987: f32 = 0.9871668;

/// The air at `height` (m), at f32 with the barometric power in double.
pub fn atmosphere(height: f32) -> Atmosphere {
    let temperature = GROUND_TEMPERATURE - height * TEMPERATURE_LAPSE;
    let base = 1.0 - height * PRESSURE_LAPSE / REFERENCE_TEMPERATURE;
    let pressure = (f64::from(base).powf(BAROMETRIC_EXPONENT) as f32) * GROUND_PRESSURE;
    Atmosphere {
        temperature,
        pressure,
        density: pressure / (temperature * AIR_GAS_CONSTANT),
        inv_pressure: 1.0 / pressure,
        pressure_115: pressure * PRESSURE_FACTOR_115,
        pressure_0987: pressure * PRESSURE_FACTOR_0987,
    }
}
