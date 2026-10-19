//! Bit-exact ports of Richard Burns Rally 1.02 physics functions.
//!
//! See `notes/rust-physics.md` for the x87 precision model. `tests/vectors.rs` replays
//! the captured calls of each port against the original.

// The ports follow the asm's comparisons, so a NaN takes the same branch as in the original: `!(a > b)` is not
// `a <= b`, and `f32::clamp` would change NaN results. Constants keep the exe's exact bit patterns, and some ports
// take the original's argument list.
#![allow(
    clippy::neg_cmp_op_on_partial_ord,
    clippy::manual_clamp,
    clippy::excessive_precision,
    clippy::too_many_arguments
)]

pub mod car_body;
pub mod collision;
pub mod engine_driveline;
pub mod entities;
pub mod fpu;
pub mod load;
pub mod math;
pub mod mem;
pub mod ngp;
pub mod ntweak;
pub mod physics_core;
pub mod rigid_body;
pub mod snapshot;
pub mod spring_damper;
pub mod stage;
pub mod store;
pub mod tyre;
pub mod vcu;
pub mod wheel;

/// The physics a run follows: 1.02's, or 1.02's with NGP 7.4.778 (`notes/ngp.md`, "Shape in the code"). The ports of
/// exe functions NGP rewrites or patches take it; NGP's own functions are in [`ngp`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Variant {
    #[default]
    Stock102,
    Ngp778,
}

impl Variant {
    /// The variant of a port that takes NGP's state as an `Option` (`None` for 1.02).
    pub fn of<T>(ngp: Option<T>) -> Self {
        if ngp.is_some() {
            Variant::Ngp778
        } else {
            Variant::Stock102
        }
    }
}
