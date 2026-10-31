//! The brake walls: the quadtree queries and the wall probes, generic over the store. The geometry is
//! `brake_wall`; the hosts are in `layout/wall_tree.rs`.

use super::brake_wall::{self, segment_hit, SegmentHit};
use super::layout::wall_tree::{RaycastCalls, VfE8Host, WallVfCcHost};
use super::store::CollisionStore;

/// The query shape: a point, or a box (its centre and half extents).
#[derive(Clone, Copy)]
enum Shape {
    Point([f32; 2]),
    Box([f32; 4]),
}

fn query<S: CollisionStore>(store: &mut S, wall: u32, shape: Shape, collector: u32) -> u32 {
    let mut stack = [0u32; 257];
    stack[1] = store.wall_root(wall);
    store.clear_collector(collector);
    let mut top = 1usize;

    loop {
        let node = stack[top];
        let mut next = top - 1;
        let (items, link) = store.wall_node(node);
        if items == 0 {
            if link != 0 {
                let [first, second] = store.wall_children(link);
                if child_hit(store, shape, first) {
                    stack[top] = first;
                    next = top;
                }
                if child_hit(store, shape, second) {
                    stack[next + 1] = second;
                    next += 1;
                }
            }
        } else {
            store.collector_push(collector, node);
        }

        if next == 0 {
            break;
        }
        top = next;
    }
    u32::from(store.collector_count(collector) != 0)
}

fn child_hit<S: CollisionStore>(store: &S, shape: Shape, child: u32) -> bool {
    let [cx, cy, hx, hy] = store.wall_box(child);
    match shape {
        Shape::Point([x, y]) => (x - cx).abs() < hx && (y - cy).abs() < hy,
        Shape::Box([x, y, sx, sy]) => (x - cx).abs() < hx + sx && (y - cy).abs() < hy + sy,
    }
}

/// Collects the leaf nodes containing a point.
pub fn query_point<S: CollisionStore>(store: &mut S, wall: u32, point: u32, collector: u32) -> u32 {
    let shape = Shape::Point([store.f32(point), store.f32(point + 4)]);
    query(store, wall, shape, collector)
}

/// Collects the leaf nodes overlapping a box.
pub fn query_box<S: CollisionStore>(store: &mut S, wall: u32, bounds: u32, collector: u32) -> u32 {
    let shape = Shape::Box(store.f32s(bounds));
    query(store, wall, shape, collector)
}

/// Whether the wall has a leaf under the ten-unit box around the point's x/y.
pub fn query_xy_box<S: CollisionStore>(store: &S, wall: u32, point: u32) -> u32 {
    let shape = Shape::Box([store.f32(point), store.f32(point + 4), 10.0, 10.0]);
    let mut stack = [0u32; 257];
    stack[1] = store.wall_root(wall);
    let mut top = 1usize;
    let mut found = false;

    loop {
        let node = stack[top];
        let mut next = top - 1;
        let (items, link) = store.wall_node(node);
        if items == 0 {
            if link != 0 {
                let [first, second] = store.wall_children(link);
                if child_hit(store, shape, first) {
                    stack[top] = first;
                    next = top;
                }
                if child_hit(store, shape, second) {
                    stack[next + 1] = second;
                    next += 1;
                }
            }
        } else {
            found = true;
        }

        if next == 0 {
            break;
        }
        top = next;
    }
    u32::from(found)
}

/// Queries the wall with the corners' x/y box: each corner is fetched through the host frame, and the box of
/// their `x` and `y` is queried against the wall's leaves unless it is too wide. Returns 1 when the query found
/// any.
pub fn wall_vf_cc<S: CollisionStore>(
    host: &mut impl WallVfCcHost,
    store: &mut S,
    this: u32,
    a: u32,
    b: u32,
    entry: u32,
) -> u32 {
    let frame_at = ((entry - 4) & !0xf) - 0x30;
    let corner_at = frame_at + 0x20;
    let bounds_at = frame_at + 0x10;
    let mut corners = [[0.0; 2]; 2];
    for (corner, matrix) in corners.iter_mut().zip([a, b]) {
        let frame = host.frame(frame_at - 4, this);
        host.fetch_x(frame_at - 0xc, matrix, corner_at, frame);
        *corner = [store.f32(corner_at), store.f32(corner_at + 4)];
    }
    let bounds = brake_wall::corners_box(corners[0], corners[1]);
    for (k, value) in bounds.into_iter().enumerate() {
        store.set_f32(bounds_at + 4 * k as u32, value);
    }
    if brake_wall::box_too_wide(bounds[2], bounds[3]) {
        return 0;
    }
    let collector = store.wall_leaves(this);
    u32::from(host.query_box(frame_at - 0xc, this, bounds_at, collector) != 0)
}

