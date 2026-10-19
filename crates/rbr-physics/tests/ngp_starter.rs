//! NGP's starter against direct calls of the original in the emulator (`scripts/ngp-starter-vectors.py` on
//! `cov-fwd41b.snap-10880`): 2,000 fuzzed calls, each with NGP's wall-clock hold set to the hold the port counts in
//! simulation time.

use rbr_physics::engine_driveline::layout::DriveWorker;
use rbr_physics::ngp::starter::starter;

#[test]
fn starter_matches_original_calls() {
    let (mut n, mut cranked) = (0, 0);
    for line in include_str!("vectors/ngp-starter.txt").lines() {
        let w: Vec<u64> = line.split(' ').map(|x| u64::from_str_radix(x, 16).unwrap()).collect();
        let f = |k: usize| f32::from_bits(w[k] as u32);
        let (dt, band) = (f(3), f(4));
        let mut d = DriveWorker::default();
        d.this.lane_gate = w[0] as u32;
        d.obj.ready = w[1] as u32;
        d.state.drive.engine_speed = f(5);
        d.obj.speed_term = f(6);
        d.obj.rate = f(7);
        // the hold before the step: the original's E less this step's dt
        let before = w[2] - (f64::from(dt) * 1.0e6).round() as u64;
        let (v, held) = starter(&mut d, dt, band, before);
        let got =
            [d.this.scaled_term, d.this.speed_factor, d.obj.speed_term, d.state.drive.engine_speed].map(f32::to_bits);
        let want = [w[9], w[10], w[11], w[12]].map(|x| x as u32);
        let same = |a: u32, b: u32| a == b || (f32::from_bits(a).is_nan() && f32::from_bits(b).is_nan());
        assert!((0..4).all(|k| same(got[k], want[k])), "{line}: {got:08x?}");
        let (v, want_v) = (f64::from(v), f64::from_bits(w[8]));
        assert!(v.to_bits() == want_v.to_bits() || (v.is_nan() && want_v.is_nan()), "{line}: {v}");
        assert_eq!(held == 0, w[13] == 1, "{line}: held {held}");
        cranked += usize::from(v != 0.0);
        n += 1;
    }
    assert_eq!(n, 2000);
    assert!(cranked > 500, "{cranked}");
}
