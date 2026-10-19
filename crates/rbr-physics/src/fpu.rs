//! x87 arithmetic at 24-bit precision control.
//!
//! The game runs the x87 at PC=24 on the physics path. A chain of x87 operations
//! is then exact f64 arithmetic with each result rounded to 24 significant bits.
//! [`round24`] is that rounding. The exponent range is not limited, as on the x87.
//!
//! Under NGP the control word is `0x27f` (PC=53, `notes/ngp.md`) once the race runs: a chain's results round to f64
//! instead ([`round_x87`]); f64 arithmetic models that, short of the x87's wider exponent range. During the load it is
//! still D3D's `0x7f`, so NGP's physics then runs at PC=24 too. The host sets the control word's precision
//! ([`set_pc53`]), as the game's thread has it; [`precision`] gives a variant's.

use std::cell::Cell;

use crate::Variant;

/// The precision control of the x87 control word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precision {
    /// PC=24 (`0x7f`): 1.02's, and NGP's during the load.
    Pc24,
    /// PC=53 (`0x27f`): NGP's once the race runs.
    Pc53,
}

thread_local! {
    static PC53: Cell<bool> = const { Cell::new(true) };
}

/// Sets whether the thread's control word is PC=53 (the host's, from a snapshot or the game's state). PC=53 until set.
/// 1.02 runs at PC=24 regardless.
pub fn set_pc53(pc53: bool) {
    PC53.with(|c| c.set(pc53));
}

/// The precision `variant`'s physics runs at now: PC=24 for 1.02; under NGP the control word's ([`set_pc53`]).
pub fn precision(variant: Variant) -> Precision {
    match variant {
        Variant::Ngp778 if PC53.with(Cell::get) => Precision::Pc53,
        _ => Precision::Pc24,
    }
}

/// Fraction bits of an f64 that are dropped to keep 24 significant bits.
const DROP: u32 = 53 - 24;
const FRAC_MASK: u64 = (1 << DROP) - 1;
const HALF: u64 = 1 << (DROP - 1);
/// `2^54`, used to lift subnormal inputs into the normal range (exact).
const TWO_54: f64 = 18014398509481984.0;

/// An x87 register result at the variant's [`precision`]: [`round24`] at PC=24, unchanged (f64) at PC=53.
pub fn round_x87(x: f64, variant: Variant) -> f64 {
    match precision(variant) {
        Precision::Pc24 => round24(x),
        Precision::Pc53 => x,
    }
}

/// Round `x` to 24 significant bits, ties to even.
///
/// Zero, infinities and NaN are returned unchanged. A result that rounds past
/// the f64 range becomes infinity.
pub fn round24(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let bits = x.to_bits();
    if bits & !(1 << 63) < 1 << 52 {
        // Subnormal f64: scaling by 2^54 is exact and keeps the result normal.
        // The rounded value is a multiple of 2^-1074, so scaling back is exact.
        return round24(x * TWO_54) / TWO_54;
    }
    let rem = bits & FRAC_MASK;
    let mut kept = bits & !FRAC_MASK;
    let odd = kept & (1 << DROP) != 0;
    if rem > HALF || (rem == HALF && odd) {
        // Carry may run into the exponent, which gives the next binade.
        kept += 1 << DROP;
    }
    f64::from_bits(kept)
}

/// The x86 "integer indefinite": what `cvttss2si`/`cvtss2si` and x87 `fistp` store for NaN or an out-of-range value.
pub const INT_INDEFINITE: i32 = i32::MIN;

/// x86 float-to-int with truncation (`cvttss2si`/`cvttsd2si`, the C cast): NaN and values outside the i32 range give
/// [`INT_INDEFINITE`]. Rust's `as i32` saturates and maps NaN to 0, so ports use this instead.
pub fn trunc_i32(x: f64) -> i32 {
    let t = x.trunc();
    if (-2147483648.0..=2147483647.0).contains(&t) {
        t as i32
    } else {
        INT_INDEFINITE
    }
}

