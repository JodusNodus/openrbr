//! NGP's damper helpers against direct calls of the original in the emulator at NGP's x87 control word
//! (`scripts/ngp-damper-vectors.py` on `snap1.snap-10880`): 1,000 fuzzed calls each of `0x1004d2f0`, `0x1004d3c0` and
//! `0x1004d380`.

use rbr_physics::ngp::damper::{scratch_factor, step1, step2};

#[test]
fn damper_helpers_match_original_calls() {
    let mut n = 0;
    for line in include_str!("vectors/ngp-damper.txt").lines() {
        let w: Vec<u64> = line.split(' ').map(|x| u64::from_str_radix(x, 16).unwrap()).collect();
        let f = |k: usize| f32::from_bits(w[k] as u32);
        let (a, b, c, d, temperature) = (f(1), f(2), f(3), f(4), f(5));
        let factor = scratch_factor(f(6), f(7), f(8));
        let got = match w[0] {
            0x1004_d2f0 => f64::from(step1(a, b, c, d, temperature)),
            0x1004_d3c0 => f64::from(step1(a, b, c, factor, temperature)),
            0x1004_d380 => step2(a, b, factor),
            other => panic!("{other:#x}"),
        };
        let want = f64::from_bits(w[9]);
        // a NaN's sign is not compared (as rust_step's classify: the emulator and arm64 differ from x86 there)
        assert!(got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan()), "{line}: {got} != {want}");
        n += 1;
    }
    assert_eq!(n, 3000);
}
