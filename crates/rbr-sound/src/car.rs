//! The car's sounds from one physics step's inputs (`rbr_recording::sound`), as voice commands: the engine voice
//! (`CCarSound::M6a0` 0x4b06a0), the gear and diff whine (`CCarSound::Me510` 0x4ae510, `E31Transmission::Vf08`
//! 0x4d2fe0), the turbo whistle and the blow-off valve (`CCarSound::Vf08` 0x4b0b50, `CalcVol` 0x4ac8c0). Pure: the
//! gains and pitches per step, no audio. rbr-recomp `notes/sound.md` sections 4 and 5.

use rbr_recording::sound::{NgpSound, Sound};

use crate::engine::EngineNoise;
use crate::voice::{Command, Loop, Repeat, Shot};

/// `1 / (2 pi)` (0x731b1c).
const INV_TAU: f32 = 0.159_154_94;
/// Gear and diff whine pitch per tooth-revolution per second (0x731b18, 0x731b14), and the pitch clamp (0x72ccc0,
/// 0x72d150).
const GEAR_PITCH: f32 = 0.000_674_763_8;
const DIFF_PITCH: f32 = 0.001_349_527_6;
const PITCH_RANGE: (f32, f32) = (0.125, 4.0);
/// Whine volumes below this are silent.
const QUIET: f32 = 0.001;
/// Ours, for a straight-cut dog box: the whine louder than 1.02's (`DOG_BOX`), a coast whine off the throttle while
/// the gearbox turns (at least [`COAST`] of full load, gone below [`COAST_SPEED`] rad/s of its shaft), and outside
/// the car at [`OUTSIDE_WHINE`] (1.02 plays it in the car only).
const DOG_BOX: f32 = 1.8;
const COAST: f32 = 0.4;
const COAST_SPEED: f32 = 20.0;
const OUTSIDE_WHINE: f32 = 0.35;
/// A gear change clicks above this (0x795e94).
pub(crate) const GEAR_CHANGE: f32 = 0.001;
/// The gear-change click's volume factor.
const GEAR_CHANGE_GAIN: f32 = 0.9;
/// Turbo volume per turbo speed times boost, its silence threshold, and pitch per turbo speed.
const TURBO_VOLUME: f32 = 7.692_3e-5;
const TURBO_QUIET: f32 = 0.01;
const TURBO_PITCH: f32 = 0.0004;
/// The blow-off valve arms when the turbo flow is above twice the last step's (0x72ad94), the flow capped at 0.6
/// (0x72f684), and plays at `min(flow * 1.6667, 1)` (0x7319d0) on the next frame. 1.02 then plays it again each time
/// the last has played 4000 samples (the sample is 6570 long), until the flow falls to 0.01 (0x72f440) or the volume
/// to 0.1 (0x72b068): a stutter through each lift. Ours: once, and armed again only after the volume has fallen to
/// 0.1.
pub(crate) const BLOW_OFF_RISE: f32 = 2.0;
pub(crate) const BLOW_OFF_CAP: f32 = 0.6;
const BLOW_OFF_SCALE: f32 = 1.666_666_6;
const BLOW_OFF_OFF: f32 = 0.1;
/// Ours: the whine's load and pitch follow the drivetrain through a one-pole of this time constant, seconds, as the
/// load words jump by a quarter to a half of their level from step to step and would make the whine warble.
const WHINE_SMOOTH: f32 = 0.04;

/// The volumes the gains multiply: the `audio.ini` amps and the `Ingame.ini` `SOUNDDEF` volumes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gains {
    /// `SOUNDDEF::EXHAUST` volume times the `Engine` amp.
    pub engine: f32,
    /// `SOUNDDEF::TRANSMISSION` volume times the `EngineTransmission` and `TransmissionDiff` amps.
    pub gearbox: f32,
    pub diff: f32,
    /// `SOUNDDEF::BLOWOFFVALVE` volume times the `Turbo` and `BlowOffValve` amps.
    pub turbo: f32,
    pub blow_off: f32,
    /// `SOUNDDEF::TIRE` volume times the `GravelParticles` amp.
    pub tyre: f32,
    /// The `Offtrack` amp.
    pub offroad: f32,
    /// `SOUNDDEF::RANGED` volume times the `Impacts` amp.
    pub ranged: f32,
    /// The `ImpactScreams` amp.
    pub scream: f32,
    /// The `Squeeks`, `Suspension` and `DriveShaftBroken` amps.
    pub squeak: f32,
    pub suspension: f32,
    pub drive_shaft: f32,
    /// `SOUNDDEF::EXHAUSTPOP` volume times the `Pop`, `ExhaustPop` and `Bang` amps.
    pub pop: f32,
    pub exhaust_pop: f32,
    pub bang: f32,
    /// The `Enginefire` amp.
    pub fire: f32,
    /// `SOUNDDEF::EXHAUST` volume times the `Engine` and `EnviormentalReflections` amps (`CCarSound::Vf04`: the
    /// reflection voices take the master's amp 13 times the engine voice's volume `+0x1d8`).
    pub reflections: f32,
}