/// The first leaf segment (in the query's order) whose record the point is inside of: the index word and the hit.
fn first_hit<S: CollisionStore>(store: &S, wall: u32, point: u32, hits: u32) -> Option<(u32, SegmentHit)> {
    let mesh = store.wall_mesh(wall);
    let p = [store.f32(point), store.f32(point + 4)];
    (0..store.collector_count(hits)).find_map(|i| {
        let seg = store.collector_leaf(hits, i);
        (0..store.leaf_count(seg)).find_map(|j| {
            let word = store.leaf_word(seg, j);
            let s = word & 0x3fff;
            segment_hit(store.wall_segment(mesh, s), p).map(|hit| (word, hit))
        })
    })
}

/// The first wall segment the point is inside of answers: `out` gets its normal, perpendicular distance and
/// penetration, and the flags word (the index's bit 15, bit 1 for the latch, bit 2 when the index's bit 14 emits).
/// Bit 14 emits the penetration signal and answers 0; otherwise the answer is 1.
pub fn wall_raycast<S: CollisionStore>(
    calls: &mut impl RaycastCalls,
    store: &mut S,
    wall: u32,
    point: u32,
    out: u32,
    hits: u32,
) -> u32 {
    if query_point(store, wall, point, hits) == 0 {
        store.set_wall_latch(wall, 0);
        return 0;
    }
    let Some((word, hit)) = first_hit(store, wall, point, hits) else {
        store.set_wall_latch(wall, 0);
        return 0;
    };
    let latch = if store.wall_latch(wall) != 0 { 2 } else { 0 };
    let gate = word >> 15;
    let flags = latch | gate;
    store.store_wall_probe(out, &hit, flags);
    store.set_wall_latch(wall, 0);
    if word & 0x4000 == 0 {
        return 1;
    }
    store.store_wall_flags(out, flags | 4);
    calls.emit_penetration(store.penetration_emitter(), out);
    0
}

/// With `out` and `flag` both set, fetches the two end matrices `a` and `b` and tests the segment between them
/// against each edge of the wall's leaf segments, in order, until one hits. A crossing within an edge sets the
/// latch; unless the index word's bit 14 is set it is the hit: `out` gets the point, the normal (`z` = 0), the offset,
/// `t` and the wall. Returns 1 on a hit, else 0.
pub fn brake_wall_vf_e8<S: CollisionStore>(
    host: &mut impl VfE8Host,
    store: &mut S,
    this: u32,
    [a, b, _c, out, flag]: [u32; 5],
    entry: u32,
) -> u32 {
    if out == 0 || flag == 0 {
        return 0;
    }
    let frame = ((entry - 4) & !0xf) - 0x80;
    let (first, second) = (frame + 0x60, frame + 0x70);
    host.fetch_x(frame - 0xc, a, first, store.frame_root());
    host.fetch_x(frame - 0xc, b, second, store.frame_root());
    let point = |at: u32| [store.f32(at), store.f32(at + 4), store.f32(at + 8)];
    let (p0, p1) = (point(first), point(second));
    let Some(d) = brake_wall::sweep(p0, p1) else {
        return 0;
    };
    let leaves = store.wall_leaves(this);
    for i in 0..store.collector_count(leaves) {
        let seg = store.collector_leaf(leaves, i);
        for j in 0..store.leaf_count(seg) {
            let word = store.leaf_word(seg, j);
            let [_, from, _, to] = store.wall_segment(store.wall_mesh(this), word & 0x3fff);
            let Some(hit) = brake_wall::edge_cross(p0, p1, d, from, to) else {
                continue;
            };
            store.set_wall_latch(this, 1);
            if word & 0x4000 != 0 {
                continue;
            }
            store.store_wall_crossing(out, this, &hit);
            return 1;
        }
    }
    0
}

/// Copies the point's x and y to the 8-byte buffer at `tmp`, then raycasts the manager's wall through it; 0 without
/// a wall. The raycast's leaf list lives below `tmp` in the caller's frame.
pub fn wall_probe<S: CollisionStore>(
    calls: &mut impl RaycastCalls,
    store: &mut S,
    manager: u32,
    point: u32,
    out: u32,
    tmp: u32,
) -> u32 {
    let (x, y) = (store.u32(point), store.u32(point + 4));
    let wall = store.u32(manager + 4);
    store.set_u32(tmp, x);
    store.set_u32(tmp + 4, y);
    if wall == 0 {
        return 0;
    }
    wall_raycast(calls, store, wall, tmp, out, tmp - 0x1a0)
}
