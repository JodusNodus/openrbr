//! The atmosphere's immutable coefficients in NGP 7.4.778's read-only data.

use crate::mem::Mem;
use crate::ngp::atmosphere::AtmosphereConstants;

/// Read from a live snapshot or the boot's DLL mapping at `base`.
pub fn constants(mem: &Mem, base: u32) -> AtmosphereConstants {
    let at = |address: u32| base + (address - 0x1000_0000);
    let f = |address| mem.f32(at(address));
    AtmosphereConstants {
        ground_temperature: f(0x1014_8ac8),
        temperature_lapse: f(0x1014_8a48),
        pressure_lapse: f(0x1014_8a4c),
        reference_temperature: f(0x1014_8ac4),
        barometric_exponent: f64::from_bits(
            u64::from(mem.u32(at(0x1014_8aa8))) | (u64::from(mem.u32(at(0x1014_8aac))) << 32),
        ),
        ground_pressure: f(0x1014_8ad0),
        air_gas_constant: f(0x1014_8ac0),
        pressure_factor_115: f(0x1014_8a74),
        pressure_factor_0987: f(0x1014_8a68),
    }
}
