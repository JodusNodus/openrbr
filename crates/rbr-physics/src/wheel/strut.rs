//! The strut's travel at rest.

/// The weight of the droop stop in the rest travel: 0.67, one step below the f32 nearest to it.
const DROOP_WEIGHT: f32 = 0.669_999_96;
/// The weight of the bump stop: 0.33.
const BUMP_WEIGHT: f32 = 0.33;

/// The strut's rest travel: `0.67 * droop_stop + 0.33 * bump_stop`, from the stop positions of the spring-damper
/// definition (`OuterBumpRubberPos` and `InnerBumpRubberPos`; named by use). The droop product comes first, then the bump
/// product, then the sum.
pub fn rest_travel(bump_stop: f32, droop_stop: f32) -> f32 {
    droop_stop * DROOP_WEIGHT + bump_stop * BUMP_WEIGHT
}
