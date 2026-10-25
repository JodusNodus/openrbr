//! The Mem store of the collision subsystem: the layout's reads and writes behind `CollisionStore`.

use super::col_grids::{ColGrid, PHYSICS_GLOBAL};
use super::e36_box::{self, E36Box};
use super::event_pool::{self, EVENT_BODY, EVENT_FLAGS, EVENT_MATERIAL_A, EVENT_MATERIAL_B, EVENT_NEXT, EVENT_PARTNER};
use super::mesh_query::{load_material_pair, material_manager};
use super::solver::{
    BODY_CONTACT_ROW, BODY_CONTACT_ROW_SIZE, BODY_OFFSET_ROW, BODY_POSITION, FRAME_GLOBAL, SOLVER_CONTACT_COUNT,
    SOLVER_CONTACT_ROW, SOLVER_CONTACT_ROWS, SOLVER_EVENTS, SOLVER_EVENT_COUNT, SOLVER_OFFSET_COUNT, SOLVER_OFFSET_ROW,
    SOLVER_OFFSET_ROWS,
};
use super::walls::{HITS_COUNT, PENETRATION_EMITTER, SEG_COUNT, SEG_INDEX, WALL_LATCH, WALL_LEAVES, WALL_MESH};
use super::{grid_edges, mesh_query};
use super::{list, sort, take_grid_flags, Body, GRID_TAG};
use crate::collision::brake_wall::{EdgeCross, SegmentHit};
use crate::collision::contact_solver::{self, ContactMaterial, ContactRow, RowEvent};
use crate::collision::face_sweep::{Bounds, Face};
use crate::collision::obb::{ObbContact, ObbFrame, SphereContact};
use crate::collision::overlap::SatBox;
use crate::collision::store::{
    CollisionStore, EntryFrame, EventIds, EventPlane, RayHit, Side, SurfaceRecord, TreeNode,
};
use crate::entities::layout::ROOT_IDENTITY;
use crate::math::vec::{clear_vec4, Vec3, Vec4};
use crate::mem::Mem;
use crate::physics_core;
use crate::physics_core::layout::{vtable_target, SIGNAL_MANAGER};

/// `CCollisionEvent::Vf1Cx` (0x4f52f0), the release `list::drain` runs on each event.
const EVENT_RELEASE: u32 = 0x4f52f0;
/// The event's contact points, plane and the body flag byte.
const EVENT_POINT_PARTNER: u32 = 0x60;
const EVENT_POINT_BODY: u32 = 0xc0;
const EVENT_PLANE: u32 = 0x70;
const BODY_FLAGS: u32 = 0x60;
const BODY_PROGRESS: u32 = 0x290;
/// The material flag byte (`+0x158`) whose bit 3 suppresses the event.
const MATERIAL_FLAGS: u32 = 0x158;

impl CollisionStore for Mem {
    fn vec4(&self, at: u32) -> Vec4 {
        Vec4::load(self, at)
    }

    fn store_vec4(&mut self, at: u32, value: Vec4) {
        value.store(self, at);
    }

    fn vtable_slot(&self, object: u32, slot: u32) -> u32 {
        vtable_target(self, object, slot)
    }

    fn first_event(&self, solver: u32) -> u32 {
        self.u32(solver + SOLVER_EVENTS)
    }

    fn event_list_len(&self, solver: u32) -> u32 {
        self.u32(solver + SOLVER_EVENTS + 8)
    }

    fn event_count(&self, solver: u32) -> u32 {
        self.u32(solver + SOLVER_EVENT_COUNT)
    }

    fn next_event(&self, event: u32) -> u32 {
        self.u32(event + EVENT_NEXT)
    }

    fn event_releases_to_pool(&self, event: u32) -> bool {
        vtable_target(self, event, 0x1c) == EVENT_RELEASE
    }

    fn drain_event_list(&mut self, solver: u32) {
        list::drain(self, solver + SOLVER_EVENTS, 1);
    }

