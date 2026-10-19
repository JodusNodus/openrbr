//! The collision meshes: the water mesh array, `CCollisionMeshOpen` and `CCollisionMeshWater`, and
//! `LoadCollisionMeshes` with its placement and shape records.

use rbr_formats::trk;

use super::files::{NoParent, StageHost};
use super::{MESH_COUNT, MESH_SLOTS};
use crate::collision::layout::{alloc_collision_id, collision_mesh_factory, lookup_twin, move_to_cell, unlink, Vec4};
use crate::entities::layout::{construct_entity, EntityArgs};
use crate::mem::Mem;
use crate::physics_core::signal::Filing;
use crate::physics_core::{layout as physics, setup};
use crate::rigid_body::layout::HALF;
use crate::stage::{self, Stuff, MESH_RECORD};

/// `CCollisionMeshOpen` (vtable 0x7395e4) and `CCollisionMeshWater` (0xc0 bytes, vtable 0x73974c): the bounds, the
/// cell link (next, previous, owner), the self word, the record and the four words after it.
const WATER_SIZE: u32 = 0xC0;
const OPEN_VTABLE: u32 = 0x0073_95E4;
const WATER_VTABLE: u32 = 0x0073_974C;
const LOW: u32 = 0x70;
const HIGH: u32 = 0x80;
const LINK: u32 = 0x90;
const SELF: u32 = 0x9C;
const RECORD: u32 = 0xA0;
const LEVEL: u32 = 0xB0;

/// An entity's parent (`CEntity+0x44`).
const PARENT: u32 = 0x44;

/// A collision-mesh descriptor (`CSigCollisionMesh`): the id, flags, name, vertex and face counts and arrays, the
/// shape record and the material byte.
const DESC_SIZE: u32 = 0x34;
const DESC_FLAGS: u32 = 0x4;
const DESC_NAME: u32 = 0x8;
const DESC_VERTEX_COUNT: u32 = 0xC;
const DESC_FACE_COUNT: u32 = 0xE;
const DESC_VERTICES: u32 = 0x10;
const DESC_FACES: u32 = 0x14;
const DESC_SHAPE: u32 = 0x2C;
const DESC_MATERIAL: u32 = 0x30;

/// The exe's 1.0 (`CtorWater`'s `+0xb8`) and the x87 half `Vf15CW` multiplies by.
const ONE: u32 = 0x0079_5E14;

/// `E38_004FAFA0` (0x4fafa0) on the mesh array: each mesh leaves its physics-state list and is deleted; the count
/// becomes 0.
pub fn clear_meshes(mem: &mut Mem, host: &mut impl StageHost, array: u32) {
    for i in 0..MESH_SLOTS {
        let mesh = mem.u32(array + 4 * i);
        if mesh == 0 {
            continue;
        }
        let list = mem.u32(mesh + 0x18);
        if list != 0 {
            mem.set_u32(list + 8, mem.u32(list + 8).wrapping_sub(1));
            unlink(mem, mesh + 0x10);
        }
        let mesh = mem.u32(array + 4 * i);
        if mesh != 0 {
            host.delete(mem, mesh);
        }
        mem.set_u32(array + 4 * i, 0);
    }
    mem.set_u32(array + MESH_COUNT, 0);
}

/// `E38Ops::M_B220` (0x4fb220) on the mesh array: a water mesh for each of the `n` records at `records`, while the
/// array has room, each filed (its slot 87, `Vf15CW`) as it is added.
pub fn add_meshes(mem: &mut Mem, host: &mut impl StageHost, array: u32, n: u32, records: u32) {
    for i in 0..n {
        let count = mem.u32(array + MESH_COUNT);
        if count >= MESH_SLOTS {
            return;
        }
        let at = host.alloc(mem, WATER_SIZE);
        if at != 0 {
            construct_water(mem, at, records + i * MESH_RECORD);
        }
        mem.set_u32(array + 4 * count, at);
        attach_water(mem, host, mem.u32(array + 4 * count));
        mem.set_u32(array + MESH_COUNT, count + 1);
    }
}

/// `CCollisionMeshOpen::CtorOpen(flags)` (0x4fb010) at `at`: the entity (options `flags | 0x100`), a cleared link
/// and bounds, and the self word.
pub fn construct_open(mem: &mut Mem, at: u32, flags: u32) {
    let args = EntityArgs { definition: 0, sort_key: 0, parent: 0, options: flags | 0x100 };
    construct_entity(mem, at, &args, &mut NoParent);
    mem.set_u32(at, OPEN_VTABLE);
    mem.set_u32(at + LINK + 4, 0);
    mem.set_u32(at + LINK, 0);
    mem.set_u32(at + LINK + 8, 0);
    mem.set_u32(at + SELF, at);
    Vec4::default().store(mem, at + LOW);
    Vec4::default().store(mem, at + HIGH);
}

