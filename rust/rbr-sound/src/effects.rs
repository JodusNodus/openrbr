//! The car's smaller effects in `CCarSound` (`notes/sound.md` 5.5), read in the disassembly. Pure.
//!
//! - The ignition: the `DrivetrainState` handler (0x4ae510, 0x4ae73b-0x4ae7a6) keeps the starter word `p[9]`
//!   (`CDrive +0x1c8`) while the engine is stopped (`+0x41924`, which `M6a0` sets below the low rpm and clears above
//!   it), and when it rises from 0 calls `Vf54` (0x4ad090): `CAR::IGNITION Ignition1` at the car, at the `Ignition`
//!   amp, min distance 10 m, unless the last one still plays.
//! - The dog rings: the handler keeps `f = p[4] / 2 pi * 0.5` (`p[4]` is `|CDrive::M5A10|` while a gear engages in
//!   `CDrive::M8570` 0x508570, else 0) and `p[5]` (the engagement's level, `dri +0x560`). Each frame with `f > 0`
//!   (`CCarSound::Vf08` 0x4b1085-0x4b111d) a clock gains the frame's time and, once it reaches `f / 2 pi`, loses `1 /
//!   f` and plays a random `CAR::DOGRINGS DR1-8` (`E31Transmission::M3510` 0x4d3510) in the car, 2D, at `p[5] * 0.7`.
//!   The clock is never reset but by the car's reset.
//! - The broken cooler: `M6a0` keeps `min(p[5], 1) * 0.25` of the `EngineNoise` payload (`+0x41abc`), and `Md2b0`
//!   (0x4ad2b0) loops `Audio\Cars\Effects\cooler.wav` at the car at the `BrokenCooler` amp times it while it is at
//!   least 0.001 (min distance 5 m), and stops it below. What sets the engine state's `p[5]` is not traced.
//! - The wipers: `Md490` (0x4ad490), in the car, with the glass on (`0x167e834`, the in-car camera modes) and
//!   precipitation, plays `CAR::WIPER WiperEngine` (`WiperEngine` amp) and `WiperUp` or `WiperDown` (`Wiper` amp),
//!   2D, when `E88Wiper`'s direction (`0x167e830`) differs from `CCarSound +0x41928`. 1.02 never writes `+0x41928`
//!   after setting it to 1, so it starts both again every frame of each return stroke. Ours: both play once at each
//!   change of direction, `WiperUp` when the blades turn back (the direction 0, which 1.02 plays) and `WiperDown`
//!   when they set out again. The direction changes every 0.4 s while the precipitation is above 0
//!   (`E88Wiper::Tick` 0x5f8720, `rbr_render::glass::model::Wiping`).
//!
//! Ours: the frame clock of the dog rings runs on the physics step, and the effects volume option is 1.

use rbr_recording::sound::Sound;

use crate::voice::{Command, Loop, Repeat, Shot};

/// 1 / 2 pi (0x731b1c) and the half (0x795e18) of the dog rings' rate; their volume factor (0x72f674).
const INV_TAU: f32 = 0.159_154_94;
const DOG_RATE: f32 = 0.5;
const DOG_GAIN: f32 = 0.7;
/// The cooler's factor (0x730888) and its threshold (0x795e94).
const COOLER_SCALE: f32 = 0.25;
const COOLER_QUIET: f32 = 0.001;
/// Seconds per wiper stroke (`E88Wiper`: 1.3823 rad at 3.4558 rad/s).
pub const WIPER_STROKE: f32 = 0.4;

/// The effects' amps (`audio.ini`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectGains {
    pub ignition: f32,
    pub cooler: f32,
    pub wiper_engine: f32,
    pub wiper: f32,
}

impl Default for EffectGains {
    /// The retail values.
    fn default() -> Self {
        EffectGains { ignition: 1.0, cooler: 1.0, wiper_engine: 0.25, wiper: 0.25 }
    }
}

/// What the effects keep between steps.
#[derive(Debug, Clone, Default)]
pub struct Effects {
    pub gains: EffectGains,
    /// The banks' sizes: the dog rings.
    dog_rings: usize,
    /// The starter word while stopped (`+0x41858`), the dog rings' clock (`+0x41b14`).
    starter: f32,
    dog_clock: f32,
    /// The wipers' time running and the direction last played.
    wiper_time: f32,
    wiper_played: Option<bool>,
}

impl Effects {
    pub fn new(gains: EffectGains, dog_rings: usize) -> Effects {
        Effects { gains, dog_rings, ..Effects::default() }
    }

