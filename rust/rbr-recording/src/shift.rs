//! The shift-point record a live stepping process writes after each frame (`rbr-standalone --live`; the viewer's
//! working dials light their shift lights from it, `rbr_render::dials`). Ours. Little-endian:
//!
//! ```text
//! u32 tag        'SHF1'
//! u32 length     84
//! f32 up[10]     each gear's upshift point, rpm (the VCU definition's `Gear<N>Upshift`, `*(car+0x850)` +0x18 +
//!                0x20 * gear; 0: none): 0 reverse, 1 neutral, 2 first gear, ...
//! f32 down[10]   each gear's downshift point, rpm (`Gear<N>Downshift`, +0x28 + 0x20 * gear)
//! f32 limit      the rev limit, rpm (`RPMLimit`, +0x188)
//! ```
//!
//! It is one of the tagged records (`crate::live`), so a reader that does not know it skips it by its length.

use std::io;

/// The tag.
pub const SHF1: [u8; 4] = *b"SHF1";
/// The gears the record holds.
pub const GEARS: usize = 10;
const PAYLOAD_BYTES: usize = 4 * (2 * GEARS + 1);

/// One shift-point record.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ShiftPoints {
    pub up: [f32; GEARS],
    pub down: [f32; GEARS],
    pub limit: f32,
}

impl ShiftPoints {
    /// The record.
    pub fn to_bytes(&self) -> Vec<u8> {
        let payload: Vec<u8> =
            self.up.iter().chain(&self.down).chain([&self.limit]).flat_map(|f| f.to_le_bytes()).collect();
        let mut out = Vec::with_capacity(8 + PAYLOAD_BYTES);
        crate::live::write_record(&mut out, SHF1, &payload);
        out
    }

    /// Reads the record's payload (the bytes after its tag and length, `crate::live::read_records`).
    pub fn from_payload(payload: &[u8]) -> io::Result<ShiftPoints> {
        let payload: &[u8; PAYLOAD_BYTES] = payload.try_into().map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, format!("a shift-point record of {} bytes", payload.len()))
        })?;
        let float = |i: usize| f32::from_le_bytes(payload[4 * i..4 * i + 4].try_into().unwrap());
        Ok(ShiftPoints {
            up: std::array::from_fn(float),
            down: std::array::from_fn(|g| float(GEARS + g)),
            limit: float(2 * GEARS),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_round_trips() {
        let mut record = ShiftPoints { limit: 7500.0, ..ShiftPoints::default() };
        record.up[2] = 6000.0;
        record.down[3] = 3640.0;
        let mut out = record.to_bytes();
        out.extend_from_slice(b"SCN1");
        let records = crate::live::read_records(&mut &out[..]).unwrap();
        assert_eq!(records[0].0, SHF1);
        assert_eq!(ShiftPoints::from_payload(&records[0].1).unwrap(), record);
        assert!(ShiftPoints::from_payload(&[0; 4]).is_err());
    }
}
