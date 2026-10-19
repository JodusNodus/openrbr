//! The two NGP wrappers inside `CScratch::Step` that scale by the tyre's inclination: `asin` of the align's dot
//! product (`scratch+0x3ac`), in degrees. `0x1005be40` (the call to `Accumulate` at `0x5362ab`) scales the blend
//! pairs at `m_def+0x244` and, past 10 degrees, the heat lerps at `+0x204`; NGP's `Step` wrapper (`0x1005b660`)
//! restores both after `Step`. `0x1005bc50` (the call to `HeatStep` at `0x536724`) scales `scratch+0x3c8` for the
//! call and stores a heat term at `+0x4e8`. All SSE, in doubles where the asm uses them.

/// `minsd a, b` / `maxsd a, b`: `b` when they are unordered.
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

/// The double of `f32` pi (`10148ec0`).
const PI_F32: f64 = 3.141_592_741_012_573_2;

/// `asin(clamp(dot, -1, 1))` in degrees (the CRT's `asin`; Rust's agrees on the snapshots, unchecked beyond).
pub fn degrees(dot: f32) -> f64 {
    let x = max_sd(-1.0, min_sd(1.0, f64::from(dot)));
    x.asin() * 180.0 / PI_F32
}

/// `0x1005be40`'s factor for the inclination `deg`, and whether it also scales the heat lerps (past 10 degrees).
/// Positive: `1 - (0.01 d + 0.0025 (d - 5))` (the second term past 5), with `d` capped at 45. Negative (or NaN):
/// with `d = min(34, -deg)`, `1 + 0.01 d` up to 5, then 1.05, and past 10 `1.05 - 0.025 (d - 10)`.
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

/// `0x1005bc50`'s factor on `scratch+0x3c8` while the contact flag (`+0x480`) is set: `(1 + 0.1 m^2) * s`, with
/// `m = min(10, |deg|)` and `s` the def's `+0x10c` when above 0, else 1. Without contact it is 1.
pub fn heat_scale(deg: f64, def_10c: f32) -> f32 {
    let s = if def_10c > 0.0 { def_10c } else { 1.0 };
    let m = min_sd(10.0, deg.abs());
    (m * 0.1 * m + 1.0) as f32 * s
}