    fn release_event(&mut self, solver: u32, event: u32) {
        list::release(self, solver + SOLVER_EVENTS, event + EVENT_NEXT);
    }

    fn pool_event(&mut self, event: u32) {
        event_pool::vf1cx(self, event, 1);
    }

    fn event_body(&self, event: u32) -> u32 {
        self.u32(event + EVENT_BODY)
    }

    fn event_partner(&self, event: u32) -> u32 {
        self.u32(event + EVENT_PARTNER)
    }

    fn event_flags(&self, event: u32) -> u32 {
        self.u32(event + EVENT_FLAGS)
    }

    fn store_event_flags(&mut self, event: u32, flags: u32) {
        self.set_u32(event + EVENT_FLAGS, flags);
    }

    fn event_suppressed(&self, event: u32) -> bool {
        event_pool::is_suppressed(self, event)
    }

    fn event_time(&self, event: u32) -> f32 {
        sort::event_get_94(self, event)
    }

    fn event_plane(&self, event: u32) -> EventPlane {
        EventPlane {
            flags: self.u32(event + EVENT_FLAGS),
            plane: Vec4::load(self, event + EVENT_PLANE),
            point: Vec3::load(self, event + EVENT_POINT_BODY),
        }
    }

    fn event_normal(&self, event: u32) -> Vec4 {
        Vec4::load(self, event + EVENT_PLANE)
    }

    fn refresh_event_materials(&mut self, event: u32) {
        event_pool::refresh_materials(self, event);
    }

    fn side_suppresses(&self, event: u32, side: Side) -> bool {
        let id = match side {
            Side::Body => EVENT_MATERIAL_B,
            Side::Partner => EVENT_MATERIAL_A,
        };
        let material = physics_core::layout::material::tag_entry(self, material_manager(self), self.u32(event + id));
        self.u8(material + MATERIAL_FLAGS) & 8 != 0
    }

    fn row_event(&self, event: u32) -> RowEvent {
        RowEvent {
            point_a: Vec4::load(self, event + EVENT_POINT_PARTNER),
            point_b: Vec4::load(self, event + EVENT_POINT_BODY),
            plane: Vec4::load(self, event + EVENT_PLANE),
            ids_a: [self.u32(event + 0x8c), self.u32(event + 0x90)],
            ids_b: [self.u32(event + 0xa4), self.u32(event + 0xa8)],
            flags: self.u32(event + EVENT_FLAGS),
        }
    }

    fn event_material(&self, event: u32) -> (u32, ContactMaterial) {
        let tag = contact_solver::pick_material_tag(load_material_pair(self, event));
        let entry = physics_core::layout::material::tag_entry(self, material_manager(self), tag);
        let material = ContactMaterial {
            friction_fast: self.f32(entry + 0x108),
            friction_slow: self.f32(entry + 0x118),
            restitution_fast: self.f32(entry + 0x128),
            restitution_slow: self.f32(entry + 0x138),
        };
        (tag, material)
    }

    fn event_other_point(&self, event: u32, side: u32) -> u32 {
        event + if side == 0 { EVENT_POINT_PARTNER } else { EVENT_POINT_BODY }
    }

    fn body_active(&self, body: u32) -> bool {
        self.u8(body + BODY_FLAGS) & 4 != 0
    }

    fn body_progress(&self, body: u32) -> f32 {
        self.f32(body + BODY_PROGRESS)
    }

    fn body_position(&self, body: u32) -> Vec4 {
        Vec4::load(self, body + BODY_POSITION)
    }

    fn store_body_position(&mut self, body: u32, value: Vec4) {
        value.store(self, body + BODY_POSITION);
    }

    fn body_contact_rows(&self, body: u32) -> u32 {
        self.u32(body + BODY_CONTACT_ROW)
    }

    fn set_body_contact_rows(&mut self, body: u32, rows: u32) {
        self.set_u32(body + BODY_CONTACT_ROW, rows);
    }

