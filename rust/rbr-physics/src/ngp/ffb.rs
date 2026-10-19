//! NGP's force-feedback curve (`0x10042120`, thiscall on the car, st0), NGP's replacement of `CCar::FfbNorm`
//! (`0x4e76c0`) at `CFFBController::EmitWheel` and `EmitForce` (`notes/ngp-ffb.md`). Where 1.02 divides the filtered
//! steering torque by `MaxFFTorque` and clamps, NGP saturates it: `sign(T) * (1 - 0.975^(|T| * 100 / S))`, with `S`
//! the car def's `+0x44a0` (`ForceFeedbackSensitivity_NGP`, after NGP's per-surface ini scaling at load).

/// `0x10042120` on the filtered torque `T` (car `+0xaec`), the car's `+0x1ec` word (`gated`: nonzero gives 0) and `S`.
/// `100 / S` and `|T| * (100 / S)` are f32 (`divss`, `mulss`); `pow` runs in double on `0.975f` and rounds to f32
/// (`cvtsd2ss`), and `1 - p` and the sign's product are f32. The sign is NGP's (0 for a zero `T`).
pub fn curve(torque: f32, gated: bool, sensitivity: f32) -> f32 {
    if gated {
        return 0.0;
    }
    let exponent = torque.abs() * (100.0 / sensitivity);
    let p = f64::from(0.975f32).powf(f64::from(exponent)) as f32;
    super::sign(torque) as f32 * (1.0 - p)
}

#[cfg(test)]
mod tests {
    use super::curve;

    #[test]
    fn saturates_with_the_sign_of_the_torque() {
        assert_eq!(curve(0.0, false, 100.0), 0.0);
        assert_eq!(curve(-0.0, false, 100.0), 0.0);
        assert_eq!(curve(50.0, true, 100.0), 0.0);
        // Half force near |T| = 27.4 and 90 % near 90.9 at S = 100 (notes/ngp-ffb.md).
        assert!((curve(27.37, false, 100.0) - 0.5).abs() < 1e-3);
        assert!((curve(-90.9, false, 100.0) + 0.9).abs() < 1e-3);
        // A larger S is a weaker force.
        assert!(curve(30.0, false, 125.0) < curve(30.0, false, 100.0));
        assert!(curve(1e6, false, 100.0) <= 1.0);
    }
}
