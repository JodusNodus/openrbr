//! `CBody::VfD0` (0x4e1eb0) and `CBody::VfDC` (0x4e2850): the point, edge and pair contacts against another
//! entity.

use super::grid_contact::{self as collide_big, ContactKind};
use super::layout::collide_hosts::{VfD0Host, VfDcHost};
use super::store::CollisionStore;

/// CBody::VfD0's frame (0x4e1eb0), from the stack pointer `S` after its prologue: `push ebp`, the 16-byte alignment,
/// the exception record and 0xe78 bytes of locals, then `ebx`, `esi` and `edi`.
struct VfD0Frame {
    s: u32,
}

impl VfD0Frame {
    const MATERIAL: u32 = 0x1c;
    const HIT: u32 = 0x20;
    const DIR: u32 = 0x30;
    const NORMAL: u32 = 0x40;
    const DESC_A: u32 = 0x5c;
    const U: u32 = 0x64;
    const T: u32 = 0x6c;
    /// The point walk's second descriptor and the point contact's midpoint.
    const DESC_B: u32 = 0x70;
    const FROM: u32 = 0x80;
    /// The edge contact's midpoint.
    const EDGE_MID: u32 = 0x90;
    const HIT2: u32 = 0xa0;
    const BACK: u32 = 0xb0;
    const AT: u32 = 0xc0;
    const PROBE: u32 = 0xd0;
    const NORMAL2: u32 = 0xe0;
    const HIT_SPAN: u32 = 0xf0;
    const EDGE_SPAN: u32 = 0x100;
    /// This body's grid in the other entity's frame (`FetchY`).
    const GRID: u32 = 0x110;
    /// The other entity's grid in its own frame (`M6610`).
    const OTHER_GRID: u32 = 0x7c0;

    fn at(&self, offset: u32) -> u32 {
        self.s + offset
    }
}

const WORLD_POINT_CONTACTS: u32 = 4;
const WORLD_EDGE_CONTACTS: u32 = 8;
/// The other entity's flags byte (+0x60): a negative one is collided through `CollideInit`; 0x40 sends the point walk
/// through its slot 0xf0.
const ENTITY_FLAG_VF_F0: u8 = 0x40;

