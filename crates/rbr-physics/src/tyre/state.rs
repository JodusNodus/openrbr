//! Setters of the tyre state that the wheel calls.

use super::layout::{sig, Frame};

/// Sets the state's active flag, and the flag word of the noise signal block.
pub fn set_active(f: &mut Frame, value: u32) {
    f.state.active = value;
    f.scratch.signal[sig::FLAG] = value;
}

/// Sets the penetration depth to `outer_radius - height` when `0 < height < outer_radius`, else 0, where the outer
/// radius is the setup's tread depth plus carcass radius. The compares are `0 < v`, then `outer > v`, so a NaN height
/// gives 0.
pub fn set_penetration_depth(f: &mut Frame, height: f32) {
    let outer_radius = f.setup.tread_depth + f.setup.carcass_radius;
    f.state.penetration_depth = if 0.0 < height && outer_radius > height { outer_radius - height } else { 0.0 };
}

/// Clears the bias: no effect (the original's only work is calls whose result it discards), so nothing is written.
pub fn clear_bias() {}