    fn body_offset_row(&self, body: u32) -> u32 {
        self.u32(body + BODY_OFFSET_ROW)
    }

    fn set_body_offset_row(&mut self, body: u32, row: u32) {
        self.set_u32(body + BODY_OFFSET_ROW, row);
    }

    fn frame_root(&self) -> u32 {
        self.u32(FRAME_GLOBAL)
    }

    fn solver_contact_count(&self, solver: u32) -> u32 {
        self.u32(solver + SOLVER_CONTACT_COUNT)
    }

    fn set_solver_contact_count(&mut self, solver: u32, count: u32) {
        self.set_u32(solver + SOLVER_CONTACT_COUNT, count);
    }

    fn solver_contact_body(&self, solver: u32, index: u32) -> u32 {
        self.u32(solver + SOLVER_CONTACT_ROWS + index * SOLVER_CONTACT_ROW)
    }

    fn solver_contact_row(&self, solver: u32, index: u32) -> u32 {
        solver + SOLVER_CONTACT_ROWS + index * SOLVER_CONTACT_ROW
    }

    fn open_contact_row(&mut self, rows: u32, body: u32) {
        self.set_u32(rows, body);
        self.set_u32(rows + 4, 0);
        self.set_u32(body + BODY_CONTACT_ROW, rows);
    }

    fn contact_row_count(&self, rows: u32) -> u32 {
        self.u32(rows + 4)
    }

    fn set_contact_row_count(&mut self, rows: u32, count: u32) {
        self.set_u32(rows + 4, count);
    }

    fn store_contact_row(&mut self, rows: u32, index: u32, row: &ContactRow) {
        let at = rows + 0x10 + index * BODY_CONTACT_ROW_SIZE;
        let words = [
            row.suppressed,
            row.tag,
            row.ids[0],
            row.ids[1],
            row.restitution[0].to_bits(),
            row.restitution[1].to_bits(),
            row.friction[0].to_bits(),
            row.friction[1].to_bits(),
        ];
        for (i, word) in words.into_iter().enumerate() {
            self.set_u32(at + 4 * i as u32, word);
        }
        for (i, v) in [row.point, row.plane, row.composed, row.other].into_iter().enumerate() {
            v.store(self, at + 0x20 + 0x10 * i as u32);
        }
    }

    fn solver_offset_count(&self, solver: u32) -> u32 {
        self.u32(solver + SOLVER_OFFSET_COUNT)
    }

    fn set_solver_offset_count(&mut self, solver: u32, count: u32) {
        self.set_u32(solver + SOLVER_OFFSET_COUNT, count);
    }

    fn solver_offset_row(&self, solver: u32, index: u32) -> u32 {
        solver + SOLVER_OFFSET_ROWS + SOLVER_OFFSET_ROW * index
    }

    fn offset_row_body(&self, row: u32) -> u32 {
        self.u32(row)
    }

    fn offset_row_count(&self, row: u32) -> u32 {
        self.u32(row + 4)
    }

    fn set_offset_row_count(&mut self, row: u32, count: u32) {
        self.set_u32(row + 4, count);
    }

    fn offset(&self, row: u32, index: u32) -> Vec4 {
        Vec4::load(self, row + 0x10 + 0x10 * index)
    }

    fn store_offset(&mut self, row: u32, index: u32, value: Vec4) {
        value.store(self, row + 0x10 + 0x10 * index);
    }

    fn open_offset_row(&mut self, row: u32, body: u32) {
        self.set_u32(row, body);
        self.set_u32(body + BODY_OFFSET_ROW, row);
        self.set_u32(row + 4, 1);
    }

    fn obb_frame(&self, object: u32) -> ObbFrame {
        E36Box::new(object).frame(self)
    }

    fn sphere_input(&self, at: u32) -> (Vec4, f32) {
        (Vec4::load(self, at), self.f32(at + 0x10))
    }

