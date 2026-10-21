//! NGP's blend of the tyre block's two material tags, in SSE. Unlike 1.02's (`wheel::layout::tag_blend`), the sine
//! terms take the same weights as the lerps (1.02 weighs the first material's sines by `w` and the second's by
//! `1 - w`, and its lerps the other way), and the sines are the C runtime's `sin` in f64 rounded to f32
//! ([`layout::tag_blend`](super::layout::tag_blend) also remaps a second tag of 254 to the first and skips the blend
//! without the physics definition).

use std::f32::consts::TAU;

/// What the blend reads of a material: the period and gain of its sine term, and the two values it lerps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    pub period: f32,
    pub gain: f32,
    /// The value the blend takes, and the value the total takes beyond the blend (named by use).
    pub blend_value: f32,
    pub total_value: f32,
}

/// `sin(phase_b * r) + sin(phase_a * r)` for the material's ratio `r = 2 pi / period`, each sine rounded to f32.
fn sines(period: f32, phase_a: f32, phase_b: f32) -> f32 {
    let r = TAU / period;
    let s1 = f64::from(r * phase_b).sin() as f32;
    let s2 = f64::from(r * phase_a).sin() as f32;
    s1 + s2
}

/// The blend and the total for the two materials and the weight `w` (the second material's share) at the block's two
/// phase inputs.
pub fn blend([m1, m2]: [Material; 2], w: f32, phase_a: f32, phase_b: f32) -> (f32, f32) {
    let inv = 1.0 - w;
    let mut sine_term = 0.0;
    // `!= 0.0` takes a NaN period too, as the original's unordered compare does
    if m1.period != 0.0 {
        sine_term = sines(m1.period, phase_a, phase_b) * (inv * m1.gain) + 0.0;
    }
    if m2.period != 0.0 {
        sine_term += sines(m2.period, phase_a, phase_b) * (w * m2.gain);
    }
    let blend = (inv * m1.blend_value + w * m2.blend_value) + sine_term;
    let total = (inv * m1.total_value + w * m2.total_value) + blend;
    (blend, total)
}
