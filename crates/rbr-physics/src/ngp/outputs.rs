//! NGP's changes to the tyre's outputs: 1.02's computation with the cap scale of outputs 14 and 16 repointed to 1
//! (`tyre::outputs`); NGP then scales output 14 by 4.25, capped at 1, and sets output 16 to 1.

use crate::tyre::outputs::{compute_outputs as compute_outputs_102, ComputeInput, ComputeOutputs};

const OUTPUT_14_SCALE: f32 = 4.25;

/// `x` capped at 1; a NaN stays.
fn capped(x: f32) -> f32 {
    if x > 1.0 {
        1.0
    } else {
        x
    }
}

pub fn compute_outputs(x: &ComputeInput) -> ComputeOutputs {
    let out = compute_outputs_102(x);
    ComputeOutputs { out14: capped(out.out14 * OUTPUT_14_SCALE), out16: 1.0, ..out }
}
