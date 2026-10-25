//! `Mem`'s `EntityStore` for the entity layer around the hinges: the walker marks, the ground probe's record, the nodes,
//! frames and transforms, the dummy object, the exclusion sets and the car's body slots.

use super::collide_init::IDENTITY_GLOBAL;
use super::entity_slots::EXCL_SET;
use crate::entities::frame::EntityFrame;
use crate::entities::ground::Contact;
use crate::entities::store::{EntityStore, SlotLookup};
use crate::entities::transform::Transform;
use crate::entities::walk::Node;
use crate::math::vec::Vec3;
use crate::mem::Mem;
use crate::stage;

const WALKER_COUNT: u32 = 8;
/// The two walker marks of a CEntity (the walker, then its serial).
const WALKER_MARKS: [u32; 2] = [0x4, 0x20];
/// The solid's force and torque accumulators.
const BODY_FORCE: u32 = 0x370;
const BODY_TORQUE: u32 = 0x310;

/// Offsets of the child link and the depth of a walked node (`E43PairWalk::W9D0` and the others), and of the two words a
/// pair node holds (`E43PairWalk::MD50`, `MD70`).
const NODE_CHILD: u32 = 0x04;
const NODE_DEPTH: u32 = 0x08;
const PAIR_FIRST: u32 = 0x00;
const PAIR_SECOND: u32 = 0x04;

const FRAME_PARENT: u32 = 0x04;
const FRAME_DEPTH: u32 = 0x08;
const FRAME_OWNER: u32 = 0x0c;

const GRID_COUNT_FIRST: u32 = 0x68D;

const TABLE_SELF: u32 = 0x800;
const TABLE_COUNT: u32 = 0x804;
const SLOT_SIZE: u32 = 8;

/// The dummy's centre (+0x94) and half extents (+0xa0), as stored (x, z, y: see `dummy::box_corners`); its span group (+0xb0),
/// span segment (+0x7c) and post segment (+0x80); and the flag word the segments keep at +0x5c (bit 3, `8`).
const DUMMY_CENTER: u32 = 0x94;
const DUMMY_HALF: u32 = 0xa0;
const DUMMY_GROUP: u32 = 0xb0;
const DUMMY_SPAN: u32 = 0x7c;
const DUMMY_POST: u32 = 0x80;
const SEGMENT_FLAGS: u32 = 0x5c;
const SEGMENT_HIT: u32 = 0x8;

/// The flags object (the global's +0x24 word, `g_1659b70`), whose byte at +0x160cc holds the flags `CEntity::E43VfD4` tests.
const FLAGS_OBJECT: u32 = 0x24;
const FLAGS_BYTE: u32 = 0x160CC;

const CAR_BODY_SLOTS: u32 = 0x480;

impl EntityStore for Mem {
    fn identity(&self) -> u32 {
        self.u32(IDENTITY_GLOBAL)
    }

    fn walker_count(&self, walker: u32) -> u32 {
        self.u32(walker + WALKER_COUNT)
    }

    fn store_walker(&mut self, this: u32, mark: u32, walker: u32) {
        self.set_u32(this + WALKER_MARKS[mark as usize], walker);
    }

    fn store_serial(&mut self, this: u32, mark: u32, serial: u32) {
        self.set_u32(this + WALKER_MARKS[mark as usize] + 4, serial);
    }

    fn clear_accumulators(&mut self, this: u32) -> u32 {
        for at in [BODY_FORCE, BODY_TORQUE] {
            self.map(this + at, &[0; 16]);
        }
        this + BODY_TORQUE
    }

    fn probe_host(&self) -> u32 {
        stage::layout::CollisionQuery::GroundProbe.system(self)
    }

    fn probe_output(&self, at: u32) -> [f32; 13] {
        self.f32s(at)
    }

    fn store_probe_contact(&mut self, d: u32, contact: &Contact) {
        for (i, v) in contact.second.into_iter().enumerate() {
            self.set_f32(d + 0x10 + 4 * i as u32, v);
        }
        for (i, v) in contact.point.into_iter().enumerate() {
            self.set_f32(d + 0x20 + 4 * i as u32, v);
        }
        self.set_u32(d + 0x2C, 0);
        self.set_u32(d + 0x30, 0);
        self.set_f32(d + 0x34, contact.depth);
        self.set_u32(d + 0x38, contact.owner);
    }

