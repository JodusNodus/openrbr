//! The collision mesh queries: ray seeds, the separating-axis tests, the node walk, the segment and ray tests,
//! `CCollisionSystem::Vf00` and the face query, generic over the store.

use super::layout::solver_hosts::{CellQueryHost, RayHost, SampleHost};
use super::mesh_hit::{
    self, channel_sum, face_samples, face_vertex, face_word, nibble_sum, CHANNEL_SCALE, FACE_BYTES, NIBBLE_SCALE,
};
use super::overlap::{self};
use super::store::{CollisionStore, RayHit, SurfaceRecord};
use super::triangle::{self, PointTriangle, RayTriangleResult, SegmentTriangle};
use crate::math::vec::Vec3;

/// E38RaySeeds (0x4f6550): clears the ray object's seed bytes and words, then its two vectors (host calls). Returns
/// `object`.
pub fn ray_seeds<S: CollisionStore>(host: &mut impl RayHost, store: &mut S, object: u32) -> u32 {
    store.clear_ray_seeds(object);
    let (first, second) = store.ray_vectors(object);
    host.vec4_clear(first);
    host.vec4_clear(second);
    object
}

/// E38RayClear (0x4f6740): clears the ray object's per-test fields. The original leaves eax as the caller had it.
pub fn ray_clear<S: CollisionStore>(store: &mut S, object: u32) {
    store.clear_ray(object);
}

/// E38Ops::M_D230 (0x4fd230): true (1) when no world axis separates the box at `this` from the box given by
/// `other_center` and `other_extent`.
pub fn world_axes_overlap<S: CollisionStore>(store: &S, this: u32, other_center: u32, other_extent: u32) -> u32 {
    u32::from(overlap::world_axes_overlap(store.sat_box(this), store.vec3(other_center), store.vec3(other_extent)))
}

/// E38Ops::M_D2C0 (0x4fd2c0): true (1) when no world or cross axis separates the box at `this` from the other box.
pub fn box_sat_overlap<S: CollisionStore>(store: &S, this: u32, other_center: u32, other_extent: u32) -> u32 {
    u32::from(overlap::box_sat_overlap(store.sat_box(this), store.vec3(other_center), store.vec3(other_extent)))
}

/// E38 node walk (0x4fdd60, `FUN_004fdd60`), stdcall with three stack words: writes the leaves under `root` whose boxes
/// overlap the query box at `query` (centre, then half extents at +12) to `out`, after the count at +0. A leaf is a
/// node with faces (bits 0..20 of +0x18). A node without faces pushes its children (+0x1c, and the one 0x20 past it)
/// whose bit 21 at +0x18 (or +0x38) is clear and whose boxes overlap the query; the last pushed is walked first.
pub fn node_walk<S: CollisionStore>(store: &mut S, query: u32, root: u32, out: u32) {
    let mut pending = if root == 0 { Vec::new() } else { vec![root] };
    let mut leaves = Vec::new();
    while let Some(node) = pending.pop() {
        let tree = store.tree_node(node);
        if tree.faces() != 0 {
            leaves.push(node);
            continue;
        }
        if tree.link == 0 {
            continue;
        }
        for (child, word) in store.child_pair(tree.link) {
            if word & 0x20_0000 == 0 && world_axes_overlap(store, child, query, query + 12) != 0 {
                pending.push(child);
            }
        }
    }
    store.set_u32(out, leaves.len() as u32);
    for (index, leaf) in leaves.iter().enumerate() {
        store.set_u32(out + 4 + 4 * index as u32, *leaf);
    }
}

/// E39_SegTest170 (0x4ff170, fastcall: `this` in ecx, stack words `a`, `b` and `out`, ret 0xc): the segment's ends
/// `a` and `b` (three floats each) go to the box at `tmp`, the original's frame below the return address, and
/// `node_walk` (0x4fdd60) runs on the object's tree (`this+8`) with it. The records go to `out`, or to the grid's own
/// list (`this+0x1014`) when `out` is 0. Returns 1 when the list has records, else 0.
pub fn seg_test170<S: CollisionStore>(store: &mut S, this: u32, a: u32, b: u32, out: u32, tmp: u32) -> u32 {
    let lst = if out == 0 { store.own_list(this) } else { out };
    store.set_u32(lst, 0);
    for k in 0..3 {
        let (centre, half) = (store.f32(a + 4 * k), store.f32(b + 4 * k));
        store.set_f32(tmp + 4 * k, centre);
        store.set_f32(tmp + 12 + 4 * k, half);
    }
    let root = store.tree_root(this);
    node_walk(store, tmp, root, lst);
    u32::from(store.u32(lst) != 0)
}

