//! The steering wheel's force-feedback command (`notes/ngp.md` "Force feedback"): the force the physics gives (`FfbNorm` in 1.02,
//! NGP's curve under NGP, in [-1, 1]) scaled by the player's settings into the device magnitude, -10000 to 10000.
//!
//! 1.02: `E28_GetFFBVal` (0x4c5f90) multiplies by `STRENGTH` and negates when `DIRECTION` is `INVERSE`;
//! `USInputManagerImpl::Tic` (0x4cbf50) sends `_ftol(x * -10000)` (truncated) and `SetFFBValue` (0x4c6b40) clamps.
//! NGP sends `floor(-10000 * x)` from its FFB thread (`0x1003c390`) and zeroes a small force while the wheel is near
//! the centre (`ffbDeadZone`, `ffbDeadZoneThreshold`, `0x10064620`).
//!
//! The magnitude's sign is DirectInput's: a positive magnitude with the force's direction 0 on the steering axis.
//! The force is the self-aligning torque, opposite to the steering (`T` against the payload's steering in the
//! reference drives), so the magnitude has the steering's sign.

use crate::stage::State;

/// The player's force-feedback settings: the game's options menu (`FRCFEEDBK`, `STRENGTH`, `DIRECTION`) and NGP's
/// dead zone (`RichardBurnsRally.ini` `[NGP]`; 0 disables it, NGP's default).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    /// 0 to 1 (`0x1660ed8`).
    pub strength: f32,
    /// `DIRECTION` `INVERSE` (0 at `0x1660edc`): the force is negated.
    pub inverse: bool,
    /// The steering input below which a small force is zeroed (NGP `ffbDeadZone`, a fraction of half the lock).
    pub dead_zone: f32,
    /// The force, as a fraction of the maximum, below which it is zeroed in the dead zone (`ffbDeadZoneThreshold`).
    pub dead_zone_threshold: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { enabled: true, strength: 1.0, inverse: false, dead_zone: 0.0, dead_zone_threshold: 0.0 }
    }
}

/// The largest magnitude `SetFFBValue` sends.
pub const MAX_MAGNITUDE: i32 = 10_000;

/// The magnitude for `force` (1.02 truncates `x * -10000`, NGP floors it), clamped to [`MAX_MAGNITUDE`]; 0 with
/// force feedback off. NaN gives 0 (`_ftol` would give `i32::MIN`, which the clamp turns into -10000; a NaN force
/// is the physics' fault, not a command).
pub fn magnitude(force: f32, settings: &Settings, ngp: bool) -> i32 {
    if !settings.enabled || force.is_nan() {
        return 0;
    }
    let mut x = force * settings.strength;
    if settings.inverse {
        x = -x;
    }
    let scaled = x * -(MAX_MAGNITUDE as f32);
    let m = if ngp { scaled.floor() } else { scaled.trunc() };
    (m as i32).clamp(-MAX_MAGNITUDE, MAX_MAGNITUDE)
}

/// NGP's dead zone (`0x10064620`): 0 while the steering input is inside `dead_zone` and the magnitude is below the
/// threshold (stored as `int(threshold * 10000)`), else `m`. With `dead_zone` 0 nothing is inside.
pub fn dead_zone(m: i32, steering: f32, settings: &Settings) -> i32 {
    let threshold = (settings.dead_zone_threshold * MAX_MAGNITUDE as f32) as i32;
    if steering.abs() < settings.dead_zone && m.abs() < threshold {
        0
    } else {
        m
    }
}

/// The game's racing gate (the exe's `0x5403e0` case 0xb, which NGP 7.4.778 calls at `0x1003c3eb`): the force goes
/// out only in state 1 (driving), with force feedback on (`0x1660ed4`) and `[0x167ea70]`'s slot 0 not running or past
/// its end (`0x5fe390(0)`); else 0. The other way the case holds, `[[0x8951d0] + 4]` set, is not traced.
///
/// NGP's FFB thread sends the force EmitForce stored last, which no attempt resets: the gate opens inside the first
/// driving StepForward, before its EmitForce, so the first step of an attempt sends the previous attempt's last force
/// (`notes/ngp.md` "Force feedback"). 1.02 sends from EmitForce itself, so its first step sends its own force.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gate {
    /// The force of the last driving step (NGP's stored `x`), carried over from the previous attempt at first.
    stored: f32,
    open: bool,
}

impl Gate {
    /// An attempt's gate, closed, with `carried` the previous attempt's last force (0 for the first).
    pub fn new(carried: f32) -> Gate {
        Gate { stored: carried, open: false }
    }

    /// The force the wheel gets after a step that left the car's force at `force` and the application in `state`.
    pub fn step(&mut self, force: f32, state: State, ngp: bool) -> f32 {
        let open = state == State::Driving;
        let sent = match (open, self.open) {
            (false, _) => 0.0,
            (true, false) if ngp => self.stored,
            (true, _) => force,
        };
        if open {
            self.stored = force;
        }
        self.open = open;
        sent
    }

    /// The force the next attempt starts with ([`Gate::new`]).
    pub fn stored(&self) -> f32 {
        self.stored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gate_sends_0_outside_the_driving_and_ngp_s_carried_force_first() {
        let mut gate = Gate::new(0.25);
        assert_eq!(gate.step(0.5, State::Countdown, true), 0.0);
        assert_eq!(gate.step(0.5, State::Driving, true), 0.25);
        assert_eq!(gate.step(0.75, State::Driving, true), 0.75);
        assert_eq!(gate.step(0.125, State::Ended, true), 0.0);
        assert_eq!(gate.stored(), 0.75);
        let mut old = Gate::new(0.25);
        assert_eq!((old.step(0.5, State::Driving, false), old.step(0.75, State::Driving, false)), (0.5, 0.75));
    }

    #[test]
    fn scales_negates_and_rounds_like_each_variant() {
        let s = Settings::default();
        assert_eq!(magnitude(0.5, &s, false), -5000);
        assert_eq!(magnitude(-1.0, &s, false), 10_000);
        // A fraction: 1.02 truncates toward 0, NGP floors.
        assert_eq!(magnitude(0.12345, &s, false), -1234);
        assert_eq!(magnitude(0.12345, &s, true), -1235);
        let weak = Settings { strength: 0.5, inverse: true, ..s };
        assert_eq!(magnitude(0.5, &weak, false), 2500);
        assert_eq!(magnitude(0.5, &Settings { enabled: false, ..s }, false), 0);
        assert_eq!(magnitude(f32::NAN, &s, true), 0);
        assert_eq!(magnitude(3.0, &s, false), -10_000);
    }

    #[test]
    fn dead_zone_needs_both_a_centred_wheel_and_a_small_force() {
        let s = Settings { dead_zone: 0.04, dead_zone_threshold: 0.1, ..Settings::default() };
        assert_eq!(dead_zone(900, 0.01, &s), 0);
        assert_eq!(dead_zone(1000, 0.01, &s), 1000);
        assert_eq!(dead_zone(900, 0.05, &s), 900);
        assert_eq!(dead_zone(900, 0.01, &Settings::default()), 900);
    }
}
