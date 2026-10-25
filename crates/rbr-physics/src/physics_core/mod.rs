//! The physics core: the arithmetic of the step (vectors, matrices, force accumulation, the mounted body's motion, the
//! detectors' gates and the contact loop's step fraction, the passenger's springs, the clock's pacing) and the rules
//! of the signal dispatch. `layout` holds everything that touches the game's memory and the calls into the original:
//! the object images, the constructors, the detector and contact walks and their hosts. Each port is checked against
//! the original's own calls (`harness/vcu_vectors.py --set physics_core`, `tests/physics_core_vectors.rs`).

pub mod contact;
pub mod contact_loop;
pub mod debug;
pub mod detect;
pub mod detectors;
pub mod layout;
pub mod math;
pub mod pacing;
pub mod passenger;
pub mod passenger_step;
pub mod pose;
pub mod prof;
pub mod signal;
pub mod step;
pub mod store;
