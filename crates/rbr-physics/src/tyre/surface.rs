//! The tyre type's friction on the surface classes, by wetness and tread wear.

use crate::math::abs_asm;

/// What the surface friction reads, for two surface classes `first` and `second`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceInput {
    /// The tyre type's dry base friction (`SFric`), wet slope (`WetSlope`) and wear factor (`WearFtr`) of each class.
    pub base_first: f32,
    pub base_second: f32,
    pub wet_slope_first: f32,
    pub wet_slope_second: f32,
    pub wear_factor_first: f32,
    pub wear_factor_second: f32,
    /// The tyre type's design wetness (`WetnessDesign`) and the wetness of each class.
    pub design_wetness: f32,
    pub wetness_first: f32,
    pub wetness_second: f32,
    /// The wear of the tread sector.
    pub sector_wear: f32,
}

/// The friction of the two classes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceFriction {
    pub first: f32,
    pub second: f32,
}

/// The friction of each class: its base friction plus the wet slope times the distance of its wetness from the
/// design wetness, scaled by its wear factor, which the sector's wear moves from 1 to the class's factor.
pub fn surface_friction(b: &SurfaceInput) -> SurfaceFriction {
    let a = abs_asm(b.design_wetness - b.wetness_first);
    let c = abs_asm(b.design_wetness - b.wetness_second);
    let first = b.wet_slope_first * a + b.base_first;
    let second = b.wet_slope_second * c + b.base_second;
    SurfaceFriction {
        first: ((b.wear_factor_first - 1.0) * b.sector_wear + 1.0) * first,
        second: ((b.wear_factor_second - 1.0) * b.sector_wear + 1.0) * second,
    }
}
