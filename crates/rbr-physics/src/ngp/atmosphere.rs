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

pub use rbr_formats::ngp::AtmosphereConstants;

/// The air at `height` (m), at f32 with the barometric power in double.
pub fn atmosphere(height: f32, c: &AtmosphereConstants) -> Atmosphere {
    let temperature = c.ground_temperature - height * c.temperature_lapse;
    let base = 1.0 - height * c.pressure_lapse / c.reference_temperature;
    let pressure = (f64::from(base).powf(c.barometric_exponent) as f32) * c.ground_pressure;
    Atmosphere {
        temperature,
        pressure,
        density: pressure / (temperature * c.air_gas_constant),
        inv_pressure: 1.0 / pressure,
        pressure_115: pressure * c.pressure_factor_115,
        pressure_0987: pressure * c.pressure_factor_0987,
    }
}
