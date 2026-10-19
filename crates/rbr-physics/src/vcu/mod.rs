//! Vehicle control unit (`NPhysics::CVehicleControlUnit`, exe 0x536880-0x5380a0). `layout` holds the
//! typed views of the game memory; the other modules are the ported functions. The asm (not the C++
//! in `src/rbr`) decides the operation order and the NaN behaviour; `notes/physics.md` "VCU" has the
//! semantics. Each function is checked against the original's calls (`harness/vcu_vectors.py`).

pub mod clutch;
pub mod cut;
pub mod dispatch;
pub mod gear;
pub mod layout;
pub mod locks;
pub mod slip;
pub mod step;
pub mod store;
pub mod tables;
