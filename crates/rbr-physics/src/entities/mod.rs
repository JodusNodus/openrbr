//! Entity layer of the exe (`src/rbr/RBR_E43_*`, `RBR_E44_*`): the CEntity frames and matrices, CGround
//! and the CHingedRigidBody sims. The ports (`frame`, `mat`, `ground`, `hinged`) take and return typed
//! values; `layout` is the only code that reads or writes the game's memory (offsets and stores), as
//! `vcu::layout` is for the VCU. Each function is checked against the original's own calls
//! (`harness/vcu_vectors.py --set entities`, `tests/entities_vectors.rs`). The plan and the batch
//! status are in `entities/README.md` (git 9c28c0b8).

pub mod collide;
pub mod def;
pub mod dummy;
pub mod e90_group;
pub mod e90_seg;
pub mod exclusion;
pub mod frame;
pub mod functor;
pub mod ground;
pub mod hinge_d160;
pub mod hinge_d860;
pub mod hinge_db40;
pub mod hinge_e070;
pub mod hinge_e4f0;
pub mod hinge_efe0;
pub mod hinge_f350;
pub mod hinge_f950;
pub mod hinge_fd00;
pub mod hinge_pivot;
pub mod hinge_signals;
pub mod hinge_sim;
pub mod hinge_slip;
pub mod hinge_solve;
pub mod hinged;
pub mod layout;
pub mod limit;
pub mod mat;
pub mod slots;
pub mod transform;
pub mod tree;
pub mod walk;
