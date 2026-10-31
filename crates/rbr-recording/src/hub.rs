//! The hub-offset record a live stepping process writes after each frame (`rbr-standalone --live`; the viewer places
//! each tyre by it, `rbr_render::playback`). Ours. Little-endian:
//!
//! ```text
//! u32 tag         'HUB1'
//! u32 length      16
//! f32 offset[4]   each wheel's tyre `HubOffset` (the tyre tail's +0x6c, `wheel/layout/fields.rs`), wheels 0 to 3
//! ```
//!
//! The wheel block's hub (`+0x140`) is the hub moved along the axle by `-HubOffset` (`M3B900`), and `Copy100` moves it
//! back to place the wheel; the recordings' wheel blocks do not hold the offset, which lives in the tyre.
//!
//! It is one of the tagged records (`crate::live`), so a reader that does not know it skips it by its length.

use std::io;

use crate::{sized, word, WHEELS};

/// The tag.
pub const HUB1: [u8; 4] = *b"HUB1";
const PAYLOAD_BYTES: usize = 4 * WHEELS;

/// The tagged record of the wheels' hub offsets.
pub fn record(offsets: [f32; WHEELS]) -> Vec<u8> {
    let payload: Vec<u8> = offsets.iter().flat_map(|o| o.to_le_bytes()).collect();
    crate::live::record(HUB1, &payload)
}

/// Reads the record's payload (the bytes after its tag and length, `crate::live::read_records`).
pub fn from_payload(payload: &[u8]) -> io::Result<[f32; WHEELS]> {
    let payload: &[u8; PAYLOAD_BYTES] = sized(payload, format!("a hub record of {} bytes", payload.len()))?;
    Ok(std::array::from_fn(|k| f32::from_bits(word(payload, k))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_round_trips() {
        let offsets = [-0.045, -0.045, -0.04, 0.0];
        let mut out = record(offsets);
        out.extend_from_slice(b"SCN1");
        let records = crate::live::read_records(&mut &out[..]).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, HUB1);
        assert_eq!(from_payload(&records[0].1).unwrap(), offsets);
        assert!(from_payload(&[0; 4]).is_err());
    }
}
