//! Bindings of the collision ports (`rbr_physics::collision`, exe 0x4e0000-0x4fe000). The ABIs are the original's
//! (`harness/vector_specs/collision.py`).

use crate::{Binding, Host};

mod bodies;
mod brake_wall;
mod contacts;
mod edge;
mod queries;
mod solver;
mod vectors;

use crate::entities::{COMPOSE_B, ENTITY_VF118, FETCH_X, NAME_WALK, NODE_OF, PAIR_COMBINE_TWIN, SAMPLE_INDEXED};
pub(crate) use bodies::{body_box_direct, body_vf_cc_direct, BodyCallees, VfCcCorners, BODY_VF110, BODY_VF11C};
use bodies::{
    body_clear_grid, body_grid, body_grid_active, body_has_grid, body_link_unlink, body_vf11c, body_vf40, body_vf_cc,
    lookup_twin, move_to_cell, notify_filtered, state_unka44,
};
use brake_wall::{
    brake_wall_emit, brake_wall_is_active, brake_wall_query_box, brake_wall_query_point, brake_wall_vf_e8,
    brake_wall_vf_f0, raycast_entry, wall_raycast, wall_vf_cc, BRAKE_WALL_QUERY_BOX,
};
use contacts::{body_vf_d0, body_vf_dc};
pub(crate) use contacts::{body_vf_d8, body_vf_e8, body_vf_f4};
use edge::{edge_edge, edge_normal, edge_pair, vec_set_gap, walk_5ff0};
use queries::{
    box_sat_overlap, collect, face_query, grid_add_edge, grid_ops_m6610, grid_ray, grid_sample_a, grid_sample_b,
    grid_sync, mesh_copy_bounds, mesh_is_active, mesh_ray_worklist, node_walk, point_triangle, ray_clear, ray_seeds,
    ray_triangle, seg_test, segment_sweep, segment_test, sphere_box, system_vf00, world_axes_overlap,
};
use solver::{
    alloc_event, apply_event_impulse, apply_offsets, clear_contact_rows, contact_rows, correct_penetrations,
    drain_events, event_count, event_query_bodies, init_full, make_event, min_contact_time, plane_diff, refresh_events,
    solve_due_events, solve_event, solver_add_event, E37_PLANE_DIFF,
};
use vectors::{
    e33_scaled, float3_length, float3_scale, min_max_a, vec16_max, vec16_min, vec3_add, vec4_clear, vec4_copy_from,
    vec4_normalize, vec4_sub, vec_add_sub, vec_cross, vec_cross_dot, vec_dot, vec_normalize, vec_scale, vec_sub,
    vec_sub_dot,
};

pub(crate) const RAY_CLEAR: u32 = 0x4f6740;
pub(crate) const RAY_SEEDS: u32 = 0x4f6550;
pub(crate) const CE36_SUB_BLOCK_INIT: u32 = 0x4f3e80;
pub(crate) const E36_ALLOC_EVENT: u32 = 0x4f4f10;
pub(crate) const COLL_EVENT_INIT_FULL: u32 = 0x4f50e0;

