//! NGP's tyre friction limits use a cap scale of 1 for both end slips. The lateral end slip is then scaled by
//! 4.25 and capped at 1; the longitudinal end slip is set to 1.

use crate::tyre::friction_limits::{friction_limits as friction_limits_102, FrictionLimits, LimitsInput};

const LATERAL_END_SLIP_SCALE: f32 = 4.25;

/// `x` capped at 1; a NaN stays.
fn capped(x: f32) -> f32 {
    if x > 1.0 {
        1.0
    } else {
        x
    }
}

pub fn friction_limits(x: &LimitsInput) -> FrictionLimits {
    let out = friction_limits_102(x);
    FrictionLimits {
        lateral_end_slip: capped(out.lateral_end_slip * LATERAL_END_SLIP_SCALE),
        longitudinal_end_slip: 1.0,
        ..out
    }
}