    fn obb_input(&self, at: u32) -> [Vec4; 5] {
        [0, 0x10, 0x20, 0x30, 0x40].map(|lane| Vec4::load(self, at + lane))
    }

    fn contact_normal_w(&self, object: u32, index: u32) -> f32 {
        E36Box::new(object).contact(index).normal(self).w
    }

    fn store_sphere_contact(&mut self, object: u32, index: u32, input: Vec4, contact: &SphereContact) {
        let slot = E36Box::new(object).contact(index);
        slot.set_input(self, input);
        slot.set_depth(self, contact.depth);
        slot.set_position(self, contact.position);
        slot.clear_kind(self);
        slot.set_normal(self, contact.normal);
    }

    fn store_obb_contact(&mut self, object: u32, index: u32, input: Vec4, contact: &ObbContact) {
        let slot = E36Box::new(object).contact(index);
        let position_w = slot.position(self).w;
        slot.set_depth(self, contact.depth);
        slot.set_input(self, input);
        slot.set_position(self, contact.position.extend(position_w));
        slot.set_metric(self, contact.metric);
        slot.set_normal(self, contact.normal);
    }

    fn store_contact_tag(&mut self, object: u32, index: u32, tag: u8) {
        self.map(object.wrapping_add(index.wrapping_mul(0x40)).wrapping_add(0xC4), &[tag]);
    }

    fn entry_frame(&self, object: u32) -> EntryFrame {
        EntryFrame {
            centre_in: object,
            centre_out: object + 0x10,
            axes_in: [object + 0x60, object + 0x70, object + 0x80],
            axes_out: [object + 0x30, object + 0x40, object + 0x50],
        }
    }

    fn entry_centre(&self, object: u32) -> [f32; 3] {
        [0, 4, 8].map(|lane| self.f32(object + 0x10 + lane))
    }

    fn contact_list(&self, object: u32) -> u32 {
        object.wrapping_add(0x90)
    }

    fn collide_matrix(&self) -> u32 {
        self.u32(0x7CBB00)
    }

    fn collision_shape(&self, collision: u32) -> u32 {
        self.u32(collision + 0x680)
    }

    fn collision_is_sphere(&self, collision: u32) -> bool {
        self.u8(collision + 0x68A) & 4 != 0
    }

    fn collision_tag(&self, collision: u32) -> u8 {
        self.u8(collision + 0x68F)
    }

    fn shape_axis(&self, shape: u32, axis: u32) -> Vec4 {
        e36_box::shape_axis(self, shape, axis as usize)
    }

    fn shape_extent(&self, shape: u32, axis: u32) -> f32 {
        e36_box::shape_extent(self, shape, axis as usize)
    }

    fn cell_first(&self, cell: u32) -> u32 {
        self.u32(cell)
    }

    fn node_next(&self, node: u32) -> u32 {
        self.u32(node)
    }

    fn node_owner(&self, node: u32) -> u32 {
        self.u32(node + 12)
    }

    fn vec3(&self, at: u32) -> Vec3 {
        Vec3::load(self, at)
    }

    fn store_vec3(&mut self, at: u32, value: Vec3) {
        value.store(self, at);
    }

    fn event_point(&self, event: u32) -> u32 {
        event + EVENT_POINT_PARTNER
    }

    fn event_materials_at(&self, event: u32, side: Side) -> u32 {
        event
            + match side {
                Side::Body => EVENT_MATERIAL_B,
                Side::Partner => EVENT_MATERIAL_A,
            }
    }

    fn event_ids(&self, event: u32) -> EventIds {
        EventIds {
            first: [self.u32(event + 0x8c), self.u32(event + 0x90)],
            second: [self.u32(event + 0xa4), self.u32(event + 0xa8)],
        }
    }

    fn event_contact_outputs(&self, event: u32) -> (u32, u32) {
        (event + EVENT_POINT_BODY, event + 0xd0)
    }

