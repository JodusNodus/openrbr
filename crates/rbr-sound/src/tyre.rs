//! The tyre sounds from each wheel's `CSigTyreNoise` block (`rbr_recording::sound`, `tyres`): the surface loops of
//! `CTireSnd::M5fd0` (0x4d5fd0, read in the disassembly) and the off-road rattles of `CCarSound::Mce10` (0x4ace10).
//! Pure: gains, pitches and the samples to play per step. rbr-recomp `notes/sound.md` section 6.
//!
//! Each wheel has two surface voices playing the `TIRE::SURFACES` loops of the surface's two materials (gravel,
//! tarmac or snow; the skid loop of each above a grip maximum of 1.25), at one pitch from the surface table. A level
//! from the slip, the tyre factor and the speed sets their volume, split between the two by the table's share.
//! The third and fourth voices of a wheel are the rumble under the car (`M6750` 0x4d6750, in the car only, off a
//! surface whose first material is tarmac) and the wetness (`TIRE::SURFACES SkidWater` at the `CSigTireParticles`
//! splash that `M6560` 0x4d6560 keeps). Per frame, `M7330` (0x4d7330) plays the gravel hits (`IMPACT::GRUS`) off
//! tarmac at a rate from the tyre's force. `M7ea0` (0x4d7ea0, with fewer than 0x21 free hardware channels: every
//! system without DirectSound hardware mixing, which is any since Windows Vista) is not a mix of the four wheels: it
//! renders each wheel's two surface voices through the software 3D path (`E30MixerBase::Mce250`), so each wheel plays
//! at its full level times that path's distance gain (the mixer's `Place::Tire`), both voices at the first's volume. The original's frame-rate updates run here once per
//! physics step (ours).

use crate::tables::SURFACES;
use crate::voice::{Command, Material, Shot, WheelLoop};

/// The words of the block the handler reads (word 0, the wheel, is the slot the block comes in).
const MAX: usize = 1;
const GRIP_MAX: usize = 4;
const FACTOR: usize = 5;
const FORCE_SUM: usize = 2;
const SLIP_RATIO: usize = 8;
const SPEED: usize = 9;
const SURFACE: usize = 0x14;
/// The second surface of the contact's material pair and its share (the `pair` dwords' second and third).
const SECOND_SURFACE: usize = 0x15;
const SECOND_SHARE: usize = 0x16;
const CONTACT: usize = 0x17;

/// A grip maximum above this plays the skid loops; at 2 (0x72ad94) its share of the level is full, and from here it
/// rises by 4/3 (0x737488) per unit. Below it the share is 0.5 on a surface whose first material is tarmac (0x795e18)
/// and 0 on the others. NGP's patch of `M5fd0` (0x4d6197, 0x4d62a2) reads the threshold from NGP.dll (`10173478` on a
/// tarmac-first surface, `1017347c` on the others) instead of the exe's 1.25 (0x7344d8): NGP's settings
/// `tarmacSkidSoundThreshold` and `skidSoundThreshold` in percent (10 to 200), whose default 100 is 1 (both 1 in every
/// NGP snapshot).
const SKID: f32 = 1.0;
const SKID_FULL: f32 = 2.0;
const SKID_RISE: f32 = 1.333_333_4;
const TARMAC_REST: f32 = 0.5;
/// The tyre factor's share: full above 5000 (0x731054), else times 0.0002 (0x73748c).
const FACTOR_FULL: f32 = 5000.0;
const FACTOR_SCALE: f32 = 0.0002;
/// The slip ratio's share: full above 40 (0x73089c), else times 0.025 (0x737484).
const SLIP_FULL: f32 = 40.0;
const SLIP_SCALE: f32 = 0.025;
/// The speed term: speed times 0.05 (0x72dd78) plus slip ratio times 0.1 (0x72b068), at most 1.
const SPEED_SCALE: f32 = 0.05;
const SLIP_SPEED_SCALE: f32 = 0.1;
/// The level is `(slip + skid + factor) * 2000 (0x731d00) * speed term`, truncated and capped at 4000, and the
/// volumes take it times 1/4000 (0x737480).
const LEVEL_SCALE: f32 = 2000.0;
const LEVEL_CAP: u32 = 4000;
const LEVEL_TO_VOLUME: f32 = 0.000_25;
/// 16.16 to a float (0x7319f4).
const FIXED: f32 = 1.0 / 65536.0;

