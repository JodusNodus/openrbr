//! The contact grid between a body and the transform it is called on: the contact arithmetic of the hit tails. The
//! sequence of calls is `layout::collide_init`.

/// Bit 3 of the body's flags: the second pass runs.
pub const SECOND_PASS: u32 = 0x8;

/// The kinds a hit's event is given: 4 for a first-pass or fallback hit, 5 for a second-pass hit.
pub const HIT_KIND: u32 = 4;
pub const PAIR_HIT_KIND: u32 = 5;

/// The midpoint fraction a second-pass hit interpolates to (0.5).
pub const MIDPOINT: f32 = 0.5;

/// The fallback hit's contact point without the second pass: the point moved against the
/// normal by half the gate boxes' height (`b.z - a.z`).
pub fn fallback_point(point: [f32; 3], normal: [f32; 3], a_z: f32, b_z: f32) -> [f32; 3] {
    let height = b_z - a_z;
    std::array::from_fn(|k| point[k] - height * normal[k] * MIDPOINT)
}

/// A second-pass hit's summed normals (x87 adds, which f32 gives), `reverse + forward` per lane.
pub fn normal_sum(forward: [f32; 3], reverse: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|k| reverse[k] + forward[k])
}

/// Whether the summed normals nearly cancel: their absolute lanes sum to below 1.0 (the x87 order `|y| + (|x| + |z|)`;
/// unordered is not).
pub fn normals_cancel(sum: [f32; 3]) -> bool {
    1.0 > sum[1].abs() + (sum[0].abs() + sum[2].abs())
}

/// The plane a second-pass hit reports when the normals do not cancel: the normal and `0 - hit·normal`, in the original's
/// order (`x`, then `z`, then `y`).
pub fn contact_plane(hit: [f32; 3], normal: [f32; 3]) -> [f32; 4] {
    let dot = hit[0] * normal[0] + hit[2] * normal[2] + hit[1] * normal[1];
    [normal[0], normal[1], normal[2], 0.0 - dot]
}

/// The probe point of a second-pass hit: the midpoint moved along the normalised direction, `n + mid` per lane in the
/// original's operand order.
pub fn probe_point(mid: [f32; 3], n: [f32; 3]) -> [f32; 3] {
    [n[0] + mid[0], mid[1] + n[1], mid[2] + n[2]]
}
