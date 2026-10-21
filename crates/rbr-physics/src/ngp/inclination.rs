//! NGP's scaling of the tyre by its inclination (the angle of the tyre's plane to the ground): the grip terms and,
//! past 10 degrees, the heat lerps ([`grip_scale`]), and a heat factor on the contact ([`heat_scale`]). All SSE, in
//! doubles where the original uses them.

/// `min(a, b)` / `max(a, b)` with the second operand when they are unordered (SSE's `minsd`, `maxsd`).
fn min_sd(a: f64, b: f64) -> f64 {
    if a < b {
        a
    } else {
        b
    }
}

fn max_sd(a: f64, b: f64) -> f64 {
    if a > b {
        a
    } else {
        b
    }
}

/// The inclination in degrees from the dot product of the tyre's axis with the ground normal: `asin(clamp(dot, -1,
/// 1))` (Rust's `asin` agrees with the C runtime's on the snapshots, unchecked beyond).
pub fn degrees(dot: f32) -> f64 {
    let x = max_sd(-1.0, min_sd(1.0, f64::from(dot)));
    x.asin() * 180.0 / f64::from(std::f32::consts::PI)
}

/// The grip factor for the inclination `deg`, and whether it also scales the heat lerps (past 10 degrees). Positive:
/// `1 - (0.01 d + 0.0025 (d - 5))` (the second term past 5), with `d` capped at 45. Negative (or NaN): with
/// `d = min(34, -deg)`, `1 + 0.01 d` up to 5, then 1.05, and past 10 `1.05 - 0.025 (d - 10)`.
pub fn grip_scale(deg: f64) -> (f32, bool) {
    let (d, k) = if deg >= 0.0 {
        let slope = if deg > 5.0 { 0.0025 } else { 0.0 };
        let d = min_sd(45.0, deg);
        (d, 1.0 - ((d - 5.0) * slope + d * 0.01))
    } else {
        let d = min_sd(34.0, -deg);
        let k = if d > 10.0 {
            1.05 - (d - 10.0) * 0.025
        } else if d > 5.0 {
            1.05
        } else {
            1.0 + d * 0.01
        };
        (d, k)
    };
    (k as f32, d > 10.0)
}

/// The heat factor while the tyre is in contact: `(1 + 0.1 m^2) * s`, with `m = min(10, |deg|)` and `s` the tyre's
/// own heat gain `gain` when above 0, else 1. Without contact the factor is 1.
pub fn heat_scale(deg: f64, gain: f32) -> f32 {
    let s = if gain > 0.0 { gain } else { 1.0 };
    let m = min_sd(10.0, deg.abs());
    (m * 0.1 * m + 1.0) as f32 * s
}
