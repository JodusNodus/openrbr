//! The in-car view's head, ours: a steadier horizon on top of the game's seat shake ([`crate::views::Mount::shaken`],
//! which already moves the eye with the seat spring). It only tilts the view about the eye, from the frames' car poses
//! on the physics clock, so it is the same at any frame rate, in a replay and in a video. The view's up follows the
//! body's up through a lag of [`STEADY_SECONDS`], [`STEADY`] of the way, so the fast pitch and roll (bumps, landings,
//! kerbs) are mostly taken out and the slow ones (a hill, a banked road, the lean through a long corner) kept; then the
//! roll is levelled [`LEVEL`] of the way to the horizon. Together at most [`TILT_MAX`]. It never turns the view
//! sideways: a look into corners and slides was tried and dropped (user, 2026-11-01: it made them sick).
//!
//! A gap in the frames (a seek, a restart, the first frame shown) settles it over the [`SETTLE_SECONDS`] before; a
//! jump of the car (the recovery) starts it again from the car. Off (`[camera] head = off`,
//! [`crate::camera_prefs`]) the view is the car-fixed one.

use crate::camera::Lens;
use crate::coords::physics_to_stage;
use crate::playback::car_to_stage;
use glam::{Quat, Vec3};
use rbr_recording::pose::CarPose;

/// The share of the body's pitch and roll against its lagged up taken out.
pub const STEADY: f32 = 0.6;
/// The lag of the up the view keeps, s.
pub const STEADY_SECONDS: f32 = 0.25;
/// The share of the remaining roll levelled to the horizon.
pub const LEVEL: f32 = 0.15;
/// The most the view tilts from the body, degrees.
pub const TILT_MAX: f32 = 6.0;
/// A fresh head is settled over the frames this long before, s.
pub const SETTLE_SECONDS: f32 = 2.0;
/// A car moving this much more than its speed takes it within a frame was placed (the recovery), m.
const JUMP_METRES: f32 = 2.0;

/// The car body's frame, stage axes.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Body {
    position: Vec3,
    up: Vec3,
    forward: Vec3,
    velocity: Vec3,
}

impl Body {
    fn of(car: &CarPose) -> Body {
        let to_stage = car_to_stage(car);
        Body {
            position: to_stage.transform_point3(Vec3::ZERO),
            up: to_stage.transform_vector3(Vec3::Z).normalize(),
            forward: to_stage.transform_vector3(Vec3::NEG_Y).normalize(),
            velocity: physics_to_stage().transform_vector3(Vec3::from(car.velocity)),
        }
    }
}

/// The head following a drive's frames.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Head {
    on: bool,
    /// The body's up through the lag, stage axes.
    up: Vec3,
    /// The frame last followed and its step counter, `None` after a reset.
    last: Option<(usize, u32)>,
    /// The body at the frame last followed.
    body: Option<Body>,
}

impl Head {
    pub fn new(on: bool) -> Head {
        Head { on, ..Head::default() }
    }

    /// Turns the head on or off; either way it starts again from the car.
    pub fn set_on(&mut self, on: bool) {
        if on != self.on {
            *self = Head::new(on);
        }
    }

    /// Moves the head to frame `index` of a drive whose frame `i` is `frame(i)`: its step counter, which counts physics
    /// steps of `step` s, and the car's pose.
    pub fn follow(&mut self, frame: impl Fn(usize) -> (u32, CarPose), index: usize, step: f32) {
        if !self.on {
            return;
        }
        let now = frame(index).0;
        let settle = (SETTLE_SECONDS / step) as u32;
        let first = match self.last {
            Some((i, then)) if i <= index && then <= now && now - then <= settle => i + 1,
            _ => {
                self.body = None;
                (0..index).rev().take_while(|&i| frame(i).0.saturating_add(settle) >= now).last().unwrap_or(index)
            }
        };
        let mut then = first.checked_sub(1).map(|i| frame(i).0);
        for i in first..=index {
            let (steps, car) = frame(i);
            let body = Body::of(&car);
            match self.body.zip(then) {
                Some((before, then)) => self.tic(before, body, steps.saturating_sub(then) as f32 * step),
                None => self.start(body),
            }
            then = Some(steps);
        }
        self.last = Some((index, now));
    }

