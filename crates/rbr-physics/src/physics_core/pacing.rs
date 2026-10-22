//! How the physics step is paced: the clock's advance, the single-stepping gate and the substep schedule of the
//! contacts and the solver.

use crate::fpu::{precision, Precision};

/// The physics flag that enables single-stepping (`ENABLE_SINGLESTEPPING`) and the one that enables sub-stepping
/// (`SUB_STEPPING`), in the physics definition's flags.
const SINGLE_STEPPING: u32 = 1 << 15;
const SUB_STEPPING: u32 = 1 << 12;
/// The substep counts the contacts and solver run on while sub-stepping: those with none of these bits set. NGP patches
/// the mask from 3 (every fourth count) to 0x13: the counts 0, 4, 8 and 12 of every 32.
const SUB_STEP_MASK: u32 = 0x13;

/// The clock's advance by `dt` (s). The original's x87 path rounds the addition to f32 before widening the result back
/// to f64. Under NGP (53 bits) the addition rounds to f64.
pub fn advance_time(elapsed: f64, dt: f32) -> f64 {
    match precision() {
        Precision::Pc24 => f64::from(dt + elapsed as f32),
        Precision::Pc53 => f64::from(dt) + elapsed,
    }
}

/// Applies `ENABLE_SINGLESTEPPING` to the requested substep count and the delay left: with the flag set and a step
/// `pending`, no step runs while the delay is 0, else one runs and the delay counts down. Returns the count and the
/// delay.
pub fn single_step_count(requested: u32, flags: u32, pending: bool, delay: u32) -> (u32, u32) {
    if flags & SINGLE_STEPPING == 0 || !pending {
        (requested, delay)
    } else if delay == 0 {
        (0, delay)
    } else {
        (1, delay.wrapping_sub(1))
    }
}

/// Whether the contacts and solver run on substep `count` under these flags: always, unless `SUB_STEPPING` is set, then
/// only on the counts with none of the mask's bits set.
pub fn contact_due(flags: u32, count: u32) -> bool {
    flags & SUB_STEPPING == 0 || count & SUB_STEP_MASK == 0
}
