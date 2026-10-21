//! Car body (`NPhysics::CCar` and its neighbours in exe 0x4e4c70-0x4f3ec0 and 0x500300). `layout` holds the typed
//! views of the car's memory; the other modules are the ported functions. The asm (not the C++ in `src/rbr`)
//! decides the operation order and the NaN behaviour. The plan and the names are in `car_body/README.md` (git 9c28c0b8); each function is
//! checked against the original's calls (`harness/vector_specs/car_body.py`, `tests/car_body_vectors.rs`).

pub mod accessors;
pub mod apply_seed;
pub mod assist;
pub mod body;
pub mod body_grid;
pub mod damage;
pub mod damage_functors;
pub mod damage_ops;
pub mod damper;
pub mod drag;
pub mod edge;
pub mod emit;
pub mod forces;
pub mod forward;
pub mod helpers;
pub mod layout;
pub mod light;
pub mod links;
pub mod probe;
pub mod sample9800;
pub mod scratch_dispatch;
pub mod steering_ffb;
pub mod step_engine;
pub mod store;
pub mod transform;
pub mod tweak;
pub mod update_9af0;
pub mod update_mrr;
pub mod update_seed;
pub mod vf28x;
pub mod vf6cx;
pub mod wall_friction;
pub mod wheel;
pub mod world_placement;