/// CBody::VfD0 (0x4e1eb0) on the body `this` against the entity `other` with `matrix` (`a`) and the event list `list`;
/// `esp` is its entry stack pointer. Returns eax: 1 when it appended an event (or `CollideInit`'s eax). This body's
/// grid goes into the other entity's frame. With point contacts on (`world_flags` bit 2) and the other flagged 0x40 or
/// without a grid, each grid point is tested by the other's slot 0xf0. Else the other's grid is copied (`M6610`) and,
/// with point contacts on, each grid point is paired with the grid's centre by `EdgeEdge` (`point_contact`); when none
/// hit and edge contacts are on (bit 3), each grid edge is (`edge_contact`).
pub fn body_vf_d0<S: CollisionStore>(
    host: &mut impl VfD0Host,
    store: &mut S,
    this: u32,
    matrix: u32,
    other: u32,
    list: u32,
    esp: u32,
) -> u32 {
    let f = VfD0Frame { s: ((esp - 4) & !0xf) - 0xe90 };
    let grid = f.at(VfD0Frame::GRID);
    host.ray_seeds(f.s - 4, grid);
    if (store.entity_flags(other) as i8) < 0 {
        let eax = host.collide_init(f.s - 0x10, other, this, list);
        host.ray_clear(f.s - 4, grid);
        return eax;
    }
    if host.grid_active(f.s - 4, other) == 0 {
        host.ray_clear(f.s - 4, grid);
        return 0;
    }
    let transform = host.transform(f.s - 4, other);
    host.fetch_y(f.s - 0xc, matrix, grid, transform);
    copy_vec4(store, f.at(VfD0Frame::FROM), grid + 0x20);
    if store.world_flags() & WORLD_POINT_CONTACTS != 0
        && (store.entity_flags(other) & ENTITY_FLAG_VF_F0 != 0 || host.grid(f.s - 4, other) == 0)
    {
        let result = slot_contacts(host, store, &f, this, other, list);
        host.ray_clear(f.s - 4, grid);
        return result;
    }
    // The pushes interleave with the calls: the first transform is M6610's last argument.
    let other_grid = f.at(VfD0Frame::OTHER_GRID);
    let last = host.transform(f.s - 4, other);
    let middle = host.transform(f.s - 8, other);
    let src = host.grid(f.s - 0xc, other);
    host.grid_copy(f.s - 0x10, other_grid, src, middle, last);
    copy_vec4(store, f.at(VfD0Frame::BACK), other_grid + 0x20);
    let away = host.vf80(f.s - 4, other) != 0 || host.vf80(f.s - 4, this) == 0;
    let mut result = 0;
    if store.world_flags() & WORLD_POINT_CONTACTS != 0 {
        let n = u32::from(store.grid_edge_count(grid));
        for i in 0..n {
            result |= point_contact(host, store, &f, this, other, list, i, away);
        }
        if result != 0 {
            host.ray_clear(f.s - 4, other_grid);
            host.ray_clear(f.s - 4, grid);
            return result;
        }
    }
    if store.world_flags() & WORLD_EDGE_CONTACTS != 0 {
        if store.grid_status_flag(other_grid) & 2 != 0
            || store.grid_edge_count(other_grid) == 0
            || store.grid_edge_count(grid) == 0
        {
            host.ray_clear(f.s - 4, other_grid);
            host.ray_clear(f.s - 4, grid);
            return 0;
        }
        let m = u32::from(store.grid_pair_count(grid));
        for i in 0..m {
            result |= edge_contact(host, store, &f, this, other, list, i, away);
        }
    }
    host.ray_clear(f.s - 4, other_grid);
    host.ray_clear(f.s - 4, grid);
    result
}

/// VfD0's walk through the other entity's slot 0xf0: each grid point against the grid's centre. A hit's point is
/// fetched into this body's frame and becomes a point event.
fn slot_contacts<S: CollisionStore>(
    host: &mut impl VfD0Host,
    store: &mut S,
    f: &VfD0Frame,
    this: u32,
    other: u32,
    list: u32,
) -> u32 {
    let grid = f.at(VfD0Frame::GRID);
    let (desc_a, desc_b) = (f.at(VfD0Frame::DESC_A), f.at(VfD0Frame::DESC_B));
    let transform = host.transform(f.s - 4, other);
    host.combine(f.s - 0xc, desc_a, f.at(VfD0Frame::FROM), transform);
    let n = u32::from(store.grid_edge_count(grid));
    let mut result = 0;
    for i in 0..n {
        let transform = host.transform(f.s - 4, other);
        host.combine_twin(f.s - 0xc, desc_b, store.grid_edge(grid, i), transform);
        let material = f.at(VfD0Frame::MATERIAL);
        let args = [
            desc_a,
            desc_b,
            f.at(VfD0Frame::HIT),
            f.at(VfD0Frame::U),
            f.at(VfD0Frame::NORMAL),
            f.at(VfD0Frame::T),
            material,
            material,
        ];
        if host.vf_f0(f.s - 0x24, other, args) == 0 {
            continue;
        }
        let transform = host.transform(f.s - 4, this);
        host.fetch_x(f.s - 0xc, desc_b, f.at(VfD0Frame::AT), transform);
        let event = make_event(host, f, ContactKind::Point, this, other, f.at(VfD0Frame::AT));
        let (first, second) = store.event_sample_slots(event);
        host.sample_b(f.s - 0x10, f.at(VfD0Frame::GRID), i, first, second);
        host.append(f.s - 8, list, event);
        result = 1;
    }
    result
}