/// The rumble: `x = min(speed * 0.1 (0x72b068), 1)`, the level `min((word 1 + 1 + factor * 1.5625e-5 (0x737490))
/// * x, 1)`, the voice's volume moving a quarter (0x730888) of the way there each frame and set at half (0x795e18) of
/// it times the tyre volume; pitch `x * 0.4 + 0.8` (0x72f664, 0x72f610).
const RUMBLE_SPEED: f32 = 0.1;
const RUMBLE_FACTOR: f32 = 1.5625e-5;
const RUMBLE_SMOOTH: f32 = 0.25;
const RUMBLE_GAIN: f32 = 0.5;
const RUMBLE_PITCH: (f32, f32) = (0.4, 0.8);
/// The wetness voice decays by 0.95 (0x7355d4) a frame, to 0 below 0.001 (0x795e94); the splash sets it, at most 1.
const WET_DECAY: f32 = 0.95;
const QUIET: f32 = 0.001;
/// The gravel hits: off a tarmac-first surface a hit is due every `trunc(50 / (force sum + 0.01))` frames (0x7304a0,
/// 0x72f440), from 1 to 999, while the wheel touches the ground and the car is within 20 m (0x72f614) of the
/// listener. The pitch is the surface table's word 0 plus `(rand() % 10) * 0.02 - 0.1` (0x72fcb4, 0x72b068).
const GRUS_RATE: f32 = 50.0;
const GRUS_FORCE: f32 = 0.01;
const GRUS_FRAMES: u32 = 1000;
const GRUS_NEAR: f32 = 20.0;
const GRUS_PITCH_STEP: f32 = 0.02;
const GRUS_PITCH_OFFSET: f32 = 0.1;
/// The game's frames, which the gravel hits' counter, the rumble and the wetness decay run on (`Vf0c` 0x4d8290 from the
/// frame update, no time step): 60 a second of the drive's time here, as 1.02 runs with `WaitRetrace = true` (the
/// retail `RichardBurnsRally.ini`) on a 60 Hz display (ours: the original's rate follows its frame rate).
pub(crate) const FRAME_RATE: f32 = 60.0;

/// The off-road rattles: at or above 5 km/h (0x72ae44) with all four wheels off the road, one plays each
/// `10 / speed` seconds (0x72b154).
const OFFROAD_SPEED: f32 = 5.0;
const OFFROAD_PERIOD: f32 = 10.0;
/// The off-road samples (`CAR::OTHER Offroad1-7`).
pub const OFFROAD_SAMPLES: usize = 7;

/// How far the wheel of tyre-noise block `sig` (its position at `+0xc`, the slot's `+0x7c`) is from `at`.
pub(crate) fn distance(sig: &[u32; 24], at: [f32; 3]) -> f32 {
    (0..3).map(|i| (f32::from_bits(sig[0xc + i]) - at[i]).powi(2)).sum::<f32>().sqrt()
}

/// One wheel's surface voices for a step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelSound {
    /// The two materials and their volumes (before the tyre gain).
    pub materials: [(Material, f32); 2],
    pub skid: bool,
    pub pitch: f32,
    /// The surface is not one of the road surfaces (`E31_5fa0` gives 1).
    pub off_road: bool,
    /// The surface's first material is tarmac (`+0x278`: no rumble, no gravel hits).
    pub tarmac: bool,
    /// The gravel hits' period in frames (`+0x280`; 0 on tarmac) and their base pitch (the table's word 0, `+0x2a0`).
    pub grus_frames: u32,
    pub grus_pitch: f32,
}

fn material(word: u32) -> Material {
    match word {
        1 => Material::Tarmac,
        2 => Material::Snow,
        _ => Material::Gravel,
    }
}

