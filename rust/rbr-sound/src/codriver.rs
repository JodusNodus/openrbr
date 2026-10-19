//! The co-driver (rbr-recomp `notes/sound.md` sections 9 and 10): the pace-note phrases of `CPaceNoteHandler`'s
//! `CSigPaceNoteCallout` handler (0x4b65b0, read in the disassembly), the countdown numbers of `CSoundFX` (0x4ba810)
//! on the HUD's countdown codes (`FUN_00578c30`), and the damage calls of `CCarSound::OnSpeech` (0x4ab940) with
//! their senders. Pure: speech commands per step.

use rbr_gameplay::markers::{flag_word_id, slot_word_id};
use rbr_recording::pacenote::Callout;
use rbr_recording::sound::Sound;

use crate::voice::{Command, Shot, Speech};

/// The word ids the handler queues itself (`Word5880` 0x4b5880).
pub const AND: u32 = 0x10;
pub const INTO: u32 = 0x1f;
pub const ONTO: u32 = 0x30;

/// The tags the handler treats apart: the split (silent), the distance key (clears the pending words), the finish
/// (no INTO after it) and the bridge (ONTO, not INTO).
const TAG_DISTANCE: u32 = 0x0d;
const TAG_FINISH: u32 = 0x16;
const TAG_SPLIT: u32 = 0x17;
const TAG_BRIDGE: u32 = 0x1b;
/// Next tags that take no INTO.
const NO_INTO: [u32; 8] = [0x16, 0x13, 0x1c, 0x10, 0x14, 0x1e, 0x1d, 0x1f];

/// `FUN_00572e60`'s distances: a value snaps to the nearer end of the segment it is in (below 10 to 10, above 1000
/// to 1000).
const SNAP: [f32; 16] =
    [10.0, 15.0, 30.0, 50.0, 70.0, 100.0, 150.0, 200.0, 300.0, 400.0, 500.0, 600.0, 700.0, 800.0, 900.0, 1000.0];

/// `FUN_00572e60`.
pub fn snap(value: f32) -> f32 {
    if value < SNAP[0] {
        return SNAP[0];
    }
    for pair in SNAP.windows(2) {
        let t = (value - pair[0]) / (pair[1] - pair[0]);
        if (0.0..=1.0).contains(&t) {
            return if t < 0.5 { pair[0] } else { pair[1] };
        }
    }
    SNAP[15]
}

/// The RANGE word of a snapped distance (`M5760`'s table).
fn range_word(distance: i32) -> u32 {
    match distance {
        30 => 0xa,
        50 => 0xc,
        70 => 0xd,
        100 => 7,
        150 => 8,
        200 => 9,
        300 => 0xb,
        400 => 0,
        500 => 1,
        600 => 2,
        700 => 3,
        800 => 4,
        900 => 5,
        _ => 6,
    }
}

/// The handler's two pending flags (`+0x50` INTO, `+0x54` AND).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Phrases {
    into: bool,
    and: bool,
}

impl Phrases {
    /// `M5760(value, need)`: below 11 the next note takes INTO, below 30 an AND; otherwise, when asked, the RANGE
    /// word. The flags are left as they are from 30 up.
    fn distance(&mut self, value: i32, need: bool) -> Option<u32> {
        let snapped = snap(value as f32) as i32;
        if snapped < 11 {
            (self.into, self.and) = (true, false);
            None
        } else if snapped < 30 {
            (self.into, self.and) = (false, true);
            None
        } else if need {
            Some(range_word(snapped))
        } else {
            None
        }
    }

    /// The words one callout queues, in order.
    pub fn callout(&mut self, c: &Callout) -> Vec<u32> {
        let mut words = Vec::new();
        if c.tag == TAG_SPLIT {
            return words;
        }
        if c.tag == TAG_DISTANCE {
            (self.into, self.and) = (false, false);
            return words;
        }
        if self.and {
            words.push(AND);
        }
        self.and = false;
        words.extend([slot_word_id(c.tag), flag_word_id(c.word1)].into_iter().filter(|&w| w >= 0).map(|w| w as u32));
        // The payload's distance, or with none the key's gap to the next spoken key, which only sets the flags.
        let range =
            if c.seek_d == 0.0 { self.distance(c.gap as i32, false) } else { self.distance(c.seek_d as i32, true) };
        words.extend(range);
        if c.next_tag >= 0 && self.into {
            if c.tag != TAG_FINISH && !NO_INTO.contains(&(c.next_tag as u32)) {
                words.push(if c.tag == TAG_BRIDGE { ONTO } else { INTO });
            }
            self.into = false;
        }
        words
    }
}