/// VfD0's point contact for grid point `i`: `EdgeEdge` from the grid's centre to the point. When the reverse test hits
/// too, the contact moves to the midpoint of the two hits and is tested again along the flat `probe_direction`, its
/// normal that direction with `plane_offset`; else it stays at the point. Returns 1 for an event.
#[allow(clippy::too_many_arguments)]
fn point_contact<S: CollisionStore>(
    host: &mut impl VfD0Host,
    store: &mut S,
    f: &VfD0Frame,
    this: u32,
    other: u32,
    list: u32,
    i: u32,
    away: bool,
) -> u32 {
    let point = store.grid_edge(f.at(VfD0Frame::GRID), i);
    let (from, mid) = (f.at(VfD0Frame::FROM), f.at(VfD0Frame::DESC_B));
    let t = f.at(VfD0Frame::T);
    if edge_test(host, f, from, point, VfD0Frame::HIT, t, VfD0Frame::NORMAL) == 0 {
        return 0;
    }
    let at = if edge_test(host, f, point, from, VfD0Frame::HIT2, t, VfD0Frame::NORMAL2) != 0 {
        if !probe(host, store, f, mid, t, away, ContactKind::Point) {
            return 0;
        }
        mid
    } else {
        point
    };
    let this_transform = host.transform(f.s - 8, this);
    let transform = host.transform(f.s - 0x10, other);
    host.compose_a(f.s - 0x10, transform, at, this_transform, f.at(VfD0Frame::AT));
    to_world(host, store, f, other);
    let event = make_event(host, f, ContactKind::Point, this, other, f.at(VfD0Frame::AT));
    let (first, second) = store.event_sample_slots(event);
    host.sample_b(f.s - 0x10, f.at(VfD0Frame::GRID), i, first, second);
    host.append(f.s - 8, list, event);
    1
}

/// VfD0's edge contact for grid edge record `i`: `EdgeEdge` along the edge both ways, then the probe from the midpoint
/// of the hits along `probe_direction`. The event is at the midpoint, sampled at the hit's `edge_ratio`. Returns 1 for
/// an event.
#[allow(clippy::too_many_arguments)]
fn edge_contact<S: CollisionStore>(
    host: &mut impl VfD0Host,
    store: &mut S,
    f: &VfD0Frame,
    this: u32,
    other: u32,
    list: u32,
    i: u32,
    away: bool,
) -> u32 {
    let grid = f.at(VfD0Frame::GRID);
    let [first, second, ..] = store.grid_pair(grid, i);
    let (start, end) = (store.grid_edge(grid, u32::from(first)), store.grid_edge(grid, u32::from(second)));
    let (mid, u) = (f.at(VfD0Frame::EDGE_MID), f.at(VfD0Frame::U));
    if edge_test(host, f, start, end, VfD0Frame::HIT, u, VfD0Frame::NORMAL) == 0
        || edge_test(host, f, end, start, VfD0Frame::HIT2, u, VfD0Frame::NORMAL2) == 0
        || !probe(host, store, f, mid, u, away, ContactKind::Edge)
    {
        return 0;
    }
    let span = host.sub(f.s - 0xc, end, f.at(VfD0Frame::EDGE_SPAN), start);
    let edge_length = store.vec3(span).length3();
    let span = host.sub(f.s - 0xc, f.at(VfD0Frame::HIT), f.at(VfD0Frame::HIT_SPAN), start);
    let hit_length = store.vec3(span).length3();
    let ratio = collide_big::edge_ratio(edge_length, hit_length);
    let transform = host.transform(f.s - 0x10, other);
    let hit = f.at(VfD0Frame::HIT);
    host.compose_a(f.s - 0x10, transform, hit, store.frame_root(), hit);
    let this_transform = host.transform(f.s - 8, this);
    let transform = host.transform(f.s - 0x10, other);
    host.compose_a(f.s - 0x10, transform, mid, this_transform, mid);
    let normal = f.at(VfD0Frame::NORMAL);
    let transform = host.transform(f.s - 0x10, other);
    host.compose_b(f.s - 0x10, transform, normal, store.frame_root(), normal);
    let event = make_event(host, f, ContactKind::Edge, this, other, mid);
    let (first, second) = store.event_sample_slots(event);
    host.sample_a(f.s - 0x14, f.at(VfD0Frame::GRID), i, ratio, first, second);
    host.append(f.s - 8, list, event);
    1
}

