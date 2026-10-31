//! x87 arithmetic at 24-bit precision control.
//!
//! The game runs the x87 at PC=24 on the physics path. A chain of x87 operations
//! is then exact f64 arithmetic with each result rounded to 24 significant bits.
//! [`round24`] is that rounding. The exponent range is not limited, as on the x87.
//!
//! The control word is `0x27f` (PC=53, `notes/ngp.md`) once the race runs: a chain's results round to f64 instead
//! ([`round_x87`]); f64 arithmetic models that, short of the x87's wider exponent range. During the load it is still
//! D3D's `0x7f`, so the physics then runs at PC=24. The precision is the thread's, like the CPU's control word: the host
//! sets it ([`set_precision`], [`with_precision`]) and [`precision`] and [`round_x87`] read it.

use std::cell::Cell;

/// The precision control of the x87 control word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precision {
    /// PC=24 (`0x7f`): the load's, and 1.02's.
    Pc24,
    /// PC=53 (`0x27f`): the race's.
    Pc53,
}

thread_local! {
    static PRECISION: Cell<Precision> = const { Cell::new(Precision::Pc53) };
}

/// Sets the thread's precision (the host's, from a snapshot or the game's state). PC=53 until set.
pub fn set_precision(precision: Precision) {
    PRECISION.set(precision);
}

/// The precision the physics runs at now: the thread's.
pub fn precision() -> Precision {
    PRECISION.get()
}

/// The thread's precision set by [`Precision::scope`], the previous one restored on drop.
#[must_use = "the precision is restored when this is dropped"]
pub struct Scope(Precision);

impl Precision {
    /// Sets the thread's precision until the returned guard drops.
    pub fn scope(self) -> Scope {
        Scope(PRECISION.replace(self))
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        PRECISION.set(self.0);
    }
}

/// Runs `f` at `precision` on this thread, the previous one restored after (also on a panic).
pub fn with_precision<T>(precision: Precision, f: impl FnOnce() -> T) -> T {
    let _scope = precision.scope();
    f()
}

/// Fraction bits of an f64 that are dropped to keep 24 significant bits.
const DROP: u32 = 53 - 24;
const FRAC_MASK: u64 = (1 << DROP) - 1;
const HALF: u64 = 1 << (DROP - 1);
/// `2^54`, used to lift subnormal inputs into the normal range (exact).
const TWO_54: f64 = 18014398509481984.0;

/// An x87 register result at the thread's [`precision`]: [`round24`] at PC=24, unchanged (f64) at PC=53.
pub fn round_x87(x: f64) -> f64 {
    match precision() {
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

/// The x87 "real indefinite" (the default NaN, negative), which an invalid x87 operation gives.
const X87_INDEFINITE: f64 = f64::from_bits(0xfff8_0000_0000_0000);

/// An x87 scalar add at the thread's [`precision`]: the invalid case (`inf + -inf`) is the indefinite NaN, as on
/// the x87. The operands are f32-valued or 53-bit results, so f64 holds each result before [`round_x87`] rounds it;
/// unlike f32 the x87 keeps an extended exponent, so a value past f32's range stays finite until it is stored.
pub(crate) fn x87_add(a: f64, b: f64) -> f64 {
    if a.is_infinite() && b.is_infinite() && a.is_sign_negative() != b.is_sign_negative() {
        return X87_INDEFINITE;
    }
    round_x87(a + b)
}

/// An x87 scalar subtract at the thread's [`precision`]: the invalid case (`inf - inf`) is the indefinite NaN.
pub(crate) fn x87_sub(a: f64, b: f64) -> f64 {
    if a.is_infinite() && b.is_infinite() && a.is_sign_negative() == b.is_sign_negative() {
        return X87_INDEFINITE;
    }
    round_x87(a - b)
}

/// An x87 scalar product at the thread's [`precision`]: the invalid case (`0 * inf`) is the indefinite NaN.
pub(crate) fn x87_mul(a: f64, b: f64) -> f64 {
    if (a == 0.0 && b.is_infinite()) || (a.is_infinite() && b == 0.0) {
        return X87_INDEFINITE;
    }
    round_x87(a * b)
}

/// An x87 scalar quotient at the thread's [`precision`]: the invalid cases (`0 / 0`, `inf / inf`) are the indefinite
/// NaN.
pub(crate) fn x87_div(a: f64, b: f64) -> f64 {
    if (a == 0.0 && b == 0.0) || (a.is_infinite() && b.is_infinite()) {
        return X87_INDEFINITE;
    }
    round_x87(a / b)
}

/// An x87 product of two f32 values at the thread's [`precision`], stored as f32: an invalid product (0 * inf) is
/// the negative indefinite NaN, as the instruction does. This is not `x87_mul(a.into(), b.into()) as f32`: narrowing
/// the indefinite to f32 with `as` is a host fold (`0x7fc00000` for the constant) or the host convert instruction,
/// so the constant is written out instead.
pub(crate) fn x87_mul_f32(a: f32, b: f32) -> f32 {
    let product = x87_mul(f64::from(a), f64::from(b));
    if product.is_nan() && !a.is_nan() && !b.is_nan() {
        return f32::from_bits(0xffc0_0000);
    }
    product as f32
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