/// `M5fd0` on one block: `None` when the wheel has no contact (its voices go silent). NGP's patch
/// (0x4d60ab) takes the pair's second surface when its share is above 0.5 (0x795e18), its own road list
/// (`10171280`, called at 0x4d60df) and its skid threshold ([`NGP_SKID`]).
pub fn wheel(sig: &[u32; 24], road_surfaces: &[u32]) -> Option<WheelSound> {
    if sig[CONTACT] == 0 {
        return None;
    }
    let f = |i: usize| f32::from_bits(sig[i]);
    let surface = if f(SECOND_SHARE) > 0.5 { sig[SECOND_SURFACE] } else { sig[SURFACE] };
    let row = SURFACES[surface as usize & 0xff];
    let first = material(row[1]);
    let grip = f(GRIP_MAX);
    let factor = if f(FACTOR) > FACTOR_FULL { 1.0 } else { f(FACTOR) * FACTOR_SCALE };
    let skid_share = if grip >= SKID_FULL {
        1.0
    } else if grip >= SKID {
        (grip - SKID) * SKID_RISE
    } else if first == Material::Tarmac {
        TARMAC_REST
    } else {
        0.0
    };
    let slip = f(SLIP_RATIO);
    let slip_share = if slip > SLIP_FULL { 1.0 } else { slip * SLIP_SCALE };
    let speed = (f(SPEED) * SPEED_SCALE + slip * SLIP_SPEED_SCALE).min(1.0);
    let level = ((slip_share + skid_share + factor) * LEVEL_SCALE * speed) as i32 as u32;
    let x = level.min(LEVEL_CAP) as f32 * LEVEL_TO_VOLUME;
    let share = row[4] as f32 * FIXED;
    Some(WheelSound {
        materials: [(first, (1.0 - share) * x), (material(row[2]), share * x)],
        skid: grip > SKID,
        pitch: row[3] as f32 * FIXED,
        off_road: !road_surfaces.contains(&surface),
        tarmac: first == Material::Tarmac,
        grus_frames: if first == Material::Tarmac {
            0
        } else {
            (GRUS_RATE / (f(FORCE_SUM) + GRUS_FORCE)) as i32 as u32
        },
        grus_pitch: f32::from_bits(row[0]),
    })
}

/// The tyres' state between steps.
#[derive(Debug, Clone, Default)]
pub struct Tyres {
    road_surfaces: Vec<u32>,
    /// `GravelParticles` amp times `SOUNDDEF::TIRE` volume.
    pub gain: f32,
    /// `Offtrack` amp times the effects volume.
    pub offroad_gain: f32,
    /// The off-road timer (`+0x41944`).
    timer: f32,
    /// Per wheel: the contact flag, the tarmac flag and the hit period as the last block left them (`+0x40`,
    /// `+0x278`, `+0x280`; a block without contact leaves the last two), the hits' base pitch, the frame counter
    /// (`+0xb74`), the rumble voice's volume and the wetness level.
    contact: [bool; 4],
    tarmac: [bool; 4],
    grus_frames: [u32; 4],
    grus_pitch: [f32; 4],
    grus_count: [u32; 4],
    /// The frames of [`FRAME_RATE`] not yet counted.
    frames: f32,
    rumble: [f32; 4],
    wet: [f32; 4],
    /// The last splash each wheel's `CSigTireParticles` carried (`+0xac`).
    splash: [f32; 4],
    /// `IMPACT::GRUS` samples.
    pub grus_count_samples: usize,
}

impl Tyres {
    pub fn new(gain: f32, offroad_gain: f32, grus_samples: usize, road_surfaces: Vec<u32>) -> Tyres {
        Tyres { gain, offroad_gain, grus_count_samples: grus_samples, road_surfaces, ..Tyres::default() }
    }

