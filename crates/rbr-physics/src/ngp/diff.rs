//! NGP's wrapper of `CDrive::M9BC0` (`0x10048d50`, NGP's target of `Step1E0`'s call at `0x50a46e`): the downforce
//! (`downforce`), the drive gain's loss, and for NGP's drivetrain types (4 to 7, the low nibble of the def's `+0x640`)
//! the diffs' lock torques, before the wheel worker (`M9BC0`, or NGP's `0x100487f0` for types 4 and 5). The diff types
//! are the word's next three nibbles, one per diff (the state's terms 0 to 2): 1 keeps the lock, 2 is open, 3 is
//! locked, 4 is torque-biasing (a ratio from the car's lock values), 5 viscous. All SSE. Names are ours.

use super::drive::split_factor;
use crate::engine_driveline::layout::DriveWorker;

/// `0x10048410`: the drivetrain type, the word's low nibble (0 above 7).
pub fn drive_type(word: u32) -> u32 {
    match word & 0xf {
        t @ 0..=7 => t,
        _ => 0,
    }
}

/// `0x10048380` for diffs 0 to 2: the word's nibbles 1 to 3 (0 above 5).
pub fn diff_types(word: u32) -> [u32; 3] {
    [4, 8, 12].map(|shift| match (word >> shift) & 0xf {
        t @ 0..=5 => t,
        _ => 0,
    })
}

/// `0x100472a0`: a positive drive gain loses `loss` percent (the `+0xe88` word of NGP's settings object; unsigned).
pub fn gain_loss(gain: f32, loss: u32) -> f32 {
    if gain <= 0.0 {
        gain
    } else {
        (1.0 - loss as f32 / 100.0) * gain
    }
}

/// What the lock torques read beyond the drive worker.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiffTuning {
    /// The car's lock values per diff, under drive and on the overrun (`*(car+0x850)` `+0x1b8`/`+0x268`,
    /// `+0x738`/`+0x7e8`, `+0x9f8`/`+0xaa8`).
    pub locks: [[f32; 2]; 3],
    /// The car's `+0x864` and the limit it is compared with (`*(car+0x850)` `+0x198`): above it diff 0 opens.
    pub speed: f32,
    pub speed_limit: f32,
    /// The def's `+0x58` per diff (`0x60` apart): the lock's floor.
    pub preload: [f32; 3],
    /// The worker object's `+0x4`, `+0xc`, `+0x14`: a diff whose word is 0 opens unless it is locked (type 3).
    pub enabled: [u32; 3],
    /// NGP's axle split (`10171a90`, `split_factor`) and viscous gain (`10171aa0`).
    pub split: f32,
    pub viscous: f32,
}

const EPS: f32 = f32::EPSILON; // 101459b4
const LOCK_CAP: f32 = 0.99; // 101459d0
const VISCOUS_CAP: f32 = 100.0; // 10146b28

/// `maxss a, b` / `minss a, b`: `b` when they are unordered.
fn max_ss(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}

fn min_ss(a: f32, b: f32) -> f32 {
    if a < b {
        a
    } else {
        b
    }
}

/// The bias ratio of a lock value `x`: `(x + 1) / (1 - x)` with `x` capped at 0.99, at least 1.
fn bias_ratio(x: f32) -> f32 {
    let x = if x < 1.0 || x.is_nan() { x } else { LOCK_CAP };
    let r = (x + 1.0) / (1.0 - x);
    if 1.0 > r {
        1.0
    } else {
        r
    }
}

/// The torque-biasing lock of the two sides `a`, `b` at the ratio `r`: `(r lo - hi) / (r + 1)`, zero when the sides
/// are within the ratio, at least 0 and `preload`.
fn biasing(r: f32, a: f32, b: f32, preload: f32) -> f32 {
    let lo = if a > b { b } else { a };
    let hi = if b > a { b } else { a };
    let mut v = (r * lo - hi) / (r + 1.0);
    if lo > 0.0 && hi / lo > r {
        v = 0.0;
    }
    max_ss(preload, max_ss(0.0, v))
}

/// The viscous lock: `sqrt(min(100, |term|)) * (2 viscous * lock) + preload`.
fn viscous(term: f32, gain: f32, lock: f32, preload: f32) -> f32 {
    let s = f64::from(min_ss(VISCOUS_CAP, term.abs())).sqrt() as f32;
    s * (gain * 2.0 * lock) + preload
}