pub static BINDINGS: [Binding; 93] = [
    crate::bind("collision", 0x4f2ce0, "E36SphereBox", sphere_box),
    crate::bind("collision", 0x4f3950, "E36Collect", collect),
    crate::bind("collision", 0x4f5480, "E37Vec16::Min", vec16_min),
    crate::bind("collision", 0x4f54b0, "E37Vec16::Max", vec16_max),
    crate::bind("collision", 0x4e0020, "CBodyLink::Unlink", body_link_unlink),
    crate::bind("collision", 0x4faeb0, "CBodyLink::MoveToCell (ours)", move_to_cell),
    crate::bind("collision", 0x4f6550, "E38RaySeeds", ray_seeds),
    crate::bind("collision", 0x4f6740, "E38RayClear", ray_clear),
    crate::bind("collision", 0x4f7090, "CColGrid::SyncGrid", grid_sync),
    crate::bind("collision", 0x4fd760, "M_D760X", grid_ray),
    crate::bind("collision", 0x4fde40, "E38Ops::M_DE40X", mesh_ray_worklist),
    crate::bind("collision", 0x4fe810, "CCollisionSystem::Vf00", system_vf00),
    crate::bind("collision", 0x4fdd60, "E38NodeWalk", node_walk),
    crate::bind("collision", 0x4ff170, "E39_SegTest170", seg_test),
    crate::bind("collision", 0x4ff070, "E39_SegTest70", segment_test),
    crate::bind("collision", 0x4fe2d0, "E38Ops::M_E2D0", point_triangle),
    crate::bind("collision", 0x4febc0, "E38Ops::M_EBC0", face_query),
    crate::bind("collision", 0x4e4570, "E34WallVf::Raycast", wall_raycast),
    crate::bind("collision", 0x4e4960, "CBrakeWallManager::RaycastEntry", raycast_entry),
    crate::bind("collision", 0x4e4490, "CMultiSignal1<CSigCarBrakeWallPenetration const &>::Emit", brake_wall_emit),
    crate::bind("collision", 0x4fad90, "E38SegmentSweep", segment_sweep),
    crate::bind("collision", 0x4e03b0, "CCellGrid::LookupTwin", lookup_twin),
    crate::bind("collision", 0x4e1120, "CBody::Vf40", body_vf40),
    crate::bind("collision", 0x4e11a0, "CBody::ClearGrid", body_clear_grid),
    crate::bind("collision", 0x4e11f0, "CBody::Vf118", body_grid),
    crate::bind("collision", 0x4e0700, "E33Vec4::Scaled", e33_scaled),
    crate::bind("collision", 0x4e1230, "CBody::VfCC", body_vf_cc),
    crate::bind("collision", 0x4e3070, "CBody::VfD8", body_vf_d8),
    crate::bind("collision", 0x4e2f50, "CBody::VfE8", body_vf_e8),
    crate::bind("collision", 0x4e2e90, "CBody::VfF4", body_vf_f4),
    crate::bind("collision", 0x4e2850, "CBody::VfDC", body_vf_dc),
    crate::bind("collision", 0x4e3dc0, "CBrakeWall::QueryPoint", brake_wall_query_point),
    crate::bind("collision", 0x4fccb0, "E38Vec::M_CCB0", vec_add_sub),
    crate::bind("collision", 0x4fcd10, "E38Vec::M_CD10", vec_sub),
    crate::bind("collision", 0x4fcd60, "E38Vec::M_CD60", vec_sub_dot),
    crate::bind("collision", 0x4fcee0, "E38Vec::M_CEE0", vec_dot),
    crate::bind("collision", 0x4fcf30, "E38Vec::M_CF30", vec_cross),
    crate::bind("collision", 0x4fcfa0, "E38Vec::M_CFA0", vec_cross_dot),
    crate::bind("collision", 0x4fd030, "E38Vec::M_D030", vec_scale),
    crate::bind("collision", 0x4fd0f0, "E38_004FD0F0", vec_normalize),
    crate::bind("collision", 0x4fd230, "E38Ops::M_D230", world_axes_overlap),
    crate::bind("collision", 0x4fd2c0, "E38Ops::M_D2C0", box_sat_overlap),
    crate::bind("collision", 0x4fd480, "E38Ops::M_D480", ray_triangle),
    crate::bind("collision", 0x4fb7f0, "E38_004FB7F0", clear_contact_rows),
    crate::bind("collision", 0x4fbc20, "E38_004FBC20", refresh_events),
    crate::bind("collision", 0x4fbca0, "E38_004FBCA0", event_count),
    crate::bind("collision", 0x4fc0b0, "CCollisionSolver::Vf08", drain_events),
    crate::bind("collision", 0x4fc590, "E38_004FC590X", correct_penetrations),
    crate::bind("collision", 0x4fca20, "E38_004FCA20", solve_due_events),
    crate::bind("collision", 0x4fb890, "E38_004FB890", apply_offsets),
    crate::bind("collision", 0x4fbd30, "E38Ops::M_BD30", contact_rows),
    crate::bind("collision", 0x4fc1a0, "E38Ops::M_C1A0", solve_event),
    crate::bind("collision", 0x47d160, "CLink<CConnection<VCRegistrationZone>>::Unlink", body_link_unlink),
    crate::bind("collision", 0x43c5b0, "E01Vec4::CopyFrom", vec4_copy_from),
    crate::bind("collision", 0x43c630, "E01VecSetA::Cross", vec_cross),
    crate::bind("collision", 0x465d80, "E09Vec4::Clear", vec4_clear),
    crate::bind("collision", 0x473cd0, "CFloat3::Scale", float3_scale),
    crate::bind("collision", FLOAT3_LENGTH, "CFloat3::Length", float3_length),
    crate::bind("collision", 0x4e33e0, "CBody::CState::Unk44", state_unka44),
    crate::bind("collision", 0x4e0080, "CBodyLink::NotifyFiltered", notify_filtered),
    crate::bind("collision", 0x4e0ae0, "E33Vec4::MinMaxA", min_max_a),
    crate::bind("collision", 0x4e11b0, "CBody::Vf10C", body_has_grid),
    crate::bind("collision", 0x4e11e0, "CBody::Vf114", body_has_grid),
    crate::bind("collision", BODY_VF110, "CBody::Vf110", body_grid_active),
    crate::bind("collision", 0x4e3860, "CBrakeWall::Vf110", brake_wall_is_active),
    crate::bind("collision", 0x4fc500, "E38_004FC500", min_contact_time),
    crate::bind("collision", 0x4f6610, "E37GridOps::M6610", grid_ops_m6610),
    crate::bind("collision", E37_EDGE_EDGE, "E37GridOps::EdgeEdge", edge_edge),
    crate::bind("collision", E37_EDGE_PAIR, "E37GridOps::EdgePair", edge_pair),
    crate::bind("collision", E37_WALK_5FF0, "E37GridOps::Walk5FF0", walk_5ff0),
    crate::bind("collision", E37_M74A0, "E37GridOps::M74A0", edge_normal),
    crate::bind("collision", E37_STORE_5570, "E37VecSet::Store5570", vec_set_gap),
    crate::bind("collision", 0x4f65b0, "E37GridOps::AddEdge", grid_add_edge),
    crate::bind("collision", 0x4e42f0, "E34WallVf::VfCC", wall_vf_cc),
    crate::bind("collision", BRAKE_WALL_QUERY_BOX, "CBrakeWall::QueryBox", brake_wall_query_box),
    crate::bind("collision", BODY_VF11C, "CBody::Vf11C", body_vf11c),
    crate::bind("collision", 0x4e1eb0, "CBody::VfD0", body_vf_d0),
    crate::bind("collision", 0x4e3970, "CBrakeWall::VfF0", brake_wall_vf_f0),
    crate::bind("collision", 0x4e3a10, "CBrakeWall::VfE8", brake_wall_vf_e8),
    crate::bind("collision", E37_PLANE_DIFF, "E37EvOps::PlaneDiff", plane_diff),
    crate::bind("collision", 0x4fb070, "CCollisionMeshOpen::CopyBounds", mesh_copy_bounds),
    crate::bind("collision", 0x4fb0b0, "CCollisionMeshOpen::IsActive", mesh_is_active),
    crate::bind("collision", 0x4f4390, "E37EvOps::Query4390", event_query_bodies),
    crate::bind("collision", 0x4f4dc0, "E37EvOps::Emit4DC0", apply_event_impulse),
    crate::bind("collision", E01_VEC4_NORMALIZE, "E01Vec4::Normalize", vec4_normalize),
    crate::bind("collision", E09_VEC4_SUB, "E09Vec4::Sub", vec4_sub),
    crate::bind("collision", E21_VEC3_ADD, "E21Vec3::Add", vec3_add),
    crate::bind("collision", COLGRID_SAMPLE_A, "CColGrid::SampleA", grid_sample_a),
    crate::bind("collision", COLGRID_SAMPLE_B, "CColGrid::SampleB", grid_sample_b),
    crate::bind("collision", E33_MAKE_EVENT, "E33MakeEvent", make_event),
    crate::bind("collision", E36_ALLOC_EVENT, "E36AllocEvent", alloc_event),
    crate::bind("collision", COLL_EVENT_INIT_FULL, "CCollisionEvent::InitFull", init_full),
    crate::bind("collision", 0x4fc9a0, "CCollisionSolver::Vf04", solver_add_event),
];

