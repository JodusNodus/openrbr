//! NGP's force-feedback curve (`notes/ngp.md`, "Force feedback"). Where 1.02 divides the filtered steering torque by
//! the car's maximum force-feedback torque and clamps, NGP saturates it: `sign(T) * (1 - 0.975^(|T| * 100 / S))`, with
//! `S` the car's `ForceFeedbackSensitivity_NGP`, scaled by the ini's per-surface percentage at load
//! ([`sensitivity`]).

/// The force, -1..1, for the filtered steering torque `torque` and the sensitivity `sensitivity` (`S`). `muted`
/// gives 0. `100 / S` and `|T| * (100 / S)` are f32; the power runs in double on the f32 0.975 and rounds to f32,
/// then `1 - p` and the sign's product are f32. The sign is NGP's (0 for a zero torque).
pub fn curve(torque: f32, muted: bool, sensitivity: f32) -> f32 {
    if muted {
        return 0.0;
    }
    let exponent = torque.abs() * (100.0 / sensitivity);
    let p = f64::from(0.975f32).powf(f64::from(exponent)) as f32;
    super::sign(torque) as f32 * (1.0 - p)
}

/// The sensitivity `S` after the ini's percentage for the car's surface (`percent`: gravel, tarmac, snow; `surface`
/// 1 is tarmac, 2 snow, else gravel): a percentage above 0 other than 100 scales it (`(percent / 100f) * S`, f32), then
/// `S` is at least 1. A lower `S` is a stronger force that saturates sooner ([`curve`]).
pub fn sensitivity(base: f32, surface: u32, percent: [i32; 3]) -> f32 {
    let percent = percent[match surface {
        1 => 1,
        2 => 2,
        _ => 0,
    }];
    let s = if percent > 0 && percent != 100 { (percent as f32 / 100.0) * base } else { base };
    if s < 1.0 {
        1.0
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::{curve, sensitivity};

    #[test]
    fn saturates_with_the_sign_of_the_torque() {
        assert_eq!(curve(0.0, false, 100.0), 0.0);
        assert_eq!(curve(-0.0, false, 100.0), 0.0);
        assert_eq!(curve(50.0, true, 100.0), 0.0);
        // Half force near |T| = 27.4 and 90 % near 90.9 at S = 100 (notes/ngp.md "Force feedback").
        assert!((curve(27.37, false, 100.0) - 0.5).abs() < 1e-3);
        assert!((curve(-90.9, false, 100.0) + 0.9).abs() < 1e-3);
        // A larger S is a weaker force.
        assert!(curve(30.0, false, 125.0) < curve(30.0, false, 100.0));
        assert!(curve(1e6, false, 100.0) <= 1.0);
    }

    #[test]
    fn the_surface_percentage_scales_the_sensitivity() {
        let percent = [50, 100, 150];
        assert_eq!(sensitivity(105.0, 0, percent), 52.5);
        assert_eq!(sensitivity(105.0, 7, percent), 52.5);
        assert_eq!(sensitivity(105.0, 1, percent), 105.0);
        assert_eq!(sensitivity(105.0, 2, percent), 157.5);
        assert_eq!(sensitivity(105.0, 0, [0; 3]), 105.0);
        assert_eq!(sensitivity(105.0, 0, [-20; 3]), 105.0);
        // The floor, with or without a percentage.
        assert_eq!(sensitivity(1.5, 0, [10; 3]), 1.0);
        assert_eq!(sensitivity(0.0, 0, [100; 3]), 1.0);
    }
}