/// The half-width of SegmentSweep's query box around the segment start (`0.1`, 0x72b068).
const SWEEP_MARGIN: f32 = 0.1;

/// E38Ops::M_AC50 (0x4fac50), thiscall on the object: the mesh test of `triangle::mesh_ray_test` on the vertex block at
/// +0xa0 (16-byte vertices) and the tags at +0xb0. On a hit the distance goes to `distance` and the four tags to `out`.
/// Returns 1 on a hit.
pub fn mesh_test<S: CollisionStore>(
    store: &mut S,
    object: u32,
    origin: Vec3,
    direction: Vec3,
    out: u32,
    distance: u32,
) -> u32 {
    let block = store.mesh_block(object);
    let hit = {
        let view = &*store;
        triangle::mesh_ray_test(
            origin,
            direction,
            |i| view.mesh_point(block, i as u32),
            |i| view.mesh_tag(object, i as u32),
        )
    };
    let Some(hit) = hit else {
        return 0;
    };
    store.set_f32(distance, hit.distance);
    for (index, tag) in hit.tags.iter().enumerate() {
        store.set_u32(out + 4 * index as u32, *tag);
    }
    1
}

/// E38SegmentSweep (0x4fad90), stdcall: the cell query over the start's box (widened by `SWEEP_MARGIN`) gives four list
/// heads at `scratch + 0x20`. Each object in those lists with bit 9 of +0x60 set is tested by `mesh_test` until one
/// hits. `lo` and `hi` are written to `scratch` and `scratch + 0x10` for the query.
pub fn segment_sweep<S: CollisionStore>(
    host: &mut impl CellQueryHost<S>,
    store: &mut S,
    scratch: u32,
    start_at: u32,
    end_at: u32,
    out: u32,
    distance: u32,
) {
    let start = store.vec3(start_at);
    let direction = store.vec3(end_at) - start;
    let (lo, hi, cells) = (scratch, scratch + 0x10, scratch + 0x20);
    let widen = |sign: f32| Vec3 {
        x: start.x + sign * SWEEP_MARGIN,
        y: start.y + sign * SWEEP_MARGIN,
        z: start.z + sign * SWEEP_MARGIN,
    };
    store.store_vec3(lo, widen(-1.0));
    store.store_vec3(hi, widen(1.0));
    host.query_cells(store, lo, hi, cells);

    let mut hit = 0;
    for slot in 0..4 {
        let head = store.u32(cells + 4 * slot);
        if head == 0 {
            continue;
        }
        let mut node = store.cell_first(head);
        while node != 0 && hit == 0 {
            let object = store.node_owner(node);
            if object != 0 && store.mesh_flags(object) & 0x200 != 0 {
                hit = mesh_test(store, object, start, direction, out, distance);
            }
            node = store.node_next(node);
        }
        if hit != 0 {
            break;
        }
    }
}

