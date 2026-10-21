//! `CHingedRigidBody::SimF950` (0x51f950): the hinge's per-step update. The hinge rate (+0x40c) takes the solve's value
//! (+0x624) times the step, plus its own value, clamped to `[-10, 10]`; a sign change away from a nonzero rate zeroes it.
//! The angle (+0x3fc) then advances by the stored rate times the step. The calls are the caller's (`Calls`): the
//! `SetDirty`s, the rate's scale into the definition's vector, `SimF280` and `SimD160`, and the state signal's emit.
//!
//! The sums are single precision (the asm's SSE), so there is no x87 model here. The asm and the C++ agree on the clamp,
//! the sign test and the order of the calls.

/// The bound of the hinge rate, on both sides.
pub const RATE_LIMIT: f32 = 10.0;

/// The calls `SimF950` makes. The objects are the body's, at the original's addresses.
pub trait Calls {
    /// `CSolid::E43Vf6C` (0x52cc30) on the body with the step: the body's own integrate, for a body with no owner car.
    /// Returns eax.
    fn integrate(&mut self, this: u32, dt: f32) -> u32;
    /// `NTweak::CBase::SetDirty` (0x52f450) on the tweak object at `at`.
    fn set_dirty(&mut self, at: u32);
    /// `CFloat3::Scale` (0x473cd0) on the vector at `vec`: `s` times it, written to `out`.
    fn scale(&mut self, vec: u32, s: f32, out: u32);
    /// `CEntity::M51F280` (0x51f280, the limit update) on the body.
    fn apply_stops(&mut self, this: u32);
    /// `CHingedRigidBody::SimD160` (0x51d160) on the body.
    fn frame(&mut self, this: u32);
    /// `CMultiSignal1<const CSigMrRBodyState&>::Emit` (0x51f700) on the signal at `signal`, with the payload at `sig`.
    /// Returns eax.
    fn emit(&mut self, signal: u32, sig: u32) -> u32;
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

/// The new hinge rate for `x` (the solve's value times the step, plus the old rate) when the old rate is `old`: `x`
/// clamped to the bound, or 0 when the clamp or `x` changes sign away from a nonzero `old`.
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
    /// The hinge rate (+0x40c).
    pub rate: f32,
    /// The hinge angle (+0x3fc): the stored rate times the step, plus the old angle.
    pub angle: f32,
}

/// The rate and the angle after the step. `solved` is the solve's value (+0x624), `rate` and `angle` the body's before it.
pub fn advance(solved: f32, dt: f32, rate: f32, angle: f32) -> Advance {
    let rate = next_rate(solved * dt + rate, rate);
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
