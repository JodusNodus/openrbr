//! The fences: `CFenceHandler::Read` (0x6038c0), the `E90` pools and groups and the `CDummyObject` posts.

use rbr_formats::fnc;

use super::files::NoParent;
use crate::collision::grid::AllocHost;
use crate::collision::layout::{lookup_twin, move_to_cell};
use crate::entities::layout::{construct_entity, EntityArgs};
use crate::mem::Mem;
use crate::physics_core::layout as physics;

/// The fence handler (`g_fenceHandler`, `NStickSystem::CFenceHandler`) and its group pool (`+0x720`); a pool
/// (`E90Pool`, 0x10 bytes) is count, capacity, the external-storage flag (a byte) and the data.
const FENCE_HANDLER: u32 = 0x0167_EA90;
const FENCE_GROUPS: u32 = 0x720;
const POOL_COUNT: u32 = 0x0;
const POOL_CAP: u32 = 0x4;
const POOL_EXT: u32 = 0x8;
const POOL_DATA: u32 = 0xC;
/// `E90Group` (0x50): its dummy pool (`+0`), flags, bounds (centre, half extents), its fence and post types, its two
/// materials, the spans' block and their pool (`+0x40`).
const GROUP_SIZE: u32 = 0x50;
const GROUP_FLAGS: u32 = 0x10;
const GROUP_CENTER: u32 = 0x14;
const GROUP_HALF: u32 = 0x20;
const GROUP_TYPE: u32 = 0x2C;
const GROUP_POST_TYPE: u32 = 0x30;
const GROUP_MATERIAL: u32 = 0x34;
const GROUP_RAIL_MATERIAL: u32 = 0x38;
const GROUP_MEM: u32 = 0x3C;
const GROUP_SEGS: u32 = 0x40;
/// `NStickSystem::CDummyObject` (0xb4, vtable 0x74f64c): a `CEntity`, its neighbours (`+0x64` next, `+0x68` previous),
/// position, flags (4), span and post segments, cell link (`+0x84`, its item `+0x90`), box centre and half extents (x,
/// z, y as the file has them), colour word, group.
const DUMMY_SIZE: u32 = 0xB4;
const DUMMY_VTABLE: u32 = 0x0074_F64C;
const DUMMY_NEXT: u32 = 0x64;
const DUMMY_PREV: u32 = 0x68;
const DUMMY_POS: u32 = 0x6C;
const DUMMY_FLAGS: u32 = 0x78;
const DUMMY_SPAN: u32 = 0x7C;
const DUMMY_POST: u32 = 0x80;
const DUMMY_LINK: u32 = 0x84;
const DUMMY_LINK_ITEM: u32 = 0x90;
const DUMMY_CENTER: u32 = 0x94;
const DUMMY_HALF: u32 = 0xA0;
const DUMMY_COLOR: u32 = 0xAC;
const DUMMY_GROUP: u32 = 0xB0;

/// `CFenceHandler::Read` (0x6038c0) on the stage's `.fnc` (`file`): with the cell grid made, the group pool takes one
/// group per file group, each its dummy pool and one dummy per post (linked to the previous one, `m_group` its
/// group); then every dummy is filed in its grid cell (`CDummyObject::Relink`). The renderer's calls
/// (`CFenceRenderer::LoadTextures`, `UpdateLights`) are not the physics'.
pub fn load_fence(mem: &mut Mem, host: &mut impl AllocHost, file: &fnc::FncFile) {
    if physics::cell_grid(mem) == 0 || file.groups.is_empty() {
        return;
    }
    let pool = FENCE_HANDLER + FENCE_GROUPS;
    let groups = new_pool(mem, host, pool, file.groups.len() as u32, GROUP_SIZE, construct_group);
    for (i, g) in file.groups.iter().enumerate() {
        let group = groups + GROUP_SIZE * i as u32;
        mem.set_u32(pool + POOL_COUNT, i as u32 + 1);
        let dummies = new_pool(mem, host, group, g.posts.len() as u32, DUMMY_SIZE, construct_dummy);
        mem.set_u32(group + GROUP_TYPE, g.fence_type);
        mem.set_u32(group + GROUP_POST_TYPE, g.post_type);
        mem.set_u32(group + GROUP_MATERIAL, g.material);
        mem.set_u32(group + GROUP_RAIL_MATERIAL, g.rail_material);
        store_f32s(mem, group + GROUP_CENTER, &g.center);
        store_f32s(mem, group + GROUP_HALF, &g.half_extent);
        for (j, post) in g.posts.iter().enumerate() {
            let dummy = dummies + DUMMY_SIZE * j as u32;
            mem.set_u32(group + POOL_COUNT, j as u32 + 1);
            if j != 0 {
                let prev = dummy - DUMMY_SIZE;
                mem.set_u32(prev + DUMMY_NEXT, dummy);
                mem.set_u32(dummy + DUMMY_PREV, prev);
            }
            store_f32s(mem, dummy + DUMMY_POS, &post.position);
            store_f32s(mem, dummy + DUMMY_CENTER, &post.center);
            store_f32s(mem, dummy + DUMMY_HALF, &post.half_extent);
            mem.set_u32(dummy + DUMMY_COLOR, post.color);
            mem.set_u32(dummy + DUMMY_GROUP, group);
        }
    }
    for (i, g) in file.groups.iter().enumerate() {
        let dummies = mem.u32(groups + GROUP_SIZE * i as u32 + POOL_DATA);
        for j in 0..g.posts.len() as u32 {
            relink_dummy(mem, host, dummies + DUMMY_SIZE * j);
        }
    }
}

