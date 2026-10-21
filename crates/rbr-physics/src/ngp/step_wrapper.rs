//! NGP's scaling around the tyre step: three of the contact's inputs lose a few percent with the car's speed
//! ([`speed_factor`]), and the tyre's heat lerps scale with the surface's heat setting ([`surface_scale`]) for the
//! step. All SSE.

/// The factor of the car's speed `v` (m/s): `1 - min(200, 3.6 |v|) / 200 * 0.025`, so 1 at rest and 0.975 from
/// 200 km/h.
pub fn speed_factor(v: f32) -> f32 {
    let kmh = v.abs() * 3.6;
    let capped = if 200.0 < kmh { 200.0 } else { kmh };
    1.0 - capped / 200.0 * 0.025
}

/// NGP's heat settings, percent, of the surface classes it scales (named by the class they apply to).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceHeatPercent {
    pub class_1: u32,
    pub class_2: u32,
    pub class_3: u32,
}

/// The heat lerps' factor for the contact's surface class and the tyre type: the settings percentage, as a fraction
/// and squared, for class 2 with types 0 to 2, class 1 with types 3 to 5 and class 3 with type 6; `None` otherwise
/// or for a 0 percentage.
pub fn surface_scale(class: u32, tyre_type: u32, percent: SurfaceHeatPercent) -> Option<f32> {
    let p = match (class, tyre_type) {
        (2, 0..=2) => percent.class_2,
        (1, 3..=5) => percent.class_1,
        (3, 6) => percent.class_3,
        _ => return None,
    };
    (p != 0).then(|| {
        let s = f64::from(p) as f32 / 100.0;
        s * s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_factor_caps_at_200_kmh() {
        assert_eq!(speed_factor(0.0), 1.0);
        assert_eq!(speed_factor(-100.0), 0.975);
    }

    #[test]
    fn surface_scale_matches_class_and_type() {
        let ninety = SurfaceHeatPercent { class_1: 90, class_2: 90, class_3: 90 };
        assert_eq!(surface_scale(2, 1, ninety), Some(0.9f32 * 0.9));
        assert_eq!(surface_scale(2, 3, ninety), None);
        assert_eq!(surface_scale(3, 6, SurfaceHeatPercent::default()), None);
    }
}