    fn init_event(&mut self, event: u32, kind: u32, body: u32, tag: u32, payload: u32) {
        for offset in [0x10, 0xc, 0x14] {
            self.set_u32(event + offset, 0);
        }
        // M_AFA0: the handle on the root identity, the count one more than the root's (two vtable stores, both
        // replaced).
        let root = self.u32(ROOT_IDENTITY);
        self.set_u32(event + 4, root);
        let count = if root == 0 { 0 } else { self.u32(root + 8).wrapping_add(1) };
        self.set_u32(event + 8, count);
        self.set_u32(event, event_pool::EVENT_BASE_VTABLE);
        clear_vec4().store(self, event + 0x40);
        clear_vec4().store(self, event + 0x50);
        for offset in [0x80, 0x84, 0x88, 0xac, 0xb0, 0xb4] {
            self.set_u32(event + offset, 0);
        }
        self.set_u32(event + 0xe0, tag);
        self.set_u32(event + 0xa8, 0);
        self.set_u32(event + 0xa4, 0);
        for k in 0..16 {
            self.set_u32(event + 0x60 + 4 * k, self.u32(payload + 4 * k));
        }
        self.set_u32(event + EVENT_BODY, body);
        self.set_u32(event + EVENT_FLAGS, kind);
    }

    fn count_event(&mut self) {
        self.set_u32(event_pool::EVENT_COUNT, self.u32(event_pool::EVENT_COUNT).wrapping_add(1));
    }

    fn free_event(&self) -> u32 {
        self.u32(event_pool::FREE_LIST_HEAD)
    }

    fn take_free_event(&mut self, event: u32) {
        if self.u32(event + 0x14) == event_pool::FREE_LIST_HEAD {
            self.set_u32(event_pool::FREE_LIST_COUNT, self.u32(event_pool::FREE_LIST_COUNT).wrapping_sub(1));
            list::unlink(self, event + 0x0c);
        }
    }

    fn take_grid_hit_flags(&mut self, grid: u32) -> [u32; 2] {
        let flags = [u32::from(self.u8(grid + 0x688)), u32::from(self.u8(grid + 0x689))];
        self.map(grid + 0x688, &[0, 0]);
        flags
    }

    fn grid_tag(&self, grid: u32) -> u32 {
        u32::from(self.u8(grid + GRID_TAG))
    }

    fn store_event_tag(&mut self, event: u32, tag: u32) {
        self.set_u32(event + EVENT_MATERIAL_B + 4, tag);
        self.set_u32(event + EVENT_MATERIAL_B, tag);
        self.set_u32(event + EVENT_MATERIAL_B + 8, 0);
    }

    fn entity_flags(&self, entity: u32) -> u8 {
        self.u8(entity + BODY_FLAGS)
    }

    fn world_flags(&self) -> u32 {
        self.u32(self.u32(self.u32(PHYSICS_GLOBAL) + 0x24) + 0x160cc)
    }

    fn body_grid(&self, body: u32) -> u32 {
        Body::new(body).grid(self)
    }

    fn grid_edge_count(&self, grid: u32) -> u8 {
        ColGrid::new(grid).edge_count(self)
    }

    fn grid_pair_count(&self, grid: u32) -> u8 {
        ColGrid::new(grid).pair_count(self)
    }

    fn grid_status_flag(&self, grid: u32) -> u8 {
        ColGrid::new(grid).status_flag(self)
    }

    fn grid_edge(&self, grid: u32, index: u32) -> u32 {
        ColGrid::new(grid).edge(index)
    }

    fn grid_pair(&self, grid: u32, index: u32) -> [u8; 6] {
        ColGrid::new(grid).pair(self, index as usize)
    }

    fn take_body_grid_flags(&mut self, body: u32) -> (u32, u32) {
        take_grid_flags(self, body)
    }

