//! The impact sounds from the damage handler's `CSigCarDamage` payload (`rbr_recording::sound`, `hit`), as
//! `CCollisionSnd::OnDamage` (0x4b27c0) and `Vf64` (0x4b2ed0) play them. Pure. rbr-recomp `notes/sound.md` section 7.
//!
//! The handler keeps its last payload until the next impact overwrites it, so a new impact is a payload that differs
//! from the last step's (ours: the original runs inside the emit). Words 0 and 2 (the functor's result and tag) are
//! left out: `E36DamageOps::Slot1` rewrites them on calls that emit nothing. Only the ranged set sounds in retail
//! (`SOUNDDEF::SHATTER` and `GLASS` are muted), and the scream in the car.

use crate::voice::{Command, Shot};

/// The payload's state word and impact power.
const STATE: usize = 1;
const POWER: usize = 4;
/// The impact sounds from a power of 100 (0x72bf10); the ranged set takes the power plus 500 (0x731cd4) and plays
/// from 200 (0x730890), its index rising by 0.005 per unit above 200. Above 8000 (0x731e08) in the car, the speech
/// voice stops and the scream plays.
const MIN_POWER: f32 = 100.0;
const RANGED_ADD: f32 = 500.0;
const RANGED_FROM: f32 = 200.0;
const RANGED_STEP: f32 = 0.005;
const SCREAM_POWER: f32 = 8000.0;

/// The impact state between steps.
#[derive(Debug, Clone, Default)]
pub struct Impacts {
    /// `SOUNDDEF::RANGED` volume times the `Impacts` amp.
    pub ranged_gain: f32,
    /// The `ImpactScreams` amp.
    pub scream_gain: f32,
    pub ranged_count: usize,
    pub scream_count: usize,
    last: Option<[u32; 18]>,
}

impl Impacts {
    pub fn new(ranged_gain: f32, scream_gain: f32, ranged_count: usize, scream_count: usize) -> Impacts {
        Impacts { ranged_gain, scream_gain, ranged_count, scream_count, last: None }
    }

    /// The commands for a step's payload; `rand` stands for the original's `rand()` calls. The first payload seen
    /// is taken as the state before the drive.
    pub fn step(&mut self, hit: &[u32; 20], inside: bool, mut rand: impl FnMut() -> u32) -> Vec<Command> {
        let emitted: [u32; 18] = std::array::from_fn(|i| hit[if i == 0 { 1 } else { i + 2 }]);
        let new = self.last.is_some_and(|last| last != emitted);
        self.last = Some(emitted);
        let power = f32::from_bits(hit[POWER]);
        if !new || power < MIN_POWER {
            return Vec::new();
        }
        let mut out = Vec::new();
        if power > SCREAM_POWER && inside && self.scream_count > 0 {
            out.push(Command::StopSpeech);
            out.push(Command::Shot { shot: Shot::Scream(rand() as usize % self.scream_count), gain: self.scream_gain });
        }
        if (1..4).contains(&(hit[STATE] as i32)) {
            if let Some(index) = ranged_index(power + RANGED_ADD, self.ranged_count, &mut rand) {
                out.push(Command::Shot { shot: Shot::Ranged(index), gain: self.ranged_gain });
            }
        }
        out
    }
}

/// `Vf64`'s pick: `rand() % 3` plus the steps above 200, and past the end `count - 1 - rand() % 3`.
pub fn ranged_index(v: f32, count: usize, rand: &mut impl FnMut() -> u32) -> Option<usize> {
    if v < RANGED_FROM || count == 0 {
        return None;
    }
    let index = (rand() % 3) as usize + ((v - RANGED_FROM) * RANGED_STEP) as usize;
    Some(if index >= count { (count - 1).saturating_sub((rand() % 3) as usize) } else { index })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(power: f32) -> [u32; 20] {
        let mut hit = [0; 20];
        hit[STATE] = 2;
        hit[POWER] = power.to_bits();
        hit
    }

    #[test]
    fn the_ranged_index_follows_the_power() {
        let mut zero = || 0;
        assert_eq!(ranged_index(150.0, 20, &mut zero), None);
        assert_eq!(ranged_index(1500.0, 20, &mut zero), Some(6));
        assert_eq!(ranged_index(9000.0, 20, &mut || 2), Some(17));
    }

    #[test]
    fn a_new_payload_plays_once_and_a_hard_one_screams_in_the_car() {
        let mut impacts = Impacts::new(1.0, 0.5, 20, 2);
        assert!(impacts.step(&hit(900.0), true, || 0).is_empty());
        assert!(impacts.step(&hit(900.0), true, || 0).is_empty());
        let mut functor_only = hit(900.0);
        functor_only[0] = 7;
        functor_only[2] = 3;
        assert!(impacts.step(&functor_only, true, || 0).is_empty());
        let out = impacts.step(&hit(1000.0), true, || 0);
        assert_eq!(out, [Command::Shot { shot: Shot::Ranged(6), gain: 1.0 }]);
        let out = impacts.step(&hit(9000.0), true, || 1);
        assert_eq!(out[..2], [Command::StopSpeech, Command::Shot { shot: Shot::Scream(1), gain: 0.5 }]);
        assert_eq!(impacts.step(&hit(9500.0), false, || 0).len(), 1);
    }
}
