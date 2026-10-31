//! The gearbox record a live stepping process writes after each frame (`rbr-standalone --live`; the viewer's H-shifter
//! rev-matches from it). Ours. Little-endian:
//!
//! ```text
//! u32 tag          'GBX1'
//! u32 length       40
//! f32 drive_speed  the engine speed per unit of gear ratio the wheels give (rad/s): the drive state's +0x384 times
//!                  the definition's +0xa54, the term `CDrive::M5A10` (0x505a10) compares the engine speed over the
//!                  gear's ratio with
//! u32 count        the gear count (`NumberOfGears`, the definition's +0x654), reverse and neutral included
//! f32 ratios[8]    each gear's ratio (the definition's gear bands, +0x694 + 0x50 * gear): 0 reverse, 1 neutral,
//!                  2 first gear, ...
//! ```
//!
//! So the engine speed in gear `g` with the clutch engaged is about `ratios[g] * drive_speed`.
//!
//! It is one of the tagged records (`crate::live`), so a reader that does not know it skips it by its length.

use std::io;

/// The tag.
pub const GBX1: [u8; 4] = *b"GBX1";
/// The ratios the record holds.
pub const GEARS: usize = 8;
const PAYLOAD_BYTES: usize = 8 + 4 * GEARS;

/// One gearbox record.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Gearbox {
    pub drive_speed: f32,
    pub count: u32,
    pub ratios: [f32; GEARS],
}

impl Gearbox {
    /// The engine speed gear `gear` would give with the clutch engaged, rad/s; `None` for a gear the car does not have.
    pub fn engine_speed(&self, gear: u8) -> Option<f32> {
        let ratio = *self.ratios.get(usize::from(gear)).filter(|_| u32::from(gear) < self.count)?;
        Some((ratio * self.drive_speed).abs())
    }

    /// The record.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(PAYLOAD_BYTES);
        payload.extend_from_slice(&self.drive_speed.to_le_bytes());
        payload.extend_from_slice(&self.count.to_le_bytes());
        self.ratios.iter().for_each(|r| payload.extend_from_slice(&r.to_le_bytes()));
        crate::live::record(GBX1, &payload)
    }

    /// Reads the record's payload (the bytes after its tag and length, `crate::live::read_records`).
    pub fn from_payload(payload: &[u8]) -> io::Result<Gearbox> {
        let payload: &[u8; PAYLOAD_BYTES] =
            crate::sized(payload, format!("a gearbox record of {} bytes", payload.len()))?;
        Ok(Gearbox {
            drive_speed: f32::from_bits(crate::word(payload, 0)),
            count: crate::word(payload, 1),
            ratios: std::array::from_fn(|g| f32::from_bits(crate::word(payload, 2 + g))),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_round_trips() {
        let record = Gearbox { drive_speed: 42.5, count: 7, ratios: [-3.5, 0.0, 3.2, 2.1, 1.5, 1.2, 1.0, 0.0] };
        let mut out = record.to_bytes();
        out.extend_from_slice(b"SCN1");
        let records = crate::live::read_records(&mut &out[..]).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, GBX1);
        assert_eq!(Gearbox::from_payload(&records[0].1).unwrap(), record);
        assert!(Gearbox::from_payload(&[0; 4]).is_err());
        assert_eq!(record.engine_speed(0), Some(148.75));
        assert_eq!(record.engine_speed(3), Some(2.1 * 42.5));
        assert_eq!(record.engine_speed(7), None);
    }
}
