//! The game memory the physics core math reads and writes: the typed views of `E46Vec`, `E46Mat`, the row arrays
//! and the table-index output. The ports in `math` never touch memory; this file is the only edge.

mod collide_work;
mod detector_calls;
mod globals;
mod lists;
mod mounted_calls;
mod notify;
mod passenger_step;
mod physics_ctor;
mod prof_calls;
mod step_forward;
mod vectors;

pub use collide_work::{advance_contact_bodies, advance_contact_bodies_remainder, collide_window};
pub use detector_calls::{
    body_pair_detect, e46_l2_detect, edge_pairs, giant_penetration_detect, kernel_collect, kernel_entity,
    l1_collision_detect, l1_penetration_detect, l2_penetration_detect, penetration_check, run_collision_detectors,
    set_cells, set_edge_result, visit_list470, xy, KernelArgs, KernelEntity,
};
pub use globals::{
    cdef_debug_flags, cdef_flags, cell_grid, collision_global, entity_list, link_registrar, physics, physics_global,
    relay_link, relay_target, release_target, set_app_callbacks, set_cell_grid, signal_manager, state_of, wall_manager,
    water_table, Registrar, CELL_GRID_GLOBAL, PHYSICS_GLOBAL, REGISTRARS, TELEMETRY_GLOBAL, WALL_MANAGER_GLOBAL,
    ZERO_VEC4,
};
pub(crate) use globals::{COLLISION_GLOBAL, SIGNAL_MANAGER};
pub use lists::{
    conn_owner, copy_vec4, emits_quietly, f32, link_next, list_head, mounted_state, push_item, reg_wall, set_f32,
    set_mounted_state, set_signal_guard, set_word, signal_count_at, signal_guard, signal_overflow, signal_slot,
    table_word, unlink_a, unlink_item, vtable_target, word,
};
pub use mounted_calls::{mounted_vf64, mounted_vf6c, MountedVf64Host, MountedVf6cHost};
pub use notify::{
    entf_create, entity_rel, entity_vf00, entity_vf158, notify_stuff, EntityVf158Host, Filer, FilingHost, NotifyHost,
    REGISTRY,
};
pub use passenger_step::{passenger_pull, passenger_step};
pub use physics_ctor::{
    byte, cdef_global, clear_scratch_block, construct_material_manager, set_cdef_global, set_create_result,
    set_physics_global, set_physics_hook, set_physics_pointers, set_physics_tail, set_scratch_vtable, store_cdef,
    store_material, store_physics_front, store_solver_def, store_state, Ported, CONN_AT,
};
pub use prof_calls::{prof_chan_enter, prof_enter, prof_exit, store_frequency};
pub use step_forward::{
    seed_penetration_entry, seed_penetration_exit, seed_step_forward_entry, seed_work5500_edi, step_forward,
    step_scratch_events, virtual_target, StepStack,
};
pub use vectors::{
    accumulate_forces, cache_1dc, cache_1ec, floats, force_accum, force_scale, inverse_mass, mat, mat_mul,
    mat_scale_rows, offset_540, set_force_accum, set_table_index, set_vec3s, set_vec4s, table_index_hint, vec3s, vec4s,
    vec_add,
};
