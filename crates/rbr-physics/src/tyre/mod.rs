//! The tyre: one contact patch of a wheel, from the carcass and tread through the soil to the forces and heat it
//! makes. `layout` holds the typed views of the game memory; the other modules are the physics, in physical terms.
//! Operation order is fixed (`notes/physics/tyre.md`, section 5).

pub mod contact_frame;
pub mod contact_patch;
pub mod forces;
pub mod friction_curve;
pub mod friction_limits;
pub mod grip;
pub mod heat;
pub mod hold;
pub mod layout;
pub mod locate;
pub mod material;
pub mod noise_signal;
pub mod output_block;
pub mod patch_forces;
pub mod relax;
pub mod sliding;
pub mod slip;
pub mod soil;
pub mod solve;
pub mod state;
pub mod step;
pub mod store;
pub mod surface;