pub(crate) const CSTATE_UNKA44: u32 = 0x4e33e0;
const E37_WALL_SYNC: u32 = 0x4f7090;
const E09_VEC4_CLEAR: u32 = 0x465d80;
pub(crate) const E01_VEC4_COPY_FROM: u32 = 0x43c5b0;
const SAMPLE_TABLE: u32 = 0x16592b8;

pub(crate) const E45_CELL_QUERY: u32 = 0x5240c0;
pub(crate) const FLOAT3_LENGTH: u32 = 0x473b30;
pub(crate) const BODY_GRID: u32 = 0x4e11f0;

/// The original callees that E36Collect calls, run in the emulator. The virtual slots are thiscall on their owner, the
/// E45 query is cdecl, and CFloat3::Length returns its result in st0.
struct Callees<'a>(&'a dyn Host);

pub(crate) const E01_VEC4_NORMALIZE: u32 = 0x43c6a0;
const E21_VEC3_ADD: u32 = 0x4a19e0;
const E09_VEC4_SUB: u32 = 0x465da0;
const E33_MAKE_EVENT: u32 = 0x4e0d00;
pub(crate) const COLGRID_SAMPLE_A: u32 = 0x4e0840;
pub(crate) const COLGRID_SAMPLE_B: u32 = 0x4e0950;
pub(crate) const E37_GRID_COPY: u32 = 0x4f6610;
pub(crate) const E37_EDGE_EDGE: u32 = 0x4f8280;
const E37_EDGE_PAIR: u32 = 0x4f8560;
const E37_WALK_5FF0: u32 = 0x4f5ff0;
const E37_M74A0: u32 = 0x4f74a0;
const E37_STORE_5570: u32 = 0x4f5570;
const MAT_COLLIDE_INIT: u32 = 0x51bd30;
