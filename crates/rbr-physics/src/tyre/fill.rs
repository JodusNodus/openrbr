//! 0x532f10 `CScratch::FillOutputs`: copies the contact vectors (`+0x160` and the `src` vector) and the `pair` dwords
//! into the signal block (`+0x420..+0x47c`), and derives the signal scalars from the state, the definition and the
//! divisor. The copies move raw dwords, and the flag at `+0x47c` is set to 1. The compares follow the asm (`x > y`, so
//! a NaN takes the other branch).

use super::layout::{sig, Frame};
use crate::math::abs_asm;
use crate::math::vec::Vec4;

/// The asm's constants: 0.75 (0x72f448), 0.5 (0x795e18), 320 (0x72bf14) and 1000 (0x737b70).
const K_SCALE: f32 = 0.75;
const K_OFFSET: f32 = 0.5;
const K_CONST: f32 = 320.0;
const K_KILO: f32 = 1000.0;

/// Stores the four lanes of `v` as raw dwords from word `at` of the signal block.
fn put_raw_vec(signal: &mut [u32; 24], at: usize, v: Vec4) {
    for (k, lane) in [v.x, v.y, v.z, v.w].into_iter().enumerate() {
        signal[at + k] = lane.to_bits();
    }
}

/// `FillOutputs` on the frame. `src` is the vector the caller passes, `pair` the three dwords, and `divisor` the float.
pub fn fill_outputs(f: &mut Frame, src: Vec4, pair: [u32; 3], divisor: f32) {
    let (s, st, setup) = (&f.scratch, &f.state, &f.setup);
    let max_e = if s.f3e0 > s.f3e4 { s.f3e0 } else { s.f3e4 };
    let sum = (s.force_a + s.force_b) + s.force_t1;
    let v = (setup.f8c + setup.f7c - s.arg_c - st.tread_48) / divisor;
    let slip_diff = abs_asm(s.slip_v[1] - s.long_contact_vel);
    let mut signal = s.signal;
    let signal_max = f32::from_bits(signal[sig::MAX]);

    put_raw_vec(&mut signal, sig::K_VEC, s.k_vec);
    put_raw_vec(&mut signal, sig::SRC_VEC, src);
    signal[sig::SPIN] = s.spin.to_bits();
    signal[sig::SLIP_RATIO] = abs_asm(s.slip_ratio_term).to_bits();
    signal[sig::SPEED] = s.speed.to_bits();
    signal[sig::DEBUG_ID] = setup.debug_id;
    signal[sig::SCALED_MAX] = (max_e * K_SCALE + K_OFFSET).to_bits();
    signal[sig::FACTOR] = s.tyre_factor_2.to_bits();
    for (k, word) in pair.into_iter().enumerate() {
        signal[sig::PAIR + k] = word;
    }
    signal[sig::CONST_320] = K_CONST.to_bits();
    signal[sig::FORCE_SUM] = (sum * K_KILO).to_bits();
    if v > signal_max {
        signal[sig::MAX] = v.to_bits();
    }
    signal[sig::SLIP_DIFF] = slip_diff.to_bits();
    signal[sig::FLAG] = 1;
    f.scratch.signal = signal;
}