/// One `EdgeEdge` of VfD0 on the other entity's grid: hit and normal into the frame at `hit` and `normal`.
fn edge_test(host: &mut impl VfD0Host, f: &VfD0Frame, from: u32, to: u32, hit: u32, t: u32, normal: u32) -> u32 {
    host.edge_edge(
        f.s - 0x1c,
        f.at(VfD0Frame::OTHER_GRID),
        from,
        to,
        f.at(hit),
        t,
        f.at(normal),
        f.at(VfD0Frame::MATERIAL),
    )
}

/// The midpoint of the two hits to `mid`, the normalised `probe_direction` to the frame's direction, and the probe
/// `EdgeEdge` from `mid + dir` back to `mid`. On a hit, the normal is the direction with its `plane_offset` through the
/// new hit. False when the probe misses.
fn probe<S: CollisionStore>(
    host: &mut impl VfD0Host,
    store: &mut S,
    f: &VfD0Frame,
    mid: u32,
    t: u32,
    away: bool,
    kind: ContactKind,
) -> bool {
    let (hit, dir) = (f.at(VfD0Frame::HIT), f.at(VfD0Frame::DIR));
    let centre = collide_big::midpoint(store.vec3(hit), store.vec3(f.at(VfD0Frame::HIT2)));
    store.store_vec3(mid, centre);
    let back = store.vec3(f.at(VfD0Frame::BACK));
    let from = store.vec3(f.at(VfD0Frame::FROM));
    store.store_vec3(dir, collide_big::probe_direction(centre, back, from, away, kind));
    host.normalize(f.s - 4, dir);
    let probe = host.add(f.s - 0x20, mid, f.at(VfD0Frame::PROBE), dir);
    if edge_test(host, f, probe, mid, VfD0Frame::HIT, t, VfD0Frame::NORMAL) == 0 {
        return false;
    }
    let dir = store.vec3(dir);
    let normal = f.at(VfD0Frame::NORMAL);
    store.store_vec3(normal, dir);
    store.set_f32(normal + 12, collide_big::plane_offset(dir, store.vec3(hit)));
    true
}

/// A point contact's hit and normal through the other entity's transform into the world (`ComposeA`, `ComposeB`).
fn to_world<S: CollisionStore>(host: &mut impl VfD0Host, store: &mut S, f: &VfD0Frame, other: u32) {
    let (hit, normal) = (f.at(VfD0Frame::HIT), f.at(VfD0Frame::NORMAL));
    let transform = host.transform(f.s - 0x10, other);
    host.compose_a(f.s - 0x10, transform, hit, store.frame_root(), hit);
    let transform = host.transform(f.s - 0x10, other);
    host.compose_b(f.s - 0x10, transform, normal, store.frame_root(), normal);
}

fn make_event(host: &mut impl VfD0Host, f: &VfD0Frame, kind: ContactKind, this: u32, other: u32, at: u32) -> u32 {
    let (hit, normal) = (f.at(VfD0Frame::HIT), f.at(VfD0Frame::NORMAL));
    host.make_event(f.s - 0x28, kind, hit, this, other, at, normal)
}

/// CBody::VfDC's frame (0x4e2850), from the stack pointer `S` after its prologue: `push ebp`, the 16-byte alignment,
/// the exception record, 0x14c4 bytes of locals (`_chkstk`), then `ebx`, `esi` and `edi`.
struct VfDcFrame {
    s: u32,
}

