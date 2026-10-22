//! The friction limits at the current load: the friction coefficients and the slips of the friction curve's peak and
//! end, blended from the tyre type's load tables.

/// The two entries a blend reads: the table's values at the two entries around the load.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendPair {
    pub at_a: f32,
    pub at_b: f32,
}

/// `(1 - weight) * at_a + at_b * weight`, in the original's single-precision order.
fn blend(pair: BlendPair, weight: f32) -> f32 {
    (1.0 - weight) * pair.at_a + pair.at_b * weight
}

/// What the limits read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LimitsInput {
    /// The blend weight between the two load entries.
    pub weight: f32,
    /// The tyre type's tables at the two load entries: the cornering and traction friction factors and the slips at
    /// their peaks.
    pub cornering_friction: BlendPair,
    pub traction_friction: BlendPair,
    pub cornering_peak_slip: BlendPair,
    pub traction_peak_slip: BlendPair,
    /// The share of the slip that is lateral and longitudinal (the slip directions).
    pub lateral_share: f32,
    pub longitudinal_share: f32,
    /// The static and kinetic friction of the surface times the tyre's reference friction.
    pub static_scale: f32,
    pub kinetic_scale: f32,
    /// The cornering and traction stiffness, passed through unchanged.
    pub cornering_stiffness: f32,
    pub traction_stiffness: f32,
}

/// The limits of the friction curve, for the lateral and the longitudinal direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrictionLimits {
    /// The cornering and traction stiffness, as they came in.
    pub cornering_stiffness: f32,
    pub traction_stiffness: f32,
    /// The lateral peak friction coefficient, and the kinetic one split to the lateral direction.
    pub lateral_peak: f32,
    pub lateral_kinetic: f32,
    /// The longitudinal peak friction coefficient, and the kinetic one split to the longitudinal direction.
    pub longitudinal_peak: f32,
    pub longitudinal_kinetic: f32,
    /// The lateral slip at the peak and where the curve reaches the sliding friction.
    pub lateral_peak_slip: f32,
    pub lateral_end_slip: f32,
    /// The same for the longitudinal direction.
    pub longitudinal_peak_slip: f32,
    pub longitudinal_end_slip: f32,
}

/// `x * scale` capped at 1 from above (a compare then `ja`: a NaN keeps the product).
fn scaled_clamped(x: f32, scale: f32) -> f32 {
    let v = x * scale;
    if v > 1.0 {
        1.0
    } else {
        v
    }
}

/// The limits. The end slip is the peak slip times 1.02's 3, capped at 1; here the scale is NGP's 1, and
/// `ngp::outputs` rescales the lateral one.
pub fn friction_limits(x: &LimitsInput) -> FrictionLimits {
    const END_SLIP_SCALE: f32 = 1.0;
    let w = x.weight;
    let cornering = blend(x.cornering_friction, w);
    let traction = blend(x.traction_friction, w);
    let cornering_slip = blend(x.cornering_peak_slip, w);
    let traction_slip = blend(x.traction_peak_slip, w);
    // Both kinetic limits use the cornering factor, as the original does.
    let kinetic = cornering * x.kinetic_scale;
    FrictionLimits {
        cornering_stiffness: x.cornering_stiffness,
        traction_stiffness: x.traction_stiffness,
        lateral_peak: cornering * x.static_scale,
        lateral_kinetic: kinetic * x.lateral_share,
        longitudinal_peak: traction * x.static_scale,
        longitudinal_kinetic: kinetic * x.longitudinal_share,
        lateral_peak_slip: cornering_slip,
        lateral_end_slip: scaled_clamped(cornering_slip, END_SLIP_SCALE),
        longitudinal_peak_slip: traction_slip,
        longitudinal_end_slip: scaled_clamped(traction_slip, END_SLIP_SCALE),
    }
}