/// `E90GroupPool::Init(n, 0)` (0x603350) and `E90DummyPool::Init(n, 0)` (0x603760) on the empty pool at `pool`: its
/// capacity `n` and its data `new T[n]` (the count, then `n` elements of `size` bytes, each constructed by `ctor`).
/// Returns the data.
fn new_pool(mem: &mut Mem, host: &mut impl AllocHost, pool: u32, n: u32, size: u32, ctor: fn(&mut Mem, u32)) -> u32 {
    mem.set_u32(pool + POOL_COUNT, 0);
    mem.set_u32(pool + POOL_CAP, n);
    mem.set_u8(pool + POOL_EXT, 0);
    let block = host.allocate(n * size + 4);
    mem.set_u32(block, n);
    let data = block + 4;
    for k in 0..n {
        ctor(mem, data + size * k);
    }
    mem.set_u32(pool + POOL_DATA, data);
    data
}

/// An empty `E90Pool` at `at`.
fn construct_pool(mem: &mut Mem, at: u32) {
    mem.set_u32(at + POOL_COUNT, 0);
    mem.set_u32(at + POOL_CAP, 0);
    mem.set_u8(at + POOL_EXT, 0);
    mem.set_u32(at + POOL_DATA, 0);
}

/// `E90Group::E90Group` (0x601370): its two pools empty, no flags, no span block.
fn construct_group(mem: &mut Mem, at: u32) {
    construct_pool(mem, at);
    mem.set_u32(at + GROUP_FLAGS, 0);
    mem.set_u32(at + GROUP_MEM, 0);
    construct_pool(mem, at + GROUP_SEGS);
}

/// `CDummyObject::CDummyObject` (0x600c30): a root `CEntity(0, 0, 0, 0)`, no neighbours or segments, flags 4, its
/// cell link empty with itself as the item.
fn construct_dummy(mem: &mut Mem, at: u32) {
    let args = EntityArgs { definition: 0, sort_key: 0, parent: 0, options: 0 };
    construct_entity(mem, at, &args, &mut NoParent);
    mem.set_u32(at, DUMMY_VTABLE);
    for off in [DUMMY_NEXT, DUMMY_PREV, DUMMY_SPAN, DUMMY_POST] {
        mem.set_u32(at + off, 0);
    }
    for off in [0, 4, 8] {
        mem.set_u32(at + DUMMY_LINK + off, 0);
    }
    mem.set_u32(at + DUMMY_FLAGS, 4);
    mem.set_u32(at + DUMMY_LINK_ITEM, at);
}

/// `CDummyObject::Relink` (0x600bb0): the dummy's link to the head of the grid cell under its position (x, z).
fn relink_dummy(mem: &mut Mem, host: &mut impl AllocHost, dummy: u32) {
    let (x, z) = (mem.f32(dummy + DUMMY_POS), mem.f32(dummy + DUMMY_POS + 8));
    let cell = lookup_twin(host, mem, physics::cell_grid(mem), x, z);
    move_to_cell(mem, dummy + DUMMY_LINK, cell);
}

fn store_f32s(mem: &mut Mem, at: u32, v: &[f32; 3]) {
    for (k, &x) in v.iter().enumerate() {
        mem.set_f32(at + 4 * k as u32, x);
    }
}
