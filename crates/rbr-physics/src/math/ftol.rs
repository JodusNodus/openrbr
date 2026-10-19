//! The CRT's `_ftol2` (0x70c2c4): the x87 `st(0)` truncated to an int64, returned in `edx:eax`. The callers keep the low
//! word (`eax`). The conversion is pure, so it is here and not behind a host.

/// `_ftol2`: `x` truncated toward zero. A NaN or a value outside the i64 range gives `i64::MIN` (0x8000000000000000),
/// whose low word is 0, as the original's `fistp` does.
pub fn ftol2(x: f64) -> i64 {
    let t = x.trunc();
    if (-9223372036854775808.0..9223372036854775808.0).contains(&t) {
        t as i64
    } else {
        i64::MIN
    }
}
