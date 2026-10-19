//! NGP's downforce against direct calls of the original in the emulator (`scripts/ngp-downforce-vectors.py` on
//! `snap1.snap-10880`): 3,000 fuzzed calls (the outputs and the path, by eax), with reversing, damage states, zero and NaN speeds and forces.

use rbr_physics::ngp::downforce::{downforce, DownforceIn, Path};

#[test]
fn downforce_matches_original_calls() {
    let mut n = 0;
    for line in include_str!("vectors/ngp-downforce.txt").lines() {
        let w: Vec<u32> = line.split(' ').map(|x| u32::from_str_radix(x, 16).unwrap()).collect();
        let f = |k: usize| f32::from_bits(w[k]);
        let i = DownforceIn {
            damage: (w[0] != 0).then_some([w[1], w[2]]),
            front: f(3),
            rear: f(4),
            density: f(5),
            speed: f(6),
            wheel_arms: [f(7), f(8), f(9), f(10)],
            force_sum: f(11),
            moment_sum: f(12),
        };
        let o = downforce(&i);
        let got =
            [o.speed_ratio_sq, o.front_force, o.rear_force, o.force_sum, o.moment_sum, o.drag_factor].map(f32::to_bits);
        assert_eq!(got, w[13..19], "{i:?}");
        let eax = match o.path {
            Path::NoDownforce => 8,
            Path::NoSpeed(direction) => direction as u32,
            Path::Applied => 0xffff_0474, // the wheel at car+0x474
        };
        assert_eq!(eax, w[19], "{i:?}");
        n += 1;
    }
    assert_eq!(n, 3000);
}