/// The HUD's countdown codes: 8 at 3 s, 7 at 2 s, 6 at 1 s, 9 at 0 (while above -2), once each. A mark the
/// countdown was already below when first seen is skipped (ours: a drive joined late, as a snapshot's, says nothing).
const COUNTDOWN: [(f32, u32); 4] = [(3.0, 3), (2.0, 2), (1.0, 1), (0.0, 0)];
const COUNTDOWN_END: f32 = -2.0;

/// The damage calls' senders: the battery at or below 6 (0x731b0c) sends 1, no brakes 4, water above 120 (0x731b08)
/// 5; the broken gearbox (the drivetrain's word 4 times `1 / (2 pi) * 0.5`) and the damage handler's speech ring
/// send 8.
const BATTERY_FLAT: f32 = 6.0;
const WATER_HOT: f32 = 120.0;
const INV_TAU: f32 = 0.159_154_94;

/// The co-driver's state between steps.
#[derive(Debug, Clone, Default)]
pub struct CoDriver {
    phrases: Phrases,
    /// The countdown codes already sent (HUD `+0x250`), and the highest countdown seen.
    countdown: [bool; 4],
    countdown_seen: Option<f32>,
    /// The damage calls already made (`CCarSound +0x4c`; never reset in a drive).
    latch: u32,
    /// The speech ring index last seen.
    ring: Option<u32>,
    /// A replay: no damage calls (`E19IsState2`).
    pub replay: bool,
    /// The co-driver switch is off (option `+0x3e8 & 4`, inferred): no pace notes. The countdown and the damage
    /// calls do not test it.
    pub muted: bool,
}

impl CoDriver {
    /// The commands of one step: the callouts' words, the countdown, and the damage calls.
    pub fn step(&mut self, sound: &Sound, callouts: &[Callout], countdown: Option<f32>) -> Vec<Command> {
        let mut out = Vec::new();
        for c in callouts.iter().filter(|_| !self.muted) {
            out.extend(self.phrases.callout(c).into_iter().map(|w| Command::Speech(Speech::Word(w))));
        }
        if let Some(t) = countdown.filter(|&t| t > COUNTDOWN_END) {
            let seen = *self.countdown_seen.get_or_insert(t);
            self.countdown_seen = Some(seen.max(t));
            for (k, &(at, number)) in COUNTDOWN.iter().enumerate() {
                if t <= at && seen > at && !self.countdown[k] {
                    self.countdown[k] = true;
                    out.push(if number == 0 {
                        Command::Shot { shot: Shot::Go, gain: 1.0 }
                    } else {
                        Command::Speech(Speech::Number(number))
                    });
                }
            }
        }
        let mut calls = Vec::new();
        if sound.battery <= BATTERY_FLAT {
            calls.push(1);
        }
        if sound.brakes == 0 {
            calls.push(4);
        }
        if sound.water > WATER_HOT {
            calls.push(5);
        }
        if f32::from_bits(sound.drivetrain[4]) * INV_TAU * 0.5 != 0.0 {
            calls.push(8);
        }
        if self.ring.is_some_and(|ring| ring != sound.speech_ring) {
            calls.push(8);
        }
        self.ring = Some(sound.speech_ring);
        for code in calls {
            out.extend(self.damage_call(code));
        }
        out
    }

    /// The damage calls of other senders: the suspension's broken damper (7) and the engine fire (6).
    pub fn damage_calls(&mut self, codes: &[u32]) -> Vec<Command> {
        codes.iter().filter_map(|&code| self.damage_call(code)).collect()
    }