impl Default for Gains {
    /// The retail values.
    fn default() -> Self {
        Gains {
            engine: 1.0,
            gearbox: 0.35,
            diff: 0.35,
            turbo: 0.2,
            blow_off: 1.0,
            tyre: 1.0,
            offroad: 1.0,
            ranged: 1.0,
            scream: 0.5,
            squeak: 1.0,
            suspension: 1.0,
            drive_shaft: 1.0,
            pop: 0.5,
            exhaust_pop: 0.5,
            bang: 0.5,
            fire: 1.0,
            reflections: 1.0,
        }
    }
}

/// What the car's sound keeps between steps.
#[derive(Debug, Clone, Default)]
pub struct Car {
    pub gains: Gains,
    /// The last step's turbo flow (`+0x418bc`).
    turbo_flow: f32,
    /// The blow-off valve is armed (`+0x41690`), and it has played since its volume was last at 0.1 or below (ours).
    blow_off: bool,
    vented: bool,
    /// The smoothed gearbox and diff loads and pitches (ours, [`WHINE_SMOOTH`]); `None` before the first step.
    whine: Option<[f32; 4]>,
    /// The gear-change flag was set on the last step (ours: the standalone has no `M8480` to clear it).
    gear_change: bool,
    /// The click's threshold and volume NGP patches in for 0x795e94's [`GEAR_CHANGE`] and the 0.9 when the car's FMOD
    /// bank has `gear_int` (`crate::fmod::FmodCar::gear_click`).
    pub gear_click: Option<(f32, f32)>,
    /// Steps since the start, for the 10-step fade-in of `CalcVol`.
    steps: u32,
    /// The left and right door are off the car (`+0x41a9c`, `+0x41aa0`): the reflections let the engine through.
    pub doors_off: [bool; 2],
}

impl Car {
    pub fn new(gains: Gains) -> Car {
        Car { gains, ..Car::default() }
    }

    /// The commands for one step's inputs, with the camera in the car or not (`g_8d5338[0x41bb]`), and NGP's sound
    /// words. The whine loads' caps (the exe's 0x72f00c, 0x731028) are NGP.dll's: the handler divides by a per-car
    /// normaliser (`+0x41738`, `+0x41744`) taken here as the cap itself (inferred), and NGP's engine init sets the caps
    /// and the normalisers (CCarSound `+0x4190c`, `+0x41918`) alike.
    /// `frame`: a game frame runs in this step (the blow-off's `Mcad0`).
    pub fn step(&mut self, sound: &Sound, inside: bool, ngp: &NgpSound, frame: bool) -> Vec<Command> {
        // CalcVol: a counter from 10 down to 0, the gains taking (10 - counter) / 10.
        self.steps = (self.steps + 1).min(10);
        let fade = self.steps as f32 * 0.1;
        let inside_gain = if inside { 1.0 } else { 0.0 };
        let mut out =
            vec![Command::Engine(EngineNoise::from_payload(&sound.engine, ngp.min_rpm)), Command::Inside(inside)];
        out.push(Command::Gain { voice: Loop::Engine, gain: self.gains.engine * fade });
        out.push(Command::Reflections { gain: self.gains.reflections * fade, doors_off: self.doors_off });

        let d = |i: usize| sound.drivetrain_f32(i);
        let gear_pitch = whine_pitch(sound.drivetrain[1] as f32 * d(0) * INV_TAU, GEAR_PITCH);
        let diff_pitch = whine_pitch(sound.drivetrain[3] as f32 * d(2) * INV_TAU, DIFF_PITCH);
        let (gear_cap, diff_cap) = (ngp.gear_load_cap, ngp.diff_load_cap);
        // 1.02 plays the transmission voices in the car only (CalcVol's +8 and +0xc take the in-car flag); ours
        // outside too, quieter, with the dog box's coast whine.
        let coast = COAST * (1.0 - sound.engine_f32(1).clamp(0.0, 1.0));
        let turning = |speed: f32| (speed.abs() / COAST_SPEED).min(1.0);
        let gear_load = (d(6).min(gear_cap) / gear_cap).max(coast * turning(d(0)));
        let diff_load = (d(7).min(diff_cap) / diff_cap).max(coast * turning(d(2)));
        let target = [gear_load, diff_load, gear_pitch, diff_pitch];
        let k = 1.0 - (-rbr_recording::STEP_SECONDS / WHINE_SMOOTH).exp();
        let [gear_load, diff_load, gear_pitch, diff_pitch] = match self.whine {
            Some(last) => std::array::from_fn(|i| last[i] + (target[i] - last[i]) * k),
            None => target,
        };
        self.whine = Some([gear_load, diff_load, gear_pitch, diff_pitch]);
        let quiet = |v: f32| if v < QUIET { 0.0 } else { v };
        let whine = DOG_BOX * fade * if inside { inside_gain } else { OUTSIDE_WHINE };
        out.push(Command::Loop {
            voice: Loop::Gearbox,
            gain: quiet(gear_load * self.gains.gearbox) * whine,
            pitch: gear_pitch,
        });
        out.push(Command::Loop {
            voice: Loop::Diff,
            gain: quiet(diff_load * self.gains.diff) * whine,
            pitch: diff_pitch,
        });
        let (threshold, click) = self.gear_click.unwrap_or((GEAR_CHANGE, GEAR_CHANGE_GAIN));
        let change = d(8) > threshold;
        if change && !self.gear_change && inside {
            out.push(Command::Shot { shot: Shot::GearChange, gain: click * fade });
        }
        self.gear_change = change;

        let e = |i: usize| sound.engine_f32(i);
        let turbo_speed = e(2);
        let mut turbo = turbo_level(sound);
        if turbo < TURBO_QUIET {
            turbo = 0.0;
        }
        if !inside {
            turbo /= 3.0;
        }
        out.push(Command::Loop {
            voice: Loop::Turbo,
            gain: turbo * self.gains.turbo * fade,
            pitch: turbo_speed * TURBO_PITCH,
        });
        let mut flow = e(6);
        if self.turbo_flow * BLOW_OFF_RISE < flow {
            flow = flow.min(BLOW_OFF_CAP);
            self.blow_off = !self.vented;
        }
        self.turbo_flow = flow;
        let volume = blow_off_volume(flow);
        self.vented &= volume > BLOW_OFF_OFF;
        // Mcad0 (0x4acad0), once a frame.
        if frame && self.blow_off {
            self.blow_off = false;
            if volume > BLOW_OFF_OFF {
                self.vented = true;
                out.push(Command::Effect {
                    shot: Shot::BlowOff,
                    gain: volume * self.gains.blow_off * fade,
                    pitch: 1.0,
                    repeat: Repeat::IfIdle,
                });
            }
        }
        out
    }
}

