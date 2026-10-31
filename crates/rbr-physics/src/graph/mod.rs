//! The debug graphs' pushes: the 1D push (the engine's two graphs, gated by flag `0x200`) and the 2D
//! push (the car's graph, flag `0x10`). A graph samples every `period`-th push into a ring of samples; nothing in
//! the step reads the ring. The retail game leaves both flags clear and the car's graph null.

pub mod layout;

/// The push counter after a push and the quotient: `(counter + 1) % period`, a sample when it is 0. `None` for a
/// period of 0, where the division faults.
pub fn tick(counter: u32, period: u32) -> Option<(u32, u32)> {
    let n = counter.wrapping_add(1);
    Some((n.checked_rem(period)?, n / period))
}

/// Whether the ring's cursor `next`, past a sample of `stride` bytes, is at or past the ring's end (unsigned): it
/// goes back to `base` then.
pub fn wraps(next: u32, base: u32, count: u32, stride: u32) -> bool {
    next >= base.wrapping_add(count.wrapping_mul(stride))
}
