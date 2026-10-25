//! Body collision, brake walls, collision grids and the contact solver.
//!
//! The model modules hold the geometry and arithmetic in physical terms; `layout` holds the game memory, the original's
//! calls and the sequencing around them.

pub mod body_contact;
pub mod brake_wall;
pub mod cells;
pub mod contact_solver;
pub mod e36;
pub mod edge_pair;
pub mod events;
pub mod face_sweep;
pub mod grid_contact;
pub mod layout;
pub mod mesh_grid;
pub mod mesh_hit;
pub mod obb;
pub mod overlap;
pub mod solver_pass;
pub mod store;
pub mod triangle;
