//! NGP's drivetrain diffs. For NGP's drivetrain types (4 to 7, the low nibble of the drive definition's
//! `DriveTrain_NGP` word) the diffs' lock torques are set before the wheel worker runs (the original wheel worker,
//! or [`super::axle`] for types 4 and 5), after the downforce ([`super::downforce`]) and the drive gain's loss
//! ([`after_loss`]). The diff types are the word's next three nibbles, one per diff (the state's terms 0 to 2: centre,
//! front, rear): 1 keeps the lock, 2 is open, 3 is locked, 4 is torque-biasing (a ratio from the car's lock values),
//! 5 viscous. All SSE.

use super::drive::split_factor;
use crate::engine_driveline::layout::DriveWorker;

/// The drivetrain type: the word's low nibble (0 above 7).
pub fn drive_type(word: u32) -> u32 {
    match word & 0xf {
        t @ 0..=7 => t,
        _ => 0,
    }
}

/// The diffs' types: the word's nibbles 1 to 3 (0 above 5).
pub fn diff_types(word: u32) -> [u32; 3] {
    [4, 8, 12].map(|shift| match (word >> shift) & 0xf {
        t @ 0..=5 => t,
        _ => 0,
    })
}

/// A positive drive gain after the drivetrain loss of `loss` percent (NGP's setting, unsigned).
pub fn after_loss(gain: f32, loss: u32) -> f32 {
    if gain <= 0.0 {
        gain
    } else {
        (1.0 - loss as f32 / 100.0) * gain
    }
}

/// What the lock torques read beyond the drive worker.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiffTuning {
    /// The VCU's lock values per diff (centre, front, rear), under throttle and under brake: the first knot of each
    /// throttle and brake map.
    pub locks: [[f32; 2]; 3],
    /// The handbrake, and the VCU's `centre_diff_handbrake_release` it is compared with: above it the centre diff
    /// opens.
    pub handbrake: f32,
    pub handbrake_release: f32,
    /// The drive definition's `CenterDiffMaxTorque`, `FrontDiffMaxTorque` and `RearDiffMaxTorque`: the lock's floor.
    pub max_torque: [f32; 3],
    /// The worker object's enable word of each diff: a diff whose word is 0 opens unless it is locked (type 3).
    pub enabled: [u32; 3],
    /// NGP's front/rear drive split ([`super::drive::split_factor`]) and the max torque word the viscous lock is
    /// scaled by.
    pub split: f32,
    pub viscous_gain: f32,
}

const EPS: f32 = f32::EPSILON;
/// The cap of a lock value in the bias ratio.
const LOCK_CAP: f32 = 0.99;
/// The cap of the term in the viscous lock.
const VISCOUS_CAP: f32 = 100.0;

/// `max(a, b)` and `min(a, b)` with `b` when they are unordered (SSE's `maxss`, `minss`).
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
/// are within the ratio, at least 0 and `floor`.
fn biasing(r: f32, a: f32, b: f32, floor: f32) -> f32 {
    let lo = if a > b { b } else { a };
    let hi = if b > a { b } else { a };
    let mut v = (r * lo - hi) / (r + 1.0);
    if lo > 0.0 && hi / lo > r {
        v = 0.0;
    }
    max_ss(floor, max_ss(0.0, v))
}

/// The viscous lock: `sqrt(min(100, |term|)) * (2 gain * lock) + floor`.
fn viscous(term: f32, gain: f32, lock: f32, floor: f32) -> f32 {
    let s = f64::from(min_ss(VISCOUS_CAP, term.abs())).sqrt() as f32;
    s * (gain * 2.0 * lock) + floor
}

/// The sign of a wheel's latch bound: the negated sign bit, 0 for zero.
fn neg_sign(v: f32) -> f32 {
    if v == 0.0 {
        0.0
    } else if v.is_sign_negative() {
        1.0
    } else {
        -1.0
    }
}

/// The lock torques (the state's terms 0 to 2, `scale`) for an NGP drivetrain type, from the drive worker as it is
/// after the downforce and the gain's loss.
pub fn lock_torques(w: &DriveWorker, types: [u32; 3], t: &DiffTuning) -> [f32; 3] {
    let s = &w.state;
    let base = s.drive_gain - s.pack_gain * w.this.loc0.value;
    // Per wheel: the magnitude of its force and of its torque on the diff.
    let mut force = [0.0f32; 4];
    let mut diff_torque = [0.0f32; 4];
    for k in 0..4 {
        let wheel = w.def.wheels[k];
        let axle = w.def.axles[wheel.pack as usize];
        let blk = &s.blocks[k];
        // the wheel's weight, truncated, signed by its axle's direction
        let signed_weight = crate::fpu::trunc_i32(f64::from(wheel.weight)).wrapping_mul(axle.direction);
        let signed = signed_weight as f32;
        let mut clutch = (w.this.wheel_offset[k] - blk.clutch_load * blk.slip.value) * signed;
        let prev_force = signed * blk.prev_force.value;
        force[k] = prev_force.abs();
        let drive = split_factor(axle.scale * 0.5 * base, k, t.split);
        let latched = blk.prev_gate.value.wrapping_mul(signed_weight);
        let direction = if latched != 0 {
            latched.wrapping_neg() as f32
        } else if prev_force == 0.0 {
            neg_sign(drive + clutch)
        } else {
            neg_sign(prev_force)
        };
        let mut bound = direction * blk.latch_bound;
        if w.obj.wheel_enabled[k] == 0 {
            clutch = (signed_weight.wrapping_neg() as f32) * blk.slip.value * blk.clutch_load;
            bound = 0.0;
        }
        diff_torque[k] = (bound + clutch).abs();
    }

    let gain = s.drive_gain;
    let ratio: [f32; 3] = std::array::from_fn(|g| {
        let [on, off] = t.locks[g];
        bias_ratio(if gain >= 0.0 { on } else { off })
    });
    let lock = |g: usize, kind: u32, x: f32, y: f32, kept: f32, scale: &[f32; 3]| match kind {
        3 => x + y,
        4 => biasing(ratio[g], x, y, t.max_torque[g]),
        1 => scale[g],
        5 => viscous(s.terms[g].term, t.viscous_gain, t.locks[g][0], t.max_torque[g]),
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
        let mut v = lock(g, types[g], diff_torque[k0], diff_torque[k1], axle_lock[g], &scale);
        if EPS > force[k0] && EPS > force[k1] {
            v = 0.0;
        }
        axle_lock[g] = v;
        scale[g] = v;
    }

    let mut v =
        lock(0, types[0], diff_torque[0] + diff_torque[1], diff_torque[2] + diff_torque[3], axle_lock[0], &scale);
    if force.iter().all(|x| *x < EPS) {
        v = 0.0;
    }
    scale[0] = v;
    if t.handbrake > t.handbrake_release {
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
        assert_eq!(after_loss(-1.0, 50), -1.0);
        assert_eq!(after_loss(0.0, 50), 0.0);
        assert_eq!(after_loss(100.0, 25), 75.0);
        assert!(after_loss(f32::NAN, 0).is_nan());
    }

    #[test]
    fn bias_ratio_caps_the_lock() {
        assert_eq!(bias_ratio(0.5), 3.0);
        assert_eq!(bias_ratio(2.0), bias_ratio(0.99));
        assert_eq!(bias_ratio(-0.5), 1.0);
    }
}
