//! The brake-wall hit (28 bytes): the wall normal, the penetration, the perpendicular distance and the flags.
//! Offsets are the original's (`collision::layout::walls::wall_raycast` writes them). `+0x0`/`+0x4` are the normal's
//! x/y, `+0x8` is 0.0, `+0xc` is unwritten residue the port round-trips, `+0x10` is `1 - perp/along`, `+0x14` is the
//! signed distance along the normal, `+0x18` the flags (bit 0 from the wall index's bit 15, bit 1 the wall latch).

use crate::car_body::step::WallHit;
use crate::math::vec::Vec4;

/// Byte offsets of the hit's words.
pub const NORMAL_AT: usize = 0x0;
pub const PENETRATION_AT: usize = 0x10;
pub const PERPENDICULAR_AT: usize = 0x14;
pub const FLAGS_AT: usize = 0x18;
/// The hit's length, bytes.
pub const LEN: usize = 28;

impl WallHit {
    /// The hit from its 28 bytes, preserving every word (including the residue at `+0xc`).
    pub fn from_bytes(bytes: &[u8; LEN]) -> Self {
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let lane = |at: usize| f32::from_bits(word(at));
        WallHit {
            normal: Vec4 { x: lane(0), y: lane(4), z: lane(8), w: lane(12) },
            penetration: lane(PENETRATION_AT),
            perpendicular: lane(PERPENDICULAR_AT),
            flags: word(FLAGS_AT),
        }
    }

    /// The hit's 28 bytes, in the original's order (the residue lane goes back bit-exact).
    pub fn to_bytes(&self) -> [u8; LEN] {
        let mut out = [0u8; LEN];
        for (at, word) in [
            (0, self.normal.x.to_bits()),
            (4, self.normal.y.to_bits()),
            (8, self.normal.z.to_bits()),
            (12, self.normal.w.to_bits()),
            (PENETRATION_AT, self.penetration.to_bits()),
            (PERPENDICULAR_AT, self.perpendicular.to_bits()),
            (FLAGS_AT, self.flags),
        ] {
            out[at..at + 4].copy_from_slice(&word.to_le_bytes());
        }
        out
    }

    /// The penetration word, as the impulse's `p` argument takes it.
    pub fn penetration_bits(&self) -> u32 {
        self.penetration.to_bits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_residue() {
        let bytes: [u8; LEN] = std::array::from_fn(|k| (k * 7 + 3) as u8);
        assert_eq!(WallHit::from_bytes(&bytes).to_bytes(), bytes);
        // A typed hit must also preserve signalling NaNs, payloads and signed zero in every float lane.
        for word in [0x7f80_0001u32, 0x7fc0_1234, 0xffc0_5678, 0x8000_0000, 0x7f80_0000] {
            let bytes = std::array::from_fn(|k| word.to_le_bytes()[k % 4]);
            assert_eq!(WallHit::from_bytes(&bytes).to_bytes(), bytes);
        }
    }

    #[test]
    fn contact_needs_flag_and_penetration() {
        let mut hit = WallHit::default();
        assert!(!hit.in_contact());
        hit.flags = 1;
        hit.penetration = 0.5;
        assert!(!hit.in_contact());
        hit.penetration = 0.500_000_1;
        assert!(hit.in_contact());
    }
}
