//! The tagged records a live stepping process (`rbr-standalone --live`) writes between each frame's DMG1 and SCN1
//! records. Each record is a 4-byte tag, a u32 little-endian payload length and the
//! payload. SCN1 is always present and last, so it ends the list: a reader takes the tags it knows, skips the others
//! by their length and stops at SCN1, without waiting for the next frame.

use std::io::{self, Read};

use crate::invalid;

/// The car's control (`rbr_gameplay::control::CarControl`'s eight words, word 6 0 when unset) the frame's last step
/// took: the `CSigCarControl` payload the in-car driver turns the wheel by (LocoMotion `OnCarControl`), which the
/// frame's echoed input payload is not; zeros before the first step.
pub const CTL1: [u8; 4] = *b"CTL1";
/// The tag that ends the tagged records: the scenery poses (a count, then 32 bytes per pose, not a length).
pub const SCN1: [u8; 4] = *b"SCN1";
/// Payload bytes a record may have, at most (a guard against a bad stream).
pub const MAX_PAYLOAD: usize = 1 << 24;

/// One tagged record: the tag, the payload length and the payload.
pub fn record(tag: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + payload.len());
    write_record(&mut out, tag, payload);
    out
}

/// Appends one record to `out`.
pub fn write_record(out: &mut Vec<u8>, tag: [u8; 4], payload: &[u8]) {
    out.extend_from_slice(&tag);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
}

/// Reads records up to the SCN1 tag, as `(tag, payload)` pairs. The SCN1 tag is read too; its body is the caller's.
pub fn read_records(reader: &mut impl Read) -> io::Result<Vec<([u8; 4], Vec<u8>)>> {
    let mut records = Vec::new();
    loop {
        let mut tag = [0; 4];
        reader.read_exact(&mut tag)?;
        if tag == SCN1 {
            return Ok(records);
        }
        let mut len = [0; 4];
        reader.read_exact(&mut len)?;
        let len = u32::from_le_bytes(len) as usize;
        if len > MAX_PAYLOAD {
            return Err(invalid(format!("a {len}-byte live record")));
        }
        let mut payload = vec![0; len];
        reader.read_exact(&mut payload)?;
        records.push((tag, payload));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_are_read_up_to_scn1() {
        let mut out = Vec::new();
        write_record(&mut out, *b"SND1", &[1, 2, 3]);
        write_record(&mut out, *b"XYZ9", &[]);
        out.extend_from_slice(b"SCN1 poses");
        let mut reader = &out[..];
        let records = read_records(&mut reader).unwrap();
        assert_eq!(records, [(*b"SND1", vec![1, 2, 3]), (*b"XYZ9", vec![])]);
        assert_eq!(reader, b" poses");
    }
}
