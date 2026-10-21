//! The passenger's motion: the springs the passenger's offset follows the car with, the pull towards rest and the
//! decoding of a time of day. The objects and the calls around them are in `layout`.

use super::math::Vec4;
use crate::math::ftol::ftol2;

/// One lane of the passenger's spring (named by use): `rate` and `offset` are the lane's two state words, which relax
/// towards `rate_target` and `offset_target`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringLane {
    pub rate: f32,
    pub rate_target: f32,
    pub offset: f32,
    pub offset_target: f32,
}

/// The spring's gains: on the rate error, on the offset error and on the passenger's velocity.
const SPRING_RATE_GAIN: f32 = 900.0;
const SPRING_OFFSET_GAIN: f32 = 30000.0;
const SPRING_VELOCITY_GAIN: f32 = 10.0;
/// The factor of the spring force on the rate (`-1 / 10`: the force is divided by a mass of 10, with the sign that
/// restores).
const SPRING_FORCE_SCALE: f32 = -0.1;

/// The spring of the three lanes, for the time step `dt` (s): with `k = dt * SPRING_FORCE_SCALE`, a lane's force is
/// `f = 900 * (rate - rate_target) + 30000 * (offset - offset_target) + 10 * velocity`, and the new rate and offset are
/// `rate + f * k` and `offset + rate * dt` (the new rate). Returns the new `(rate, offset)`.
pub fn spring_step(lane: SpringLane, velocity: f32, dt: f32) -> (f32, f32) {
    let k = dt * SPRING_FORCE_SCALE;
    let f = SPRING_RATE_GAIN * (lane.rate - lane.rate_target) + SPRING_OFFSET_GAIN * (lane.offset - lane.offset_target);
    let f = SPRING_VELOCITY_GAIN * velocity + f;
    let rate = f * k + lane.rate;
    let offset = rate * dt + lane.offset;
    (rate, offset)
}

/// A time of day split into its parts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClockTime {
    /// 1 for a time of 0 or above, else -1.
    pub sign: i32,
    pub days: u32,
    pub hours: u32,
    pub minutes: u32,
    /// The seconds, with the fraction of the time.
    pub seconds: f32,
}

const SECS_PER_DAY: u32 = 86400;
const SECS_PER_HOUR: u32 = 3600;
const SECS_PER_MINUTE: u32 = 60;

/// The sign of `time` (s), and its magnitude split into days, hours, minutes and, with the fraction, seconds.
pub fn decode_time(time: f64) -> ClockTime {
    let (sign, magnitude) = if 0.0 <= time { (1, time) } else { (-1i32, -time) };
    let whole = ftol2(magnitude) as u32;
    let fraction = (magnitude - whole as f64) as f32;
    let day_rest = whole % SECS_PER_DAY;
    let hour_rest = day_rest % SECS_PER_HOUR;
    let seconds = (hour_rest % SECS_PER_MINUTE) as f32;
    ClockTime {
        sign,
        days: whole / SECS_PER_DAY,
        hours: day_rest / SECS_PER_HOUR,
        minutes: hour_rest / SECS_PER_MINUTE,
        seconds: fraction + seconds,
    }
}

/// The dead zone of the pull, per lane (x, y, z): the offset is pulled back by the part of it past `DEAD_POS` above
/// zero and `DEAD_NEG` below.
const DEAD_POS: [f32; 3] = [0.005, 0.01, 0.01];
const DEAD_NEG: [f32; 3] = [-0.005, -0.02, -0.01];
/// The gains of the pull (named by use): on the rate and on the dead-banded offset (the pull), then on the offset and on
/// the rate (the spring).
const PULL_RATE_GAIN: [f32; 3] = [800.0, 900.0, 900.0];
const PULL_SLACK_GAIN: [f32; 3] = [20000.0, 60000.0, 50000.0];
const SPRING_OFFSET: [f32; 3] = [20000.0, 35000.0, 35000.0];
const SPRING_RATE: [f32; 3] = [300.0, 500.0, 400.0];
/// The passenger's velocity gain, and the factor `dt * RATE_SCALE` that moves the rate by the work vector.
const VELOCITY_GAIN: f32 = 30.0;
const RATE_SCALE: f32 = -1.0 / 30.0;

/// The dead band of one lane: the part of `h` past its zone on its own side of zero, 0 inside the zone.
fn dead_band(h: f32, pos: f32, neg: f32) -> f32 {
    if h >= 0.0 {
        let t = h - pos;
        if t > 0.0 {
            t
        } else {
            0.0
        }
    } else {
        let t = h - neg;
        if t < 0.0 {
            t
        } else {
            0.0
        }
    }
}

/// -1, 0 or 1: the sign of `x`.
fn sign(x: f32) -> i32 {
    i32::from(x > 0.0) - i32::from(x < 0.0)
}

fn lanes(v: Vec4) -> [f32; 4] {
    [v.x, v.y, v.z, v.w]
}

fn vec4(l: [f32; 4]) -> Vec4 {
    Vec4 { x: l[0], y: l[1], z: l[2], w: l[3] }
}

/// The pull of the passenger's `rate` and `offset` vectors towards rest, given the passenger's `velocity` and `dt`
/// (s). Lanes 0-2 as the original. The work vector's lane 3 is 0: the original's is stack garbage (`harness/diverge.py`
/// masks the lane-3 words it reaches). Returns the new `(rate, offset)`.
pub fn pull_to_rest(rate: Vec4, offset: Vec4, velocity: Vec4, dt: f32) -> (Vec4, Vec4) {
    let (rate, offset, velocity) = (lanes(rate), lanes(offset), lanes(velocity));
    let mut work = [0.0f32; 4];
    for i in 0..3 {
        let slack = dead_band(offset[i], DEAD_POS[i], DEAD_NEG[i]);
        let pull = PULL_RATE_GAIN[i] * rate[i] + PULL_SLACK_GAIN[i] * slack;
        let pull = if sign(pull) == sign(slack) { pull } else { 0.0 };
        let spring = (pull + SPRING_OFFSET[i] * offset[i]) + SPRING_RATE[i] * rate[i];
        work[i] = velocity[i] * VELOCITY_GAIN + spring;
    }
    let scale = dt * RATE_SCALE;
    let rate: [f32; 4] = std::array::from_fn(|i| work[i] * scale + rate[i]);
    let offset: [f32; 4] = std::array::from_fn(|i| rate[i] * dt + offset[i]);
    (vec4(rate), vec4(offset))
}

/// The point the passenger's step walks: the gravity of the physics object plus the host's offset, lane by lane.
pub fn walk_point(gravity: [f32; 3], offset: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| gravity[i] + offset[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_band_keeps_only_the_part_past_its_zone() {
        assert_eq!(dead_band(0.003, 0.005, -0.005), 0.0);
        assert_eq!(dead_band(-0.003, 0.005, -0.005), 0.0);
        assert!((dead_band(0.1, 0.005, -0.005) - 0.095).abs() < 1e-6);
        assert!((dead_band(-0.1, 0.005, -0.005) + 0.095).abs() < 1e-6);
    }

    #[test]
    fn decode_time_splits_days_hours_minutes_and_seconds() {
        let t = decode_time(86400.0 + 2.0 * 3600.0 + 3.0 * 60.0 + 4.5);
        assert_eq!((t.sign, t.days, t.hours, t.minutes, t.seconds), (1, 1, 2, 3, 4.5));
        assert_eq!(decode_time(-61.0).sign, -1);
    }
}
