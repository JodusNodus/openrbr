//! The suspension sounds of `CCarSound`'s damper handler (0x4ae260, read in the disassembly) from each wheel's damper
//! signal words (`rbr_recording::sound`, `dampers`): squeaks (`CAR::SQUEEKS`, `CCarSound::Vf5c` 0x4ac2c0 and
//! `CTireSnd::M7700` 0x4d7700), bump-stop knocks as ranged impacts at the wheel (`CCollisionSnd::Vf64` 0x4b2ed0),
//! the bottoming (`CAR::SUSPENSION`, `M77c0` 0x4d77c0), and the broken-damper call. Pure. rbr-recomp `notes/sound.md` 6.3.
//!
//! The handler runs on each damper emit, which the wheel's frame finish (`CWheel::Vf28` 0x53f230) makes once a frame
//! before it resets the words (`CSpringDamper::ResetOutputs` 0x52e010). The live picture runs that reset once an
//! answer, so the words here are the frame's emitted payload: the peak its maximum strut velocity (as the damper step
//! keeps its `peak_velocity`), the bump its last bump-stop hit. This model still keeps the frame's words itself over
//! the answers of a frame: the peak the largest seen, the bump the last nonzero one, 0 without one. Ours: the frame
//! is 1/60 s (the original's follows its frame rate).

use rbr_recording::sound::Sound;

use crate::impact::ranged_index;
use crate::voice::{Command, Shot};

/// The damper words: [2] bump velocity, [4] valid (0 once the damper is broken).
const BUMP: usize = 2;
const VALID: usize = 4;
/// A peak change of 1 (0x795e14) or more squeaks in the car, one time in ten (`rand() % 10 == 0`); the squeak plays at
/// half pitch (0x3f000000).
const SQUEAK_CHANGE: f32 = 1.0;
const SQUEAK_ODDS: u32 = 10;
const SQUEAK_PITCH: f32 = 0.5;
/// In the car a bump above 0.01 (0x72f440) knocks: a ranged impact of power `(bump + 1) * 200` (0x795e14, 0x730890);
/// above 0.8 (0x72f610) the bottoming plays too.
const KNOCK: f32 = 0.01;
const KNOCK_POWER: f32 = 200.0;
const BOTTOMING: f32 = 0.8;
/// The broken damper's call (`CSigCarDamageSpeech` 7: `weve_got_fire` in 1.02's table at 0x7c37e0).
pub const BROKEN_CALL: u32 = 7;

/// The game's frames a second (ours, as the gravel hits' clock in `tyre`).
const FRAME_RATE: f32 = 60.0;

/// The suspension's state between steps.
#[derive(Debug, Clone, Default)]
pub struct Suspension {
    /// The `Squeeks` and `Suspension` amps (times the effects volume); the ranged gain.
    pub squeak_gain: f32,
    pub bottoming_gain: f32,
    pub ranged_gain: f32,
    pub squeaks: usize,
    pub bottomings: usize,
    pub ranged: usize,
    /// The last frame's peak per wheel (`+0x1050`) and the last bump word seen.
    peak: [f32; 4],
    bump: [Option<u32>; 4],
    /// This frame's words so far, and the seconds left of it.
    frame_peak: [f32; 4],
    frame_bump: [f32; 4],
    frame_wait: f32,
}

impl Suspension {
    pub fn new(gains: [f32; 3], counts: [usize; 3]) -> Suspension {
        let [squeak_gain, bottoming_gain, ranged_gain] = gains;
        let [squeaks, bottomings, ranged] = counts;
        Suspension { squeak_gain, bottoming_gain, ranged_gain, squeaks, bottomings, ranged, ..Suspension::default() }
    }

