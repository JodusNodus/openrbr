//! NGP's `CScratch::ComputeOutputs` (`0x5320c0`), which NGP calls through its wrapper `0x1005b4a0` (in
//! `CScratch::StepInner`'s call at `0x535484`). NGP repoints the function's scale of outputs 14 and 16 (`0x72b2a0`, 3)
//! to its own 1 (`1014fbc4`, `tyre::outputs` under `Ngp778`); the wrapper then scales output 14 by 4.25, capped at 1,
//! and sets output 16 to 1.

use crate::tyre::outputs::{compute_outputs as compute_outputs_102, ComputeInput, ComputeOutputs};
use crate::Variant;

const SCALE: f32 = f32::from_bits(0x4088_0000); // 10148ef8, 4.25

/// `x` capped at 1 (`comiss` then `jbe`, and the wrapper's `minss` with 1 first: a NaN stays).
fn capped(x: f32) -> f32 {
    if x > 1.0 {
        1.0
    } else {
        x
    }
}

/// `0x1005b4a0`.
pub fn compute_outputs(x: &ComputeInput) -> ComputeOutputs {
    let out = compute_outputs_102(x, Variant::Ngp778);
    ComputeOutputs { out14: capped(out.out14 * SCALE), out16: 1.0, ..out }
}