    fn event_sample_slots(&self, event: u32) -> (u32, u32) {
        (event + 0xa4, event + 0xa8)
    }

    fn store_event_samples(&mut self, event: u32, first: u32, second: u32) {
        self.set_u32(event + 0xa4, first);
        self.set_u32(event + 0xa8, second);
    }

    fn grid_bounds(&self, grid: u32) -> Bounds {
        grid_edges::edge_edge_bounds(self, grid)
    }

    fn grid_face_count(&self, grid: u32) -> u32 {
        grid_edges::edge_edge_face_count(self, grid)
    }

    fn grid_face_is_valid(&self, grid: u32, index: u32) -> bool {
        grid_edges::edge_edge_face_is_valid(self, grid, index)
    }

    fn grid_face(&self, grid: u32, index: u32) -> Face {
        grid_edges::edge_edge_face(self, grid, index)
    }

    fn grid_face_samples(&self, grid: u32, index: u32) -> [u8; 3] {
        grid_edges::edge_edge_sample_indices(self, grid, index)
    }

    fn grid_face_vertices(&self, grid: u32, indices: [u8; 3]) -> [Vec3; 3] {
        grid_edges::edge_edge_vertices(self, grid, indices)
    }

    fn grid_material(&self, grid: u32) -> u32 {
        grid_edges::edge_edge_hit_word(self, grid)
    }

    fn grid_sample_dimensions(&self, grid: u32) -> Option<(u16, u16)> {
        grid_edges::edge_edge_sample_dimensions(self, grid)
    }

    fn grid_vertex_samples(&self, grid: u32, indices: [u8; 3]) -> [[u8; 2]; 3] {
        grid_edges::edge_edge_vertex_samples(self, grid, indices)
    }

    fn grid_sample_bytes(&self, grid: u32, first: i32, second: i32) -> [u8; 2] {
        grid_edges::edge_edge_sample_bytes(self, grid, first, second)
    }

    fn store_grid_sample_bytes(&mut self, grid: u32, bytes: [u8; 2]) {
        grid_edges::store_edge_edge_sample_bytes(self, grid, bytes);
    }

    fn grid_vertex(&self, grid: u32, slot: u8) -> Vec3 {
        Vec3::load(self, grid + (u32::from(slot) + 4) * 0x10)
    }

    fn grid_pair_row(&self, grid: u32, index: u32) -> [u8; 2] {
        let row = grid + index.wrapping_mul(6);
        [self.u8(row + 0x484), self.u8(row + 0x485)]
    }

    fn grid_sample_slot(&self, grid: u32, slot: u8) -> u8 {
        self.u8(grid + 0x690 + u32::from(slot))
    }

    fn grid_sample_pair(&self, grid: u32, sample: u8) -> [u8; 2] {
        let at = self.u32(grid + 0x400) + 2 * u32::from(sample);
        [self.u8(at), self.u8(at + 1)]
    }

    fn copy_grid(&mut self, grid: u32, src: u32) {
        self.copy(grid, src, ColGrid::SIZE);
    }

    fn grid_blocks(&self, grid: u32) -> (u32, u32) {
        (grid + 0x40, grid + 0x220)
    }

    fn grid_half_scale(&self, grid: u32) -> f32 {
        self.f32(grid + 0x3c)
    }

    fn store_grid_bounds(&mut self, grid: u32, low: Vec4, high: Vec4, centre: Vec4, half: Vec4) {
        low.store(self, grid);
        high.store(self, grid + 0x10);
        centre.store(self, grid + 0x20);
        half.store(self, grid + 0x30);
    }

    fn sat_box(&self, at: u32) -> SatBox {
        mesh_query::load_sat_box(self, at)
    }

    fn tree_node(&self, node: u32) -> TreeNode {
        TreeNode { word: self.u32(node + 0x18), link: self.u32(node + 0x1c) }
    }

    fn child_pair(&self, link: u32) -> [(u32, u32); 2] {
        [(link, self.u32(link + 0x18)), (link + 0x20, self.u32(link + 0x38))]
    }

