//! The hinge's per-step update. The hinge rate takes the solve's angular acceleration times the step, plus its own value,
//! clamped to `[-10, 10]` rad/s; a sign change away from a nonzero rate zeroes it. The angle then advances by the stored
//! rate times the step. The body's angular velocity is the hinge axis times the rate, and its linear velocity is cleared.
//! The calls are the caller's ([`Calls`]): the dirty marks, the axis scaled by the rate, the end stops, the pose, and the
//! emit of the state signal.
//!
//! The sums are single precision (SSE), so there is no x87 model here.

/// The bound of the hinge rate, on both sides, rad/s.
pub const RATE_LIMIT: f32 = 10.0;

/// The calls the update makes. The objects are the body's.
pub trait Calls {
    /// The plain solid body's integration of the body over `dt`, for a body with no owner car. Returns its result.
    fn integrate_unowned(&mut self, body: u32, dt: f32) -> u32;
    /// Marks the tweak object at `at` dirty.
    fn mark_dirty(&mut self, at: u32);
    /// The vector at `vector` scaled by `s`, written to `out`.
    fn scale_vector(&mut self, vector: u32, s: f32, out: u32);
    /// The end stops ([`super::hinge_stops::apply_stops`]) on the body.
    fn apply_stops(&mut self, body: u32);
    /// Poses the body ([`super::hinge_pose::update_pose`]).
    fn update_pose(&mut self, body: u32);
    /// Emits the state signal `signal` with the payload at `payload`. Returns the emit's result.
    fn emit_state(&mut self, signal: u32, payload: u32) -> u32;
}

/// The sign the clamp and the zero rule use: 1, -1, or 0 (also for NaN, which compares as neither).
fn sign(x: f32) -> i32 {
    if x > 0.0 {
        1
    } else if x < 0.0 {
        -1
    } else {
        0
    }
}

/// The new hinge rate for the candidate `x` (the angular acceleration times the step, plus the old rate) when the old rate
/// is `old`: `x` clamped to the bound, or 0 when the clamp or `x` changes sign away from a nonzero `old`.
pub fn next_rate(x: f32, old: f32) -> f32 {
    let (clamped, sx) = if x < -RATE_LIMIT {
        (-RATE_LIMIT, -1)
    } else if x > RATE_LIMIT {
        (RATE_LIMIT, 1)
    } else {
        (x, sign(x))
    };
    if sx != sign(old) && old != 0.0 {
        0.0
    } else {
        clamped
    }
}

/// The rate and the angle the body keeps after the step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Advance {
    /// The hinge rate, rad/s.
    pub rate: f32,
    /// The hinge angle, rad: the stored rate times the step, plus the old angle.
    pub angle: f32,
}

/// The rate and the angle after a step of `dt` seconds. `acceleration` is the solve's angular acceleration, rad/s^2; `rate`
/// and `angle` are the body's before it.
pub fn advance(acceleration: f32, dt: f32, rate: f32, angle: f32) -> Advance {
    let rate = next_rate(acceleration * dt + rate, rate);
    Advance { rate, angle: rate * dt + angle }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_is_clamped_to_both_bounds() {
        assert_eq!(next_rate(12.0, 3.0), RATE_LIMIT);
        assert_eq!(next_rate(-12.0, -3.0), -RATE_LIMIT);
    }

    #[test]
    fn sign_change_zeroes_a_nonzero_rate_only() {
        assert_eq!(next_rate(-1.0, 2.0), 0.0);
        assert_eq!(next_rate(-1.0, -2.0), -1.0);
        assert_eq!(next_rate(-1.0, 0.0), -1.0);
        assert_eq!(next_rate(0.0, 2.0), 0.0);
    }

    #[test]
    fn angle_advances_by_the_stored_rate() {
        let a = advance(0.5, 0.25, 1.0, 3.0);
        assert_eq!(a.rate, 1.125);
        assert_eq!(a.angle, 1.125 * 0.25 + 3.0);
        assert_eq!(advance(0.0, 0.25, 2.0, 0.0).angle, 0.5);
    }
}