    /// `OnSpeech`: each code once, none in a replay.
    fn damage_call(&mut self, code: u32) -> Option<Command> {
        let bit = 1 << (code & 0x1f);
        if self.replay || self.latch & bit != 0 {
            return None;
        }
        self.latch |= bit;
        Some(Command::Speech(Speech::Damage(code)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn callout(tag: u32, word1: u32, seek_d: f32, gap: f32, next_tag: i32) -> Callout {
        Callout { tag, word1, seek_d, gap, next_tag }
    }

    #[test]
    fn distances_snap_to_the_nearer_call() {
        assert_eq!(snap(4.0), 10.0);
        assert_eq!(snap(22.0), 15.0);
        assert_eq!(snap(23.0), 30.0);
        assert_eq!(snap(124.0), 100.0);
        assert_eq!(snap(125.0), 150.0);
        assert_eq!(snap(5000.0), 1000.0);
    }

    #[test]
    fn a_callout_gives_its_words_and_distance() {
        let mut p = Phrases::default();
        // Tag 3 (MEDIUMLEFT), flag 0x400 (LONG), 140 m to the next note: "medium left long 150".
        assert_eq!(p.callout(&callout(3, 0x400, 140.0, 140.0, 10)), [0x25, 0x23, 8]);
        // Tag 1 (90_LEFT) with no payload distance, 8 m to tag 10: "90 left into", then "90 right".
        assert_eq!(p.callout(&callout(1, 0, 0.0, 8.0, 10)), [0x0e, INTO]);
        assert_eq!(p.callout(&callout(10, 0, 0.0, 200.0, 2)), [0x0f]);
        // 20 m: an AND before the next note.
        assert_eq!(p.callout(&callout(4, 0, 0.0, 20.0, 5)), [0x19]);
        assert_eq!(p.callout(&callout(5, 0, 0.0, 300.0, 6)), [AND, 0x17]);
    }

    #[test]
    fn into_onto_and_their_exceptions() {
        let mut p = Phrases::default();
        // A bridge right before a note: "bridge onto".
        assert_eq!(p.callout(&callout(0x1b, 0, 0.0, 5.0, 3)), [0x11, ONTO]);
        // Before a jump (next tag 0x14) no INTO, and the flag is spent.
        assert_eq!(p.callout(&callout(3, 0, 0.0, 5.0, 0x14)), [0x25]);
        assert_eq!(p.callout(&callout(3, 0, 0.0, 300.0, 4)), [0x25]);
        // At the end of the list the flag stays for the next callout.
        assert_eq!(p.callout(&callout(3, 0, 0.0, -1.0, -1)), [0x25]);
        assert_eq!(p.callout(&callout(4, 0, 0.0, 300.0, 5)), [0x19, INTO]);
        // The distance key clears both, the split says nothing.
        p.callout(&callout(4, 0, 0.0, 20.0, 5));
        assert_eq!(p.callout(&callout(TAG_DISTANCE, 0, 0.0, 0.0, 5)), []);
        assert_eq!(p.callout(&callout(TAG_SPLIT, 0, 0.0, 0.0, 5)), []);
        assert_eq!(p.callout(&callout(5, 0, 0.0, 300.0, 6)), [0x17]);
    }

    #[test]
    fn the_countdown_says_three_two_one_go_once() {
        let mut c = CoDriver::default();
        let sound = Sound { battery: 12.0, brakes: 1, ..Sound::default() };
        let mut said = Vec::new();
        for k in 0..900 {
            said.extend(c.step(&sound, &[], Some(7.0 - k as f32 * 0.01)));
        }
        assert_eq!(
            said,
            [
                Command::Speech(Speech::Number(3)),
                Command::Speech(Speech::Number(2)),
                Command::Speech(Speech::Number(1)),
                Command::Shot { shot: Shot::Go, gain: 1.0 },
            ]
        );
        // Joined at 1.5 s: only "one" and the go.
        let mut late = CoDriver::default();
        let said: Vec<_> = (0..300).flat_map(|k| late.step(&sound, &[], Some(1.5 - k as f32 * 0.01))).collect();
        assert_eq!(said, [Command::Speech(Speech::Number(1)), Command::Shot { shot: Shot::Go, gain: 1.0 }]);
    }

    #[test]
    fn damage_calls_play_once_and_never_in_a_replay() {
        let mut c = CoDriver::default();
        let mut sound = Sound { battery: 12.0, brakes: 1, water: 130.0, ..Sound::default() };
        assert_eq!(c.step(&sound, &[], None), [Command::Speech(Speech::Damage(5))]);
        assert!(c.step(&sound, &[], None).is_empty());
        sound.speech_ring = 1;
        assert_eq!(c.step(&sound, &[], None), [Command::Speech(Speech::Damage(8))]);
        let mut replay = CoDriver { replay: true, ..CoDriver::default() };
        assert!(replay.step(&sound, &[], None).is_empty());
    }
}
