//! The damage-level fractions: how much of a wheel, body and scale each damage level (0..=3) stands for, and the clamp
//! that keeps a damage fraction in 0..=1. The tables the car's construction fills are in `layout`.

pub use super::layout::damage_tables::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelFractions {
    pub wheel: f32,
    pub body: f32,
    pub scale: f32,
    pub level_three: u32,
}

/// The damage-level fractions: the wheel and body shares, the scale, and whether the level is the highest. Unknown levels
/// give zero.
pub fn level_fractions(level: u32) -> LevelFractions {
    match level {
        1 => LevelFractions { wheel: 0.5, body: 0.5, scale: 0.2, level_three: 0 },
        2 => LevelFractions { wheel: 0.8, body: 0.95, scale: 0.6, level_three: 0 },
        3 => LevelFractions { wheel: 1.0, body: 1.0, scale: 1.0, level_three: 1 },
        _ => LevelFractions { wheel: 0.0, body: 0.0, scale: 0.0, level_three: 0 },
    }
}

/// Clamps a damage fraction to 0..=1 with ordered compares: a NaN and a signed zero pass through.
pub fn clamp_damage(value: f32) -> f32 {
    if value < 0.0 {
        0.0
    } else if value > 1.0 {
        1.0
    } else {
        value
    }
}