/// The sign `0x10048d50` gives a wheel's latch bound: the negated sign bit, 0 for zero.
fn neg_sign(v: f32) -> f32 {
    if v == 0.0 {
        0.0
    } else if v.is_sign_negative() {
        1.0
    } else {
        -1.0
    }
}

/// The lock torques (the state's terms 0 to 2, `scale`) for an NGP drivetrain type, from the drive worker as
/// `0x10048d50` reads it (after the downforce and the gain's loss).
pub fn lock_torques(w: &DriveWorker, types: [u32; 3], t: &DiffTuning) -> [f32; 3] {
    let s = &w.state;
    let base = s.drive_gain - s.pack_gain * w.this.loc0.value;
    // Per wheel: the magnitude of its force (`a`) and of its torque on the diff (`b`).
    let mut a = [0.0f32; 4];
    let mut b = [0.0f32; 4];
    for k in 0..4 {
        let wheel = w.def.wheels[k];
        let axle = w.def.axles[wheel.pack as usize];
        let blk = &s.blocks[k];
        let m = crate::fpu::trunc_i32(f64::from(wheel.weight)).wrapping_mul(axle.direction);
        let mf = m as f32;
        let mut c = (w.this.wheel_offset[k] - blk.clutch_load * blk.slip.value) * mf;
        let f = mf * blk.prev_force.value;
        a[k] = f.abs();
        let drive = split_factor(axle.scale * 0.5 * base, k, t.split);
        let latched = blk.prev_gate.value.wrapping_mul(m);
        let sign = if latched != 0 {
            latched.wrapping_neg() as f32
        } else if f == 0.0 {
            neg_sign(drive + c)
        } else {
            neg_sign(f)
        };
        let mut v = sign * blk.latch_bound;
        if w.obj.wheel_enabled[k] == 0 {
            c = (m.wrapping_neg() as f32) * blk.slip.value * blk.clutch_load;
            v = 0.0;
        }
        b[k] = (v + c).abs();
    }

    let gain = s.drive_gain;
    let ratio: [f32; 3] = std::array::from_fn(|g| {
        let [on, off] = t.locks[g];
        bias_ratio(if gain >= 0.0 { on } else { off })
    });
    let lock = |g: usize, kind: u32, x: f32, y: f32, kept: f32, scale: &[f32; 3]| match kind {
        3 => x + y,
        4 => biasing(ratio[g], x, y, t.preload[g]),
        1 => scale[g],
        5 => viscous(s.terms[g].term, t.viscous, t.locks[g][0], t.preload[g]),
        2 => 0.0,
        _ => kept,
    };

    let mut scale = [0, 1, 2].map(|g| s.terms[g].scale);
    let mut axle_lock = [0.0f32; 3];
    for pair in 0..2 {
        let g = w.def.wheels[2 * pair].pack as usize;
        if g > 2 {
            continue;
        }
        let (k0, k1) = (2 * pair, 2 * pair + 1);
        let mut v = lock(g, types[g], b[k0], b[k1], axle_lock[g], &scale);
        if EPS > a[k0] && EPS > a[k1] {
            v = 0.0;
        }
        axle_lock[g] = v;
        scale[g] = v;
    }

    let mut v = lock(0, types[0], b[0] + b[1], b[2] + b[3], axle_lock[0], &scale);
    if a.iter().all(|x| *x < EPS) {
        v = 0.0;
    }
    scale[0] = v;
    if t.speed > t.speed_limit {
        scale[0] = 0.0;
    }
    for g in 0..3 {
        if t.enabled[g] == 0 && types[g] != 3 {
            scale[g] = 0.0;
        }
    }
    scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_come_from_the_nibbles() {
        assert_eq!(drive_type(0x4436), 6);
        assert_eq!(drive_type(0x4438), 0);
        assert_eq!(diff_types(0x4436), [3, 4, 4]);
        assert_eq!(diff_types(0x5f65), [0, 0, 5]);
    }

    #[test]
    fn only_a_positive_gain_loses() {
        assert_eq!(gain_loss(-1.0, 50), -1.0);
        assert_eq!(gain_loss(0.0, 50), 0.0);
        assert_eq!(gain_loss(100.0, 25), 75.0);
        assert!(gain_loss(f32::NAN, 0).is_nan());
    }

    #[test]
    fn bias_ratio_caps_the_lock() {
        assert_eq!(bias_ratio(0.5), 3.0);
        assert_eq!(bias_ratio(2.0), bias_ratio(0.99));
        assert_eq!(bias_ratio(-0.5), 1.0);
    }
}
