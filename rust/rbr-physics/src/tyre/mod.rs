//! Tyre (`NPhysics::CTyre`, the CScratch step, exe 0x531ba0-0x5360c0). `layout` holds the typed views of
//! the game memory; the other modules are the ported functions. Operation order follows the asm, not the
//! C++ in `src/rbr` (rbr-recomp `notes/physics/tyre.md`, section 5). `tyre/README.md` (git 9c28c0b8) has the names and the plan.

pub mod align;
pub mod contact_patch;
pub mod curve;
pub mod fill;
pub mod force;
pub mod forces;
pub mod friction;
pub mod grip;
pub mod heat;
pub mod layout;
pub mod locate;
pub mod material;
pub mod outputs;
pub mod relax;
pub mod slip;
pub mod solve;
pub mod solve_patch;
pub mod state;
pub mod step;
pub mod step_inner;
pub mod store;
pub mod write;
