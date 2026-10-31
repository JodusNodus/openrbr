//! The HUD record a live stepping process writes after each frame (`harness/live.py`'s `hud`, `rbr-standalone --live`),
//! which the viewer reads. Little-endian:
//!
//! ```text
//! u32 tag           'HUD1' (0x31445548) or 'HUD2' (0x32445548)
//! u32 gear          the drive's gear index: 0 reverse, 1 neutral, 2 first gear, ...
//! f32 engine_speed  rad/s
//! f32 speed         m/s
//! HUD2 only, the race clock (`rbr_gameplay::race_clock`):
//! f32 time          the race time, s, penalties included
//! f32 countdown     the start countdown, s (at or below 0 after the go)
//! f32 penalties     the outside-assistance penalties added so far, s
//! u32 flags         bit 0: the clock runs; bit 1: the jump start was taken
//! ```
//!
//! `harness/live.py` writes HUD1 (the original has no race clock to read there); `rbr-standalone --live` writes HUD2.

use std::io::{self, Read};

use crate::{invalid, word};

/// The tags.
pub const HUD1: u32 = 0x3144_5548;
pub const HUD2: u32 = 0x3244_5548;
/// The records' sizes.
pub const HUD1_BYTES: usize = 16;
pub const HUD2_BYTES: usize = 32;

const RUNNING: u32 = 1;
const JUMP_START: u32 = 2;

/// One HUD record.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Hud {
    pub gear: u32,
    pub engine_speed: f32,
    pub speed: f32,
    /// HUD2's race clock.
    pub race: Option<RaceHud>,
}

/// HUD2's race clock.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RaceHud {
    pub time: f32,
    pub countdown: f32,
    pub penalties: f32,
    pub running: bool,
    pub jump_start: bool,
}

impl Hud {
    /// The record: HUD2 with a race clock, else HUD1.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HUD2_BYTES);
        out.extend_from_slice(&if self.race.is_some() { HUD2 } else { HUD1 }.to_le_bytes());
        out.extend_from_slice(&self.gear.to_le_bytes());
        out.extend_from_slice(&self.engine_speed.to_le_bytes());
        out.extend_from_slice(&self.speed.to_le_bytes());
        if let Some(race) = self.race {
            let flags = if race.running { RUNNING } else { 0 } | if race.jump_start { JUMP_START } else { 0 };
            for word in [race.time.to_bits(), race.countdown.to_bits(), race.penalties.to_bits(), flags] {
                out.extend_from_slice(&word.to_le_bytes());
            }
        }
        out
    }

    /// Reads one record of either tag.
    pub fn read(reader: &mut impl Read) -> io::Result<Hud> {
        let mut head = [0u8; HUD1_BYTES];
        reader.read_exact(&mut head)?;
        let race = match word(&head, 0) {
            HUD1 => None,
            HUD2 => {
                let mut rest = [0u8; HUD2_BYTES - HUD1_BYTES];
                reader.read_exact(&mut rest)?;
                let flags = word(&rest, 3);
                Some(RaceHud {
                    time: f32::from_bits(word(&rest, 0)),
                    countdown: f32::from_bits(word(&rest, 1)),
                    penalties: f32::from_bits(word(&rest, 2)),
                    running: flags & RUNNING != 0,
                    jump_start: flags & JUMP_START != 0,
                })
            }
            tag => {
                return Err(invalid(format!("{tag:#010x} is not a HUD record")));
            }
        };
        Ok(Hud {
            gear: word(&head, 1),
            engine_speed: f32::from_bits(word(&head, 2)),
            speed: f32::from_bits(word(&head, 3)),
            race,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_records_round_trip() {
        let hud1 = Hud { gear: 3, engine_speed: 512.5, speed: 21.25, race: None };
        let race = RaceHud { time: 27.455, countdown: -12.5, penalties: 10.0, running: true, jump_start: true };
        let hud2 = Hud { race: Some(race), ..hud1 };
        let mut stream = hud1.to_bytes();
        stream.extend(hud2.to_bytes());
        assert_eq!(stream.len(), HUD1_BYTES + HUD2_BYTES);
        let mut reader = stream.as_slice();
        assert_eq!(Hud::read(&mut reader).unwrap(), hud1);
        assert_eq!(Hud::read(&mut reader).unwrap(), hud2);
        assert!(reader.is_empty());
    }

    #[test]
    fn hud1_is_live_py_s_layout() {
        // harness/live.py: struct.pack("<I", HUD_MAGIC) + gear + engine speed + speed
        let bytes = Hud { gear: 2, engine_speed: 1.0, speed: 2.0, race: None }.to_bytes();
        assert_eq!(&bytes[..4], b"HUD1");
        assert_eq!(bytes[4..8], 2u32.to_le_bytes());
        assert_eq!(bytes[8..12], 1.0f32.to_le_bytes());
        assert_eq!(bytes[12..16], 2.0f32.to_le_bytes());
    }

    #[test]
    fn other_tags_are_errors() {
        let mut bytes = Hud::default().to_bytes();
        bytes[3] = b'3';
        assert_eq!(Hud::read(&mut bytes.as_slice()).unwrap_err().kind(), io::ErrorKind::InvalidData);
    }
}
