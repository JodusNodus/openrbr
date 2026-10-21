//! The collision mesh queries: ray seeds, the separating-axis tests, the node walk, the segment and ray tests,
//! `CCollisionSystem::Vf00` and the face query.

use crate::math::vec::Vec3;
use crate::mem::Mem;

use super::vec_ops::{vec3_dot, vec3_normalize, vec4_copy_from};
use crate::collision::e38::{
    self, channel_sum, nibble_sum, CellQueryHost, MaterialPair, MeshHit, PointTriangle, RayHost, RayTriangleResult,
    SampleHost, SatBox, CHANNEL_SCALE, NIBBLE_SCALE,
};
use crate::collision::elem::SegmentTriangle;

/// E38RaySeeds (0x4f6550): clears the ray object's seed bytes and words, then its two vectors (host calls). Returns
/// `object`.
pub fn ray_seeds(host: &mut impl RayHost, mem: &mut Mem, object: u32) -> u32 {
    mem.set_u8(object + 0x688, 0);
    mem.set_u8(object + 0x689, 0);
    mem.set_u32(object + 0x680, 0);
    mem.set_u32(object + 0x400, 0);
    mem.set_u32(object + 0x408, 0);
    mem.set_u32(object + 0x40c, 0);
    mem.set_u32(object + 0x410, 0);
    mem.set_u8(object + 0x68b, 0);
    mem.set_u8(object + 0x68c, 0);
    mem.set_u8(object + 0x68d, 0);
    mem.set_u8(object + 0x68e, 0);
    host.vec4_clear(object);
    host.vec4_clear(object + 0x10);
    object
}

/// E38RayClear (0x4f6740): clears the ray object's per-test fields. The original leaves eax as the caller had it.
pub fn ray_clear(mem: &mut Mem, object: u32) {
    mem.set_u32(object + 0x400, 0);
    mem.set_u32(object + 0x408, 0);
    mem.set_u32(object + 0x40c, 0);
    mem.set_u8(object + 0x68b, 0);
    mem.set_u8(object + 0x68d, 0);
    mem.set_u32(object + 0x680, 0);
}

/// E38Ops::M_D230 (0x4fd230): true (1) when no world axis separates the box at `this` from the box given by
/// `other_center` and `other_extent`.
pub fn world_axes_overlap(mem: &Mem, this: u32, other_center: u32, other_extent: u32) -> u32 {
    u32::from(e38::world_axes_overlap(
        load_sat_box(mem, this),
        Vec3::load(mem, other_center),
        Vec3::load(mem, other_extent),
    ))
}

/// E38Ops::M_D2C0 (0x4fd2c0): true (1) when no world or cross axis separates the box at `this` from the other box.
pub fn box_sat_overlap(mem: &Mem, this: u32, other_center: u32, other_extent: u32) -> u32 {
    u32::from(e38::box_sat_overlap(
        load_sat_box(mem, this),
        Vec3::load(mem, other_center),
        Vec3::load(mem, other_extent),
    ))
}

/// E38 node walk (0x4fdd60, `FUN_004fdd60`), stdcall with three stack words: writes the leaves under `root` whose boxes
/// overlap the query box at `query` (centre, then half extents at +12) to `out`, after the count at +0. A leaf is a
/// node with faces (bits 0..20 of +0x18). A node without faces pushes its children (+0x1c, and the one 0x20 past it)
/// whose bit 21 at +0x18 (or +0x38) is clear and whose boxes overlap the query; the last pushed is walked first.
pub fn node_walk(mem: &mut Mem, query: u32, root: u32, out: u32) {
    let mut pending = if root == 0 { Vec::new() } else { vec![root] };
    let mut leaves = Vec::new();
    while let Some(node) = pending.pop() {
        if mem.u32(node + 0x18) & 0x1f_ffff != 0 {
            leaves.push(node);
            continue;
        }
        let child = mem.u32(node + 0x1c);
        if child == 0 {
            continue;
        }
        if mem.u32(child + 0x18) & 0x20_0000 == 0 && world_axes_overlap(mem, child, query, query + 12) != 0 {
            pending.push(child);
        }
        if mem.u32(child + 0x38) & 0x20_0000 == 0 && world_axes_overlap(mem, child + 0x20, query, query + 12) != 0 {
            pending.push(child + 0x20);
        }
    }
    mem.set_u32(out, leaves.len() as u32);
    for (index, leaf) in leaves.iter().enumerate() {
        mem.set_u32(out + 4 + 4 * index as u32, *leaf);
    }
}