    /// The commands and damage-call codes of a step of `dt` seconds, `inside` with the camera in the car, each
    /// wheel `wheels` m from the listener (the squeaks and the bottoming play at their wheel, `M7700` 0x4d7700 and
    /// `M77c0` 0x4d77c0: the tyre slot's `+0x7c`); `rand` stands for the original's `rand()`. The first step only
    /// takes the words in.
    pub fn step(
        &mut self,
        sound: &Sound,
        inside: bool,
        wheels: [f32; 4],
        dt: f32,
        mut rand: impl FnMut() -> u32,
    ) -> (Vec<Command>, Vec<u32>) {
        let mut out = Vec::new();
        let mut calls = Vec::new();
        let mut first = false;
        for (k, words) in sound.dampers.iter().enumerate() {
            let velocity = sound.strut_velocity[k];
            if velocity.abs() > self.frame_peak[k].abs() {
                self.frame_peak[k] = velocity;
            }
            match self.bump[k] {
                Some(b) if b != words[BUMP] => self.frame_bump[k] = f32::from_bits(words[BUMP]),
                Some(_) => {}
                None => first = true,
            }
            self.bump[k] = Some(words[BUMP]);
        }
        if first {
            self.peak = self.frame_peak;
            self.frame_peak = [0.0; 4];
            return (out, calls);
        }
        self.frame_wait -= dt;
        if self.frame_wait > 0.0 {
            return (out, calls);
        }
        self.frame_wait += 1.0 / FRAME_RATE;
        for (k, words) in sound.dampers.iter().enumerate() {
            let (peak, bump) = (self.frame_peak[k], self.frame_bump[k]);
            (self.frame_peak[k], self.frame_bump[k]) = (0.0, 0.0);
            let change = (self.peak[k] - peak).abs();
            self.peak[k] = peak;
            // Vf5c: in the car only.
            if inside && change >= SQUEAK_CHANGE && rand().is_multiple_of(SQUEAK_ODDS) && self.squeaks > 0 {
                let shot = Shot::Squeak(rand() as usize % self.squeaks);
                out.push(Command::Hit { shot, gain: self.squeak_gain, pitch: SQUEAK_PITCH, distance: wheels[k] });
            }
            // App state 1 (the race; the standalone's drive is always in it).
            if words[VALID] == 0 {
                calls.push(BROKEN_CALL);
            }
            if bump > KNOCK && inside {
                if let Some(index) = ranged_index((bump + 1.0) * KNOCK_POWER, self.ranged, &mut rand) {
                    out.push(Command::Shot { shot: Shot::Ranged(index), gain: self.ranged_gain });
                }
                if bump > BOTTOMING && self.bottomings > 0 {
                    let shot = Shot::Bottoming(rand() as usize % self.bottomings);
                    out.push(Command::Hit { shot, gain: self.bottoming_gain, pitch: 1.0, distance: wheels[k] });
                }
            }
        }
        (out, calls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / FRAME_RATE;

    fn sound(bump: f32, velocity: f32, valid: u32) -> Sound {
        let mut sound = Sound::default();
        for k in 0..4 {
            sound.dampers[k] = [k as u32, 0, bump.to_bits(), 0, valid];
            sound.strut_velocity[k] = velocity;
        }
        sound
    }

    #[test]
    fn a_new_bump_knocks_in_the_car_and_bottoms_above_08() {
        let mut s = Suspension::new([1.0; 3], [18, 4, 20]);
        assert_eq!(s.step(&sound(0.0, 0.0, 1), true, [0.0; 4], FRAME, || 0), (vec![], vec![]));
        let (out, _) = s.step(&sound(0.9, 0.0, 1), true, [0.0; 4], FRAME, || 0);
        let shots: Vec<_> = out
            .iter()
            .map(|c| match c {
                Command::Shot { shot, .. } | Command::Hit { shot, .. } => *shot,
                _ => unreachable!(),
            })
            .collect();
        // (0.9 + 1) * 200 = 380: index rand % 3 + trunc(180 * 0.005) = 0.
        assert_eq!(shots, [Shot::Ranged(0), Shot::Bottoming(0)].repeat(4));
        // The same word again is no new hit; outside the car a new one is silent.
        assert!(s.step(&sound(0.9, 0.0, 1), true, [0.0; 4], FRAME, || 0).0.is_empty());
        assert!(s.step(&sound(0.5, 0.0, 1), false, [0.0; 4], FRAME, || 0).0.is_empty());
    }

    #[test]
    fn a_fast_peak_change_squeaks_one_time_in_ten_and_a_broken_damper_calls_7() {
        let mut s = Suspension::new([1.0; 3], [18, 4, 20]);
        s.step(&sound(0.0, 0.0, 1), true, [0.0; 4], FRAME, || 0);
        let (out, calls) = s.step(&sound(0.0, 1.5, 0), true, [0.0; 4], FRAME, || 10);
        assert_eq!(out.len(), 4);
        assert_eq!(calls, [BROKEN_CALL; 4]);
        assert!(s.step(&sound(0.0, 0.0, 1), true, [0.0; 4], FRAME, || 3).0.is_empty());
    }

    #[test]
    fn a_frame_takes_the_largest_velocity_and_the_last_bump_of_its_steps() {
        let mut s = Suspension::new([1.0; 3], [18, 4, 20]);
        s.step(&sound(0.0, 0.0, 1), true, [0.0; 4], 0.007, || 0);
        // Three steps of 7 ms: the first ends the frame (nothing changed), the next two make the following one.
        assert!(s.step(&sound(0.0, 0.0, 1), true, [0.0; 4], 0.007, || 0).0.is_empty());
        assert!(s.step(&sound(0.9, -1.5, 1), true, [0.0; 4], 0.007, || 0).0.is_empty());
        let (out, _) = s.step(&sound(0.9, 0.2, 1), true, [0.0; 4], 0.007, || 0);
        // The 0.9 bump came in the frame's first step: a knock and the bottoming per wheel; the -1.5 peak squeaks.
        assert_eq!(out.len(), 12);
    }
}
