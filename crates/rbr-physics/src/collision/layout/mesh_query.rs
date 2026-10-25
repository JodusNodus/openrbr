//! The collision mesh's memory: the mesh queries are `collision::mesh`, generic over `CollisionStore`; here are the
//! views and small copies they and the other layout files use.

use crate::math::vec::Vec3;
use crate::mem::Mem;

use super::vec_ops::vec4_copy_from;
use crate::collision::contact_solver::MaterialPair;
use crate::collision::overlap::SatBox;
use crate::collision::triangle::MeshHit;

pub use crate::collision::mesh::{
    box_sat_overlap, face_query, mesh_ray_query, mesh_ray_worklist, mesh_test, node_walk, point_triangle, ray_clear,
    ray_seeds, ray_triangle, seg_test, seg_test170, segment_sweep, segment_test, store_point_triangle,
    store_ray_triangle_result, store_segment_triangle, system_frame, system_vf00, world_axes_overlap, SystemFrame,
};

/// Loads a separating-axis box: the centre at +0 and the half extents at +12.
pub fn load_sat_box(mem: &Mem, at: u32) -> SatBox {
    SatBox { center: Vec3::load(mem, at), half_extent: Vec3::load(mem, at + 12) }
}

/// Loads the two tags and the weight that E38_004FB720 picks from. The table is at +0xac, or at +0x80 when bit 0x800 of
/// the flags word at +0xe4 is set.
pub fn load_material_pair(mem: &Mem, object: u32) -> MaterialPair {
    let table = if mem.u32(object + 0xe4) & 0x800 == 0 { 0xac } else { 0x80 };
    MaterialPair {
        low: mem.u32(object + table),
        high: mem.u32(object + table + 4),
        weight: mem.f32(object + table + 8),
    }
}

/// The material manager that `Tag520940` is called on: the application's +0x24 pointer, plus 0x18.
pub fn material_manager(mem: &Mem) -> u32 {
    let application = mem.u32(0x0165_9b70);
    mem.u32(application + 0x24) + 0x18
}

/// The vertex base of a collision mesh (+0xa0). Vertex `i` is four floats at `base + 16 * i`.
pub fn mesh_vertices(mem: &Mem, object: u32) -> u32 {
    mem.u32(object + 0xa0)
}

/// Mesh tag `index` (+0xb0 + 4 * index).
pub fn mesh_tag(mem: &Mem, object: u32, index: usize) -> u32 {
    mem.u32(object + 0xb0 + index as u32 * 4)
}

/// Stores the four tags of a mesh hit at `at`.
pub fn store_mesh_hit_tags(mem: &mut Mem, at: u32, hit: &MeshHit) {
    for (index, tag) in hit.tags.iter().enumerate() {
        mem.set_u32(at + index as u32 * 4, *tag);
    }
}

/// Zeroes the 0x804-byte collision mesh scratch block at `object` (E38_004FAD10, 0x4fad10).
pub fn store_zero_mesh_scratch(mem: &mut Mem, object: u32) {
    for offset in (0..0x804).step_by(4) {
        mem.set_u32(object + offset, 0);
    }
}

/// CCollisionMeshOpen::CopyBounds (0x4fb070), thiscall on the mesh with `(a, b)`: its lower bound (+0x70) is copied to
/// `a` and its upper bound (+0x80) to `b`, each by `vec4_copy_from`. Returns the second copy's eax, `b`.
pub fn mesh_copy_bounds(mem: &mut Mem, mesh: u32, a: u32, b: u32) -> u32 {
    vec4_copy_from(mem, a, mesh + 0x70);
    vec4_copy_from(mem, b, mesh + 0x80)
}
