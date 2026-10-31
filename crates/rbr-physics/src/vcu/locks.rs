//! Diff locks. A lock is the largest of a throttle map, a brake map and a speed map at the current speed.
//! The three results are stored as f32 before the compare, so the port works in f32 throughout.

use super::layout::{Def, Knot, Locks, Pedals, SpeedKnot};
use super::tables::{lookup_pair_table, table_blend};

/// The largest of the throttle and brake lock and the speed lock, with ties broken toward the throttle,
/// then the brake. The comparisons are ordered, so a NaN never wins.
pub fn lock_max(pedals: Pedals, throttle_map: &[Knot], brake_map: &[Knot], speed_map: &[SpeedKnot], speed: f32) -> f32 {
    let r1 = table_blend(throttle_map, pedals.throttle, 0.0, 1.0) as f32;
    let r2 = table_blend(brake_map, pedals.brake, 0.0, 1.0) as f32;
    let r3 = lookup_pair_table(speed_map, speed) as f32;
    if r1 >= r2 && r1 >= r3 {
        r1
    } else if r2 >= r1 && r2 >= r3 {
        r2
    } else {
        r3
    }
}

/// The diff locks for the drive's channels 0 (centre), 1 (front) and 2 (rear), as the values the
/// drive clamps to [0, 1] itself. `drive_speed` is the drive's speed; the speed key is that times 0.3.
/// Front and rear use the same key.
///
/// The centre lock is 0 while the handbrake is at or above the release value. Otherwise the
/// left-foot maps are used when the threshold is nonzero and both pedals are above it. A NaN
/// threshold takes the normal maps.
pub fn diff_locks(pedals: Pedals, def: &Def, drive_speed: f32) -> [f32; 3] {
    let speed = drive_speed * 0.3;
    let lft = def.left_foot_brake_threshold;
    let centre = if !(def.centre_diff_handbrake_release > pedals.handbrake) {
        0.0
    } else if lft == 0.0 || !(pedals.brake > lft) || !(pedals.throttle > lft) {
        lock_max(pedals, &def.centre_throttle_lock, &def.centre_brake_lock, &def.centre_speed_lock, speed)
    } else {
        lock_max(pedals, &def.lf_centre_throttle_lock, &def.lf_centre_brake_lock, &def.lf_centre_speed_lock, speed)
    };
    let front = lock_max(pedals, &def.front_throttle_lock, &def.front_brake_lock, &def.front_speed_lock, speed);
    let rear = lock_max(pedals, &def.rear_throttle_lock, &def.rear_brake_lock, &def.rear_speed_lock, speed);
    [centre, front, rear]
}

impl Locks {
    /// Stores the lock that `lock_output` gives for the value on a channel.
    pub fn set(&mut self, channel: usize, value: f32) {
        self.torque[channel] = lock_output(value, self.ratio[channel], self.enabled, self.max_torque[channel]);
    }
}

/// The lock the drive stores for a value. Without the gate it is 0. With it, the
/// value is clamped to [0, 1] and cut by the channel's ratio band: |ratio| below 1 gives 0, from 1 to 2 gives
/// (|ratio| - 1) times the value, and 2 or more keeps the value. The maximum torque multiplies last.
pub fn lock_output(value: f32, ratio: f32, enabled: bool, max_torque: f32) -> f32 {
    if !enabled {
        return 0.0;
    }
    let value = if 0.0 > value {
        0.0
    } else if value > 1.0 {
        1.0
    } else {
        value
    };
    let x = if ratio > 0.0 { ratio } else { 0.0 - ratio };
    let value = if 2.0 > x {
        if 1.0 > x {
            0.0
        } else {
            (x - 1.0) * value
        }
    } else {
        value
    };
    max_torque * value
}

#[cfg(test)]
mod tests {
    use super::super::layout::KNOTS;
    use super::*;
    use crate::fpu::Precision;