/// E39_SegTest170 (0x4ff170, fastcall: `this` in ecx, stack words `a`, `b` and `out`, ret 0xc): the segment's ends
/// `a` and `b` (three floats each) go to the box at `tmp`, the original's frame below the return address, and
/// `node_walk` (0x4fdd60) runs on the object's tree (`this+8`) with it. The records go to `out`, or to the grid's own
/// list (`this+0x1014`) when `out` is 0. Returns 1 when the list has records, else 0.
pub fn seg_test170(mem: &mut Mem, this: u32, a: u32, b: u32, out: u32, tmp: u32) -> u32 {
    let lst = if out == 0 { this + 0x1014 } else { out };
    mem.set_u32(lst, 0);
    for k in 0..3 {
        mem.set_f32(tmp + 4 * k, mem.f32(a + 4 * k));
        mem.set_f32(tmp + 12 + 4 * k, mem.f32(b + 4 * k));
    }
    node_walk(mem, tmp, mem.u32(this + 8), lst);
    u32::from(mem.u32(lst) != 0)
}

/// The half-width of SegmentSweep's query box around the segment start (`0.1`, 0x72b068).
const SWEEP_MARGIN: f32 = 0.1;

/// E38Ops::M_AC50 (0x4fac50), thiscall on the object: the mesh test of `e38::mesh_ray_test` on the vertex block at
/// +0xa0 (16-byte vertices) and the tags at +0xb0. On a hit the distance goes to `distance` and the four tags to `out`.
/// Returns 1 on a hit.
pub fn mesh_test(mem: &mut Mem, object: u32, origin: Vec3, direction: Vec3, out: u32, distance: u32) -> u32 {
    let block = mem.u32(object + 0xa0);
    let hit = {
        let view = &*mem;
        e38::mesh_ray_test(
            origin,
            direction,
            |i| Vec3::load(view, block + 16 * i as u32),
            |i| view.u32(object + 0xb0 + 4 * i as u32),
        )
    };
    let Some(hit) = hit else {
        return 0;
    };
    mem.set_f32(distance, hit.distance);
    for (index, tag) in hit.tags.iter().enumerate() {
        mem.set_u32(out + 4 * index as u32, *tag);
    }
    1
}

/// E38SegmentSweep (0x4fad90), stdcall: the cell query over the start's box (widened by `SWEEP_MARGIN`) gives four list
/// heads at `scratch + 0x20`. Each object in those lists with bit 9 of +0x60 set is tested by `mesh_test` until one
/// hits. `lo` and `hi` are written to `scratch` and `scratch + 0x10` for the query.
pub fn segment_sweep(
    host: &mut impl CellQueryHost,
    mem: &mut Mem,
    scratch: u32,
    start_at: u32,
    end_at: u32,
    out: u32,
    distance: u32,
) {
    let start = Vec3::load(mem, start_at);
    let direction = Vec3::load(mem, end_at) - start;
    let (lo, hi, cells) = (scratch, scratch + 0x10, scratch + 0x20);
    let widen = |sign: f32| Vec3 {
        x: start.x + sign * SWEEP_MARGIN,
        y: start.y + sign * SWEEP_MARGIN,
        z: start.z + sign * SWEEP_MARGIN,
    };
    widen(-1.0).store(mem, lo);
    widen(1.0).store(mem, hi);
    host.query_cells(mem, lo, hi, cells);

    let mut hit = 0;
    for slot in 0..4 {
        let head = mem.u32(cells + 4 * slot);
        if head == 0 {
            continue;
        }
        let mut node = mem.u32(head);
        while node != 0 && hit == 0 {
            let object = mem.u32(node + 0xc);
            if object != 0 && mem.u32(object + 0x60) & 0x200 != 0 {
                hit = mesh_test(mem, object, start, direction, out, distance);
            }
            node = mem.u32(node);
        }
        if hit != 0 {
            break;
        }
    }
}

