//! The collision subsystem's game memory: typed views of its objects, the original's calls and their sequencing.

mod body_contacts;
pub mod body_hosts;
pub mod body_slots;
mod cbody;
mod cell_grid;
mod col_grids;
pub mod collide_hosts;
pub mod dictionary;
mod e36_box;
pub mod e36_collect;
pub mod event_pool;
mod events;
mod grid_edges;
pub mod grid_elems;
pub mod grid_hosts;
pub mod links;
pub mod list;
mod mesh_query;
mod solver;
pub mod solver_hosts;
pub mod sort;
pub mod state;
mod vec_ops;
pub mod wall_tree;
mod walls;

pub use body_contacts::{body_vf_d0, body_vf_dc};
pub(in crate::collision) use cbody::BODY_GRID;
pub use cbody::{
    aabb_pretest, body_grid_active, body_has_grid, box_edge_hit_frame, box_edge_hit_sp, clear_grid, ctor,
    grid_edge_count, grid_hit_args, record_edge_hits_frame, reset_state_and_base, select_grid, state_ctor,
    state_unka44, store_grid_hit, store_vf_f4_hit, take_grid_flags, vtable_slot, Body, CtorHost, IDENTITY_WORKER_SLOT,
};
pub use cell_grid::{cell_grid_ctor, lookup_twin, move_to_cell, unlink, CellGrid, Link};
pub(in crate::collision) use col_grids::GRID_TAG;
pub use col_grids::{
    alloc_collision_id, build_col_grid, col_grid_at, collision_mesh_factory, load_col_grid, load_mesh_desc, sample_a,
    sample_b, select_table, store_col_grid, store_samples, sync_grid, ColGrid, GridSyncHost,
};
pub use e36_box::{obb_test, set_obb_extents, shape_axis, shape_extent, sphere_box, E36Box, E36Contact};
pub use events::{
    apply_event_impulse, init_full, make_event_at, make_event_frame, make_event_payload, prefill_pool,
    query_event_bodies, relative_point_velocity, store_payload, EventBodyHost, EVENT_SIZE,
};
pub use grid_edges::{
    edge_edge_bounds, edge_edge_face, edge_edge_face_count, edge_edge_face_is_valid, edge_edge_hit_word,
    edge_edge_sample_bytes, edge_edge_sample_dimensions, edge_edge_sample_indices, edge_edge_vertex_samples,
    edge_edge_vertices, edge_normal, edge_pair, grid_add_edge, grid_ops_m6610, store_edge_edge_hit,
    store_edge_edge_sample_bytes, vec_set_gap, walk_5ff0,
};
pub use mesh_query::{
    box_sat_overlap, face_query, load_material_pair, load_sat_box, material_manager, mesh_copy_bounds, mesh_ray_query,
    mesh_ray_worklist, mesh_tag, mesh_test, mesh_vertices, node_walk, point_triangle, ray_clear, ray_seeds,
    ray_triangle, seg_test, seg_test170, segment_sweep, segment_test, store_mesh_hit_tags, store_point_triangle,
    store_ray_triangle_result, store_segment_triangle, store_zero_mesh_scratch, system_frame, system_vf00,
    world_axes_overlap, SystemFrame,
};
pub use solver::{
    add_body_offset, apply_offsets, clear_contact_rows, contact_rows, correct_penetrations, ctor_solver, drain_events,
    event_count, mark_suppressing_materials, min_contact_time, refresh_events, solve_due_events, solve_event,
};
pub use vec_ops::{
    min_max_a, scaled, vec16_max, vec16_min, vec3_add, vec3_add_stale, vec3_add_sub, vec3_cross, vec3_cross_dot,
    vec3_dot, vec3_normalize, vec3_scale, vec3_sub, vec3_sub_dot, vec4_copy_from, vec4_normalize, vec4_sub_stale,
    Vec4Pair,
};
pub use walls::{brake_wall_vf_e8, brake_wall_vf_f0, wall_probe, wall_raycast, wall_vf_cc};

/// The shared four-lane vector (`math::vec::Vec4`), used for body bounds and temporary collision data.
pub use crate::math::vec::Vec4;