    fn tree_root(&self, owner: u32) -> u32 {
        self.u32(owner + 8)
    }

    fn own_list(&self, owner: u32) -> u32 {
        owner + 0x1014
    }

    fn list_len(&self, list: u32) -> u32 {
        self.u32(list)
    }

    fn list_record(&self, list: u32, index: u32) -> u32 {
        self.u32(list + 4 * index)
    }

    fn mesh_table(&self, owner: u32, index: u32) -> u32 {
        self.u32(owner + 0xc + 4 * index)
    }

    fn mesh_vertex(&self, table: u32, slot: u16) -> Vec3 {
        Vec3::load(self, table + 12 * u32::from(slot))
    }

    fn face_bytes(&self, at: u32) -> [u8; 0x12] {
        std::array::from_fn(|i| self.u8(at + i as u32))
    }

    fn mesh_flags(&self, object: u32) -> u32 {
        self.u32(object + 0x60)
    }

    fn mesh_block(&self, object: u32) -> u32 {
        mesh_query::mesh_vertices(self, object)
    }

    fn mesh_point(&self, block: u32, index: u32) -> Vec3 {
        Vec3::load(self, block + 16 * index)
    }

    fn mesh_tag(&self, object: u32, index: u32) -> u32 {
        mesh_query::mesh_tag(self, object, index as usize)
    }

    fn clear_ray_seeds(&mut self, object: u32) {
        self.set_u8(object + 0x688, 0);
        self.set_u8(object + 0x689, 0);
        self.set_u32(object + 0x680, 0);
        self.set_u32(object + 0x400, 0);
        self.set_u32(object + 0x408, 0);
        self.set_u32(object + 0x40c, 0);
        self.set_u32(object + 0x410, 0);
        self.set_u8(object + 0x68b, 0);
        self.set_u8(object + 0x68c, 0);
        self.set_u8(object + 0x68d, 0);
        self.set_u8(object + 0x68e, 0);
    }

    fn ray_vectors(&self, object: u32) -> (u32, u32) {
        (object, object + 0x10)
    }

    fn clear_ray(&mut self, object: u32) {
        self.set_u32(object + 0x400, 0);
        self.set_u32(object + 0x408, 0);
        self.set_u32(object + 0x40c, 0);
        self.set_u8(object + 0x68b, 0);
        self.set_u8(object + 0x68d, 0);
        self.set_u32(object + 0x680, 0);
    }

    fn store_surface_record(&mut self, out: u32, record: &SurfaceRecord) {
        record.point.store(self, out);
        record.normal.store(self, out + 0x10);
        self.set_f32(out + 0x1c, record.plane_offset);
        self.set_f32(out + 0x20, record.channels[0]);
        self.set_f32(out + 0x24, record.channels[1]);
        self.set_u32(out + 0x28, record.surfaces[0]);
        self.set_u32(out + 0x2c, record.surfaces[1]);
        self.set_f32(out + 0x30, record.t);
        self.set_u32(out + 0x34, record.flags[0]);
        self.set_u32(out + 0x38, record.flags[1]);
    }

    fn ray_hit(&self, hit: u32) -> RayHit {
        RayHit {
            normal: Vec3::load(self, hit + 0xc),
            face: self.u32(hit + 0x18),
            table: self.u32(hit + 0x1c),
            t: self.f32(hit + 0x20),
            u: self.f32(hit + 0x24),
            v: self.f32(hit + 0x28),
        }
    }

    fn store_ray_hit(&mut self, hit: u32, value: &RayHit) {
        value.normal.store(self, hit + 0xc);
        self.set_f32(hit + 0x20, value.t);
        self.set_f32(hit + 0x24, value.u);
        self.set_f32(hit + 0x28, value.v);
        self.set_u32(hit + 0x1c, value.table);
        self.set_u32(hit + 0x18, value.face);
    }

