//! The Mem store of the collision subsystem: the layout's reads and writes behind `CollisionStore`.

use super::e36_box::{self, E36Box};
use super::event_pool::{self, EVENT_BODY, EVENT_FLAGS, EVENT_MATERIAL_A, EVENT_MATERIAL_B, EVENT_NEXT, EVENT_PARTNER};
use super::mesh_query::{load_material_pair, material_manager};
use super::solver::{
    BODY_CONTACT_ROW, BODY_CONTACT_ROW_SIZE, BODY_OFFSET_ROW, BODY_POSITION, FRAME_GLOBAL, SOLVER_CONTACT_COUNT,
    SOLVER_CONTACT_ROW, SOLVER_CONTACT_ROWS, SOLVER_EVENTS, SOLVER_EVENT_COUNT, SOLVER_OFFSET_COUNT, SOLVER_OFFSET_ROW,
    SOLVER_OFFSET_ROWS,
};
use super::{list, sort, GRID_TAG};
use crate::collision::contact_solver::{self, ContactMaterial, ContactRow, RowEvent};
use crate::collision::obb::{ObbContact, ObbFrame, SphereContact};
use crate::collision::store::{CollisionStore, EntryFrame, EventIds, EventPlane, Side};
use crate::entities::layout::ROOT_IDENTITY;
use crate::math::vec::{clear_vec4, Vec3, Vec4};
use crate::mem::Mem;
use crate::physics_core;
use crate::physics_core::layout::vtable_target;

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
}