impl VfDcFrame {
    const SIZE: u32 = 0x14e0;
    /// EdgePair's t and the word after it, which it is given but does not write.
    const T: u32 = 0x14;
    const SCRATCH: u32 = 0x18;
    /// The overlap box: its corners, centre and half extent.
    const HI: u32 = 0x20;
    const LO: u32 = 0x30;
    const CENTRE: u32 = 0x40;
    const HIT: u32 = 0x50;
    const HALF: u32 = 0x60;
    const NORMAL: u32 = 0x70;
    /// The two fetched grids' boxes, joined.
    const UNION_LO: u32 = 0x80;
    const UNION_HI: u32 = 0x90;
    const AT: u32 = 0xa0;
    /// This body's grid through `b` and through `a` (`FetchY`), and the other entity's grid (`M6610`).
    const GRID_B: u32 = 0xb0;
    const GRID_A: u32 = 0x760;
    const OTHER_GRID: u32 = 0xe10;

    fn at(&self, offset: u32) -> u32 {
        self.s + offset
    }
}

/// CBody::VfDC (0x4e2850) on the body `this` against the entity `other`, with the matrices `a` and `b` and the event
/// list `list`; `esp` is its entry stack pointer. Returns eax: 1 when it appended an event.
///
/// This body's grid goes into the other entity's frame twice (through `a` and `b`) and the other's grid is copied
/// (`M6610`). Each of this body's edge pairs whose edge (through `b`) meets the box where the three grids overlap is
/// tested by `EdgePair` (`vf_dc_pair`).
#[allow(clippy::too_many_arguments)]
pub fn body_vf_dc<S: CollisionStore>(
    host: &mut impl VfDcHost,
    store: &mut S,
    this: u32,
    a: u32,
    b: u32,
    other: u32,
    list: u32,
    esp: u32,
) -> u32 {
    let f = VfDcFrame { s: ((esp - 4) & !0xf) - VfDcFrame::SIZE };
    let (grid_a, grid_b) = (f.at(VfDcFrame::GRID_A), f.at(VfDcFrame::GRID_B));
    host.ray_seeds(f.s - 4, grid_a);
    host.ray_seeds(f.s - 4, grid_b);
    let src = host.grid(f.s - 4, other);
    let transform = host.vf34(f.s - 4, other);
    host.fetch_y(f.s - 0xc, a, grid_a, transform);
    let transform = host.transform(f.s - 4, other);
    host.fetch_y(f.s - 0xc, b, grid_b, transform);
    if store.body_grid(this) == 0 || src == 0 || store.grid_edge_count(src) == 0 || store.grid_edge_count(grid_a) == 0 {
        host.ray_clear(f.s - 4, grid_b);
        host.ray_clear(f.s - 4, grid_a);
        return 0;
    }
    // The pushes interleave with the calls: the first transform is M6610's last argument.
    let other_grid = f.at(VfDcFrame::OTHER_GRID);
    let last = host.transform(f.s - 4, other);
    let middle = host.transform(f.s - 8, other);
    host.grid_copy(f.s - 0x10, other_grid, src, middle, last);
    overlap_box(store, &f);
    let grid = store.body_grid(this);
    let pairs = store.grid_pair_count(grid);
    let mut result = 0;
    for i in 0..u32::from(pairs) {
        result |= vf_dc_pair(host, store, &f, this, other, list, i);
    }
    host.ray_clear(f.s - 4, other_grid);
    host.ray_clear(f.s - 4, grid_b);
    host.ray_clear(f.s - 4, grid_a);
    result
}