    fn ray_hit_t(&self, hit: u32) -> f32 {
        self.f32(hit + 0x20)
    }

    fn set_ray_hit_t(&mut self, hit: u32, t: f32) {
        self.set_f32(hit + 0x20, t);
    }

    fn store_ray_hit_normal(&mut self, hit: u32, normal: Vec3) {
        normal.store(self, hit + 0xc);
    }

    fn wall_root(&self, wall: u32) -> u32 {
        wall + 0x64
    }

    fn wall_node(&self, node: u32) -> (u32, u32) {
        (self.u32(node + 0x10), self.u32(node + 0x14))
    }

    fn wall_children(&self, link: u32) -> [u32; 2] {
        [link, link + 0x18]
    }

    fn wall_box(&self, node: u32) -> [f32; 4] {
        [self.f32(node), self.f32(node + 4), self.f32(node + 8), self.f32(node + 12)]
    }

    fn clear_collector(&mut self, collector: u32) {
        self.set_u32(collector + HITS_COUNT, 0);
    }

    fn collector_count(&self, collector: u32) -> u32 {
        self.u32(collector + HITS_COUNT)
    }

    fn collector_push(&mut self, collector: u32, node: u32) {
        let count = self.u32(collector + HITS_COUNT);
        self.set_u32(collector + count * 4, node);
        self.set_u32(collector + HITS_COUNT, count + 1);
    }

    fn collector_leaf(&self, collector: u32, index: u32) -> u32 {
        self.u32(collector + 4 * index)
    }

    fn wall_latch(&self, wall: u32) -> u32 {
        self.u32(wall + WALL_LATCH)
    }

    fn set_wall_latch(&mut self, wall: u32, value: u32) {
        self.set_u32(wall + WALL_LATCH, value);
    }

    fn wall_leaves(&self, wall: u32) -> u32 {
        wall + WALL_LEAVES
    }

    fn wall_mesh(&self, wall: u32) -> u32 {
        self.u32(wall + WALL_MESH)
    }

    fn wall_segment(&self, mesh: u32, index: u32) -> [[f32; 2]; 4] {
        let base = mesh + 8 * index;
        std::array::from_fn(|k| {
            let at = base + 8 * k as u32;
            [self.f32(at), self.f32(at + 4)]
        })
    }

    fn leaf_count(&self, leaf: u32) -> u32 {
        self.u32(leaf + SEG_COUNT)
    }

    fn leaf_word(&self, leaf: u32, j: u32) -> u32 {
        u32::from(self.u16(self.u32(leaf + SEG_INDEX) + 2 * j))
    }

    fn penetration_emitter(&self) -> u32 {
        self.u32(SIGNAL_MANAGER) + PENETRATION_EMITTER
    }

    fn store_wall_probe(&mut self, out: u32, hit: &SegmentHit, flags: u32) {
        self.set_f32(out, hit.normal[0]);
        self.set_f32(out + 4, hit.normal[1]);
        self.set_f32(out + 8, 0.0);
        self.set_f32(out + 0x10, hit.penetration);
        self.set_f32(out + 0x14, hit.signed_distance);
        self.set_u32(out + 0x18, flags);
    }

    fn store_wall_flags(&mut self, out: u32, flags: u32) {
        self.set_u32(out + 0x18, flags);
    }

    fn store_wall_crossing(&mut self, out: u32, wall: u32, hit: &EdgeCross) {
        for (k, v) in hit.point.into_iter().enumerate() {
            self.set_f32(out + 4 * k as u32, v);
        }
        self.set_f32(out + 0x10, hit.normal[0]);
        self.set_f32(out + 0x14, hit.normal[1]);
        self.set_f32(out + 0x18, 0.0);
        self.set_f32(out + 0x1c, hit.offset);
        self.set_f32(out + 0x34, hit.t);
        self.set_u32(out + 0x38, wall);
    }
}
