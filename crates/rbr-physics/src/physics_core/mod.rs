//! Physics core port (E45/E46 in `src/rbr`): the math leaves first. `math` holds the typed operations, `layout` the
//! only code that reads or writes the game's memory. Each port is checked against the original's own calls
//! (`harness/vcu_vectors.py --set physics_core`, `tests/physics_core_vectors.rs`). The plan and the names are in
//! `physics_core/README.md` (git 9c28c0b8).

pub mod construct;
pub mod contact;
pub mod debug;
pub mod detect;
pub mod detectors;
pub mod flags;
pub mod layout;
pub mod material;
pub mod math;
pub mod mounted;
pub mod mounted_body;
pub mod mounted_ctor;
pub mod mounted_pose;
pub mod passenger;
pub mod physics;
pub mod prof;
pub mod setup;
pub mod signal;