/// VfDC's overlap box (name ours): the join of the two fetched grids' boxes (`E33Vec4::Min`, `Max`), met with the other
/// grid's box, to the frame's corners, centre and half extent.
fn overlap_box<S: CollisionStore>(store: &mut S, f: &VfDcFrame) {
    let (a, b) = (f.at(VfDcFrame::GRID_A), f.at(VfDcFrame::GRID_B));
    let other = f.at(VfDcFrame::OTHER_GRID);
    let union_lo = store.vec4(a).min_lanes(store.vec4(b));
    store.store_vec4(f.at(VfDcFrame::UNION_LO), union_lo);
    let union_hi = store.vec4(a + 0x10).max_lanes(store.vec4(b + 0x10));
    store.store_vec4(f.at(VfDcFrame::UNION_HI), union_hi);
    let lo = store.vec4(other).max_lanes(union_lo);
    store.store_vec4(f.at(VfDcFrame::LO), lo);
    let hi = store.vec4(other + 0x10).min_lanes(union_hi);
    store.store_vec4(f.at(VfDcFrame::HI), hi);
    let (centre, half) = collide_big::overlap_centre_half(
        lo,
        hi,
        store.f32(f.at(VfDcFrame::CENTRE) + 12),
        store.f32(f.at(VfDcFrame::HALF) + 12),
    );
    store.store_vec4(f.at(VfDcFrame::CENTRE), centre);
    store.store_vec4(f.at(VfDcFrame::HALF), half);
}

/// VfDC's edge pair `i`: its edge through `b` against the overlap box (`BoxHit`, 0x4e0e40), then `EdgePair` on the
/// other grid with the edge through `a` and through `b`. A hit and its normal go into the world through the other's
/// transform; the contact point into this body's frame (`AT`). The event takes the grid's two flag bytes, which are
/// cleared. Returns 1 for an event.
fn vf_dc_pair<S: CollisionStore>(
    host: &mut impl VfDcHost,
    store: &mut S,
    f: &VfDcFrame,
    this: u32,
    other: u32,
    list: u32,
    i: u32,
) -> u32 {
    let (grid_a, grid_b) = (f.at(VfDcFrame::GRID_A), f.at(VfDcFrame::GRID_B));
    let [b0, b1, ..] = store.grid_pair(grid_b, i);
    let (from, to) = (store.grid_edge(grid_b, u32::from(b0)), store.grid_edge(grid_b, u32::from(b1)));
    if !crate::collision::overlap::segment_overlaps_box(
        store.vec4(from),
        store.vec4(to),
        store.vec4(f.at(VfDcFrame::CENTRE)),
        store.vec4(f.at(VfDcFrame::HALF)),
    ) {
        return 0;
    }
    let [a0, a1, ..] = store.grid_pair(grid_a, i);
    let other_grid = f.at(VfDcFrame::OTHER_GRID);
    let (hit, normal, at) = (f.at(VfDcFrame::HIT), f.at(VfDcFrame::NORMAL), f.at(VfDcFrame::AT));
    let args = [
        store.grid_edge(grid_a, u32::from(a0)),
        store.grid_edge(grid_a, u32::from(a1)),
        from,
        to,
        hit,
        f.at(VfDcFrame::T),
        normal,
        f.at(VfDcFrame::SCRATCH),
        other_grid,
        0,
    ];
    if host.edge_pair(f.s - 0x2c, other_grid, args) == 0 {
        return 0;
    }
    let identity = store.frame_root();
    let transform = host.transform(f.s - 0x10, other);
    host.compose_a(f.s - 0x10, transform, hit, identity, hit);
    let identity = store.frame_root();
    let transform = host.transform(f.s - 0x10, other);
    host.compose_b(f.s - 0x10, transform, normal, identity, normal);
    let this_transform = host.transform(f.s - 8, this);
    host.compose_a(f.s - 0x10, store.frame_root(), hit, this_transform, at);
    let t = store.u32(f.at(VfDcFrame::T));
    let event = host.pair_event(f.s - 0x28, hit, t, this, other, at, normal);
    let (flag_688, flag_689) = store.take_body_grid_flags(this);
    store.store_event_samples(event, flag_689, flag_688);
    host.append(f.s - 8, list, event);
    1
}

/// E01Vec4::CopyFrom (0x43c5b0): the sixteen bytes at `src` go to `dst`, read whole before the store.
fn copy_vec4<S: CollisionStore>(store: &mut S, dst: u32, src: u32) {
    let value = store.vec4(src);
    store.store_vec4(dst, value);
}
