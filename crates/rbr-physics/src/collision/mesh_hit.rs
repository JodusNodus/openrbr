//! The surface record of a ray hit on a collision mesh: the point, the normal, the colour-channel sums and the two
//! surface lookups, from the hit face's packed bytes.

use crate::math::vec::Vec3;

/// 1/15, the scale of a nibble sum.
pub const NIBBLE_SCALE: f32 = 1.0 / 15.0;
/// 1/31, the scale of a 5-bit channel sum.
pub const CHANNEL_SCALE: f32 = 1.0 / 31.0;

/// The size of a face record, bytes.
pub const FACE_BYTES: usize = 0x12;

/// The little-endian halfword at byte `at` of a face record.
pub fn face_word(face: &[u8; FACE_BYTES], at: usize) -> u16 {
    u16::from_le_bytes([face[at], face[at + 1]])
}

/// The vertex slot `i` (0 to 2) of a face record.
pub fn face_vertex(face: &[u8; FACE_BYTES], i: usize) -> u16 {
    face_word(face, 2 * i)
}

/// The three sample bytes at `at` (12 or 15) of a face record.
pub fn face_samples(face: &[u8; FACE_BYTES], at: usize) -> [u8; 3] {
    [face[at], face[at + 1], face[at + 2]]
}

/// The 5-bit channel sum a face word gives (bits 10, 5 and 0), weighted by `v`, `u` and `w` (in that order).
pub fn channel_sum(word: u16, v: f32, u: f32, w: f32) -> f32 {
    let channel = |shift: u32| f32::from((word >> shift) & 0x1f);
    (channel(10) * v + channel(5) * u) + channel(0) * w
}

/// The low (or high, with `shift` 4) nibble sum of three face bytes, weighted by `w`, `u` and `v` (in that order).
pub fn nibble_sum(bytes: [u8; 3], shift: u32, w: f32, u: f32, v: f32) -> f32 {
    let nibble = |byte: u8| f32::from((byte >> shift) & 0xf);
    (nibble(bytes[0]) * w + nibble(bytes[1]) * u) + nibble(bytes[2]) * v
}

/// The mesh ray's hit: the ray parameter `t`, the normal (normalised), the barycentric coordinates `u` and `v`, the
/// dot of the face's first vertex with the normal, and the face record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceHit {
    pub t: f32,
    pub normal: Vec3,
    pub u: f32,
    pub v: f32,
    pub vertex_dot_normal: f32,
    pub face: [u8; FACE_BYTES],
}

/// A surface lookup: the byte for `byte` at the sample `low`, `high`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampleQuery {
    pub byte: u32,
    pub low: f32,
    pub high: f32,
}

/// The segment record of a mesh ray hit: the point, the normal, the plane offset (the negated dot of the first vertex
/// with the normal), the two channel sums, the two lookups, `t`, and the two face flags.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentRecord {
    pub point: Vec3,
    pub normal: Vec3,
    pub plane_offset: f32,
    pub channels: [f32; 2],
    pub lookups: [SampleQuery; 2],
    pub t: f32,
    pub flags: [u32; 2],
}

/// The record after a mesh ray hit: the point `direction * t + origin`, the normal and the plane offset, the channel
/// sums and the two lookups. The float order is the original's.
pub fn segment_record(origin: Vec3, direction: Vec3, hit: SurfaceHit) -> SegmentRecord {
    let f = hit.face;
    let w8 = u16::from_le_bytes([f[8], f[9]]);
    let w6 = u16::from_le_bytes([f[6], f[7]]);
    let (u, v) = (hit.u, hit.v);
    let w = 1.0_f32 - (v + u);
    let t = hit.t;
    let lookup = |byte: u8, bytes: [u8; 3]| SampleQuery {
        byte: u32::from(byte),
        low: nibble_sum(bytes, 0, v, w, u) * NIBBLE_SCALE,
        high: nibble_sum(bytes, 4, v, w, u) * NIBBLE_SCALE,
    };
    SegmentRecord {
        point: Vec3 { x: direction.x * t + origin.x, y: direction.y * t + origin.y, z: direction.z * t + origin.z },
        normal: hit.normal,
        plane_offset: -hit.vertex_dot_normal,
        channels: [channel_sum(w8, v, u, w) * CHANNEL_SCALE, channel_sum(w6, v, u, w) * CHANNEL_SCALE],
        lookups: [lookup(f[10], [f[0xe], f[0xc], f[0xd]]), lookup(f[11], [f[0x11], f[0xf], f[0x10]])],
        t,
        flags: [u32::from(w6 >> 15), u32::from(w8 >> 15)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_are_the_reciprocals() {
        assert_eq!(NIBBLE_SCALE.to_bits(), 0x3d88_8889);
        assert_eq!(CHANNEL_SCALE.to_bits(), 0x3d04_2108);
    }
}
