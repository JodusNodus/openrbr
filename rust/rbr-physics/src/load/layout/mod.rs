//! The load's objects on game memory: the owners built and the files' root groups walked on them (`super` describes
//! the order).

mod complete_setup;
mod files;
mod instance;
mod parts;
mod statics;

pub use complete_setup::{
    construct_complete_setup, construct_setup_options, copy_car_setups, copy_setups, drive_options, options_file,
    select_drive_options, select_top_mounts, select_tyre_type, set_driver_aids, setup_file, setups, store_seed,
    tyre_type, Setups, WheelSetup, COMPLETE_SETUP_SIZE, SEED_SIZE, SETUP_OPTIONS_SIZE,
};
pub use files::{new_physics_def, physics_file};
pub(crate) use instance::body_event_listened;
pub use instance::{new_body, new_car, new_wheel, BodyKind};
pub use parts::{CarParts, Factories, PartFactory};
pub use statics::{
    construct_signal_manager, emit_car_meshes, enter_tic_sample, fire_collision_mesh, new_replay_stand_in,
    store_registry, store_statics, REPLAY_STAND_IN_SIZE, SIGNAL_COUNT, SIGNAL_SIZE,
};