/// E38Ops::M_D480 (0x4fd480): ray/triangle intersection of `origin` and `direction` with the triangle `v0`, `v1`, `v2`,
/// stored at `output`. The asm reads every input before it writes the output. Returns 1 on a hit.
pub fn ray_triangle(mem: &mut Mem, origin: u32, direction: u32, v0: u32, v1: u32, v2: u32, output: u32) -> u32 {
    let result = e38::ray_triangle(
        Vec3::load(mem, origin),
        Vec3::load(mem, direction),
        Vec3::load(mem, v0),
        Vec3::load(mem, v1),
        Vec3::load(mem, v2),
    );
    store_ray_triangle_result(mem, output, &result);
    u32::from(result.hit)
}

/// The constants M_D760X reads from the image: 2.0 (0x72ad94, the start distance), 1.0 (0x795e14), 0.0 (0x795e50), 1/15
/// (0x7399bc, the nibble scale) and 1/31 (0x7399c0, the 5-bit channel scale).
const SAMPLE_START: f32 = 2.0;
const ONE: f32 = 1.0;
const ZERO: f32 = 0.0;

/// The nearest face found so far by M_D760X: its ray parameter, barycentric `u` and `v`, the unnormalised normal, and
/// the face and grid block it came from.
#[derive(Clone, Copy)]
struct NearestFace {
    t: f32,
    u: f32,
    v: f32,
    normal: Vec3,
    face: u32,
    base: u32,
}

/// The low and high nibble sums (scaled by 1/15) of the three face bytes at `at`, as M_D760X passes them to the table.
fn sample_sums(mem: &Mem, at: u32, w: f32, u: f32, v: f32) -> (f32, f32) {
    let bytes = [mem.u8(at), mem.u8(at + 1), mem.u8(at + 2)];
    let low = nibble_sum(bytes, 0, w, u, v) * NIBBLE_SCALE;
    (low, nibble_sum(bytes, 4, w, u, v) * NIBBLE_SCALE)
}

/// E38 grid ray query (0x4fd760, `FUN_004fd760`), thiscall on the grid with four stack words: the nearest face of the
/// records in `list` (the grid's own list at +0x1014 when `list` is 0) that the ray from `start` to `end` hits, with
/// distance at most 1. The hit record goes to `out`. The ray's centre box is the start (`box_sat_overlap`) with the
/// half direction as its extent. Returns 1 on a hit, else 0.
pub fn mesh_ray_query(
    host: &mut impl SampleHost,
    mem: &mut Mem,
    grid: u32,
    start_at: u32,
    end_at: u32,
    out: u32,
    list: u32,
) -> u32 {
    let start = Vec3::load(mem, start_at);
    let direction = Vec3::load(mem, end_at) - start;
    let half = direction.scale(0.5);
    let list = if list == 0 { grid + 0x1014 } else { list };
    let mut nearest =
        NearestFace { t: SAMPLE_START, u: 0.0, v: 0.0, normal: Vec3 { x: 0.0, y: 0.0, z: 0.0 }, face: 0, base: 0 };
    for index in 1..=mem.u32(list) {
        let record = mem.u32(list + 4 * index);
        let word = mem.u32(record + 0x18);
        let base = mem.u32(grid + 0xc + 4 * (word >> 22));
        if base == 0 || !e38::box_sat_overlap(load_sat_box(mem, record), start, half) {
            continue;
        }
        let first = mem.u32(record + 0x1c);
        for face_index in 0..(word & 0x1f_ffff) {
            let face = first + 0x12 * face_index;
            let vertex = |slot: u32| base + 12 * u32::from(mem.u16(face + 2 * slot));
            let hit = e38::ray_triangle(
                start,
                direction,
                Vec3::load(mem, vertex(0)),
                Vec3::load(mem, vertex(1)),
                Vec3::load(mem, vertex(2)),
            );
            if let RayTriangleResult { hit: true, distance: Some(t), u: Some(u), v: Some(v), normal: Some(normal) } =
                hit
            {
                if t >= ZERO && t < nearest.t {
                    nearest = NearestFace { t, u, v, normal, face, base };
                }
            }
        }
    }
    if nearest.t > ONE {
        return 0;
    }

    let NearestFace { t, u, v, face, base, .. } = nearest;
    let normal = nearest.normal.normalize_sse();
    let w = ONE - (v + u);
    (direction.scale(t) + start).store(mem, out);
    normal.store(mem, out + 0x10);
    let corner = Vec3::load(mem, base + 12 * u32::from(mem.u16(face)));
    mem.set_f32(out + 0x1c, -corner.dot(normal));
    mem.set_f32(out + 0x20, channel_sum(mem.u16(face + 8), v, u, w) * CHANNEL_SCALE);
    mem.set_f32(out + 0x24, channel_sum(mem.u16(face + 6), v, u, w) * CHANNEL_SCALE);
    let (low, high) = sample_sums(mem, face + 12, w, u, v);
    let surface = host.sample(u32::from(mem.u8(face + 10)), low, high) & 0xff;
    mem.set_u32(out + 0x28, surface);
    let (low, high) = sample_sums(mem, face + 15, w, u, v);
    let surface = host.sample(u32::from(mem.u8(face + 11)), low, high) & 0xff;
    mem.set_u32(out + 0x2c, surface);
    mem.set_f32(out + 0x30, t);
    mem.set_u32(out + 0x34, u32::from(mem.u16(face + 6) >> 15));
    mem.set_u32(out + 0x38, u32::from(mem.u16(face + 8) >> 15));
    1
}