    fn grid_count(&self, grid: u32) -> u32 {
        u32::from(self.u8(grid + GRID_COUNT_FIRST))
    }

    fn load_node(&self, at: u32) -> Node {
        Node { child: self.u32(at + NODE_CHILD), depth: self.u32(at + NODE_DEPTH) }
    }

    fn store_pair(&mut self, node: u32, first: u32, second: u32) -> u32 {
        self.set_u32(node + PAIR_FIRST, first);
        self.set_u32(node + PAIR_SECOND, second);
        node
    }

    fn load_entity_frame(&self, at: u32) -> EntityFrame {
        EntityFrame {
            parent: self.u32(at + FRAME_PARENT),
            depth: self.u32(at + FRAME_DEPTH),
            owner: self.u32(at + FRAME_OWNER),
        }
    }

    fn load_transform(&self, at: u32) -> Transform {
        Transform { matrix: self.u32(at), node: self.u32(at + 4) }
    }

    fn vf120_target(&self, entity: u32) -> u32 {
        self.u32(self.u32(entity) + 0x11c)
    }

    fn material_slot(&self, table: u32, index: u8) -> SlotLookup {
        let slots = self.u32(table + TABLE_SELF);
        if slots == 0 {
            return SlotLookup::NoTable;
        }
        if index as u32 >= self.u32(table + TABLE_COUNT) {
            return SlotLookup::OutOfRange;
        }
        SlotLookup::At(slots + SLOT_SIZE * index as u32)
    }

    fn entity_flags(&self) -> (u32, u8) {
        let object = self.u32(self.u32(crate::physics_core::layout::PHYSICS_GLOBAL) + FLAGS_OBJECT);
        (object, self.u8(object + FLAGS_BYTE))
    }

    fn dummy_extents(&self, this: u32) -> (Vec3, Vec3) {
        (Vec3::load(self, this + DUMMY_CENTER), Vec3::load(self, this + DUMMY_HALF))
    }

    fn dummy_group(&self, this: u32) -> u32 {
        self.u32(this + DUMMY_GROUP)
    }

    fn dummy_span(&self, this: u32) -> u32 {
        self.u32(this + DUMMY_SPAN)
    }

    fn dummy_post(&self, this: u32) -> u32 {
        self.u32(this + DUMMY_POST)
    }

    fn mark_segment_hit(&mut self, segment: u32) {
        let flags = self.u32(segment + SEGMENT_FLAGS);
        self.set_u32(segment + SEGMENT_FLAGS, flags | SEGMENT_HIT);
    }

    fn vec4(&self, at: u32) -> [f32; 4] {
        self.f32s(at)
    }

    fn store_vec4(&mut self, at: u32, value: [f32; 4]) {
        for (i, v) in value.into_iter().enumerate() {
            self.set_f32(at + 4 * i as u32, v);
        }
    }

    fn store_vec3(&mut self, at: u32, value: Vec3) {
        self.set_f32(at, value.x);
        self.set_f32(at + 4, value.y);
        self.set_f32(at + 8, value.z);
    }

    fn exclusion_set(&self, entity: u32) -> u32 {
        self.u32(entity + EXCL_SET)
    }

    fn store_exclusion_set(&mut self, entity: u32, set: u32) {
        self.set_u32(entity + EXCL_SET, set);
    }

    fn exclusion_count(&self, set: u32) -> u32 {
        self.u32(set)
    }

    fn store_exclusion_count(&mut self, set: u32, count: u32) {
        self.set_u32(set, count);
    }

    fn exclusion_member(&self, set: u32, index: u32) -> u32 {
        self.u32(set + (index + 1) * 4)
    }

    fn store_exclusion_member(&mut self, set: u32, index: u32, member: u32) {
        self.set_u32(set + (index + 1) * 4, member);
    }

    fn car_body_slot(&self, car: u32, index: u32) -> u32 {
        self.u32(car.wrapping_add(CAR_BODY_SLOTS + index * 4))
    }

    fn store_car_body_slot(&mut self, car: u32, index: u32, body: u32) {
        self.set_u32(car.wrapping_add(CAR_BODY_SLOTS + index * 4), body);
    }
}
