//! Vehicle control unit: the gear logic, the launch clutch, the rev-limit cut and the diff locks. `layout`
//! holds the typed views of the game memory; the other modules are the ported functions. The operation order
//! and the NaN behaviour are fixed; `notes/physics.md` "VCU" has the semantics. Each function is checked
//! against the recorded calls (`harness/vcu_vectors.py`).

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
