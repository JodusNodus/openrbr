//! The load/store layer of the entity ports: the only code that addresses the game's memory (the
//! offsets, the vtable word and the stores), as `vcu::layout` is for the VCU. Each function reads the
//! bytes its original reads, calls the port with typed values, and stores what the original stores,
//! in the original's order.

mod classes;
mod collide_init;
mod construct;
mod detach;
mod e90;
mod entity_slots;
mod frames;
mod functor;
mod hinge_state;
mod materials;
mod store_mem;
mod walk_nodes;

pub use crate::entities::dummy::{dummy_box, dummy_span_test, DummyCalls};
pub use crate::entities::exclusion::{
    add_exclusion_at as add_exclusion, alloc_exclusion_at as alloc_exclusion, remove_exclusion_at as remove_exclusion,
};
pub use crate::entities::frame::{entity_hook_at, old_hook_at};
pub use crate::entities::ground::box_probe;
pub use crate::entities::hinge_step::{
    hinge_limit_residual, hinge_limit_sum, hinge_rotation, limit_update_at, sim_cff0_at, sim_d160_at, sim_e1d0_at,
    sim_ead0_at, sim_f950_at,
};
pub use crate::entities::material_table::{sample_indexed, Indexed};
pub use crate::entities::slots::{clear_car_bodies, componentwise_mul, init_bd0, Clear480Calls};
pub use classes::{
    TweakKind, BASE_VTABLE, ENTITY_DEF_VTABLE, GROUND_DEF_NAME, GROUND_DEF_SIZE, GROUND_DEF_VTABLE, GROUND_OPTIONS,
    GROUND_SINGLETON, GROUND_SIZE, GROUND_TWEAK_DICT, GROUND_TWEAK_MEMBERS, GROUND_VTABLE, HINGED_DEF_NAME,
    HINGED_TWEAK_DICT, MATERIAL_TWEAK_DICT,
};
pub use collide_init::{box_probe_frame, collide_init, CollideHost, ProbeFrame};
pub use construct::{
    append_child, child_head, construct_base, construct_entity, construct_entity_def, construct_ground_def,
    construct_hinged, construct_hinged_def, ground_def_new, ground_make, hinged_def_new, new_hinged,
    release_entity_def, sibling, EntityArgs, HINGED_DEF_SIZE, HINGED_SIZE,
};
pub use detach::{
    sim_da60, sim_f350, sim_fd00, step_big, DA60Calls, F350Calls, FD00Calls, StepBigCalls, FD00_SLOT_48, FD00_SLOT_54,
    FD00_SLOT_90,
};
pub use e90::{
    add_bend, add_fixed, add_particle, add_pin, add_spring, build_e90_group, build_e90_post, build_e90_span,
    construct_e90_seg, d3dx_vec3_normalize, destroy_e90_seg, e90_seg_init, init_e90_bend_pool, init_e90_part_pool,
    init_e90_pin_pool, init_e90_ptr_tab, init_e90_seg_pool, E90BuildCalls, E90PoolCalls, E90SegInitCalls,
    E90SegPoolCalls, E90SegmentCalls,
};
pub use entity_slots::{load_excluder, vf_cc, vf_d0, zero_hook};
pub use frames::{construct_entity_frame, construct_frame, ROOT_IDENTITY};
pub use functor::{call_stored, Calls as FunctorCalls, TAIL40};
pub use hinge_state::{
    angle_tweak, limit_object, pivot_frame, position_at, rate_tweak, scaled_vector, set_hinge_link, slip_frame,
    solve_frame, store_walk_vector, vec3, vf_e070,
};
pub use materials::{build_material_table, fill_ones, store_material_table};
pub use walk_nodes::{
    append_name_index, copy_grid, copy_mat16, empty_hook, node_of, old_frame_of, vtable_target, walker_address,
    GRID_FIRST_BOXES, GRID_SECOND_BOXES, GRID_STRIDE,
};
