//! NGP's atmosphere (`0x1004b5a0`, `notes/ngp.md` "Atmosphere"): temperature, pressure and density at the car's
//! height, updated every engine step. The engine and heat models read them through NGP's `.data`.

/// The words `0x1004b5a0` stores, in its order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    /// Kelvin (`10171adc`).
    pub temperature: f32,
    /// Pascal (`10171ad8`).
    pub pressure: f32,
    /// kg/m^3 (`10171ae0`).
    pub density: f32,
    /// 1 / pressure (`101767c8`).
    pub inv_pressure: f32,
    /// 1.15 pressure (`101767cc`).
    pub pressure_115: f32,
    /// 0.9871668 pressure (`101767d0`).
    pub pressure_0987: f32,
}

const T0: f32 = f32::from_bits(0x4392_9333); // 10148ac8, 293.15
const LAPSE_T: f32 = f32::from_bits(0x3BD4_FDF3); // 10148a48, 0.0065 (one bit below LAPSE_P)
const LAPSE_P: f32 = f32::from_bits(0x3BD4_FDF4); // 10148a4c, 0.0065
const T0_P: f32 = f32::from_bits(0x4390_1333); // 10148ac4, 288.15
const EXPONENT: f64 = f64::from_bits(0x4015_051E_C000_0000); // 10148aa8, 5.255 as an f32 widened
const P0: f32 = f32::from_bits(0x47C5_DA00); // 10148ad0, 101300
const R_AIR: f32 = f32::from_bits(0x438F_8000); // 10148ac0, 287
const K_115: f32 = f32::from_bits(0x3F93_3333); // 10148a74, 1.15
const K_0987: f32 = f32::from_bits(0x3F7C_B6F7); // 10148a68, 0.9871668

/// `0x1004b5a0` for the car height `h` (car `+0x148`).
pub fn atmosphere(h: f32) -> Atmosphere {
    let temperature = T0 - h * LAPSE_T;
    // 0x1004b650: the barometric formula, its power in double (the CRT's pow)
    let base = 1.0 - h * LAPSE_P / T0_P;
    let pressure = (f64::from(base).powf(EXPONENT) as f32) * P0;
    Atmosphere {
        temperature,
        pressure,
        density: pressure / (temperature * R_AIR),
        inv_pressure: 1.0 / pressure,
        pressure_115: pressure * K_115,
        pressure_0987: pressure * K_0987,
    }
}
