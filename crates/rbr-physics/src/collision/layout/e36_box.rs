//! E36's collision box and contact slots, and the sphere and OBB tests.

use crate::mem::Mem;

use crate::collision::e36::{self, ObbFrame};
use crate::math::vec::Vec4;

/// E36's collision object: an OBB frame, half-extents, and four 0x40-byte contact records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct E36Box {
    base: u32,
}

impl E36Box {
    pub const fn new(base: u32) -> Self {
        Self { base }
    }

    pub fn center(self, mem: &Mem) -> Vec4 {
        Vec4::load(mem, self.base + 0x10)
    }

    pub fn extent(self, mem: &Mem, axis: usize) -> f32 {
        mem.f32(self.base + 0x20 + axis as u32 * 4)
    }

    pub fn basis(self, mem: &Mem, axis: usize) -> [f32; 3] {
        let at = self.base + 0x30 + axis as u32 * 0x10;
        [mem.f32(at), mem.f32(at + 4), mem.f32(at + 8)]
    }

    pub fn contact(self, index: u32) -> E36Contact {
        E36Contact::new(self.base + index * 0x40)
    }

    /// The OBB frame: the centre, the half extents, and the basis rows.
    pub fn frame(self, mem: &Mem) -> ObbFrame {
        ObbFrame {
            center: self.center(mem),
            extents: [self.extent(mem, 0), self.extent(mem, 1), self.extent(mem, 2)],
            basis: [self.basis(mem, 0), self.basis(mem, 1), self.basis(mem, 2)],
        }
    }
}

/// One E36 result slot, relative to the start of an E36Box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct E36Contact {
    base: u32,
}

impl E36Contact {
    const POSITION: u32 = 0x90;
    const NORMAL: u32 = 0xA0;
    const INPUT: u32 = 0xB0;
    const DEPTH: u32 = 0xC0;
    const KIND: u32 = 0xC8;

    pub const fn new(base: u32) -> Self {
        Self { base }
    }

    pub fn position(self, mem: &Mem) -> Vec4 {
        Vec4::load(mem, self.base + Self::POSITION)
    }

    pub fn set_position(self, mem: &mut Mem, value: Vec4) {
        value.store(mem, self.base + Self::POSITION);
    }

    pub fn normal(self, mem: &Mem) -> Vec4 {
        Vec4::load(mem, self.base + Self::NORMAL)
    }

    pub fn set_normal(self, mem: &mut Mem, value: Vec4) {
        value.store(mem, self.base + Self::NORMAL);
    }

    pub fn set_input(self, mem: &mut Mem, value: Vec4) {
        value.store(mem, self.base + Self::INPUT);
    }

    pub fn set_depth(self, mem: &mut Mem, value: f32) {
        mem.set_f32(self.base + Self::DEPTH, value);
    }

    pub fn set_metric(self, mem: &mut Mem, value: f32) {
        mem.set_f32(self.base + Self::KIND, value);
    }

    pub fn clear_kind(self, mem: &mut Mem) {
        mem.set_u32(self.base + Self::KIND, 0);
    }
}

/// The OBB axis `axis` (0 to 2) of a collision shape: its direction vector at +0x20, +0x30, +0x40.
pub fn shape_axis(mem: &Mem, shape: u32, axis: usize) -> Vec4 {
    Vec4::load(mem, shape + 0x20 + axis as u32 * 0x10)
}

/// The half-length of OBB axis `axis` (0 to 2) of a collision shape: the float at +0x10, +0x14, +0x18.
pub fn shape_extent(mem: &Mem, shape: u32, axis: usize) -> f32 {
    mem.f32(shape + 0x10 + axis as u32 * 4)
}

/// E36SphereBox (0x4f2ce0): a separated sphere writes nothing. Otherwise the contact at `index` takes the sphere as its
/// input, then its depth, position, cleared kind, and normal.
pub fn sphere_box(mem: &mut Mem, object: u32, sphere_at: u32, index_at: u32) -> u32 {
    let shape = E36Box::new(object);
    let frame = shape.frame(mem);
    let sphere = Vec4::load(mem, sphere_at);
    let radius = mem.f32(sphere_at + 0x10);
    if !e36::sphere_overlaps(&frame, sphere, radius) {
        return 0;
    }

    let contact = shape.contact(mem.u32(index_at));
    contact.set_input(mem, sphere);
    let fields = e36::sphere_contact(&frame, sphere, radius, contact.normal(mem).w);
    contact.set_depth(mem, fields.depth);
    contact.set_position(mem, fields.position);
    contact.clear_kind(mem);
    contact.set_normal(mem, fields.normal);
    1
}

/// The second vector of E36ObbTest's input at `vector_at`: the lengths of the other box's three axes, its half extents
/// (E36Collect stores them at its frame +0x60, 0x4f3bf4-0x4f3c14; the w lane stays as the frame held it).
pub fn set_obb_extents(mem: &mut Mem, vector_at: u32, lengths: [f32; 3]) {
    for (at, length) in (vector_at + 0x10..).step_by(4).zip(lengths) {
        mem.set_f32(at, length);
    }
}

/// E36ObbTest (0x4f3030): a separated pair writes nothing. Otherwise the contact at `index` takes its depth, the input
/// vector `a`, the position (the position's w lane is kept), the metric, and the normal.
pub fn obb_test(mem: &mut Mem, object: u32, vector_at: u32, index_at: u32) -> u32 {
    let shape = E36Box::new(object);
    let frame = shape.frame(mem);
    let a = Vec4::load(mem, vector_at);
    let b = Vec4::load(mem, vector_at + 0x10);
    let c = Vec4::load(mem, vector_at + 0x20);
    let d = Vec4::load(mem, vector_at + 0x30);
    let e = Vec4::load(mem, vector_at + 0x40);
    if !e36::obb_overlaps(&frame, a, b, c, d, e) {
        return 0;
    }

    let contact = shape.contact(mem.u32(index_at));
    let fields = e36::obb_contact(&frame, a, b, contact.normal(mem).w);
    let position_w = contact.position(mem).w;
    contact.set_depth(mem, fields.depth);
    contact.set_input(mem, a);
    contact.set_position(mem, Vec4 { x: fields.position.x, y: fields.position.y, z: fields.position.z, w: position_w });
    contact.set_metric(mem, fields.metric);
    contact.set_normal(mem, fields.normal);
    1
}
