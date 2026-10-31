//! The steering-force record a live stepping process writes after each frame (`rbr-standalone --live`; the viewer
//! drives the wheel's force feedback from it). Ours: the game sends the force to the wheel, not a record.
//! Little-endian:
//!
//! ```text
//! u32 tag     'FFB1'
//! u32 length  4
//! f32 force   the steering force (`steering_ffb::force`), [-1, 1] before the player's settings
//! ```
//!
//! It is one of the tagged records (`crate::live`), so a reader that does not know it skips it by its length.

use std::io;

/// The tag.
pub const FFB1: [u8; 4] = *b"FFB1";
const PAYLOAD_BYTES: usize = 4;

/// One steering-force record.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Steering {
    pub force: f32,
}

impl Steering {
    /// The record.
    pub fn to_bytes(&self) -> Vec<u8> {
        crate::live::record(FFB1, &self.force.to_le_bytes())
    }

    /// Reads the record's payload (the bytes after its tag and length, `crate::live::read_records`).
    pub fn from_payload(payload: &[u8]) -> io::Result<Steering> {
        let payload: &[u8; PAYLOAD_BYTES] =
            crate::sized(payload, format!("a force record of {} bytes", payload.len()))?;
        Ok(Steering { force: f32::from_le_bytes(*payload) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_round_trips() {
        let record = Steering { force: -0.375 };
        let mut reader = &record.to_bytes()[..];
        let mut out = Vec::new();
        out.extend_from_slice(reader);
        out.extend_from_slice(b"SCN1");
        reader = &out[..];
        let records = crate::live::read_records(&mut reader).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, FFB1);
        assert_eq!(Steering::from_payload(&records[0].1).unwrap(), record);
        assert!(Steering::from_payload(&[0; 8]).is_err());
    }
}