/// The turbo's level, 0 to 1: its speed times its boost, scaled.
pub(crate) fn turbo_level(sound: &Sound) -> f32 {
    (sound.engine_f32(2) * TURBO_VOLUME * sound.engine_f32(3)).clamp(0.0, 1.0)
}

/// The blow-off valve's volume for turbo flow `flow`.
pub(crate) fn blow_off_volume(flow: f32) -> f32 {
    flow.min(BLOW_OFF_CAP) * BLOW_OFF_SCALE
}

/// The whine pitch: tooth-revolutions per second times `factor`, 1 when that speed is 0, clamped to 0.125-4.
fn whine_pitch(rate: f32, factor: f32) -> f32 {
    let rate = if rate == 0.0 { 1.0 } else { rate };
    (rate * factor).clamp(PITCH_RANGE.0, PITCH_RANGE.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sound() -> Sound {
        let mut sound = Sound::default();
        sound.engine[0] = 300f32.to_bits();
        // Full throttle: no coast whine.
        sound.engine[1] = 1f32.to_bits();
        sound.drivetrain[0] = 200f32.to_bits();
        sound.drivetrain[1] = 30;
        sound.drivetrain[2] = 100f32.to_bits();
        sound.drivetrain[3] = 10;
        sound.drivetrain[6] = 300f32.to_bits();
        sound.drivetrain[7] = 9000f32.to_bits();
        sound
    }

    fn find(commands: &[Command], voice: Loop) -> (f32, f32) {
        commands
            .iter()
            .find_map(|c| match *c {
                Command::Loop { voice: v, gain, pitch } if v == voice => Some((gain, pitch)),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn whine_follows_shaft_speed_teeth_and_load() {
        let mut car = Car::new(Gains::default());
        for _ in 0..10 {
            car.step(&sound(), true, &NgpSound::default(), true);
        }
        let commands = car.step(&sound(), true, &NgpSound::default(), true);
        let (gain, pitch) = find(&commands, Loop::Gearbox);
        assert!((pitch - 30.0 * 200.0 * INV_TAU * GEAR_PITCH).abs() < 1e-6);
        assert!((gain - 0.5 * 0.35 * DOG_BOX).abs() < 1e-6);
        let (gain, pitch) = find(&commands, Loop::Diff);
        assert!((pitch - 10.0 * 100.0 * INV_TAU * DIFF_PITCH).abs() < 1e-6);
        assert!((gain - 0.35 * DOG_BOX).abs() < 1e-6);
        let outside = car.step(&sound(), false, &NgpSound::default(), true);
        assert!((find(&outside, Loop::Gearbox).0 - 0.5 * 0.35 * DOG_BOX * OUTSIDE_WHINE).abs() < 1e-6);
    }

    #[test]
    fn the_dog_box_whines_off_the_throttle_while_it_turns() {
        let mut car = Car::new(Gains::default());
        let mut s = sound();
        s.engine[1] = 0f32.to_bits();
        s.drivetrain[6] = 0f32.to_bits();
        let commands = (0..11).map(|_| car.step(&s, true, &NgpSound::default(), true)).last().unwrap();
        assert!((find(&commands, Loop::Gearbox).0 - COAST * 0.35 * DOG_BOX).abs() < 1e-6);
        // Stopped, it fades out through the smoothing.
        s.drivetrain[0] = 0f32.to_bits();
        let gain = |car: &mut Car| find(&car.step(&s, true, &NgpSound::default(), true), Loop::Gearbox).0;
        assert!(gain(&mut car) > 0.0);
        assert_eq!((0..50).map(|_| gain(&mut car)).last(), Some(0.0));
    }

    #[test]
    fn the_whine_caps_and_the_engines_low_rpm() {
        let mut car = Car::new(Gains::default());
        let ngp = NgpSound { gear_load_cap: 1200.0, diff_load_cap: 12000.0, min_rpm: 3000.0, idle_rpm: 0.0 };
        let commands = (0..11).map(|_| car.step(&sound(), true, &ngp, true)).last().unwrap();
        // Loads 300 of 1200 and 9000 of 12000.
        assert!((find(&commands, Loop::Gearbox).0 - 0.25 * 0.35 * DOG_BOX).abs() < 1e-6);
        assert!((find(&commands, Loop::Diff).0 - 0.75 * 0.35 * DOG_BOX).abs() < 1e-6);
        // 300 rad/s is 2865 rpm, below the low rpm of 3000.
        assert!(commands.iter().any(|c| matches!(c, Command::Engine(e) if e.stopped && e.target_rpm == 3000)));
    }

    #[test]
    fn whine_pitch_is_clamped_and_one_at_rest() {
        assert_eq!(whine_pitch(0.0, GEAR_PITCH), 0.125);
        assert_eq!(whine_pitch(1e7, GEAR_PITCH), 4.0);
    }

    #[test]
    fn the_gains_fade_in_over_ten_steps() {
        let mut car = Car::new(Gains::default());
        let first = car.step(&sound(), true, &NgpSound::default(), true);
        assert!(first.contains(&Command::Gain { voice: Loop::Engine, gain: 0.1 }));
    }

    #[test]
    fn turbo_and_blow_off() {
        let mut car = Car::new(Gains::default());
        let mut s = sound();
        s.engine[2] = 10000f32.to_bits();
        s.engine[3] = 1.0f32.to_bits();
        s.engine[6] = 0.1f32.to_bits();
        let first = car.step(&s, true, &NgpSound::default(), true);
        let blow_off = |out: &[Command]| out.iter().any(|c| matches!(c, Command::Effect { shot: Shot::BlowOff, .. }));
        assert!(blow_off(&first));
        let (gain, pitch) = find(&first, Loop::Turbo);
        assert!((gain - 0.76923 * 0.2 * 0.1).abs() < 1e-5);
        assert!((pitch - 4.0).abs() < 1e-6);
        // It plays once while the flow holds, and arms again only once the volume has fallen to 0.1.
        assert!(!blow_off(&car.step(&s, true, &NgpSound::default(), true)));
        s.engine[6] = 0.2f32.to_bits();
        assert!(!blow_off(&car.step(&s, true, &NgpSound::default(), true)));
        s.engine[6] = 0.05f32.to_bits();
        assert!(!blow_off(&car.step(&s, true, &NgpSound::default(), true)));
        s.engine[6] = 0.2f32.to_bits();
        assert!(blow_off(&car.step(&s, true, &NgpSound::default(), true)));
    }

    #[test]
    fn a_gear_change_clicks_once() {
        let mut car = Car::new(Gains::default());
        let mut s = sound();
        s.drivetrain[8] = 1f32.to_bits();
        let clicks =
            |c: &[Command]| c.iter().filter(|c| matches!(c, Command::Shot { shot: Shot::GearChange, .. })).count();
        assert_eq!(clicks(&car.step(&s, true, &NgpSound::default(), true)), 1);
        assert_eq!(clicks(&car.step(&s, true, &NgpSound::default(), true)), 0);
    }
}
