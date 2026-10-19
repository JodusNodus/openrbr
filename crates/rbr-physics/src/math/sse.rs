//! The scalar SSE operations the reference code uses where the result depends on the instruction's NaN and infinity
//! rules (`addss`, `mulss`). Rust's `+` and `*` do not select a NaN operand the way the instruction does, and an
//! invalid product returns the indefinite NaN.

/// `addss`: a NaN operand is returned quieted (the right one first); `inf + -inf` is the indefinite NaN.
pub fn add(left: f32, right: f32) -> f32 {
    if right.is_nan() {
        quiet_nan(right)
    } else if left.is_nan() {
        quiet_nan(left)
    } else if left.is_infinite() && right.is_infinite() && left.is_sign_negative() != right.is_sign_negative() {
        f32::from_bits(0xffc0_0000)
    } else {
        left + right
    }
}

/// `subss`: a NaN operand is returned quieted (the right one first); `inf - inf` is the indefinite NaN.
pub fn sub(left: f32, right: f32) -> f32 {
    if right.is_nan() {
        quiet_nan(right)
    } else if left.is_nan() {
        quiet_nan(left)
    } else if left.is_infinite() && right.is_infinite() && left.is_sign_negative() == right.is_sign_negative() {
        f32::from_bits(0xffc0_0000)
    } else {
        left - right
    }
}

/// `mulss`: a NaN operand is returned quieted (the right one first); `0 * inf` is the indefinite NaN.
pub fn mul(left: f32, right: f32) -> f32 {
    if right.is_nan() {
        quiet_nan(right)
    } else if left.is_nan() {
        quiet_nan(left)
    } else if (left == 0.0 && right.is_infinite()) || (left.is_infinite() && right == 0.0) {
        // `mulss(0, inf)` and `mulss(inf, 0)` both return the negative indefinite NaN.
        f32::from_bits(0xffc0_0000)
    } else {
        left * right
    }
}

/// `divss`: a NaN operand is returned quieted (the right one first); `0 / 0` and `inf / inf` are indefinite NaNs.
pub fn div(left: f32, right: f32) -> f32 {
    if right.is_nan() {
        quiet_nan(right)
    } else if left.is_nan() {
        quiet_nan(left)
    } else if (left == 0.0 && right == 0.0) || (left.is_infinite() && right.is_infinite()) {
        f32::from_bits(0xffc0_0000)
    } else {
        left / right
    }
}

fn quiet_nan(value: f32) -> f32 {
    f32::from_bits(value.to_bits() | 0x0040_0000)
}
