//! `math::ftol::ftol2` (the CRT's `_ftol2`, 0x70c2c4): the truncation toward zero, the low word the callers keep, and the
//! indefinite value for NaN and out-of-range inputs.

use rbr_physics::math::ftol::ftol2;

#[test]
fn ftol2_truncates_like_the_crt() {
    assert_eq!(ftol2(2.9), 2);
    assert_eq!(ftol2(-2.9), -2);
    assert_eq!(ftol2(-0.5), 0);
    assert_eq!(ftol2(2147483653.0), 2147483653);
    // The callers keep the low word, so a negative value is its two's complement.
    assert_eq!(ftol2(-1.0) as u32, 0xFFFF_FFFF);
    // NaN and values outside the i64 range give i64::MIN, whose low word is 0.
    assert_eq!(ftol2(f64::NAN), i64::MIN);
    assert_eq!(ftol2(9223372036854775808.0), i64::MIN);
    assert_eq!(ftol2(-1e30), i64::MIN);
    assert_eq!(ftol2(f64::INFINITY), i64::MIN);
    assert_eq!(ftol2(f64::NAN) as u32, 0);
}
