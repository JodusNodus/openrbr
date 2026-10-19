//! NGP's wrapper `0x1005b660` of `CScratch::Step` (`0x5360c0`, the call at `0x53e2a3` in `E51Wheel::M3DD30`). It saves
//! the def's tables `+0x204..0x284` (heat lerps and blend pairs), scales three words of the step's fifth argument
//! (`+0x48`, `+0x58`, `+0x78`) by [`speed_factor`] for good, scales the heat lerps `+0x204..0x244` by
//! [`surface_scale`] for the call, runs 1.02's `Step` (through the trampoline `0x1005b540`) and restores the tables.
//! All SSE.

/// The factor of the scratch's speed `v` (`+0x360`, m/s): `1 - min(200, 3.6 |v|) / 200 * 0.025`.
pub fn speed_factor(v: f32) -> f32 {
    let kmh = v.abs() * 3.6;
    let capped = if 200.0 < kmh { 200.0 } else { kmh };
    1.0 - capped / 200.0 * 0.025
}

/// The heat lerps' factor for the surface class (the fifth argument's `+0x148`) and the tyre type (the def's `+0xc`):
/// NGP's settings percentage, squared, for class 2 with types 0 to 2 (`+0xe98`), class 1 with types 3 to 5
/// (`+0xea8`) and class 3 with type 6 (`+0xeb8`); `None` otherwise or for a 0 percentage. `percent` gives the
/// settings word at its offset.
pub fn surface_scale(class: u32, tyre_type: u32, percent: impl Fn(u32) -> u32) -> Option<f32> {
    let at = match (class, tyre_type) {
        (2, 0..=2) => 0xe98,
        (1, 3..=5) => 0xea8,
        (3, 6) => 0xeb8,
        _ => return None,
    };
    let p = percent(at);
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
        assert_eq!(surface_scale(2, 1, |_| 90), Some(0.9f32 * 0.9));
        assert_eq!(surface_scale(2, 3, |_| 90), None);
        assert_eq!(surface_scale(3, 6, |_| 0), None);
    }
}
