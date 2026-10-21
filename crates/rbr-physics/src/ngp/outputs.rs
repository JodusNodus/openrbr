//! NGP's `CScratch::ComputeOutputs` (`0x5320c0`), which NGP calls through its wrapper `0x1005b4a0` (in
//! `CScratch::StepInner`'s call at `0x535484`). NGP repoints the function's scale of the lateral and longitudinal end slips (`0x72b2a0`, 3)
//! to its own 1 (`1014fbc4`, `tyre::friction_limits`); the wrapper then scales the lateral end slip by 4.25, capped at 1,
//! and sets the longitudinal end slip to 1.

use crate::tyre::friction_limits::{friction_limits as friction_limits_102, FrictionLimits, LimitsInput};

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
pub fn friction_limits(x: &LimitsInput) -> FrictionLimits {
    let out = friction_limits_102(x);
    FrictionLimits { lateral_end_slip: capped(out.lateral_end_slip * SCALE), longitudinal_end_slip: 1.0, ..out }
}