    /// Uniform maps all at `value` (the lookup of a flat map is that value).
    fn uniform(value: f32) -> Vec<Knot> {
        vec![Knot { value }; KNOTS]
    }

    /// Speed maps at `value`, keys `k`.
    fn speed(value: f32) -> Vec<SpeedKnot> {
        (0..KNOTS).map(|k| SpeedKnot { key: k as f32, value }).collect()
    }

    /// A setup with the normal centre maps at 0.25, left-foot centre maps at 0.75 and front and rear
    /// at 0.5. At speed 0 each lock is its map's value, so the branch shows in the centre lock.
    fn def(lft: f32, release: f32) -> Def {
        Def {
            gear_upshift_rpm: [0.0; 10],
            gear_downshift_rpm: [0.0; 10],
            shift_lockout_up_s: 0.0,
            shift_lockout_down_s: 0.0,
            clutch_ramp_rate: 0.0,
            rpm_limit: 0.0,
            centre_diff_handbrake_release: release,
            left_foot_brake_threshold: lft,
            gear_count: 0,
            centre_throttle_lock: uniform(0.25),
            centre_brake_lock: uniform(0.25),
            centre_speed_lock: speed(0.25),
            lf_centre_throttle_lock: uniform(0.75),
            lf_centre_brake_lock: uniform(0.75),
            lf_centre_speed_lock: speed(0.75),
            front_throttle_lock: uniform(0.5),
            front_brake_lock: uniform(0.5),
            front_speed_lock: speed(0.5),
            rear_throttle_lock: uniform(0.5),
            rear_brake_lock: uniform(0.5),
            rear_speed_lock: speed(0.5),
        }
    }

    fn pedals(throttle: f32, brake: f32, handbrake: f32) -> Pedals {
        Pedals { throttle, brake, handbrake }
    }

    #[test]
    fn lock_max_ties_and_nan() {
        let _pc24 = Precision::Pc24.scope();
        // Ties go to the throttle, then the brake.
        assert_eq!(lock_max(pedals(0.5, 0.5, 0.0), &uniform(0.5), &uniform(0.5), &speed(0.5), 0.0), 0.5);
        assert_eq!(lock_max(pedals(0.2, 0.7, 0.0), &uniform(0.2), &uniform(0.7), &speed(0.7), 0.0), 0.7);
        assert_eq!(lock_max(pedals(0.2, 0.7, 0.0), &uniform(0.2), &uniform(0.7), &speed(0.9), 0.0), 0.9);
        // A NaN throttle lock never wins: r1 fails both compares, then r2 is checked against r3.
        assert_eq!(lock_max(pedals(0.0, 0.0, 0.0), &uniform(f32::NAN), &uniform(0.3), &speed(0.1), 0.0), 0.1);
    }

    #[test]
    fn diff_locks_branches() {
        let _pc24 = Precision::Pc24.scope();
        let p = pedals(0.8, 0.9, 1.0);
        // Handbrake at the release value: centre 0.
        assert_eq!(diff_locks(p, &def(0.5, 1.0), 0.0)[0], 0.0);
        // Left-foot maps: threshold set and both pedals above it.
        let p = pedals(0.8, 0.9, 0.0);
        assert_eq!(diff_locks(p, &def(0.5, 1.0), 0.0)[0], 0.75);
        // Threshold zero, or a pedal not above it: the normal maps.
        assert_eq!(diff_locks(p, &def(0.0, 1.0), 0.0)[0], 0.25);
        assert_eq!(diff_locks(pedals(0.4, 0.9, 0.0), &def(0.5, 1.0), 0.0)[0], 0.25);
        // A NaN threshold takes the normal maps.
        assert_eq!(diff_locks(p, &def(f32::NAN, 1.0), 0.0)[0], 0.25);
        // Front and rear do not depend on the centre branch.
        let [_, front, rear] = diff_locks(p, &def(0.5, 1.0), 0.0);
        assert_eq!((front, rear), (0.5, 0.5));
    }
}
