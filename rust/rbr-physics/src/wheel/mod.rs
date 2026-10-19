//! Wheel (`NPhysics::CWheel` and `E51Wheel`, exe 0x5390d0-0x53dd30). `layout` holds the typed views of the game memory;
//! the other modules are the ported functions. Operation order follows the asm, not the C++ in `src/rbr` (the C++
//! scalarises the SSE blends and has int round trips the asm does not; `notes/physics/wheel.md`, section 5).
//! `README.md` has the names and the plan.

pub mod axes;
pub mod blend;
pub mod block;
pub mod construct;
pub mod copy;
pub mod frame;
pub mod layout;
pub mod refresh;
pub mod select;
pub mod slots;
pub mod store;
pub mod sub;
pub mod update;
pub mod walk;
pub mod xform;