/// x86 float-to-int in round-to-nearest-even (`cvtss2si`, x87 `fistp` with the default rounding control): NaN and
/// values outside the i32 range give [`INT_INDEFINITE`].
pub fn round_i32(x: f64) -> i32 {
    let r = x.round_ties_even();
    if (-2147483648.0..=2147483647.0).contains(&r) {
        r as i32
    } else {
        INT_INDEFINITE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic xorshift64 sequence, so the tests need no RNG crate.
    fn xorshift(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    #[test]
    fn matches_f32_cast_in_f32_normal_range() {
        let mut state = 0x9e37_79b9_7f4a_7c15;
        for _ in 0..200_000 {
            let r = xorshift(&mut state);
            // Exponent field in the f32 normal range (2^-126 to 2^127), sign and mantissa random.
            let exp = 897 + (r >> 52) % 254;
            let bits = (r & !(0x7ff << 52)) | (exp << 52);
            let x = f64::from_bits(bits);
            let want = (x as f32) as f64;
            assert_eq!(round24(x).to_bits(), want.to_bits(), "x = {x:e}");
        }
    }

    #[test]
    fn small_values_already_representable_are_unchanged() {
        for x in [1.0, -0.5, 0.1f32 as f64, 3.0 * 2f64.powi(-100), 2f64.powi(-1074)] {
            assert_eq!(round24(x), x);
        }
    }

    #[test]
    fn ties_to_even() {
        // Halfway between 1 and 1 + 2^-23: the even neighbour is 1.
        assert_eq!(round24(1.0 + 2f64.powi(-24)), 1.0);
        // Halfway between 1 + 2^-23 (odd) and 1 + 2^-22 (even): goes to 1 + 2^-22.
        assert_eq!(round24(1.0 + 3.0 * 2f64.powi(-24)), 1.0 + 2f64.powi(-22));
        // Just above a tie rounds up, just below rounds down.
        let up = 1.0 + 2f64.powi(-24) + 2f64.powi(-40);
        let down = 1.0 + 2f64.powi(-24) - 2f64.powi(-40);
        assert_eq!(round24(up), 1.0 + 2f64.powi(-23));
        assert_eq!(round24(down), 1.0);
    }

    #[test]
    fn carry_into_next_binade() {
        // 2 - 2^-25 is a tie between 2 - 2^-23 (odd) and 2 (even).
        assert_eq!(round24(2.0 - 2f64.powi(-25)), 2.0);
        // Largest f64 rounds up to infinity.
        assert_eq!(round24(f64::MAX), f64::INFINITY);
    }

    #[test]
    fn exponent_is_not_clamped() {
        // Beyond f32 range: still finite and within 2^-24 relative error.
        let big = 1e300_f64;
        let small = 1e-300_f64;
        for x in [big, small, 3.5e38 * 4.0, 1e-45 / 3.0] {
            let r = round24(x);
            assert!(r.is_finite() && r != 0.0, "x = {x:e}");
            assert!(((r - x) / x).abs() <= 2f64.powi(-24), "x = {x:e}");
        }
    }

    #[test]
    fn subnormal_f64_inputs() {
        // A 53-bit subnormal rounds to 24 bits and stays a subnormal multiple of 2^-1074.
        let x = f64::from_bits(0x000f_ffff_ffff_ffff);
        let r = round24(x);
        assert!(r > 0.0);
        assert!(((r - x) / x).abs() <= 2f64.powi(-24));
        assert_eq!(round24(r), r);
        assert_eq!(round24(f64::from_bits(1)), f64::from_bits(1));
    }

    #[test]
    fn specials_and_sign() {
        assert_eq!(round24(0.0).to_bits(), 0.0f64.to_bits());
        assert_eq!(round24(-0.0).to_bits(), (-0.0f64).to_bits());
        assert_eq!(round24(f64::INFINITY), f64::INFINITY);
        assert_eq!(round24(f64::NEG_INFINITY), f64::NEG_INFINITY);
        assert!(round24(f64::NAN).is_nan());
        assert_eq!(round24(-(1.0 + 3.0 * 2f64.powi(-24))), -(1.0 + 2f64.powi(-22)));
    }

    #[test]
    fn float_to_int_matches_x86() {
        assert_eq!(trunc_i32(2.9), 2);
        assert_eq!(trunc_i32(-2.9), -2);
        assert_eq!(trunc_i32(f64::NAN), INT_INDEFINITE);
        assert_eq!(trunc_i32(-f64::NAN), INT_INDEFINITE);
        assert_eq!(trunc_i32(f64::INFINITY), INT_INDEFINITE);
        assert_eq!(trunc_i32(2147483648.0), INT_INDEFINITE);
        assert_eq!(trunc_i32(-2147483648.5), -2147483648);
        assert_eq!(round_i32(2.5), 2);
        assert_eq!(round_i32(3.5), 4);
        assert_eq!(round_i32(-2.5), -2);
        assert_eq!(round_i32(f64::NAN), INT_INDEFINITE);
        assert_eq!(round_i32(2147483647.6), INT_INDEFINITE);
    }
}
