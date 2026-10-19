//! Rigid body (`NPhysics::CSolid`, exe 0x529800-0x52d700): the free body the car's and the wheels' physics steps
//! integrate. `layout` holds the typed views of the game memory (the only raw offsets); the other modules are the
//! ported functions. `README.md` has the plan, the names and the open questions. Each function is checked against
//! the original's calls (`harness/vcu_vectors.py --set rigid-body`, `tests/rigid_body_vectors.rs`).

pub mod contact;
pub mod contact_list;
pub mod dot;
pub mod emit;
pub mod force;
pub mod host;
pub mod impulse;
pub mod integrate;
pub mod layout;
pub mod pose;
pub mod response;
pub mod rotation;
pub mod rows;
pub mod scratch;
pub mod sleep;
pub mod vote;
pub mod window;