    /// The commands for one step of `dt` seconds: the eight surface voices, the rumble and wetness loops, the gravel
    /// hits, and an off-road rattle when it is due. `splash` is each wheel's `CSigTireParticles` splash of the step,
    /// `inside` the in-car camera, `car_distance` the car's distance from the listener and `listener` where it is (a
    /// gravel hit plays at its wheel, the block's `+0xc`). `pick` gives the random
    /// numbers (the original's `rand()`).
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        blocks: &[[u32; 24]; 4],
        splash: &[Option<f32>; 4],
        speed: f32,
        dt: f32,
        inside: bool,
        (car_distance, listener): (f32, [f32; 3]),
        mut pick: impl FnMut() -> u32,
    ) -> Vec<Command> {
        let mut out = Vec::with_capacity(20);
        let mut all_off = true;
        // The frame clock: the rumble, the wetness decay and the gravel hits run on the game's frames (`Vf0c`), taken
        // as [`FRAME_RATE`] of the drive's time, so the viewer's frame rate does not change them.
        self.frames += dt * FRAME_RATE;
        let frames = self.frames.floor();
        self.frames -= frames;
        let frames = frames as u32;
        for (k, sig) in blocks.iter().enumerate() {
            if let Some(s) = splash[k] {
                self.splash[k] = s;
            }
            // The frames' decay of the wetness voice (`M7ba0`/`M7ea0`), before the block sets it again.
            for _ in 0..frames {
                self.wet[k] = if self.wet[k] * WET_DECAY < QUIET { 0.0 } else { self.wet[k] * WET_DECAY };
            }
            let sound = wheel(sig, &self.road_surfaces);
            all_off &= sound.is_some_and(|s| s.off_road);
            self.contact[k] = sound.is_some();
            if let Some(s) = sound {
                self.tarmac[k] = s.tarmac;
                self.grus_frames[k] = s.grus_frames;
                self.grus_pitch[k] = s.grus_pitch;
                // M5fd0: the splash level, at most 1 (a splash of 0 or less silences it).
                self.wet[k] = if self.splash[k] > 0.0 { self.splash[k].min(1.0) } else { 0.0 };
            }
            // M7ea0 mixes both voices at the first's volume (0x4d80e8, 0x4d81b1): on a surface of one material the
            // second voice doubles the first, the same loop from the same place.
            for slot in 0..2 {
                let (material, gain, skid, pitch) = match sound {
                    Some(s) => (s.materials[slot].0, s.materials[0].1 * self.gain, s.skid, s.pitch),
                    None => (Material::Gravel, 0.0, false, 1.0),
                };
                out.push(Command::Tyre { wheel: k as u8, slot: slot as u8, material, skid, gain, pitch });
            }
            out.push(Command::WheelLoop { wheel: k as u8, voice: WheelLoop::Wetness, gain: self.wet[k], pitch: 1.0 });
            out.push(self.rumble(k, sig, inside, frames));
        }
        for _ in 0..frames {
            if car_distance > GRUS_NEAR {
                break;
            }
            for (k, block) in blocks.iter().enumerate() {
                let n = self.grus_frames[k];
                if !(1..GRUS_FRAMES).contains(&n) || !self.contact[k] {
                    continue;
                }
                self.grus_count[k] += 1;
                if n <= self.grus_count[k] && self.grus_count_samples > 0 {
                    let pitch = self.grus_pitch[k] + (pick() % 10) as f32 * GRUS_PITCH_STEP - GRUS_PITCH_OFFSET;
                    self.grus_count[k] = 0;
                    let shot = Shot::Grus(pick() as usize % self.grus_count_samples);
                    let distance = distance(block, listener);
                    out.push(Command::Hit { shot, gain: self.gain, pitch, distance });
                }
            }
        }
        // Mce10: the speed is CarSensorData's |p+8| * 3.6; the car's speed is taken for it (inferred).
        let kmh = speed.abs() * 3.6;
        if kmh >= OFFROAD_SPEED && all_off {
            self.timer -= dt;
            if self.timer <= 0.0 {
                self.timer = OFFROAD_PERIOD / kmh;
                out.push(Command::Shot {
                    shot: Shot::Offroad(pick() as usize % OFFROAD_SAMPLES),
                    gain: self.offroad_gain,
                });
            }
        } else if !all_off {
            self.timer = 0.0;
        }
        out
    }

    /// `M6750` for wheel `k`: the rumble plays in the car on a wheel touching a surface whose first material is not
    /// tarmac, and stops otherwise (its volume kept). Word 1 of the block is the frame's largest `FillOutputs` term,
    /// which the wheel's frame finish clears (`rbr_recording::sound::TyreMax`; the standalone writer folds it).
    /// The voice's volume moves once per game frame, `frames` of them in this step.
    fn rumble(&mut self, k: usize, sig: &[u32; 24], inside: bool, frames: u32) -> Command {
        if !self.contact[k] || self.tarmac[k] || !inside {
            return Command::WheelLoop { wheel: k as u8, voice: WheelLoop::Rumble, gain: 0.0, pitch: 1.0 };
        }
        let f = |i: usize| f32::from_bits(sig[i]);
        let x = (f(SPEED) * RUMBLE_SPEED).min(1.0);
        let level = ((f(MAX) + 1.0 + f(FACTOR) * RUMBLE_FACTOR) * x).min(1.0);
        for _ in 0..frames {
            self.rumble[k] = ((level - self.rumble[k]) * RUMBLE_SMOOTH + self.rumble[k]) * RUMBLE_GAIN;
        }
        let pitch = x * RUMBLE_PITCH.0 + RUMBLE_PITCH.1;
        Command::WheelLoop { wheel: k as u8, voice: WheelLoop::Rumble, gain: self.rumble[k], pitch }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic road membership for these model tests. The complete DLL table is checked in rbr-formats.
    fn wheel(sig: &[u32; 24]) -> Option<WheelSound> {
        super::wheel(sig, &[1, 33, 101])
    }

    fn block(surface: u32, grip: f32, factor: f32, slip: f32, speed: f32) -> [u32; 24] {
        let mut sig = [0; 24];
        sig[GRIP_MAX] = grip.to_bits();
        sig[FACTOR] = factor.to_bits();
        sig[SLIP_RATIO] = slip.to_bits();
        sig[SPEED] = speed.to_bits();
        sig[SURFACE] = surface;
        sig[CONTACT] = 1;
        sig
    }

    #[test]
    fn rolling_on_gravel_is_the_level_from_slip_factor_and_speed() {
        // Surface 1: gravel, pitch 73728 / 65536, a road surface.
        let s = wheel(&block(1, 1.0, 2500.0, 4.0, 30.0)).unwrap();
        // slip 4 * 0.025 = 0.1, skid 0, factor 0.5; speed term min(1.5 + 0.4, 1) = 1; level 1200 -> 0.3.
        assert_eq!(s.materials[0].0, Material::Gravel);
        assert!((s.materials[0].1 - 0.3).abs() < 1e-6);
        assert_eq!(s.materials[1].1, 0.0);
        assert!(!s.skid);
        assert_eq!(s.pitch, 1.125);
        assert!(!s.off_road);
    }

    #[test]
    fn skidding_switches_the_loop_and_caps_the_level() {
        let s = wheel(&block(1, 3.0, 9000.0, 80.0, 50.0)).unwrap();
        assert!(s.skid);
        // (1 + 1 + 1) * 2000 = 6000, capped at 4000.
        assert_eq!(s.materials[0].1, 1.0);
        // Between 1 and 2 the skid share rises by 4/3 per unit.
        let s = wheel(&block(1, 1.375, 0.0, 0.0, 20.0)).unwrap();
        assert!((s.materials[0].1 - 0.5 * 2000.0 * 1.0 / 4000.0).abs() < 1e-3);
    }

    #[test]
    fn a_mixed_surface_splits_the_level_and_tarmac_rests_at_half() {
        // Surface 101: tarmac first, gravel second, share 57344 / 65536.
        let s = wheel(&block(101, 1.0, 0.0, 0.0, 20.0)).unwrap();
        assert_eq!((s.materials[0].0, s.materials[1].0), (Material::Snow, Material::Gravel));
        let tarmac = SURFACES.iter().position(|row| row[1] == 1 && row[4] == 0).unwrap() as u32;
        let t = wheel(&block(tarmac, 0.9, 0.0, 0.0, 20.0)).unwrap();
        assert_eq!(t.materials[0].0, Material::Tarmac);
        // skid share 0.5 at rest on tarmac: 0.5 * 2000 * 1 / 4000.
        assert!((t.materials[0].1 + t.materials[1].1 - 0.25).abs() < 1e-6);
    }

    #[test]
    fn both_surface_voices_play_at_the_first_voices_volume() {
        let mut tyres = Tyres::new(1.0, 1.0, 0, vec![1, 33, 101]);
        let sig = block(101, 1.0, 0.0, 0.0, 20.0);
        let first = wheel(&sig).unwrap().materials[0].1;
        let out = tyres.step(&[sig; 4], &[None; 4], 20.0, 1.0 / 60.0, false, (0.0, [0.0; 3]), || 0);
        let gains: Vec<(u8, Material, f32)> = out
            .iter()
            .filter_map(|c| match *c {
                Command::Tyre { wheel: 0, slot, material, gain, .. } => Some((slot, material, gain)),
                _ => None,
            })
            .collect();
        assert_eq!(gains, [(0, Material::Snow, first), (1, Material::Gravel, first)]);
    }

    #[test]
    fn the_pairs_second_surface_its_road_list_and_skid_threshold() {
        // Surface 1 (gravel) with 101 (snow first) at a share of 0.6 plays 101.
        let mut sig = block(1, 1.1, 0.0, 0.0, 20.0);
        (sig[SECOND_SURFACE], sig[SECOND_SHARE]) = (101, 0.6f32.to_bits());
        let s = wheel(&sig).unwrap();
        assert_eq!(s.materials[0].0, Material::Snow);
        // A grip maximum of 1.1 skids above the threshold of 1.
        assert!(s.skid);
        sig[SECOND_SHARE] = 0.5f32.to_bits();
        assert_eq!(wheel(&sig).unwrap().materials[0].0, Material::Gravel);
        // Surface 33 is a road surface, 72 is not.
        assert!(!wheel(&block(33, 1.0, 0.0, 0.0, 20.0)).unwrap().off_road);
        assert!(wheel(&block(72, 1.0, 0.0, 0.0, 20.0)).unwrap().off_road);
    }

    #[test]
    fn no_contact_is_silent() {
        let mut sig = block(1, 3.0, 9000.0, 80.0, 50.0);
        sig[CONTACT] = 0;
        assert_eq!(wheel(&sig), None);
    }

    #[test]
    fn gravel_hits_come_at_the_force_rate_off_tarmac() {
        let mut tyres = Tyres::new(1.0, 1.0, 21, vec![1, 33, 101]);
        // Surface 1 is gravel first: a force sum of 4.99 gives a hit every trunc(50 / 5) = 10 frames.
        let mut sig = block(1, 1.0, 0.0, 0.0, 10.0);
        sig[FORCE_SUM] = 4.99f32.to_bits();
        let hits = |c: &[Command]| c.iter().filter(|c| matches!(c, Command::Hit { shot: Shot::Grus(_), .. })).count();
        let count: usize =
            (0..100).map(|_| hits(&tyres.step(&[sig; 4], &[None; 4], 10.0, 0.01, false, (5.0, [0.0; 3]), || 4))).sum();
        assert_eq!(count, 20); // frames 10 to 50 of the first 59
                               // Beyond 20 m from the listener there are none.
        let far: usize =
            (0..100).map(|_| hits(&tyres.step(&[sig; 4], &[None; 4], 10.0, 0.01, false, (30.0, [0.0; 3]), || 4))).sum();
        assert_eq!(far, 0);
        let pitch = (0..20)
            .flat_map(|_| tyres.step(&[sig; 4], &[None; 4], 10.0, 0.01, false, (5.0, [0.0; 3]), || 4))
            .find_map(|c| match c {
                Command::Hit { pitch, .. } => Some(pitch),
                _ => None,
            });
        // The table's word 0 plus 4 * 0.02 - 0.1.
        assert!(pitch.is_some_and(|p| (p - (f32::from_bits(SURFACES[1][0]) - 0.02)).abs() < 1e-6));
    }

    #[test]
    fn the_rumble_plays_in_the_car_off_tarmac_and_the_wetness_follows_the_splash() {
        let mut tyres = Tyres::new(1.0, 1.0, 0, vec![1, 33, 101]);
        let sig = block(1, 1.0, 0.0, 0.0, 20.0);
        let loops = |c: &[Command], want: WheelLoop| -> Vec<(f32, f32)> {
            c.iter()
                .filter_map(|c| match *c {
                    Command::WheelLoop { voice, gain, pitch, .. } if voice == want => Some((gain, pitch)),
                    _ => None,
                })
                .collect()
        };
        let out = tyres.step(&[sig; 4], &[Some(0.4); 4], 20.0, 1.0 / 60.0, true, (0.0, [0.0; 3]), || 0);
        // One game frame a step. Level 1 at full speed: the volume moves a quarter there and is set at half: 0.125;
        // pitch 1.2.
        assert_eq!(loops(&out, WheelLoop::Rumble)[0], (0.125, 1.2));
        assert_eq!(loops(&out, WheelLoop::Wetness)[0].0, 0.4);
        // Settles at a fifth of the level (the set volume is the next frame's start).
        for _ in 0..100 {
            tyres.step(&[sig; 4], &[None; 4], 20.0, 1.0 / 60.0, true, (0.0, [0.0; 3]), || 0);
        }
        let out = tyres.step(&[sig; 4], &[None; 4], 20.0, 1.0 / 60.0, false, (0.0, [0.0; 3]), || 0);
        assert!((tyres.rumble[0] - 0.2).abs() < 1e-4);
        assert_eq!(loops(&out, WheelLoop::Rumble)[0].0, 0.0);
    }

    #[test]
    fn word_1_raises_the_rumble_below_full_speed() {
        let rumble = |max: f32| {
            let mut tyres = Tyres::new(1.0, 1.0, 0, vec![1, 33, 101]);
            let mut sig = block(1, 1.0, 0.0, 0.0, 2.0);
            sig[MAX] = max.to_bits();
            let out = tyres.step(&[sig; 4], &[None; 4], 2.0, 1.0 / 60.0, true, (0.0, [0.0; 3]), || 0);
            out.iter()
                .find_map(|c| match *c {
                    Command::WheelLoop { voice: WheelLoop::Rumble, gain, .. } => Some(gain),
                    _ => None,
                })
                .unwrap()
        };
        // x = 0.2: level (1 + word 1) * 0.2, a quarter of it set at half.
        assert!((rumble(0.0) - 0.025).abs() < 1e-6);
        assert!((rumble(1.5) - 0.0625).abs() < 1e-6);
    }

    #[test]
    fn off_road_rattles_follow_the_speed() {
        let mut tyres = Tyres::new(1.0, 1.0, 0, vec![1, 33, 101]);
        let grass = [block(0, 1.0, 0.0, 0.0, 10.0); 4];
        let shots = |c: &[Command]| c.iter().filter(|c| matches!(c, Command::Shot { .. })).count();
        // 36 km/h: one rattle at once, then one each 10 / 36 s.
        let mut count = 0;
        for _ in 0..100 {
            count += shots(&tyres.step(&grass, &[None; 4], 10.0, 0.01, false, (5.0, [0.0; 3]), || 3));
        }
        assert_eq!(count, 4);
        let road = [block(1, 1.0, 0.0, 0.0, 10.0); 4];
        assert_eq!(shots(&tyres.step(&road, &[None; 4], 10.0, 0.01, false, (5.0, [0.0; 3]), || 3)), 0);
        assert_eq!(shots(&tyres.step(&grass, &[None; 4], 1.0, 0.01, false, (5.0, [0.0; 3]), || 3)), 0);
    }
}
