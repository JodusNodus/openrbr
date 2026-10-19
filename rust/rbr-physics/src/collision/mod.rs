//! E33 body collision, E34 brake walls, E36 collection, and E37/E38 contact solver ports.
//!
//! Raw game memory is described only in `layout`; typed logic is split by the original call path.

pub mod body;
pub mod body_slots;
pub mod col_grid;
pub mod collide_big;
pub mod dictionary;
pub mod e36;
pub mod e38;
pub mod edge_edge;
pub mod edge_pair;
pub mod elem;
pub mod event;
pub mod grid;
pub mod layout;
pub mod links;
pub mod list;
pub mod math;
pub mod sort;
pub mod state;
pub mod vec_set;
pub mod wall;
