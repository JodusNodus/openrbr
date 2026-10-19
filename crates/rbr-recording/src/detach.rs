//! The detached car parts a live stepping process writes after each damage record (`rbr-standalone --live`), which the
//! viewer draws at their physics poses (rbr-recomp `notes/car-damage-visuals.md`, section 8). Little-endian:
//!
//! ```text
//! u32 tag       'DET1'
//! u32 len       the entries' bytes (a multiple of ENTRY_BYTES)
//! per entry (32 bytes):
//! u32 part      the index into PARTS
//! f32 position[3]     the body's world position, the physics frame (m)
//! f32 orientation[4]  its world orientation, [x, y, z, w]
//! ```
//!
//! An entry is a detached part whose body is awake: in the physics awake list (`+0x450`) with its parent cleared by the
//! detach. A part that falls asleep is left out, and the viewer keeps its last pose. A reader skips a tag it does not
//! know by its length.

use std::io;

/// The tag.
pub const DET1: [u8; 4] = *b"DET1";
/// The detachable parts, by their exterior node names (`CAR_PARTS` in `rbr-standalone`): the doors and bonnet (hinged
/// bodies), the exhaust, the bumpers and the wing (mounted bodies). The index is the record's `part`.
pub const PARTS: [&str; 7] = ["e_door_l", "e_door_r", "e_bonnet", "e_exhaust", "e_bumper_f", "e_bumper_b", "e_wing"];
/// The bytes of one entry.
pub const ENTRY_BYTES: usize = 32;

/// The index in [`PARTS`] of the node `name`.
pub fn part_index(name: &str) -> Option<u32> {
    PARTS.iter().position(|&part| part == name).map(|i| i as u32)
}

/// One detached part's body pose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartPose {
    pub part: u32,
    pub position: [f32; 3],
    pub orientation: [f32; 4],
}

/// The record with `poses` as its entries.
pub fn to_bytes(poses: &[PartPose]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(ENTRY_BYTES * poses.len());
    for pose in poses {
        payload.extend_from_slice(&pose.part.to_le_bytes());
        for value in pose.position.into_iter().chain(pose.orientation) {
            payload.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut out = Vec::with_capacity(8 + payload.len());
    crate::live::write_record(&mut out, DET1, &payload);
    out
}

/// The entries of a record's payload (the bytes after its tag and length).
pub fn parse(payload: &[u8]) -> io::Result<Vec<PartPose>> {
    if !payload.len().is_multiple_of(ENTRY_BYTES) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("{} bytes of detached parts", payload.len())));
    }
    payload
        .chunks_exact(ENTRY_BYTES)
        .map(|entry| {
            let word = |i: usize| u32::from_le_bytes(entry[4 * i..4 * i + 4].try_into().unwrap());
            let float = |i: usize| f32::from_bits(word(i));
            let part = word(0);
            if part as usize >= PARTS.len() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, format!("detached part {part}")));
            }
            Ok(PartPose {
                part,
                position: [float(1), float(2), float(3)],
                orientation: [float(4), float(5), float(6), float(7)],
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_round_trip() {
        let poses = [
            PartPose { part: 4, position: [1.0, -2.5, 0.25], orientation: [0.0, 0.0, 0.0, 1.0] },
            PartPose { part: 0, position: [0.0; 3], orientation: [0.5, 0.5, 0.5, 0.5] },
        ];
        let bytes = to_bytes(&poses);
        assert_eq!(&bytes[..4], b"DET1");
        assert_eq!(bytes.len(), 8 + 2 * ENTRY_BYTES);
        assert_eq!(parse(&bytes[8..]).unwrap(), poses);
        assert_eq!(parse(&[]).unwrap(), []);
    }

    #[test]
    fn bad_payloads_are_errors() {
        assert!(parse(&[0; 31]).is_err());
        let mut bytes = to_bytes(&[PartPose { part: 0, position: [0.0; 3], orientation: [0.0; 4] }]);
        bytes[8] = PARTS.len() as u8;
        assert!(parse(&bytes[8..]).is_err());
    }

    #[test]
    fn part_names_index_the_record() {
        assert_eq!(part_index("e_bumper_f"), Some(4));
        assert_eq!(part_index("e_body"), None);
    }
}
