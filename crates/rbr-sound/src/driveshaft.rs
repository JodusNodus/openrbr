//! The broken driveshaft's clunks (`CAR::DRIVESHAFT`, `CTireSnd::M7590` 0x4d7590, read in the decompilation), from
//! each wheel's speed in the drivetrain payload (`rbr_recording::sound`, `p[10..14]`). `CDrive::M8480` (0x508480)
//! updates a wheel's speed only while its driveshaft is broken (the damage handler's word +0x3a30 + 0xc k is 0), so
//! with the shafts whole the speeds stay 0 and nothing plays. Pure; run once a game frame. rbr-recomp `notes/sound.md` 6.4.

use rbr_recording::sound::Sound;

use crate::voice::{Command, Shot};

/// The wheel speeds' first word in the drivetrain payload.
const SPEEDS: usize = 10;
/// The clunk period is one turn of the wheel: `1 / (|speed| * 1/(2 pi))` (0x731b1c) seconds.
const TURNS_PER_RADIAN: f32 = 0.159_154_94;
/// Only above 10 rad/s (0x72b154).
const MIN_SPEED: f32 = 10.0;

/// The clunks' state between frames.
#[derive(Debug, Clone, Default)]
pub struct DriveShafts {
    /// The `DriveShaftBroken` amp (times the effects volume in the mixer) and the bank's size.
    pub gain: f32,
    pub count: usize,
    /// Each wheel's seconds since its last clunk (`+0xb90`).
    timer: [f32; 4],
}

impl DriveShafts {
    pub fn new(gain: f32, count: usize) -> DriveShafts {
        DriveShafts { gain, count, ..DriveShafts::default() }
    }

    /// The clunks of a game frame of `dt` seconds, each wheel `wheels` m from the listener; `rand` stands for the
    /// original's `rand()`.
    pub fn frame(&mut self, sound: &Sound, wheels: [f32; 4], dt: f32, mut rand: impl FnMut() -> u32) -> Vec<Command> {
        let mut out = Vec::new();
        for (k, timer) in self.timer.iter_mut().enumerate() {
            let speed = sound.drivetrain_f32(SPEEDS + k);
            let period = if speed == 0.0 { 0.0 } else { (1.0 / (speed * TURNS_PER_RADIAN)).abs() };
            if period != 0.0 && speed.abs() > MIN_SPEED {
                *timer += dt;
                if period < *timer && self.count > 0 {
                    *timer = 0.0;
                    let shot = Shot::DriveShaft(rand() as usize % self.count);
                    out.push(Command::Hit { shot, gain: self.gain, pitch: 1.0, distance: wheels[k] });
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_free_wheel_above_10_rad_s_clunks_once_a_turn() {
        let mut sound = Sound::default();
        sound.drivetrain[SPEEDS] = (-20.0f32).to_bits();
        sound.drivetrain[SPEEDS + 1] = 9.0f32.to_bits();
        let mut shafts = DriveShafts::new(0.5, 13);
        let frames = (0..120).map(|_| shafts.frame(&sound, [2.0; 4], 1.0 / 60.0, || 27).len()).collect::<Vec<_>>();
        // 20 rad/s is a turn in 0.314 s: the timer passes it on the 19th frame, then again 19 frames later.
        let at: Vec<usize> = frames.iter().enumerate().filter(|(_, &n)| n > 0).map(|(i, _)| i).collect();
        assert_eq!(at, [18, 37, 56, 75, 94, 113]);
        assert_eq!(
            shafts.frame(&sound, [2.0; 4], 1.0, || 27),
            [Command::Hit { shot: Shot::DriveShaft(1), gain: 0.5, pitch: 1.0, distance: 2.0 }]
        );
    }
}
