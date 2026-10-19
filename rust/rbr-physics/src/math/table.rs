//! `E46_IdxWrite` (0x528980): the segment of a sorted table that holds a value, walked from a hint. Physics core's
//! table lookups and the tyre's table search (`Step`) both use it. The comparisons follow the asm (`x > t` and
//! `!(t >= x)`), so a NaN value takes the same path as the original.

/// The two table indices around a value, and the fraction between them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TableIndex {
    pub lo: i32,
    pub hi: i32,
    pub frac: f32,
}

/// The segment of the `n`-entry table `t` (its entry `i` is `t(i)`) that holds `x`, walked from `hint` (the `lo`
/// already in the out struct). Above the table the segment is `(n-2, n-1)` with fraction 1, at or below its first
/// entry `(0, 1)` with fraction 0. A NaN `x` walks down from the hint, so `t` is read at `hint - 1`, which for a hint
/// of 0 is below the table: `t` decides what that reads.
pub fn table_index_frac(t: impl Fn(i32) -> f32, n: i32, x: f32, hint: i32) -> TableIndex {
    let segment = |lo: i32| {
        let (a, b) = (t(lo), t(lo + 1));
        TableIndex { lo, hi: lo + 1, frac: (x - a) / (b - a) }
    };
    if x > t(hint) {
        if x > t(n - 1) {
            return TableIndex { lo: n - 2, hi: n - 1, frac: 1.0 };
        }
        let mut c = hint + 1;
        while x > t(c) {
            c += 1;
        }
        return segment(c - 1);
    }
    if !(t(0) >= x) {
        let mut c = hint;
        loop {
            let v = t(c - 1);
            c -= 1;
            if !(v > x) {
                break;
            }
        }
        return segment(c);
    }
    TableIndex { lo: 0, hi: 1, frac: 0.0 }
}
