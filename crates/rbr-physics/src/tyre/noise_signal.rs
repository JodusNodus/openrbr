//! The tyre-noise signal block: the contact vectors, spin and slip words the tyre leaves for the sound, and the
//! derived scalars.
//!
//! The copies move raw dwords. The comparisons are `x > y`, so a NaN takes the other branch.

use super::layout::{sig, Frame};
use crate::math::abs_asm;
use crate::math::vec::Vec4;

/// The scale and offset of the largest slip phase, the constant the block carries, and the kilo of the force sum.
const PHASE_SCALE: f32 = 0.75;
const PHASE_OFFSET: f32 = 0.5;
const BLOCK_CONSTANT: f32 = 320.0;
const KILO: f32 = 1000.0;

/// Stores the four lanes of `v` as raw dwords from word `at` of the signal block.
fn put_raw_vec(signal: &mut [u32; 24], at: usize, v: Vec4) {
    for (k, lane) in [v.x, v.y, v.z, v.w].into_iter().enumerate() {
        signal[at + k] = lane.to_bits();
    }
}

/// Fills the signal block from the frame: the contact point and `contact_velocity` (the vector the wheel passes),
/// the `debug_words`, the spin, the absolute spin slip, the slip speed, the setup's debug id, the normal force, the
/// largest slip phase scaled, the soil flow sum times 1000, the slip difference, and the running maximum of the
/// compression rate: how far the penetration changed from `penetration_depth` over `divisor` (the time step), m/s.
/// The flag word is set to 1.
pub fn fill_noise_signal(f: &mut Frame, contact_velocity: Vec4, debug_words: [u32; 3], divisor: f32) {
    let (s, st, setup) = (&f.scratch, &f.state, &f.setup);
    let max_phase = if s.lateral_phase > s.longitudinal_phase { s.lateral_phase } else { s.longitudinal_phase };
    let sum = (s.soil_flow_opposed + s.soil_flow_aligned) + s.lateral_soil_flow;
    let compression_rate = (setup.tread_depth + setup.carcass_radius - s.hub_height - st.penetration_depth) / divisor;
    let slip_diff = abs_asm(s.contact_velocity[1] - s.long_slip_velocity);
    let mut signal = s.signal;
    let signal_max = f32::from_bits(signal[sig::MAX]);

    put_raw_vec(&mut signal, sig::CONTACT_POINT, s.contact_point);
    put_raw_vec(&mut signal, sig::CONTACT_VELOCITY, contact_velocity);
    signal[sig::SPIN] = s.spin.to_bits();
    signal[sig::SPIN_SLIP] = abs_asm(s.spin_slip).to_bits();
    signal[sig::SPEED] = s.speed.to_bits();
    signal[sig::DEBUG_ID] = setup.debug_id;
    signal[sig::SCALED_MAX] = (max_phase * PHASE_SCALE + PHASE_OFFSET).to_bits();
    signal[sig::NORMAL_FORCE] = s.normal_force.to_bits();
    for (k, word) in debug_words.into_iter().enumerate() {
        signal[sig::DEBUG_WORDS + k] = word;
    }
    signal[sig::CONST_320] = BLOCK_CONSTANT.to_bits();
    signal[sig::FORCE_SUM] = (sum * KILO).to_bits();
    if compression_rate > signal_max {
        signal[sig::MAX] = compression_rate.to_bits();
    }
    signal[sig::SLIP_DIFF] = slip_diff.to_bits();
    signal[sig::FLAG] = 1;
    f.scratch.signal = signal;
}
