//! The wheel: its suspension geometry, its damage, its axes and its contact with the ground. `layout` holds the typed views
//! of the game memory, the store seam and the calls out of the wheel (with the original's addresses and names); the other
//! modules are the model: pure functions over typed values. Operation order follows the original's compiled code, not the
//! C++ reconstruction (which scalarises the SSE blends and has int round trips the original does not).
//!
//! - `axes`: the inclined, wobbling axle and the deformed top mount, from the damage.
//! - `refresh`: the suspension geometry for a strut travel (wishbone, knuckle, hub, wheel rotation, steering levers).
//! - `hub`: points on the wheel axle (the tyre's centre, the carcass edges).
//! - `contact`: how two ground-probe ray blocks combine into the contact vector; `ray` limits a block's difference.
//! - `axis_force`: the force on a gated sub-object along its axis, with friction.
//! - `walk`: the steering gain and the spin's cross product.
//! - `sector`: the wheel's angular sectors and their interpolation.
//! - `strut`: the strut's travel at rest.

pub mod axes;
pub mod axis_force;
pub mod contact;
pub mod hub;
pub mod layout;
pub mod ray;
pub mod refresh;
pub mod sector;
pub mod strut;
pub mod walk;