/// The ray parameter `CCollisionSystem::Vf00` pre-seeds in its hit record (0x72e954): M_DE40X starts its search there.
const SEGMENT_START: f32 = 1.1;

/// E38Ops::M_DE40X (0x4fde40), thiscall on the system `system` with four stack words (ret 0x10): the mesh ray from the
/// point at `origin_at` along the direction at `direction_at`, over the tree at `root`. The nearest face hit so far
/// goes to the record at `out` (its ray parameter at +0x20 starts at the value the caller pre-seeded). The worklist is
/// LIFO with the root first: a node without faces pushes its children that the ray's box (the origin, half the
/// direction) overlaps, the child then the one 0x20 past it; a node with faces tests them against the table at `system
/// + 0xc + 4 * (word >> 22)`. A hit replaces the record only when `t > 0` and `t` is nearer (strictly).
pub fn mesh_ray_worklist(mem: &mut Mem, system: u32, origin_at: u32, direction_at: u32, root: u32, out: u32) {
    let half = Vec3::load(mem, direction_at).scale(0.5);
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        let word = mem.u32(node + 0x18);
        let faces = word & 0x1f_ffff;
        if faces == 0 {
            let child = mem.u32(node + 0x1c);
            if child == 0 {
                continue;
            }
            let center = Vec3::load(mem, origin_at);
            if mem.u32(child + 0x18) & 0x20_0000 == 0 && e38::box_sat_overlap(load_sat_box(mem, child), center, half) {
                pending.push(child);
            }
            if mem.u32(child + 0x38) & 0x20_0000 == 0
                && e38::box_sat_overlap(load_sat_box(mem, child + 0x20), center, half)
            {
                pending.push(child + 0x20);
            }
            continue;
        }
        let table = mem.u32(system + 0xc + 4 * (word >> 22));
        if table == 0 {
            continue;
        }
        let base = mem.u32(node + 0x1c);
        for face in 0..faces {
            let at = base + 0x12 * face;
            let vertex = |slot: u32| table + 12 * u32::from(mem.u16(at + 2 * slot));
            let hit = e38::ray_triangle(
                Vec3::load(mem, origin_at),
                Vec3::load(mem, direction_at),
                Vec3::load(mem, vertex(0)),
                Vec3::load(mem, vertex(1)),
                Vec3::load(mem, vertex(2)),
            );
            if let RayTriangleResult { hit: true, distance: Some(t), u: Some(u), v: Some(v), normal: Some(normal) } =
                hit
            {
                if t > ZERO && mem.f32(out + 0x20) > t {
                    normal.store(mem, out + 0xc);
                    mem.set_f32(out + 0x20, t);
                    mem.set_f32(out + 0x24, u);
                    mem.set_f32(out + 0x28, v);
                    mem.set_u32(out + 0x1c, table);
                    mem.set_u32(out + 0x18, at);
                }
            }
        }
    }
}