/// `CCollisionMeshWater::CtorWater(record)` (0x4fb110) at `at`: the open mesh (flags 0x200), the record, the corners'
/// bounds and the four words after the record (`+0xb8` is the exe's 1.0).
pub fn construct_water(mem: &mut Mem, at: u32, record: u32) {
    construct_open(mem, at, 0x200);
    mem.set_u32(at, WATER_VTABLE);
    mem.set_u32(at + RECORD, record);
    let corners = std::array::from_fn(|i| Vec4::load(mem, record + 0x10 * i as u32));
    let (low, high, level) = stage::water_box(corners);
    low.store(mem, at + LOW);
    high.store(mem, at + HIGH);
    mem.set_f32(at + LEVEL, 0.0);
    mem.set_f32(at + LEVEL + 4, 0.0);
    mem.set_u32(at + LEVEL + 8, mem.u32(ONE));
    mem.set_f32(at + LEVEL + 0xc, level);
}

/// `CCollisionMeshWater::Vf15CW` (0x4faf00) on `water`: its link goes to the head of the grid cell under its centre
/// (`CCellGrid::LookupTwin`, 0x4e03b0, leaving its old cell first), then it is filed with the physics state, and
/// `CEntity::E43Vf94` (0x519a80) runs.
pub fn attach_water(mem: &mut Mem, host: &mut impl StageHost, water: u32) {
    let (x, y) = stage::water_centre(Vec4::load(mem, water + LOW), Vec4::load(mem, water + HIGH), mem.f32(HALF));
    let cell = lookup_twin(host, mem, physics::cell_grid(mem), x, y);
    move_to_cell(mem, water + LINK, cell);
    reg_wall(mem, water, WATER_FILING);
    if mem.u32(water + PARENT) == 0 {
        let physics = physics::physics(mem);
        setup::wall_rel(mem, &mut physics::Filer(WATER_FILING), physics, water);
    }
}

/// How `CState::RegWall` (0x5263d0) files the wall and the waters: the wall's slot 0x88 is `CBrakeWall::Vf88`
/// (0x4e38e0, 1); the rest of the wall's and the water's are `CEntity`'s (0x519a50, 0x519a20, 0x519a40: 0).
pub(super) const WALL_FILING: Filing = (false, false, true);
const WATER_FILING: Filing = (false, false, false);

/// `CPhysics::RegWall` (0x526470) on `[0x1659b70]`: `CState::RegWall` on its state.
pub(super) fn reg_wall(mem: &mut Mem, entity: u32, (s8c, s80, s88): Filing) {
    let state = physics::state_of(mem, physics::physics(mem));
    physics::push_item(mem, physics::entity_list(state, s8c, s80, s88), entity);
}

