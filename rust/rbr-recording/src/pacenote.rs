//! The pace-note record (`PCN1`) a live stepping process writes with each frame (`rbr_recording::live`): the step's
//! `CSigPaceNoteCallout` payloads (`rbr_gameplay::markers::PaceCallout`) with what the co-driver's handler (0x4b65b0,
//! rbr-recomp `notes/sound.md` 10.2) looks up in the key list besides. Payload, little-endian:
//!
//! ```text
//! u32 count
//! then per callout:
//! u32 tag        the key's tag
//! u32 word1      the key's flag word (bit 31, the fired bit, cleared)
//! f32 seek_d     the payload's distance (E12_SeekD, 0x476bf0, at the callout's x)
//! f32 gap        E12_KeyGap (0x476b10): the key's distance to the next spoken key, -1 without one
//! i32 next_tag   the next key's tag (E12_KeyList::NextKey, 0x475de0), -1 at the end of the list
//! ```

/// The tag.
pub const PCN1: [u8; 4] = *b"PCN1";
/// Bytes per callout.
pub const CALLOUT_BYTES: usize = 20;

/// One callout as the co-driver's handler sees it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Callout {
    pub tag: u32,
    pub word1: u32,
    pub seek_d: f32,
    pub gap: f32,
    pub next_tag: i32,
}

/// The record's payload for `callouts`.
pub fn to_bytes(callouts: &[Callout]) -> Vec<u8> {
    let mut out = (callouts.len() as u32).to_le_bytes().to_vec();
    for c in callouts {
        for word in [c.tag, c.word1, c.seek_d.to_bits(), c.gap.to_bits(), c.next_tag as u32] {
            out.extend_from_slice(&word.to_le_bytes());
        }
    }
    out
}

/// The callouts of a payload, or `None` when its size does not match its count.
pub fn from_bytes(bytes: &[u8]) -> Option<Vec<Callout>> {
    let count = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?) as usize;
    if bytes.len() != 4 + CALLOUT_BYTES * count {
        return None;
    }
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    Some(
        (0..count)
            .map(|k| {
                let at = 4 + CALLOUT_BYTES * k;
                Callout {
                    tag: word(at),
                    word1: word(at + 4),
                    seek_d: f32::from_bits(word(at + 8)),
                    gap: f32::from_bits(word(at + 12)),
                    next_tag: word(at + 16) as i32,
                }
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callouts_round_trip() {
        let callouts = [
            Callout { tag: 3, word1: 0x400, seek_d: 120.0, gap: 40.0, next_tag: 10 },
            Callout { tag: 0x16, next_tag: -1, ..Callout::default() },
        ];
        let bytes = to_bytes(&callouts);
        assert_eq!(bytes.len(), 4 + 2 * CALLOUT_BYTES);
        assert_eq!(from_bytes(&bytes).unwrap(), callouts);
        assert_eq!(from_bytes(&bytes[..bytes.len() - 1]), None);
        assert_eq!(from_bytes(&to_bytes(&[])).unwrap(), []);
    }
}
