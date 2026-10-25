//! `CPassenger::Step` (0x523160) and `E45PSmalls::M2BC0` (0x522bc0): `physics_core::passenger_step` over the store.

pub use crate::physics_core::passenger_step::{passenger_pull, passenger_step};

/// The physics object's CDef (+0x24) and the gravity in it (+0x160b0).
pub(super) const PHYSICS_DEF_AT: u32 = 0x24;
pub(super) const GRAVITY_AT: u32 = 0x160b0;