/// The frame of `CCollisionSystem::Vf00` below its entry stack pointer `esp` (after `sub esp, 0x48` and `push esi`):
/// the origin and direction (the mesh ray's arguments), the hit record (its +0x20 to +0x28 words), and the entry stack
/// pointer of the lookups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemFrame {
    pub origin: u32,
    pub direction: u32,
    pub hit: u32,
    pub sample_sp: u32,
}

pub fn system_frame(esp: u32) -> SystemFrame {
    let base = esp - 0x4c;
    SystemFrame { origin: base + 0x14, direction: base + 0x8, hit: base + 0x20, sample_sp: base - 0x18 }
}

/// `CCollisionSystem::Vf00` (0x4fe810), the segment query of the collision system `system` from the point at
/// `origin_at` to the point at `end_at`: the mesh ray on the root (`mesh_ray_worklist`), then on a hit (`t` at most 1,
/// unordered misses) the record at `out` (`e38::segment_record`). Returns 1 on a hit, else 0; a miss writes nothing but
/// the frame.
pub fn system_vf00(
    host: &mut impl SampleHost,
    mem: &mut Mem,
    system: u32,
    origin_at: u32,
    end_at: u32,
    out: u32,
    frame: SystemFrame,
) -> u32 {
    let root = mem.u32(system + 8);
    if root == 0 {
        return 0;
    }
    let origin = Vec3::load(mem, origin_at);
    let direction = Vec3::load(mem, end_at) - origin;
    origin.store(mem, frame.origin);
    direction.store(mem, frame.direction);
    mem.set_f32(frame.hit + 0x20, SEGMENT_START);
    mesh_ray_worklist(mem, system, frame.origin, frame.direction, root, frame.hit);
    let t = mem.f32(frame.hit + 0x20);
    if !(t <= ONE) {
        return 0;
    }
    vec3_normalize(mem, frame.hit + 0xc);
    let normal = Vec3::load(mem, frame.hit + 0xc);
    let face_at = mem.u32(frame.hit + 0x18);
    let table = mem.u32(frame.hit + 0x1c);
    let len = vec3_dot(mem, table + 12 * u32::from(mem.u16(face_at)), frame.hit + 0xc);
    let face: [u8; 0x12] = std::array::from_fn(|i| mem.u8(face_at + i as u32));
    let hit = e38::SurfaceHit { t, normal, n0: mem.f32(frame.hit + 0x24), n1: mem.f32(frame.hit + 0x28), len, face };
    let record = e38::segment_record(origin, direction, hit);
    record.point.store(mem, out);
    record.normal.store(mem, out + 0x10);
    mem.set_f32(out + 0x1c, record.neg_len);
    mem.set_f32(out + 0x20, record.channels[0]);
    mem.set_f32(out + 0x24, record.channels[1]);
    for (i, lookup) in record.lookups.into_iter().enumerate() {
        let surface = host.sample(lookup.byte, lookup.low, lookup.high) & 0xff;
        mem.set_u32(out + 0x28 + 4 * i as u32, surface);
    }
    mem.set_f32(out + 0x30, record.t);
    mem.set_u32(out + 0x34, record.flags[0]);
    mem.set_u32(out + 0x38, record.flags[1]);
    1
}

/// Loads a separating-axis box: the centre at +0 and the half extents at +12.
pub fn load_sat_box(mem: &Mem, at: u32) -> SatBox {
    SatBox { center: Vec3::load(mem, at), half_extent: Vec3::load(mem, at + 12) }
}

