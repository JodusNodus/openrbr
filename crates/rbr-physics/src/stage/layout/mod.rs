//! The stage world on game memory: one sequence per original function, in the load's order (`super` describes it).

use crate::mem::Mem;

mod fence;
mod files;
mod meshes;
mod placement;

pub use crate::car_body::layout::DRIVE_LINE;
pub use fence::load_fence;
pub use files::{
    build_cell_grid, build_material_table, construct_wall, load_col, load_drive_line, load_materials, load_trk,
    set_wall, StageHost,
};
pub use meshes::{add_meshes, attach_water, clear_meshes, construct_open, construct_water, load_collision_meshes};
pub use placement::place;
pub(crate) use placement::FRAME_HOOKS;

/// The collision system's pointer (`g_7c755c`) and its fields: the `.col` image, the tree root, the 1024 vertex
/// slots and the slot cursor (`M_D630`'s).
pub const COLLISION_SYSTEM: u32 = 0x007C_755C;

/// The collision system a `mov ecx, [g_7c755c]` reads through its operand at `operand`: `g_7c755c` in 1.02, the slot
/// of NGP's variable its patch writes there under NGP (`ngp::layout::stage::collision_copies`). Without the exe's code in the
/// image, `g_7c755c`.
pub fn collision_at_operand(mem: &Mem, operand: u32) -> u32 {
    let mut word = [0; 4];
    let at = if mem.try_read_into(operand, &mut word) { u32::from_le_bytes(word) } else { COLLISION_SYSTEM };
    mem.u32(at)
}
const IMAGE: u32 = 0x4;
const TREE: u32 = 0x8;
const SLOTS: u32 = 0xC;
const SLOT_COUNT: u32 = 0x400;
const CURSOR: u32 = 0x1010;
/// The collision system's size, its vtable, and its two connections (+0x1820 and +0x1838: a vtable, three link words,
/// the owner and the callback).
pub const SYSTEM_SIZE: u32 = 0x1850;
const SYSTEM_VTABLE: u32 = 0x0073_99f8;
const SYSTEM_CONNECTIONS: [(u32, u32, u32); 2] =
    [(0x1820, 0x0073_99e0, 0x004f_dff0), (0x1838, 0x0073_99ec, 0x004f_e230)];

/// `E39_CollSysCtor` (0x5001c0, fastcall; the static initializer 0x723b10 runs it on the system `COLLISION_SYSTEM`
/// points at): the vtable, the image and tree words and the word after the slot cursor cleared, and the two connections, unlinked, owned
/// by the system. Returns the system.
pub fn collision_system_ctor(mem: &mut Mem, this: u32) -> u32 {
    mem.set_u32(this, SYSTEM_VTABLE);
    for at in [IMAGE, TREE, CURSOR + 4] {
        mem.set_u32(this + at, 0);
    }
    for (at, vtable, callback) in SYSTEM_CONNECTIONS {
        mem.set_u32(this + at, vtable);
        for link in 1..4 {
            mem.set_u32(this + at + 4 * link, 0);
        }
        mem.set_u32(this + at + 0x10, this);
        mem.set_u32(this + at + 0x14, callback);
    }
    this
}

/// The brake-wall manager's wall (`+4`; the manager is `physics_core::layout::wall_manager`).
const WALL: u32 = 0x4;
/// The water meshes (`g_7c7464`): 0x200 slots and the count at `+0x800`.
pub const MESH_ARRAY: u32 = 0x0165_7030;
const MESH_SLOTS: u32 = 0x200;
const MESH_COUNT: u32 = 0x800;
/// The E44 material table (`ecx` of `E44MatTable::Pick`): 256 entries of 8 bytes, the base at `+0x800` and the count at
/// `+0x804`.
pub const MATERIAL_TABLE: u32 = 0x0165_92B8;

/// The wall (`E34WallVf`, 0x21c bytes, vtable 0x737e04): the root node's copy at `+0x64` (six words), the point
/// count, the mesh fields and the point block.
const WALL_SIZE: u32 = 0x21C;
const WALL_VTABLE: u32 = 0x0073_7E04;
const WALL_ROOT: u32 = 0x64;
const WALL_MESH_COUNT: u32 = 0x20C;
const WALL_POINTS: u32 = 0x210;
const WALL_MESH: u32 = 0x214;
const WALL_POINT_BLOCK: u32 = 0x218;
