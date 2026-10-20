//! Table lookups of the VCU: the uniform pedal maps (`VcuTableBlend`, 0x536950) and the speed maps
//! (`LookupPairTable`, 0x536880). Both are piecewise linear. The asm sets the operation order and
//! the NaN behaviour; the x87 interpolation rounds each operation at the variant's precision (`fpu::round_x87`: 24 bits
//! in 1.02, 53 under NGP's race, `notes/ngp.md` "x87 precision"). Each returns st0, so a double at PC=53.

use super::layout::{Knot, SpeedKnot};
use crate::fpu::{round_x87, trunc_i32};
use crate::Variant;

/// Pedal map lookup (`E49Proxy::VcuTableBlend`, stdcall `(table, x, lo, hi, n)`): knots spread evenly
/// over `[lo, hi]`, with `n` the length of `knots`. The index `f = (x - lo) * (n - 1) / (hi - lo)`
/// clamps to the first and last knot.
pub fn table_blend(knots: &[Knot], x: f32, lo: f32, hi: f32, variant: Variant) -> f64 {
    let n = knots.len() as i32;
    let f = (x - lo) * (n - 1) as f32 / (hi - lo);
    // A NaN or f < -2^31 index here logs in the original (E01Log); no physics effect, so not ported.
    let i = knot_index(f);
    if n - 2 < i {
        return knots[(n - 1) as usize].value.into();
    }
    if i < 0 {
        return knots[0].value.into();
    }
    // x87: a = f - i, then (1 - a) * knot[i] + a * knot[i + 1].
    let i = i as usize;
    let r = |x: f64| round_x87(x, variant);
    let a = r(f as f64 - i as f64);
    let below = r(1.0 - a);
    r(r(below * knots[i].value as f64) + r(a * knots[i + 1].value as f64))
}

/// Speed map lookup (`NPhysics::LookupPairTable`, stdcall `(table, x, n)`): piecewise linear over the
/// keys, which increase with the record. Below the first key the first value is returned, above the
/// last key the last one. Between keys `k[u - 1]` and `k[u]` the x87 interpolation is
/// `(1 - f) * v[u] + f * v[u - 1]` with `f = 1 - (x - k[u-1]) / (k[u] - k[u-1])`.
///
/// `!(x > key)` is the asm's `jbe`, so a NaN `x` stops at the first record.
pub fn lookup_pair_table(knots: &[SpeedKnot], x: f32, variant: Variant) -> f64 {
    let first = knots[0];
    let last = knots[knots.len() - 1];
    if first.key >= x {
        return first.value.into();
    }
    if x > last.key {
        return last.value.into();
    }
    // The first record from 1 whose key is not below x. The check above makes one exist.
    let u = (1..knots.len()).find(|&u| !(x > knots[u].key)).expect("x is at most the last key");
    let (lo, hi) = (knots[u - 1], knots[u]);
    let r = |x: f64| round_x87(x, variant);
    let q = r(r(x as f64 - lo.key as f64) / r(hi.key as f64 - lo.key as f64));
    let f = r(1.0 - q);
    let below = r(1.0 - f);
    r(r(below * hi.value as f64) + r(f * lo.value as f64))
}

/// The asm's `cvttss2si` with its range guards: `f >= 2^31` gives `i32::MAX`; `f` in `[-2^31, 2^31)`
/// truncates toward zero; `f < -2^31` and NaN give `i32::MIN`.
fn knot_index(f: f32) -> i32 {
    if f >= 2147483648.0 {
        i32::MAX
    } else if f >= -2147483648.0 {
        trunc_i32(f as f64)
    } else {
        i32::MIN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V: Variant = Variant::Stock102;

    /// Eleven knots, knot `k` valued `k * 0.25`.
    fn knots() -> Vec<Knot> {
        (0..11).map(|k| Knot { value: k as f32 * 0.25 }).collect()
    }

    /// Eleven records with key `k` and value `10k`.
    fn pairs() -> Vec<SpeedKnot> {
        (0..11).map(|k| SpeedKnot { key: k as f32, value: 10.0 * k as f32 }).collect()
    }

    #[test]
    fn blend_clamps_below_and_above() {
        let k = knots();
        assert_eq!(table_blend(&k, -0.5, 0.0, 1.0, V), 0.0);
        assert_eq!(table_blend(&k, 2.0, 0.0, 1.0, V), 2.5);
        assert_eq!(table_blend(&k, 1.0e30, 0.0, 1.0, V), 2.5);
    }

    #[test]
    fn blend_nan_takes_the_first_knot() {
        assert_eq!(table_blend(&knots(), f32::NAN, 0.0, 1.0, V), 0.0);
    }

    #[test]
    fn blend_interpolates() {
        // x = 0.25 over [0, 1] with 11 knots: f = 2.5, halfway between knots 2 (0.5) and 3 (0.75).
        assert_eq!(table_blend(&knots(), 0.25, 0.0, 1.0, V), 0.625);
    }

    #[test]
    fn knot_index_guards() {
        assert_eq!(knot_index(f32::NAN), i32::MIN);
        assert_eq!(knot_index(-3.0e9), i32::MIN);
        assert_eq!(knot_index(-2147483648.0), i32::MIN);
        assert_eq!(knot_index(-0.5), 0);
        assert_eq!(knot_index(2147483648.0), i32::MAX);
        assert_eq!(knot_index(9.99), 9);
    }

    #[test]
    fn pair_lookup_is_piecewise_linear() {
        let p = pairs();
        let mut x = -1.0f32;
        while x < 12.0 {
            let want = if x <= 0.0 {
                0.0
            } else if x >= 10.0 {
                100.0
            } else {
                10.0 * x
            };
            let got = lookup_pair_table(&p, x, V);
            assert!((got - want as f64).abs() < 1e-4, "x = {x}: {got} vs {want}");
            x += 0.37;
        }
    }

    #[test]
    fn pair_lookup_branches() {
        let p = pairs();
        // At or below the first key: the first value.
        assert_eq!(lookup_pair_table(&p, -3.0, V), 0.0);
        // Above the last key: the last value.
        assert_eq!(lookup_pair_table(&p, 11.0, V), 100.0);
        // Records near the top (past the unrolled search of the original).
        assert_eq!(lookup_pair_table(&p, 9.5, V), 95.0);
        // NaN stops the search at record 1, and the interpolation is NaN (as in the original).
        assert!(lookup_pair_table(&p, f32::NAN, V).is_nan());
    }
}
