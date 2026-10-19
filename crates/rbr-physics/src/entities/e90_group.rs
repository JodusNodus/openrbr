//! Typed operations for `E90Group::Build` (0x601470). The native fence builders remain host calls.

/// Size of the zeroed backing allocation. Each instruction in the original uses wrapping 32-bit arithmetic.
pub fn build_size(n: u32, cols: u32, rows: u32, layers: u32) -> u32 {
    let spans = n.wrapping_sub(1);
    let a = cols.wrapping_sub(2).wrapping_mul(0x2c).wrapping_add(0x24);
    let b = cols
        .wrapping_sub(4)
        .wrapping_add(rows.wrapping_sub(1).wrapping_mul(cols.wrapping_sub(1)).wrapping_mul(4))
        .wrapping_mul(0x24);
    let span_size = a.wrapping_mul(rows).wrapping_add(b).wrapping_mul(layers).wrapping_add(0x78);
    let post_size = layers.wrapping_mul(rows).wrapping_mul(0x34).wrapping_add(0x140).wrapping_mul(n);
    span_size.wrapping_mul(spans).wrapping_add(post_size)
}

/// One `E90SegPool::Next`, with the pointer calculated before incrementing the count.
pub fn next_segment(count: u32, data: u32) -> (u32, u32) {
    (data.wrapping_add(count.wrapping_mul(0x78)), count.wrapping_add(1))
}