/// `CPC_TrackLoaderDaemon::LoadCollisionMeshes` (0x46e300): per mesh a zeroed descriptor with the mesh's name (a copy,
/// `_strdup`), material byte, shape record (a box, 0x50 bytes, or a sphere, 0x20), vertices (16 bytes apart, `w`
/// unset) and faces (the tag in byte 3, each index word's low byte in bytes 0-2, bytes 4-6 unset). A static mesh
/// takes the next free collision id, its grid is built (`CSigCollisionMesh`, whose receiver is
/// `E37SigFactory720`) and its placements are emitted as they are read; a dynamic mesh's placements name its
/// descriptor and are emitted after the last mesh with their values `(0, 0, 0, 1)`. Each descriptor goes on the
/// track state's list.
pub fn load_collision_meshes(mem: &mut Mem, host: &mut impl StageHost, data: &trk::CollisionMeshData) {
    if !host.send_shape_collision_meshes() {
        return;
    }
    let mut id = alloc_collision_id(mem);
    let mut dynamic = Vec::new();
    for mesh in &data.meshes {
        let desc = host.alloc(mem, DESC_SIZE);
        mem.map(desc, &[0; DESC_SIZE as usize]);
        host.keep_mesh_name(&mesh.name);
        let name = host.alloc(mem, mesh.name.len() as u32 + 1);
        mem.map(name, &[&mesh.name[..], &[0]].concat());
        mem.set_u32(desc + DESC_NAME, name);
        mem.set_u8(desc + DESC_MATERIAL, mesh.unknown_byte);
        if mesh.is_static() {
            mem.set_u32(desc, id);
        }
        store_shape(mem, host, desc, &mesh.shape);
        let vertex_count = mesh.vertex_count_word as u16;
        mem.map(desc + DESC_VERTEX_COUNT, &vertex_count.to_le_bytes());
        let vertices = host.alloc(mem, u32::from(vertex_count) << 4);
        mem.set_u32(desc + DESC_VERTICES, vertices);
        for (at, v) in (vertices..).step_by(0x10).zip(&mesh.vertices) {
            store_xyz(mem, at, v);
        }
        let face_count = mesh.face_count_word as u16;
        mem.map(desc + DESC_FACE_COUNT, &face_count.to_le_bytes());
        let faces = host.alloc(mem, u32::from(face_count) * 7);
        mem.set_u32(desc + DESC_FACES, faces);
        for (at, f) in (faces..).step_by(7).zip(&mesh.faces) {
            mem.set_u8(at + 3, f.tag);
            for (k, &index) in (0..).zip(&f.indices) {
                mem.set_u8(at + k, index as u8);
            }
        }
        if mesh.is_static() {
            collision_mesh_factory(mem, desc);
        }
        let collision = if mesh.is_static() { id } else { desc };
        for object in &mesh.objects {
            let mut stuff = stage::stuff(mesh, object, collision);
            if stage::wants_handle(mesh) {
                stuff.handle = host.interactive_handle(object.interactive_id).unwrap_or(0);
            }
            if mesh.is_static() {
                host.stuff(mem, &stuff);
            } else {
                dynamic.push(stuff);
            }
        }
        if mesh.is_static() {
            id = alloc_collision_id(mem);
        }
        host.push_mesh(mem, desc);
    }
    for mut stuff in dynamic {
        stuff.values = [0.0, 0.0, 0.0, mem.f32(ONE)];
        host.stuff(mem, &stuff);
    }
}

/// A placement (`CSigStuff`, 0x50 bytes, zeroed when made): the position, the values (the orientation), the extent, the
/// collision id or descriptor, the object type, the handle and the flags. `E46_Notify67d0` keeps the object at `+0x3c`.
pub(super) const STUFF_SIZE: u32 = 0x50;

pub(super) fn store_stuff(mem: &mut Mem, at: u32, stuff: &Stuff) {
    mem.map(at, &[0; STUFF_SIZE as usize]);
    stuff.position.store(mem, at);
    for (i, value) in (0..).zip(stuff.values) {
        mem.set_f32(at + 0x10 + 4 * i, value);
    }
    stuff.extent.store(mem, at + 0x20);
    mem.set_u32(at + 0x34, stuff.collision);
    mem.set_u32(at + 0x38, stuff.object_type);
    mem.set_u32(at + 0x40, stuff.handle);
    mem.set_u32(at + 0x44, stuff.flags);
}

/// `LoadCollisionMeshes`'s shape record: a box (`min` at +0, `max` at +0x10, then the identity rows the exe's 1.0
/// gives, flag 0x20000) or a sphere (centre at +0, radius at +0x10, flag 0x10000). The words between are unset.
fn store_shape(mem: &mut Mem, host: &mut impl StageHost, desc: u32, shape: &trk::CollisionShape) {
    let (size, flag) = match shape {
        trk::CollisionShape::Bounds { .. } => (0x50, 0x20000),
        trk::CollisionShape::Sphere { .. } => (0x20, 0x10000),
        trk::CollisionShape::None | trk::CollisionShape::Unknown { .. } => return,
    };
    let record = host.alloc(mem, size);
    match shape {
        trk::CollisionShape::Bounds { min, max } => {
            store_xyz(mem, record, min);
            store_xyz(mem, record + 0x10, max);
            let one = mem.u32(ONE);
            for (word, value) in [(8, one), (9, 0), (10, 0), (12, 0), (13, 0), (14, one), (16, 0), (17, one), (18, 0)] {
                mem.set_u32(record + 4 * word, value);
            }
        }
        trk::CollisionShape::Sphere { center, radius } => {
            store_xyz(mem, record, center);
            mem.set_f32(record + 0x10, *radius);
        }
        _ => unreachable!(),
    }
    mem.set_u32(desc + DESC_SHAPE, record);
    mem.set_u32(desc + DESC_FLAGS, mem.u32(desc + DESC_FLAGS) | flag);
}

/// A loader vector's three floats at `at`; the word after is not touched.
fn store_xyz(mem: &mut Mem, at: u32, v: &trk::Vec3) {
    mem.set_f32(at, v.x);
    mem.set_f32(at + 4, v.y);
    mem.set_f32(at + 8, v.z);
}
