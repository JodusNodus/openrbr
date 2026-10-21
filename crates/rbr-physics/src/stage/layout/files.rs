//! The stage's files on memory: the `.col` image, the brake wall, the cell grid, the `.mat` materials, the `.trk`
//! physics and the drive line.

use rbr_formats::{col, trk};

use super::meshes::{add_meshes, clear_meshes, load_collision_meshes, reg_wall, WALL_FILING};
use super::placement::Calls;
use super::{
    COLLISION_SYSTEM, CURSOR, DRIVE_LINE, IMAGE, MATERIAL_TABLE, MESH_ARRAY, SLOTS, SLOT_COUNT, TREE, WALL, WALL_MESH,
    WALL_MESH_COUNT, WALL_POINTS, WALL_POINT_BLOCK, WALL_ROOT, WALL_SIZE, WALL_VTABLE,
};
use crate::collision::layout::grid_hosts::AllocHost;
use crate::collision::layout::wall_tree as wall;
use crate::engine_driveline::store::EngineStore;
use crate::entities::frame::{Form, Hook, Owner};
use crate::entities::layout::{construct_entity, store_material_table, EntityArgs};
use crate::entities::material_table::{MaterialEntry, MaterialTable};
use crate::mem::Mem;
use crate::physics_core::construct::Heap;
use crate::physics_core::detect;
use crate::physics_core::layout as physics;
use crate::stage::{self, MaterialMap, Stuff};

/// The track state's `.mat` image (`CPC_TrackState+0x148`).
const MAT_IMAGE: u32 = 0x148;

/// The calls the stage load makes outside this module. Allocations go to the heap (`Heap` for objects and arrays,
/// `AllocHost` for the cell grid's blocks).
pub trait StageHost: Heap + AllocHost {
    /// `[0x7c36a8]->Vf1C() == 2`: the wet-surface meshes are built (wet weather).
    fn wet_meshes(&mut self) -> bool;
    /// Slot 7 of `object` with 1: its deleting destructor.
    fn delete(&mut self, mem: &mut Mem, object: u32);
    /// The `Settings` key `SendShapeCollisionMeshes` (`N2_411670`, default 1): the `.trk` collision meshes are
    /// loaded.
    fn send_shape_collision_meshes(&mut self) -> bool;
    /// `E10Vec4::PushD` (0x46cf80): the loader daemon keeps each mesh name in its list (`+0x74`).
    fn keep_mesh_name(&mut self, name: &[u8]);
    /// Slot 0x78 of `[[0x7ea990] + 0x40]`: the handle of interactive object `id`, `None` where the original logs
    /// "Error getting interactive object handle" (the handle is then left 0).
    fn interactive_handle(&mut self, id: u32) -> Option<u32>;
    /// The `CSigStuff` emit (signal `+0x4730`) of a placement; its receiver is `CPhysics::E46_Notify67d0` (0x5267d0,
    /// [`place`]).
    fn stuff(&mut self, mem: &mut Mem, stuff: &Stuff);
    /// Free memory of `size` bytes for the original's temporaries: a placement's `CSigStuff` (a heap block the emitter
    /// deletes after the emit) and `CColGrid::SyncGrid`'s vertices (its stack).
    fn scratch(&mut self, mem: &mut Mem, size: u32) -> u32;
    /// The track state's mesh list (`+0x360`, a `std::vector` of descriptors): `push_back(mesh)` (`E10Vec4::InsertB`,
    /// 0x46be30, when full).
    fn push_mesh(&mut self, mem: &mut Mem, mesh: u32);
}

/// The stage objects are entities with no parent: their frames hang under the root, and the constructor asks no owner.
pub(super) struct NoParent;

impl Owner for NoParent {
    fn hook(&mut self, _: u32, _: Hook, _: Form, _: u32, _: u32) -> u32 {
        unreachable!("an entity with no parent calls no owner")
    }
    fn gate(&mut self, _: u32) -> u32 {
        unreachable!("an entity with no parent calls no owner")
    }
    fn frame_of(&mut self, _: u32) -> u32 {
        unreachable!("an entity with no parent calls no owner")
    }
    fn old_frame_of(&mut self, _: u32) -> u32 {
        unreachable!("an entity with no parent calls no owner")
    }
}

/// `E39_FlagC60` (0x4ffc60) and `E38Ops::M_E570` (0x4fe570) on the collision system: the file service's read of the
/// `.col` file (`file`, placed at `image` by the caller's heap) and the walk of its objects (`M_DFF0`, 0x4fdff0). The
/// root (type 4, mode 3) builds the brake wall its definition names, the cell grid, and the water meshes, then becomes
/// the tree root; each cell takes a vertex slot, has its tree relocated and is grafted onto the root's
/// (`col::relocate`, which runs before the root's calls here: they touch other bytes). Returns 1.
///
/// The cells' slots are `col::relocate`'s, which counts from 0. `M_D630` (0x4fd630) takes the next free slot from the
/// cursor, which is the same on a system that holds no cells (`E38_004FD5F0`, 0x4fd5f0, clears it between stages).
pub fn load_col(mem: &mut Mem, host: &mut impl StageHost, image: u32, file: &[u8]) -> Result<u32, col::Error> {
    let system = mem.u32(COLLISION_SYSTEM);
    assert!(
        mem.u32(system + CURSOR) == 0 && (0..SLOT_COUNT).all(|i| mem.u32(system + SLOTS + 4 * i) == 0),
        "the collision system holds no cells"
    );
    mem.set_u32(system + IMAGE, image);
    let mut bytes = file.to_vec();
    col::relocate(&mut bytes, image)?;
    mem.map(image, &bytes);
    for (slot, vertices) in stage::vertex_slots(&bytes) {
        mem.set_u32(system + SLOTS + 4 * slot, image + vertices);
        mem.set_u32(system + CURSOR, (slot + 1) & (SLOT_COUNT - 1));
    }
    let root = stage::col_root(&bytes);
    if let Some(def) = root.wall_def {
        set_wall(mem, host, physics::wall_manager(mem), image + def);
    }
    build_cell_grid(mem, host);
    clear_meshes(mem, host, MESH_ARRAY);
    if root.wet.count != 0 && host.wet_meshes() {
        add_meshes(mem, host, MESH_ARRAY, root.wet.count, image + root.wet.at);
    }
    if root.water.count != 0 {
        add_meshes(mem, host, MESH_ARRAY, root.water.count, image + root.water.at);
    }
    mem.set_u32(system + TREE, image + root.tree);
    Ok(1)
}