/// Stores the fields of a ray/triangle result that the asm wrote: the normal at +0x0c, `distance` at +0x20, `u` at
/// +0x24 and `v` at +0x28. A miss leaves the fields it never reached untouched.
pub fn store_ray_triangle_result(mem: &mut Mem, at: u32, result: &RayTriangleResult) {
    if let Some(normal) = result.normal {
        normal.store(mem, at + 12);
    }
    if let Some(distance) = result.distance {
        mem.set_f32(at + 32, distance);
    }
    if let Some(u) = result.u {
        mem.set_f32(at + 36, u);
    }
    if let Some(v) = result.v {
        mem.set_f32(at + 40, v);
    }
}

/// Stores the fields of a segment/triangle record that the asm wrote: the normal at +0x00, `t` at +0x14, `u` at +0x18
/// and `v` at +0x1c. A miss leaves the fields it never reached untouched.
pub fn store_segment_triangle(mem: &mut Mem, at: u32, result: &SegmentTriangle) {
    if let Some(normal) = result.normal {
        normal.store(mem, at);
    }
    if let Some(t) = result.t {
        mem.set_f32(at + 0x14, t);
    }
    if let Some(u) = result.u {
        mem.set_f32(at + 0x18, u);
    }
    if let Some(v) = result.v {
        mem.set_f32(at + 0x1c, v);
    }
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

/// The collider's tree root (+8) and its own result word (+0x1014), used when the caller gives no output.
const ROOT: u32 = 0x8;
const OWN_OUT: u32 = 0x1014;

/// `E39_SegTest170(this, a, b, out)` (fastcall, three stack words, `ret 0xc`): zeroes the result word, builds the query
/// box (the point `a`, then the half extents `b`, six floats at `query` in the original's frame), walks the tree and
/// returns whether the walk found anything. A null `out` is the collider's own word.
pub fn seg_test(mem: &mut Mem, this: u32, a: u32, b: u32, out: u32, query: u32) -> u32 {
    let out = if out == 0 { this + OWN_OUT } else { out };
    mem.set_u32(out, 0);
    for lane in 0..3 {
        let (centre, half) = (mem.u32(a + 4 * lane), mem.u32(b + 4 * lane));
        mem.set_u32(query + 4 * lane, centre);
        mem.set_u32(query + 12 + 4 * lane, half);
    }
    node_walk(mem, query, mem.u32(this + ROOT), out);
    (mem.u32(out) != 0) as u32
}

/// `E39_SegTest70(this, a, b, out)` (0x4ff070, fastcall, three stack words, `ret 0xc`): as `seg_test`, with the query
/// box `e38::segment_box` of the segment from `a` to `b` (six floats at `query` in the original's frame).
pub fn segment_test(mem: &mut Mem, this: u32, a: u32, b: u32, out: u32, query: u32) -> u32 {
    let segment = e38::segment_box(Vec3::load(mem, a), Vec3::load(mem, b));
    let out = if out == 0 { this + OWN_OUT } else { out };
    segment.center.store(mem, query);
    segment.half_extent.store(mem, query + 12);
    mem.set_u32(out, 0);
    node_walk(mem, query, mem.u32(this + ROOT), out);
    (mem.u32(out) != 0) as u32
}

/// Stores the fields of a point/triangle result that the asm wrote: the normal at +0x0c, `t` at +0x24 and `u` at +0x28,
/// and on a hit 1.0 at +0x20 and the foot at +0x00.
pub fn store_point_triangle(mem: &mut Mem, at: u32, result: &PointTriangle) {
    result.normal.store(mem, at + 0xc);
    if let Some(t) = result.t {
        mem.set_f32(at + 0x24, t);
    }
    if let Some(u) = result.u {
        mem.set_f32(at + 0x28, u);
    }
    if let Some(foot) = result.foot {
        mem.set_f32(at + 0x20, ONE);
        foot.store(mem, at);
    }
}

/// E38Ops::M_E2D0 (0x4fe2d0), thiscall with five stack words (`this` is not read): `e38::point_triangle` of the point
/// at `point` and the face's vertices, stored to `out`. Returns 1 on a hit.
pub fn point_triangle(mem: &mut Mem, point: u32, v0: u32, v1: u32, v2: u32, out: u32) -> u32 {
    let [point, v0, v1, v2] = [point, v0, v1, v2].map(|at| Vec3::load(mem, at));
    let result = e38::point_triangle(point, v0, v1, v2);
    store_point_triangle(mem, out, &result);
    u32::from(result.foot.is_some())
}

/// The low (or high, with `shift` 4) nibble sum of the face bytes `[c, d, e]`, weighted as M_EBC0 does: `d` by `t`, `e`
/// by `u`, `c` by `w` (asm order), scaled by 1/15.
fn face_nibble_sum(bytes: [u8; 3], shift: u32, t: f32, u: f32, w: f32) -> f32 {
    let nibble = |byte: u8| f32::from((byte >> shift) & 0xf);
    ((nibble(bytes[1]) * t + nibble(bytes[2]) * u) + nibble(bytes[0]) * w) * NIBBLE_SCALE
}

/// The two nibble sums of the face bytes at `at` that M_EBC0 passes to the material table.
fn face_sample_sums(mem: &Mem, at: u32, t: f32, u: f32, w: f32) -> (f32, f32) {
    let bytes = [mem.u8(at), mem.u8(at + 1), mem.u8(at + 2)];
    (face_nibble_sum(bytes, 0, t, u, w), face_nibble_sum(bytes, 4, t, u, w))
}

/// E38Ops::M_EBC0 (0x4febc0), thiscall on the collider with `(point, out, list)`, `ret 0xc`: the first face of the
/// records in `list` (the collider's own list at +0x1014 when 0) that `point_triangle` hits. Every hit leaves 1.0 as
/// its distance, and only a distance below the best so far (2.0) is taken, so the first hit is the one kept. Its record
/// goes to `out`: the foot, the normal and its plane distance, the 5-bit channel sums of the face words +8 and +6, the
/// two material table samples, and the two flag bits. Returns 1 on a hit, else 0 with `out` untouched.
pub fn face_query(host: &mut impl SampleHost, mem: &mut Mem, this: u32, point_at: u32, out: u32, list: u32) -> u32 {
    let point = Vec3::load(mem, point_at);
    let list = if list == 0 { this + OWN_OUT } else { list };
    let mut found = None;
    'records: for index in 1..=mem.u32(list) {
        let record = mem.u32(list + 4 * index);
        let word = mem.u32(record + 0x18);
        let base = mem.u32(this + 0xc + 4 * (word >> 22));
        if base == 0 {
            continue;
        }
        for face_index in 0..(word & 0x1f_ffff) {
            let face = mem.u32(record + 0x1c) + 0x12 * face_index;
            let vertex = |slot: u32| Vec3::load(mem, base + 12 * u32::from(mem.u16(face + 2 * slot)));
            let result = e38::point_triangle(point, vertex(0), vertex(1), vertex(2));
            if let (Some(foot), Some(t), Some(u)) = (result.foot, result.t, result.u) {
                found = Some((foot, result.normal, t, u, face, base));
                break 'records;
            }
        }
    }
    let Some((foot, normal, t, u, face, base)) = found else {
        return 0;
    };
    normal.store(mem, out + 0x10);
    let corner = Vec3::load(mem, base + 12 * u32::from(mem.u16(face)));
    mem.set_f32(out + 0x1c, -corner.dot(normal));
    foot.store(mem, out);
    mem.set_f32(out + 0x30, ZERO);
    mem.set_u32(out + 0x38, u32::from(mem.u16(face + 8) >> 15));
    mem.set_u32(out + 0x34, u32::from(mem.u16(face + 6) >> 15));
    let w = ONE - (u + t);
    mem.set_f32(out + 0x20, channel_sum(mem.u16(face + 8), u, t, w) * CHANNEL_SCALE);
    let (low, high) = face_sample_sums(mem, face + 12, t, u, w);
    let surface = host.sample(u32::from(mem.u8(face + 10)), low, high) & 0xff;
    mem.set_u32(out + 0x28, surface);
    let (low, high) = face_sample_sums(mem, face + 15, t, u, w);
    let surface = host.sample(u32::from(mem.u8(face + 11)), low, high) & 0xff;
    mem.set_u32(out + 0x2c, surface);
    mem.set_f32(out + 0x24, channel_sum(mem.u16(face + 6), u, t, w) * CHANNEL_SCALE);
    1
}
