//! Car body: the car's step, its controls, its roll bars, wall friction, drag, damage and lights, and the typed views
//! (`layout`) of the car's memory. The original decides the operation order and the NaN behaviour. Each function is
//! checked against the original's calls (`harness/vector_specs/car_body.py`, `tests/car_body_vectors.rs`).

pub mod accessors;
pub mod assist;
pub mod body;
pub mod body_grid;
pub mod control;
pub mod controls;
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
pub mod publish;
pub mod retire;
pub mod scratch_dispatch;
pub mod sensors;
pub mod setup;
pub mod steering_ffb;
pub mod step;
pub mod store;
pub mod transform;
pub mod tweak;
pub mod wall_friction;
pub mod wheel;
pub mod wiring;
pub mod world_placement;