/// `CBrakeWallManager::SetWall` (0x4e4400) on `manager` with the definition at `def`: when the definition has points,
/// the old wall is deleted and a new one built, kept and filed with the physics state.
pub fn set_wall(mem: &mut Mem, host: &mut impl StageHost, manager: u32, def: u32) {
    if mem.u32(def + 4) == 0 {
        return;
    }
    let old = mem.u32(manager + WALL);
    if old != 0 {
        host.delete(mem, old);
        mem.set_u32(manager + WALL, 0);
    }
    let at = host.alloc(mem, WALL_SIZE);
    if at != 0 {
        construct_wall(mem, at, def);
    }
    mem.set_u32(manager + WALL, at);
    reg_wall(mem, at, WALL_FILING);
}

/// `E34WallVf::E34WallVf(def)` (0x4e4220) at `at`: the entity (options 0x40), the point count and block, and the root
/// node of the definition's tree, after `CBrakeWall::AddCount` (0x4e3810) has made the tree's pointers absolute.
pub fn construct_wall(mem: &mut Mem, at: u32, def: u32) {
    let args = EntityArgs { definition: 0, sort_key: 0, parent: 0, options: 0x40 };
    construct_entity(mem, at, &args, &mut NoParent);
    mem.set_u32(at, WALL_VTABLE);
    mem.set_u32(at + WALL_ROOT + 0x10, 0);
    mem.set_u32(at + WALL_ROOT + 0x14, 0);
    mem.set_u32(at + WALL_MESH_COUNT, 0);
    mem.set_u32(at + WALL_POINTS, mem.u32(def + 4));
    mem.set_u32(at + WALL_POINT_BLOCK, def.wrapping_add(mem.u32(def + 8)));
    let node = def.wrapping_add(mem.u32(def + 0xc));
    wall::add_count(mem, at, node, node);
    for i in 0..6 {
        mem.set_u32(at + WALL_ROOT + 4 * i, mem.u32(node + 4 * i));
    }
    mem.set_u32(at + WALL_MESH_COUNT, 0);
    mem.set_u32(at + WALL_MESH, 0);
}

/// `E45_MD60` (0x523d60, `physics_core::detect::build_cell_grid`): with no cell grid yet, a new one over the brake
/// wall's box, or over the default area when there is no wall.
pub fn build_cell_grid(mem: &mut Mem, host: &mut impl StageHost) {
    detect::build_cell_grid(mem, &mut Calls(host));
}

/// `CPC_TrackLoaderDaemon::LoadCollisionFile` (0x46a460) and `E09_LoadMaterials` (0x46a290) on a `.mat` file placed at
/// `image` (which the track state keeps at `+0x148`), then `E44MatTable::Build` (0x520190), which `CSigMaterialMaps`
/// reaches, on the material table: entry `i` takes run `i`'s width byte and its cells. Nothing is built when no entry
/// names both words.
pub fn load_materials(mem: &mut Mem, track_state: u32, image: u32, file: &[u8], wear: &[u8], wetness: &[u8]) {
    mem.set_u32(track_state + MAT_IMAGE, image);
    mem.map(image, file);
    if file.len() <= 4 {
        return;
    }
    if let Some(maps) = stage::select_maps(file, wear, wetness) {
        build_material_table(mem, MATERIAL_TABLE, image, &maps);
    }
}

/// `E44MatTable::Build` (0x520190) at `table`, as `CSigMaterialMaps` hands it the maps of the image at `image`: entry
/// `i` is run `i`'s.
pub fn build_material_table(mem: &mut Mem, table: u32, image: u32, maps: &[MaterialMap]) {
    let entries: Vec<_> = maps
        .iter()
        .enumerate()
        .map(|(i, map)| MaterialEntry { tag: i as u8, low: map.width, high: image + map.cells })
        .collect();
    store_material_table(mem, table, &MaterialTable::build(table, &entries));
}

/// `CPC_TrackState::LoadPhysics` (0x46ed20) on the parsed `.trk`: the drive line (kind 0x14) and the collision meshes
/// (kind 0x16) in file order. Its script and fence files are other loaders (the fence is [`load_materials`]).
pub fn load_trk(mem: &mut Mem, host: &mut impl StageHost, file: &trk::TrkFile) {
    for chunk in &file.chunks {
        match &chunk.payload {
            trk::ChunkPayload::DriveLine(data) => load_drive_line(mem, data),
            trk::ChunkPayload::CollisionMeshes(data) => load_collision_meshes(mem, host, data),
            trk::ChunkPayload::Opaque => {}
        }
    }
}

/// `E09_LoadDriveLine` (0x46a530) and its `CSigDriveLine`'s receiver, `CDriveLine::SetDriveLine` (0x50aec0), on the
/// drive line. The original's array of entries is freed after the emit.
pub fn load_drive_line(mem: &mut Mem, data: &trk::DriveLineData) {
    let this = mem.u32(DRIVE_LINE);
    mem.store_drive_line(this, &stage::drive_line_entries(data));
}
