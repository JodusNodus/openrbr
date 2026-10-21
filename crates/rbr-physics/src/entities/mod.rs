//! Entity layer of the exe (`src/rbr/RBR_E43_*`, `RBR_E44_*`): the CEntity frames and matrices, CGround
//! and the CHingedRigidBody sims. The ports (`frame`, `mat`, `ground`, `hinged`) take and return typed
//! values; `layout` is the only code that reads or writes the game's memory (offsets and stores), as
//! `vcu::layout` is for the VCU. Each function is checked against the original's own calls
//! (`harness/vcu_vectors.py --set entities`, `tests/entities_vectors.rs`). The plan and the batch
//! status are in `entities/README.md` (git 9c28c0b8).

pub mod collide;
pub mod def;
pub mod dummy;
pub mod exclusion;
pub mod fence_group;
pub mod fence_span;
pub mod frame;
pub mod ground;
pub mod hinge_advance;
pub mod hinge_axes;
pub mod hinge_car_transform;
pub mod hinge_detach;
pub mod hinge_frame_walks;
pub mod hinge_impulse;
pub mod hinge_limits;
pub mod hinge_offset;
pub mod hinge_pivot;
pub mod hinge_pose;
pub mod hinge_refresh;
pub mod hinge_signals;
pub mod hinge_slip;
pub mod hinge_solve;
pub mod hinge_stops;
pub mod layout;
pub mod material_table;
pub mod slots;
pub mod transform;
pub mod tree;
pub mod walk;