/// E38Ops::M_D480 (0x4fd480): ray/triangle intersection of `origin` and `direction` with the triangle `v0`, `v1`, `v2`,
/// stored at `output`. The asm reads every input before it writes the output. Returns 1 on a hit.
pub fn ray_triangle<S: CollisionStore>(
    store: &mut S,
    origin: u32,
    direction: u32,
    v0: u32,
    v1: u32,
    v2: u32,
    output: u32,
) -> u32 {
    let result = triangle::ray_triangle(
        store.vec3(origin),
        store.vec3(direction),
        store.vec3(v0),
        store.vec3(v1),
        store.vec3(v2),
    );
    store_ray_triangle_result(store, output, &result);
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

/// The low and high nibble sums (scaled by 1/15) of the three face bytes, as M_D760X passes them to the table.
fn sample_sums(bytes: [u8; 3], w: f32, u: f32, v: f32) -> (f32, f32) {
    let low = nibble_sum(bytes, 0, w, u, v) * NIBBLE_SCALE;
    (low, nibble_sum(bytes, 4, w, u, v) * NIBBLE_SCALE)
}

/// E38 grid ray query (0x4fd760, `FUN_004fd760`), thiscall on the grid with four stack words: the nearest face of the
/// records in `list` (the grid's own list at +0x1014 when `list` is 0) that the ray from `start` to `end` hits, with
/// distance at most 1. The hit record goes to `out`. The ray's centre box is the start (`box_sat_overlap`) with the
/// half direction as its extent. Returns 1 on a hit, else 0.
pub fn mesh_ray_query<S: CollisionStore>(
    host: &mut impl SampleHost,
    store: &mut S,
    grid: u32,
    start_at: u32,
    end_at: u32,
    out: u32,
    list: u32,
) -> u32 {
    let start = store.vec3(start_at);
    let direction = store.vec3(end_at) - start;
    let half = direction.scale(0.5);
    let list = if list == 0 { store.own_list(grid) } else { list };
    let mut nearest =
        NearestFace { t: SAMPLE_START, u: 0.0, v: 0.0, normal: Vec3 { x: 0.0, y: 0.0, z: 0.0 }, face: 0, base: 0 };
    for index in 1..=store.list_len(list) {
        let record = store.list_record(list, index);
        let node = store.tree_node(record);
        let base = store.mesh_table(grid, node.table());
        if base == 0 || !overlap::box_sat_overlap(store.sat_box(record), start, half) {
            continue;
        }
        for face_index in 0..node.faces() {
            let face = node.link + 0x12 * face_index;
            let bytes = store.face_bytes(face);
            let vertex = |slot: usize| store.mesh_vertex(base, face_vertex(&bytes, slot));
            let hit = triangle::ray_triangle(start, direction, vertex(0), vertex(1), vertex(2));
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
    let bytes = store.face_bytes(face);
    let normal = nearest.normal.normalize_sse();
    let w = ONE - (v + u);
    let corner = store.mesh_vertex(base, face_vertex(&bytes, 0));
    let (w6, w8) = (face_word(&bytes, 6), face_word(&bytes, 8));
    let (low, high) = sample_sums(face_samples(&bytes, 12), w, u, v);
    let first = host.sample(u32::from(bytes[10]), low, high) & 0xff;
    let (low, high) = sample_sums(face_samples(&bytes, 15), w, u, v);
    let second = host.sample(u32::from(bytes[11]), low, high) & 0xff;
    store.store_surface_record(
        out,
        &SurfaceRecord {
            point: direction.scale(t) + start,
            normal,
            plane_offset: -corner.dot(normal),
            channels: [channel_sum(w8, v, u, w) * CHANNEL_SCALE, channel_sum(w6, v, u, w) * CHANNEL_SCALE],
            surfaces: [first, second],
            t,
            flags: [u32::from(w6 >> 15), u32::from(w8 >> 15)],
        },
    );
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
pub fn mesh_ray_worklist<S: CollisionStore>(
    store: &mut S,
    system: u32,
    origin_at: u32,
    direction_at: u32,
    root: u32,
    out: u32,
) {
    let half = store.vec3(direction_at).scale(0.5);
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        let tree = store.tree_node(node);
        let faces = tree.faces();
        if faces == 0 {
            if tree.link == 0 {
                continue;
            }
            let center = store.vec3(origin_at);
            for (child, word) in store.child_pair(tree.link) {
                if word & 0x20_0000 == 0 && overlap::box_sat_overlap(store.sat_box(child), center, half) {
                    pending.push(child);
                }
            }
            continue;
        }
        let table = store.mesh_table(system, tree.table());
        if table == 0 {
            continue;
        }
        for face in 0..faces {
            let at = tree.link + 0x12 * face;
            let bytes = store.face_bytes(at);
            let vertex = |slot: usize| store.mesh_vertex(table, face_vertex(&bytes, slot));
            let hit = triangle::ray_triangle(
                store.vec3(origin_at),
                store.vec3(direction_at),
                vertex(0),
                vertex(1),
                vertex(2),
            );
            if let RayTriangleResult { hit: true, distance: Some(t), u: Some(u), v: Some(v), normal: Some(normal) } =
                hit
            {
                if t > ZERO && store.ray_hit_t(out) > t {
                    store.store_ray_hit(out, &RayHit { normal, face: at, table, t, u, v });
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
/// unordered misses) the record at `out` (`mesh_hit::segment_record`). Returns 1 on a hit, else 0; a miss writes nothing but
/// the frame.
pub fn system_vf00<S: CollisionStore>(
    host: &mut impl SampleHost,
    store: &mut S,
    system: u32,
    origin_at: u32,
    end_at: u32,
    out: u32,
    frame: SystemFrame,
) -> u32 {
    let root = store.tree_root(system);
    if root == 0 {
        return 0;
    }
    let origin = store.vec3(origin_at);
    let direction = store.vec3(end_at) - origin;
    store.store_vec3(frame.origin, origin);
    store.store_vec3(frame.direction, direction);
    store.set_ray_hit_t(frame.hit, SEGMENT_START);
    mesh_ray_worklist(store, system, frame.origin, frame.direction, root, frame.hit);
    let t = store.ray_hit_t(frame.hit);
    if !(t <= ONE) {
        return 0;
    }
    let nearest = store.ray_hit(frame.hit);
    let normal = nearest.normal.normalize_sse();
    store.store_ray_hit_normal(frame.hit, normal);
    let face: [u8; FACE_BYTES] = store.face_bytes(nearest.face);
    let len = store.mesh_vertex(nearest.table, face_vertex(&face, 0)).dot(normal);
    let hit = mesh_hit::SurfaceHit { t, normal, u: nearest.u, v: nearest.v, vertex_dot_normal: len, face };
    let record = mesh_hit::segment_record(origin, direction, hit);
    let mut surfaces = [0; 2];
    for (surface, lookup) in surfaces.iter_mut().zip(record.lookups) {
        *surface = host.sample(lookup.byte, lookup.low, lookup.high) & 0xff;
    }
    store.store_surface_record(
        out,
        &SurfaceRecord {
            point: record.point,
            normal: record.normal,
            plane_offset: record.plane_offset,
            channels: record.channels,
            surfaces,
            t: record.t,
            flags: record.flags,
        },
    );
    1
}

/// Stores the fields of a ray/triangle result that the asm wrote: the normal at +0x0c, `distance` at +0x20, `u` at
/// +0x24 and `v` at +0x28. A miss leaves the fields it never reached untouched.
pub fn store_ray_triangle_result<S: CollisionStore>(store: &mut S, at: u32, result: &RayTriangleResult) {
    if let Some(normal) = result.normal {
        store.store_vec3(at + 12, normal);
    }
    if let Some(distance) = result.distance {
        store.set_f32(at + 32, distance);
    }
    if let Some(u) = result.u {
        store.set_f32(at + 36, u);
    }
    if let Some(v) = result.v {
        store.set_f32(at + 40, v);
    }
}

/// Stores the fields of a segment/triangle record that the asm wrote: the normal at +0x00, `t` at +0x14, `u` at +0x18
/// and `v` at +0x1c. A miss leaves the fields it never reached untouched.
pub fn store_segment_triangle<S: CollisionStore>(store: &mut S, at: u32, result: &SegmentTriangle) {
    if let Some(normal) = result.normal {
        store.store_vec3(at, normal);
    }
    if let Some(t) = result.t {
        store.set_f32(at + 0x14, t);
    }
    if let Some(u) = result.u {
        store.set_f32(at + 0x18, u);
    }
    if let Some(v) = result.v {
        store.set_f32(at + 0x1c, v);
    }
}

/// `E39_SegTest170(this, a, b, out)` (fastcall, three stack words, `ret 0xc`): zeroes the result word, builds the query
/// box (the point `a`, then the half extents `b`, six floats at `query` in the original's frame), walks the tree and
/// returns whether the walk found anything. A null `out` is the collider's own word.
pub fn seg_test<S: CollisionStore>(store: &mut S, this: u32, a: u32, b: u32, out: u32, query: u32) -> u32 {
    let out = if out == 0 { store.own_list(this) } else { out };
    store.set_u32(out, 0);
    for lane in 0..3 {
        let (centre, half) = (store.u32(a + 4 * lane), store.u32(b + 4 * lane));
        store.set_u32(query + 4 * lane, centre);
        store.set_u32(query + 12 + 4 * lane, half);
    }
    let root = store.tree_root(this);
    node_walk(store, query, root, out);
    (store.u32(out) != 0) as u32
}

/// `E39_SegTest70(this, a, b, out)` (0x4ff070, fastcall, three stack words, `ret 0xc`): as `seg_test`, with the query
/// box `overlap::segment_box` of the segment from `a` to `b` (six floats at `query` in the original's frame).
pub fn segment_test<S: CollisionStore>(store: &mut S, this: u32, a: u32, b: u32, out: u32, query: u32) -> u32 {
    let segment = overlap::segment_box(store.vec3(a), store.vec3(b));
    let out = if out == 0 { store.own_list(this) } else { out };
    store.store_vec3(query, segment.center);
    store.store_vec3(query + 12, segment.half_extent);
    store.set_u32(out, 0);
    let root = store.tree_root(this);
    node_walk(store, query, root, out);
    (store.u32(out) != 0) as u32
}

/// Stores the fields of a point/triangle result that the asm wrote: the normal at +0x0c, `t` at +0x24 and `u` at +0x28,
/// and on a hit 1.0 at +0x20 and the foot at +0x00.
pub fn store_point_triangle<S: CollisionStore>(store: &mut S, at: u32, result: &PointTriangle) {
    store.store_vec3(at + 0xc, result.normal);
    if let Some(t) = result.t {
        store.set_f32(at + 0x24, t);
    }
    if let Some(u) = result.u {
        store.set_f32(at + 0x28, u);
    }
    if let Some(foot) = result.foot {
        store.set_f32(at + 0x20, ONE);
        store.store_vec3(at, foot);
    }
}

/// E38Ops::M_E2D0 (0x4fe2d0), thiscall with five stack words (`this` is not read): `triangle::point_triangle` of the point
/// at `point` and the face's vertices, stored to `out`. Returns 1 on a hit.
pub fn point_triangle<S: CollisionStore>(store: &mut S, point: u32, v0: u32, v1: u32, v2: u32, out: u32) -> u32 {
    let [point, v0, v1, v2] = [point, v0, v1, v2].map(|at| store.vec3(at));
    let result = triangle::point_triangle(point, v0, v1, v2);
    store_point_triangle(store, out, &result);
    u32::from(result.foot.is_some())
}

/// The low (or high, with `shift` 4) nibble sum of the face bytes `[c, d, e]`, weighted as M_EBC0 does: `d` by `t`, `e`
/// by `u`, `c` by `w` (asm order), scaled by 1/15.
fn face_nibble_sum(bytes: [u8; 3], shift: u32, t: f32, u: f32, w: f32) -> f32 {
    let nibble = |byte: u8| f32::from((byte >> shift) & 0xf);
    ((nibble(bytes[1]) * t + nibble(bytes[2]) * u) + nibble(bytes[0]) * w) * NIBBLE_SCALE
}

/// The two nibble sums of the face bytes that M_EBC0 passes to the material table.
fn face_sample_sums(bytes: [u8; 3], t: f32, u: f32, w: f32) -> (f32, f32) {
    (face_nibble_sum(bytes, 0, t, u, w), face_nibble_sum(bytes, 4, t, u, w))
}

/// E38Ops::M_EBC0 (0x4febc0), thiscall on the collider with `(point, out, list)`, `ret 0xc`: the first face of the
/// records in `list` (the collider's own list at +0x1014 when 0) that `point_triangle` hits. Every hit leaves 1.0 as
/// its distance, and only a distance below the best so far (2.0) is taken, so the first hit is the one kept. Its record
/// goes to `out`: the foot, the normal and its plane distance, the 5-bit channel sums of the face words +8 and +6, the
/// two material table samples, and the two flag bits. Returns 1 on a hit, else 0 with `out` untouched.
pub fn face_query<S: CollisionStore>(
    host: &mut impl SampleHost,
    store: &mut S,
    this: u32,
    point_at: u32,
    out: u32,
    list: u32,
) -> u32 {
    let point = store.vec3(point_at);
    let list = if list == 0 { store.own_list(this) } else { list };
    let mut found = None;
    'records: for index in 1..=store.list_len(list) {
        let record = store.list_record(list, index);
        let node = store.tree_node(record);
        let base = store.mesh_table(this, node.table());
        if base == 0 {
            continue;
        }
        for face_index in 0..node.faces() {
            let face = node.link + 0x12 * face_index;
            let bytes = store.face_bytes(face);
            let vertex = |slot: usize| store.mesh_vertex(base, face_vertex(&bytes, slot));
            let result = triangle::point_triangle(point, vertex(0), vertex(1), vertex(2));
            if let (Some(foot), Some(t), Some(u)) = (result.foot, result.t, result.u) {
                found = Some((foot, result.normal, t, u, face, base));
                break 'records;
            }
        }
    }
    let Some((foot, normal, t, u, face, base)) = found else {
        return 0;
    };
    let bytes = store.face_bytes(face);
    let corner = store.mesh_vertex(base, face_vertex(&bytes, 0));
    let (w6, w8) = (face_word(&bytes, 6), face_word(&bytes, 8));
    let w = ONE - (u + t);
    let (low, high) = face_sample_sums(face_samples(&bytes, 12), t, u, w);
    let first = host.sample(u32::from(bytes[10]), low, high) & 0xff;
    let (low, high) = face_sample_sums(face_samples(&bytes, 15), t, u, w);
    let second = host.sample(u32::from(bytes[11]), low, high) & 0xff;
    store.store_surface_record(
        out,
        &SurfaceRecord {
            point: foot,
            normal,
            plane_offset: -corner.dot(normal),
            channels: [channel_sum(w8, u, t, w) * CHANNEL_SCALE, channel_sum(w6, u, t, w) * CHANNEL_SCALE],
            surfaces: [first, second],
            t: ZERO,
            flags: [u32::from(w6 >> 15), u32::from(w8 >> 15)],
        },
    );
    1
}