    /// Starts again from `body`, level with it.
    fn start(&mut self, body: Body) {
        (self.up, self.body) = (body.up, Some(body));
    }

    /// One frame of `dt` s from `before` to `body`; nothing when no step ran.
    fn tic(&mut self, before: Body, body: Body, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        if (body.position - before.position).length() > JUMP_METRES + before.velocity.length() * dt {
            return self.start(body);
        }
        let share = 1.0 - (-dt / STEADY_SECONDS).exp();
        self.up = self.up.lerp(body.up, share).normalize_or(body.up);
        self.body = Some(body);
    }

    /// `lens` (the in-car view, fixed to the car) turned by the head.
    pub fn turned(&self, lens: Lens) -> Lens {
        match self.body.filter(|_| self.on) {
            Some(body) => {
                let rotation = self.rotation(body);
                Lens { forward: rotation * lens.forward, up: rotation * lens.up, ..lens }
            }
            None => lens,
        }
    }

    /// The head's tilt from the body, stage axes.
    fn rotation(&self, body: Body) -> Quat {
        let steady = Quat::from_rotation_arc(body.up, body.up.lerp(self.up, STEADY).normalize_or(body.up));
        let forward = steady * body.forward;
        let up = steady * body.up;
        let roll = signed_angle(up, Vec3::Y.reject_from(forward).normalize_or(up), forward);
        let tilt = Quat::from_axis_angle(forward, roll * LEVEL) * steady;
        // The shorter way round, so the angle is at most pi.
        let (axis, angle) = if tilt.w < 0.0 { -tilt } else { tilt }.to_axis_angle();
        Quat::from_axis_angle(axis, angle.min(TILT_MAX.to_radians()))
    }
}

