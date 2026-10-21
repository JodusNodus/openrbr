//! The load/store layer of the entity ports: the only code that addresses the game's memory (the
//! offsets, the vtable word and the stores), as `vcu::layout` is for the VCU. Each function reads the
//! bytes its original reads, calls the port with typed values, and stores what the original stores,
//! in the original's order.

mod collide_init;
mod construct;
mod detach;
mod e90;
mod entity_slots;
mod frames;
mod hinge_state;
mod materials;
mod walk_nodes;

pub use collide_init::{box_probe, box_probe_frame, collide_init, grid_count, ProbeFrame};
pub use construct::{
    append_child, child_head, construct_base, construct_entity, construct_entity_def, construct_ground_def,
    construct_hinged, construct_hinged_def, ground_def_new, ground_make, hinged_def_new, load_transform, new_hinged,
    release_entity_def, sibling, EntityArgs, HINGED_DEF_SIZE, HINGED_SIZE,
};
pub use detach::{
    clear_accumulators, init_bd0, sim_da60, sim_f350, sim_fd00, step_big, DA60Calls, F350Calls, FD00Calls,
    StepBigCalls, FD00_SLOT_48, FD00_SLOT_54, FD00_SLOT_90,
};
pub use e90::{
    add_bend, add_fixed, add_particle, add_pin, add_spring, build_e90_group, build_e90_post, build_e90_span,
    construct_e90_seg, d3dx_vec3_normalize, destroy_e90_seg, e90_seg_init, init_e90_bend_pool, init_e90_part_pool,
    init_e90_pin_pool, init_e90_ptr_tab, init_e90_seg_pool, E90BuildCalls, E90PoolCalls, E90SegInitCalls,
    E90SegPoolCalls, E90SegmentCalls,
};
pub use entity_slots::{
    add_exclusion, alloc_exclusion, clear_car_bodies, componentwise_mul, d4_flags, dummy_box, dummy_span_test,
    load_excluder, remove_exclusion, vf_cc, vf_d0, zero_hook, Clear480Calls,
};
pub use frames::{
    construct_entity_frame, construct_frame, entity_hook_at, load_entity_frame, old_hook_at, ROOT_IDENTITY,
};
pub use hinge_state::{
    car_state_c00, d160_frame, d160_words, d860_input, db40_input, efe0_frame, efe0_state, hinge_definition,
    hinge_limit_residual, hinge_limit_sum, hinge_link, hinge_linked, hinge_owned, hinge_rotation, limit_object,
    limit_update_at, load_hinge_limit, load_hinge_limits, load_hinge_state, pivot_frame, pivot_input, set_hinge_link,
    signal_input, sim_cff0_at, sim_d160_at, sim_e1d0_at, sim_ead0_at, sim_f950_at, slip_frame, slip_input,
    solve_entry_terms, solve_frame, solve_terms, store_d160_words, store_efe0_state, store_pivot, store_walk_vector,
    vec3, vf_e070,
};
pub use materials::{build_material_table, fill_ones, sample_indexed, store_material_table, Indexed};
pub use walk_nodes::{
    append_name_index, copy_grid, copy_mat16, empty_hook, load_node, node_of, old_frame_of, store_pair, vf120_target,
    vtable_target,
};