    /// The commands for one step of `dt` seconds: the engine is stopped or not, the camera in the car or not, under
    /// `precipitation` (`M9b80`, rain or snow).
    pub fn step(
        &mut self,
        sound: &Sound,
        stopped: bool,
        inside: bool,
        precipitation: f32,
        dt: f32,
        mut rand: impl FnMut() -> u32,
    ) -> Vec<Command> {
        let mut out = Vec::new();
        let d = |i: usize| sound.drivetrain_f32(i);
        if stopped {
            let rose = self.starter == 0.0 && d(9) != 0.0;
            self.starter = d(9);
            if rose {
                out.push(Command::Effect {
                    shot: Shot::Ignition,
                    gain: self.gains.ignition,
                    pitch: 1.0,
                    repeat: Repeat::IfIdle,
                });
            }
        }

        let f = d(4) * INV_TAU * DOG_RATE;
        if f > 0.0 {
            self.dog_clock += dt;
            if self.dog_clock >= f * INV_TAU {
                self.dog_clock -= 1.0 / f;
                let pick = rand() as usize;
                if inside && self.dog_rings > 0 {
                    out.push(Command::Shot { shot: Shot::DogRing(pick % self.dog_rings), gain: d(5) * DOG_GAIN });
                }
            }
        }

        let cooler = sound.engine_f32(5).min(1.0) * COOLER_SCALE;
        let cooler = if cooler >= COOLER_QUIET { cooler * self.gains.cooler } else { 0.0 };
        out.push(Command::Loop { voice: Loop::Cooler, gain: cooler, pitch: 1.0 });

        if precipitation > 0.0 {
            self.wiper_time += dt;
        }
        // `E88Wiper +0x30` starts at 1 and flips at each stop.
        let direction = ((self.wiper_time / WIPER_STROKE) as u32).is_multiple_of(2);
        if inside && precipitation > 0.0 && self.wiper_played != Some(direction) {
            self.wiper_played = Some(direction);
            let stroke = if direction { Shot::WiperDown } else { Shot::WiperUp };
            out.push(Command::Shot { shot: Shot::WiperEngine, gain: self.gains.wiper_engine });
            out.push(Command::Shot { shot: stroke, gain: self.gains.wiper });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sound(drivetrain: &[(usize, f32)], engine5: f32) -> Sound {
        let mut s = Sound::default();
        for &(i, v) in drivetrain {
            s.drivetrain[i] = v.to_bits();
        }
        s.engine[5] = engine5.to_bits();
        s
    }

    fn shots(commands: &[Command]) -> Vec<Shot> {
        commands
            .iter()
            .filter_map(|c| match *c {
                Command::Shot { shot, .. } | Command::Effect { shot, .. } => Some(shot),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_starter_cranks_the_ignition_once_while_stopped() {
        let mut fx = Effects::new(EffectGains::default(), 8);
        let crank = sound(&[(9, 0.5)], 0.0);
        assert!(shots(&fx.step(&crank, false, true, 0.0, 0.01, || 0)).is_empty(), "running");
        let mut fx = Effects::new(EffectGains::default(), 8);
        assert_eq!(shots(&fx.step(&sound(&[], 0.0), true, true, 0.0, 0.01, || 0)), []);
        assert_eq!(shots(&fx.step(&crank, true, true, 0.0, 0.01, || 0)), [Shot::Ignition]);
        assert_eq!(shots(&fx.step(&crank, true, true, 0.0, 0.01, || 0)), []);
    }

    #[test]
    fn the_dog_rings_rattle_at_the_engagement_rate_in_the_car() {
        // p[4] = 4 pi * 10: f = 10, the first after 10 / 2 pi s, then every 0.1 s.
        let engaging = sound(&[(4, 40.0 * std::f32::consts::PI), (5, 0.5)], 0.0);
        let mut fx = Effects::new(EffectGains::default(), 8);
        let steps: Vec<usize> =
            (0..200).filter(|_| !shots(&fx.step(&engaging, false, true, 0.0, 0.01, || 3)).is_empty()).collect();
        assert_eq!(steps.first(), Some(&159));
        assert!(steps.windows(2).all(|w| w[1] - w[0] == 10), "{steps:?}");
        let mut fx = Effects::new(EffectGains::default(), 8);
        assert!((0..200).all(|_| shots(&fx.step(&engaging, false, false, 0.0, 0.01, || 3)).is_empty()));
    }

    #[test]
    fn the_cooler_loops_above_its_threshold() {
        let mut fx = Effects::new(EffectGains::default(), 8);
        let gain = |c: &[Command]| {
            c.iter().find_map(|c| match *c {
                Command::Loop { voice: Loop::Cooler, gain, .. } => Some(gain),
                _ => None,
            })
        };
        assert_eq!(gain(&fx.step(&sound(&[], 2.0), false, false, 0.0, 0.01, || 0)), Some(0.25));
        assert_eq!(gain(&fx.step(&sound(&[], 0.002), false, false, 0.0, 0.01, || 0)), Some(0.0));
    }

    #[test]
    fn the_wipers_sound_at_each_turn_in_the_car_in_the_wet() {
        let mut fx = Effects::new(EffectGains::default(), 8);
        let dry = sound(&[], 0.0);
        let all: Vec<(usize, Vec<Shot>)> = (0..100)
            .map(|k| (k, shots(&fx.step(&dry, false, true, 50.0, 0.01, || 0))))
            .filter(|(_, s)| !s.is_empty())
            .collect();
        let at: Vec<usize> = all.iter().map(|(k, _)| *k).collect();
        assert_eq!(at, [0, 40, 80]);
        assert_eq!(all[0].1, [Shot::WiperEngine, Shot::WiperDown]);
        assert_eq!(all[1].1, [Shot::WiperEngine, Shot::WiperUp]);
        let mut fx = Effects::new(EffectGains::default(), 8);
        assert!((0..100).all(|_| shots(&fx.step(&dry, false, true, 0.0, 0.01, || 0)).is_empty()));
    }
}
