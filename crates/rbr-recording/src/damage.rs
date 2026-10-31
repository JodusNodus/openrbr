//! The damage record a live stepping process writes after each HUD record (`harness/live.py`'s `damage`,
//! `rbr-standalone --live`), which the viewer dents and cracks the car from (`notes/renderer.md` "Damage visuals").
//! Little-endian:
//!
//! ```text
//! u32 tag       'DMG1' (0x31474d44)
//! f32 zones[64] the damage handler's zone values (`*(car+0x9f4)+0x3808`, 0 to 1)
//! u32 code      the handler's last `CSigCarDamage` record (+0x3e60): its event code, 3 or 4 cracking a window
//! u32 index     that record's functor index (+0x3e68)
//! ```
//!
//! The `CSigCarDamage` record stays in the handler until the next impact overwrites it, so a reader sees the last
//! event again in each record; the viewer's cracks take a repeated event once.

use std::io::{self, Read};

use crate::{invalid, word};

/// The tag.
pub const DMG1: u32 = 0x3147_4d44;
/// The tagged CDM1 record (`rbr_recording::live`, after FMB1): the handler's words the service park reads, as
/// `rbr_gameplay::car_damage::CarriedDamage`'s `to_bytes` lays them out. The HUD's car status reads each repair part's
/// level from it under NGP's rules.
pub const CDM1: [u8; 4] = *b"CDM1";
/// Zone values in the record.
pub const ZONES: usize = 64;
/// The record's size.
pub const DMG1_BYTES: usize = 4 + 4 * ZONES + 8;
/// `CCar` +0x9f4: the damage handler pointer.
pub const CAR_DAMAGE_HANDLER: u32 = 0x9f4;
/// Handler offsets: the zone values, and the `CSigCarDamage` record's event code and functor index.
pub const HANDLER_ZONES: u32 = 0x3808;
pub const HANDLER_EVENT_CODE: u32 = 0x3e60;
pub const HANDLER_EVENT_INDEX: u32 = 0x3e68;

/// One damage record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Damage {
    pub zones: [f32; ZONES],
    pub code: u32,
    pub index: u32,
}

impl Default for Damage {
    fn default() -> Self {
        Damage { zones: [0.0; ZONES], code: 0, index: 0 }
    }
}

impl Damage {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(DMG1_BYTES);
        out.extend_from_slice(&DMG1.to_le_bytes());
        for zone in self.zones {
            out.extend_from_slice(&zone.to_le_bytes());
        }
        out.extend_from_slice(&self.code.to_le_bytes());
        out.extend_from_slice(&self.index.to_le_bytes());
        out
    }

    pub fn read(reader: &mut impl Read) -> io::Result<Damage> {
        let mut bytes = [0u8; DMG1_BYTES];
        reader.read_exact(&mut bytes)?;
        if word(&bytes, 0) != DMG1 {
            let tag = word(&bytes, 0);
            return Err(invalid(format!("{tag:#010x} is not a damage record")));
        }
        Ok(Damage {
            zones: std::array::from_fn(|zone| f32::from_bits(word(&bytes, 1 + zone))),
            code: word(&bytes, 1 + ZONES),
            index: word(&bytes, 2 + ZONES),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_round_trip() {
        let mut damage = Damage { code: 3, index: 5, ..Damage::default() };
        damage.zones[1] = 1.0;
        damage.zones[63] = 0.25;
        let bytes = damage.to_bytes();
        assert_eq!(bytes.len(), DMG1_BYTES);
        assert_eq!(&bytes[..4], b"DMG1");
        assert_eq!(Damage::read(&mut bytes.as_slice()).unwrap(), damage);
    }

    #[test]
    fn other_tags_are_errors() {
        let mut bytes = Damage::default().to_bytes();
        bytes[3] = b'2';
        assert_eq!(Damage::read(&mut bytes.as_slice()).unwrap_err().kind(), io::ErrorKind::InvalidData);
    }
}
