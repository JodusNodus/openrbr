//! NGP's tyre convection term (`0x1004d410`, `ngp::damper::convection`) against direct calls of the original in the
//! emulator (`scripts/ngp-convection-vectors.py` on `cov-rpl11.snap-9520`): 3,000 fuzzed calls, with NGP's CRT `pow`.

use rbr_physics::ngp::damper::{convection, Convection};

#[test]
fn convection_matches_original_calls() {
    let mut n = 0;
    for line in include_str!("vectors/ngp-convection.txt").lines() {
        let w: Vec<u64> = line.split(' ').map(|x| u64::from_str_radix(x, 16).unwrap()).collect();
        let f = |k: usize| f32::from_bits(w[k] as u32);
        let x = Convection {
            w7c: f(3),
            w8c: f(4),
            wec: f(5),
            kind: w[6] as u32,
            spin: f(7),
            car_speed: f(8),
            boost: (w[9] != 0).then(|| f(10)),
            factors: [f(11), f(12)],
        };
        let got = f64::from(convection(f(0), f(1), f(2), &x, f(13)));
        let want = f64::from_bits(w[14]);
        assert_eq!(got.to_bits(), want.to_bits(), "{line}: {got} != {want}");
        n += 1;
    }
    assert_eq!(n, 3000);
}
