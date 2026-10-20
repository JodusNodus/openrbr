//! The camera's start orbit and the go it gives: the camera host in mode 10 swings around the car while the application
//! waits in state 10, and gives the go when the swing ends.
//!
//! Original: when the load ends, `M004801d0` (0x4801d0) sets state 10 and the camera mode 10 (`M487680(10,1)`), which in
//! state 10 sets the swing angle `g_1660da0` to pi (`0x72f414`). Each frame `M004825b0` (0x4825b0) in state 10 runs
//! `CCamera::Tic` (0x48a1f0) with the frame's `facc`, whose first call is `E15CamHost::M8840` (0x488840). In mode 10,
//! state 10 and a game flow in state 1 or 2 (or a replay in its state 1), it takes `facc * 1.2` (`0x72f680`) off the
//! angle; below 0 it sets mode 1 (`M487680(1,1)`) and calls `M0047ca40` (0x47ca40), the go ([`crate::stage::Stage::go`]).
//! The mode-10 builder (`M6530`) turns the chase offset by the angle about the up axis, so the camera starts in front
//! of the car and swings round behind it. The arithmetic is the assembly's (SSE, f32).
//!
//! In the other modes (the championship's 3 to 5, [`crate::flow::Mode::camera_go`]) the angle turns at 0.3 rad/s
//! (`0x72afcc`), wrapped into `[0, 2 pi]` (`0x72f418`), and no go comes: the player's confirm arms the countdown
//! ([`crate::stage::Stage::confirm`]), and when `Tic` starts the driving it sets the player's camera
//! (`E59Host90::M00566390`, 0x566390), which ends the orbit. The wrap after the go is not kept: the mode is 1 from then
//! on, and `M8840` sets the angle to 0 for every mode but 10.

/// The angle when the orbit starts (`0x72f414`, pi).
pub const START_ANGLE: f32 = std::f32::consts::PI;
/// Radians per second of frame time (`0x72f680`).
const RATE: f32 = 1.2;
/// Radians per second of frame time where the orbit gives no go (`0x72afcc`).
const IDLE_RATE: f32 = 0.3;
/// The idle orbit's wrap (`0x72f418`, 2 pi).
const TURN: f32 = std::f32::consts::TAU;

/// The camera's part of the start: the orbit before the go, then the go's mode 1 (the player's view).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StartCamera {
    /// Mode 10, with the swing angle (`g_1660da0`), radians.
    Orbit(f32),
    /// The go has been given.
    Done,
}

impl StartCamera {
    /// `M487680(10,1)` in state 10.
    pub fn start() -> Self {
        StartCamera::Orbit(START_ANGLE)
    }

    /// `M8840`'s mode-10 branch for one frame of `dt` (`facc`) in state 10: the camera after it, and whether it gave
    /// the go. `gives_go` is the mode's ([`crate::flow::Mode::camera_go`]); without it the orbit turns on.
    pub fn tick(self, dt: f32, gives_go: bool) -> (Self, bool) {
        match self {
            StartCamera::Orbit(angle) if gives_go => {
                let angle = angle - dt * RATE;
                if 0.0 > angle {
                    (StartCamera::Done, true)
                } else {
                    (StartCamera::Orbit(angle), false)
                }
            }
            StartCamera::Orbit(angle) => {
                let mut angle = angle - dt * IDLE_RATE;
                while 0.0 > angle {
                    angle += TURN;
                }
                while angle > TURN {
                    angle -= TURN;
                }
                (StartCamera::Orbit(angle), false)
            }
            StartCamera::Done => (self, false),
        }
    }

    /// The swing angle while the orbit runs.
    pub fn angle(self) -> Option<f32> {
        match self {
            StartCamera::Orbit(angle) => Some(angle),
            StartCamera::Done => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frames until the go at a fixed `dt`, counting the frame that gives it.
    fn frames_to_go(dt: f32) -> u32 {
        let mut camera = StartCamera::start();
        for frame in 1..100_000 {
            let (next, go) = camera.tick(dt, true);
            camera = next;
            if go {
                assert_eq!(camera, StartCamera::Done);
                return frame;
            }
        }
        panic!("no go");
    }

    #[test]
    fn the_go_comes_after_pi_over_1_2_seconds() {
        // pi / 1.2 = 2.618 s: 375 frames of 7 ms (the 1.02 `MaxTimeStep`; the f32 sum stays above 0 after 374), 262 of
        // 10 ms, 158 at 60 Hz.
        assert_eq!(frames_to_go(0.007), 375);
        assert_eq!(frames_to_go(0.01), 262);
        assert_eq!(frames_to_go(1.0 / 60.0), 158);
    }

    #[test]
    fn the_angle_drops_by_dt_times_1_2_in_f32() {
        let (camera, go) = StartCamera::start().tick(0.007, true);
        assert!(!go);
        assert_eq!(camera.angle(), Some(std::f32::consts::PI - 0.007f32 * 1.2f32));
    }

    /// The go is given once, and an angle of exactly 0 is not below it.
    #[test]
    fn the_go_is_given_once_below_zero() {
        let (camera, go) = StartCamera::Orbit(0.012).tick(0.01, true);
        assert!(!go);
        assert_eq!(camera.angle(), Some(0.0));
        let (camera, go) = camera.tick(0.0, true);
        assert!(!go && camera.angle() == Some(0.0));
        let (camera, go) = camera.tick(0.001, true);
        assert!(go && camera.angle().is_none());
        assert_eq!(camera.tick(1.0, true), (StartCamera::Done, false));
    }

    /// `M8840` outside modes 1 and 2: the orbit turns at 0.3 rad/s, wraps below 0 to under 2 pi, and gives no go.
    #[test]
    fn m8840_without_the_go_the_orbit_turns_and_wraps() {
        let (camera, go) = StartCamera::start().tick(1.0, false);
        assert!(!go);
        assert_eq!(camera.angle(), Some(std::f32::consts::PI - 0.3));
        let (camera, go) = StartCamera::Orbit(0.1).tick(1.0, false);
        assert!(!go);
        assert_eq!(camera.angle(), Some(0.1 - 0.3 + TURN));
        let mut camera = StartCamera::start();
        for _ in 0..100_000 {
            let (next, go) = camera.tick(0.007, false);
            assert!(!go && next.angle().is_some_and(|a| (0.0..=TURN).contains(&a)));
            camera = next;
        }
    }
}