/// The angle from `from` to `to` about `axis`, rad: turning `from` by it about `axis` brings it towards `to`.
fn signed_angle(from: Vec3, to: Vec3, axis: Vec3) -> f32 {
    from.cross(to).dot(axis).atan2(from.dot(to))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEP: f32 = 0.007;

    /// A car at `position` (physics axes), turned `yaw` about up and `roll` about its forward, moving at `velocity`.
    fn car(position: Vec3, yaw: f32, roll: f32, velocity: Vec3) -> CarPose {
        let q = Quat::from_rotation_z(yaw) * Quat::from_rotation_y(roll);
        CarPose { position: position.into(), orientation: q.into(), velocity: velocity.into() }
    }

    /// A drive of `n` frames of `steps` steps each, the car at `pose(t)` at time `t`, s.
    fn drive(n: usize, steps: u32, pose: impl Fn(f32) -> CarPose) -> Vec<(u32, CarPose)> {
        (0..n as u32).map(|i| (i * steps, pose((i * steps) as f32 * STEP))).collect()
    }

    fn lens() -> Lens {
        Lens { eye: Vec3::ZERO, forward: Vec3::Z, up: Vec3::Y, fov: 1.0, near: 0.1 }
    }

    /// The head at the last frame of `samples`, followed frame by frame.
    fn followed(samples: &[(u32, CarPose)]) -> Head {
        let mut head = Head::new(true);
        for i in 0..samples.len() {
            head.follow(|i| samples[i], i, STEP);
        }
        head
    }

    fn degrees(a: Vec3, b: Vec3) -> f32 {
        a.angle_between(b).to_degrees()
    }

    #[test]
    fn a_still_level_car_leaves_the_view_alone() {
        let samples = drive(100, 2, |_| car(Vec3::ZERO, 0.3, 0.0, Vec3::ZERO));
        let head = followed(&samples);
        let turned = head.turned(lens());
        assert!(degrees(turned.forward, Vec3::Z) < 1e-3 && degrees(turned.up, Vec3::Y) < 1e-3);
        // Off, the view is the car's.
        assert_eq!(Head::new(false).turned(lens()), lens());
    }

    #[test]
    fn a_sudden_roll_is_mostly_taken_out_then_kept() {
        let roll = 5f32.to_radians();
        let samples = drive(400, 2, |t| car(Vec3::ZERO, 0.0, if t > 1.0 { roll } else { 0.0 }, Vec3::ZERO));
        let up = |head: &Head| head.turned(Lens { up: head.body.unwrap().up, ..lens() }).up;
        // Just after the roll the view stays near level: STEADY of it is taken out, and LEVEL of the rest.
        let just_after = samples.iter().position(|s| s.0 as f32 * STEP > 1.0).unwrap();
        let head = followed(&samples[..=just_after]);
        let left = degrees(up(&head), Vec3::Y);
        assert!(left < 5.0 * (1.0 - STEADY), "{left}");
        // Well after, the view rolls with the car again, less the levelling.
        let head = followed(&samples);
        let left = degrees(up(&head), Vec3::Y);
        assert!((left - 5.0 * (1.0 - LEVEL)).abs() < 0.05, "{left}");
        // A roll over is bounded.
        let over = drive(100, 2, |t| car(Vec3::ZERO, 0.0, t * 6.0, Vec3::ZERO));
        let head = followed(&over);
        let tilt = degrees(up(&head), head.body.unwrap().up);
        assert!(tilt <= TILT_MAX + 1e-3, "{tilt}");
    }

    #[test]
    fn a_turn_on_the_flat_does_not_turn_the_view() {
        let pose = |t: f32| {
            let yaw = 0.4 * t;
            car(Vec3::ZERO, yaw, 0.0, Vec3::new(yaw.sin(), -yaw.cos(), 0.0) * 20.0)
        };
        let head = followed(&drive(300, 2, pose));
        let body = head.body.unwrap();
        assert!(degrees(head.rotation(body) * body.forward, body.forward) < 1e-3);
    }

    #[test]
    fn the_frame_rate_does_not_change_the_head() {
        let pose = |t: f32| {
            let yaw = 0.5 * (t * 1.3).sin();
            let nose = Vec3::new(yaw.sin(), -yaw.cos(), 0.0);
            car(Vec3::ZERO, yaw, 0.08 * (t * 7.0).sin(), nose * 25.0)
        };
        // 2.4 s of 1-step frames against 3-step frames, ending on the same step.
        let fine = followed(&drive(1027, 1, pose));
        let coarse = followed(&drive(343, 3, pose));
        assert_eq!(fine.last.unwrap().1, coarse.last.unwrap().1);
        let (a, b) = (fine.turned(lens()), coarse.turned(lens()));
        assert!(degrees(a.forward, b.forward) < 0.1 && degrees(a.up, b.up) < 0.1, "{a:?} {b:?}");
    }

    #[test]
    fn a_seek_settles_as_watching_did_and_a_placed_car_starts_again() {
        let pose = |t: f32| car(Vec3::ZERO, 0.4 * t, 0.05 * (t * 5.0).sin(), Vec3::new(0.0, -15.0, 0.0));
        let samples = drive(1000, 2, pose);
        let watched = followed(&samples);
        let mut seeked = Head::new(true);
        seeked.follow(|i| samples[i], 999, STEP);
        let (a, b) = (watched.turned(lens()), seeked.turned(lens()));
        assert!(degrees(a.forward, b.forward) < 0.01 && degrees(a.up, b.up) < 0.01);
        // Back: settled again from the frames before.
        seeked.follow(|i| samples[i], 500, STEP);
        assert_eq!(seeked.last, Some((500, samples[500].0)));
        // The car placed 10 m on, turned and level: the head is the car's.
        let mut placed = samples[..=600].to_vec();
        placed.push((1202, car(Vec3::new(10.0, 0.0, 0.0), 2.0, 0.0, Vec3::ZERO)));
        let head = followed(&placed);
        assert_eq!(head.up, head.body.unwrap().up);
    }
}
