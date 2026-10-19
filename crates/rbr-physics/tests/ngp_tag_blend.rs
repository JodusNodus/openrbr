//! NGP's `M3D230` rewrite (`0x1005e8a0`, `ngp::tag_blend::blend`) against direct runs of the original in the emulator
//! (`scripts/ngp-tag-blend-vectors.py` on `cov-rpl11.snap-9520`): 3,000 fuzzed blends, with NGP's CRT `sin`.

use rbr_physics::ngp::tag_blend::{blend, Material};

#[test]
fn tag_blend_matches_original_runs() {
    let mut n = 0;
    for line in include_str!("vectors/ngp-tag-blend.txt").lines() {
        let w: Vec<u32> = line.split(' ').map(|x| u32::from_str_radix(x, 16).unwrap()).collect();
        let f = |k: usize| f32::from_bits(w[k]);
        let material = |k: usize| Material { period: f(k), gain: f(k + 1), lerp_a: f(k + 2), lerp_b: f(k + 3) };
        let (b, t) = blend([material(3), material(7)], f(0), f(1), f(2));
        let (want_b, want_t) = (f(11), f(12));
        // a NaN's sign is not compared (as rust_step's classify: the emulator and arm64 differ from x86 there)
        let same = |got: f32, want: f32| got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan());
        assert!(same(b, want_b) && same(t, want_t), "{line}: {b} {t}");
        n += 1;
    }
    assert_eq!(n, 3000);
}
