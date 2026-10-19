//! Engine and driveline (`RBR_E39`-`RBR_E43_Engine`): the drive accessors, the drive workers, the drive line and
//! the engine sim. `README.md` has the plan and the names; `tests/engine_driveline_vectors.rs` checks each port.

pub mod drive;
pub mod drive_curve;
pub mod drive_leaf;
pub mod drive_line;
pub mod drive_step;
pub mod drive_worker;
pub mod engine;
pub mod engine_def;
pub mod layout;
pub mod mat4;
pub mod sim;
pub mod store;
