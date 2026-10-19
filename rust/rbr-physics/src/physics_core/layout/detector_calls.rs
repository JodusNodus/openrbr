//! The collision detectors (`CL1`/`CL2` detectors, `E46_Kernel5bd0`, `PenCheck`, `Visit470`, `Work6a70`) and the
//! edge-sweep words they read and write.

use super::globals::physics;
use super::lists::copy_vec4;
use super::step_forward::{STEP_NODE_NEXT_AT, STEP_SCRATCH_AT, STEP_STATE_AT};
use super::vectors::{set_vec3s, set_vec4s, vec3s};
use crate::collision::layout::Vec4Pair;
use crate::entities::layout::ROOT_IDENTITY;
use crate::mem::Mem;
use crate::physics_core::detect::{EdgeBest, Xy, EDGE_START_DIST, L2_PENETRATION_VTABLE};
use crate::physics_core::detectors::{
    all_below_zero, kernel_box, kernel_skips, DetectorHost, KernelTest, KERNEL_MAX_HITS,
};

/// A 2-D point: the first two floats of the vector at `at`.
pub fn xy(mem: &Mem, at: u32) -> Xy {
    Xy { x: mem.f32(at), y: mem.f32(at + 4) }
}

/// Writes the four cell handles of a box (`out[0..4]`).
pub fn set_cells(mem: &mut Mem, at: u32, cells: [u32; 4]) {
    for (k, cell) in cells.into_iter().enumerate() {
        mem.set_u32(at + 4 * k as u32, cell);
    }
}

/// The pairs of an edge list: a count at `list`, then the ids, and the pair objects from `list + 0x30`, 0x6b0 bytes
/// apart. Each pair is (the pair object, its id).
pub fn edge_pairs(mem: &Mem, list: u32) -> Vec<(u32, u32)> {
    (0..mem.u32(list)).map(|i| (list + 0x30 + 0x6b0 * i, mem.u32(list + 4 + 4 * i))).collect()
}

/// Writes the result object of an edge sweep (`out`): the distance (`+0x30`, the start value 1.01 when nothing hit),
/// zeros at `+0x34` and `+0x38`, and, on a hit, the hit point (`+0`), the normal (`+0x10`), 1.0 and 0.0 (`+0x20` and
/// `+0x24`, meanings not known), the hit word (`+0x28` and `+0x2c`) and the distance.
pub fn set_edge_result(mem: &mut Mem, out: u32, best: Option<EdgeBest>) {
    let dist = best.map_or(EDGE_START_DIST, |best| best.hit.dist);
    if let Some(best) = best {
        let hit = best.hit;
        set_vec4s(mem, out, &[hit.point]);
        set_vec4s(mem, out + 0x10, &[hit.normal]);
        mem.set_f32(out + 0x20, 1.0);
        mem.set_f32(out + 0x24, 0.0);
        mem.set_u32(out + 0x28, hit.hit_word);
        mem.set_u32(out + 0x2c, hit.hit_word);
    }
    mem.set_f32(out + 0x30, dist);
    mem.set_u32(out + 0x34, 0);
    mem.set_u32(out + 0x38, 0);
}

/// A collision grid's word that disables it (`E37Get67C`, 0x4f7e40) and its byte the kernel tests (+0x68d).
const GRID_DISABLED_AT: u32 = 0x67c;
const GRID_SOLID_AT: u32 = 0x68d;
/// An entity's flag word (+0x60) and a cell node's entity (+0xc).
const ENTITY_FLAGS_AT: u32 = 0x60;
const CELL_NODE_ENTITY_AT: u32 = 0xc;
/// A state's entity lists the detectors walk: +0x450 (the bodies) and +0x470.
pub(super) const STATE_BODIES_AT: u32 = 0x450;
const STATE_LIST470_AT: u32 = 0x470;
/// The detectors' vtables: `CL2GiganticPenetrationDetector`, `CL2PenetrationDetector` (its destructor stores it back),
/// `E46L2Detector` and `E46L1PenDetector`.
const GIANT_VTABLE: u32 = 0x0073_f5d0;
const L2_VTABLE: u32 = 0x0073_f5a8;
const L1_PEN_VTABLE: u32 = 0x0073_f5b4;

/// The `[esp]` of a detector's body: `push ebp; mov ebp, esp; and esp, -0x10`, the three words of its SEH record,
/// `sub esp, locals` and three saved registers. The frame offsets below are from it.
fn detector_frame(entry: u32, locals: u32) -> u32 {
    ((entry - 4) & !0xf) - 0xc - locals - 0xc
}

/// Whether `grid` is a collision grid with its disabling word set (`E37Get67C`).
fn grid_disabled(mem: &Mem, grid: u32) -> bool {
    grid != 0 && mem.u32(grid + GRID_DISABLED_AT) != 0
}

/// `E33Mat34::Combine` and `E43PairWalk::MD70` (0x51ab90, 0x51ad70: the same code): the pair at `at` holds `first`
/// and `second`; returns `at`.
fn store_pair(mem: &mut Mem, at: u32, first: u32, second: u32) -> u32 {
    mem.set_u32(at, first);
    mem.set_u32(at + 4, second);
    at
}

/// `CL2GiganticPenetrationDetector::Vf04` (0x5242c0) on the detector at `this` (+4 its owner, +0x3c its word) with
/// `other`; `entry` is its entry esp. The gates (not the owner itself, the owner's grid, `other`'s grid enabled, the
/// owner's exclusion test, `other`'s slot 0x110), the grid ops of the two entities' frames (0x34 at +0x6f0, 0x30 at
/// +0x40), their box (`MinMaxA` to +0x30 and +0x20, paired with `other` at +0x18 and +0x10), and on an overlap the
/// owner's response with the pair (+0x18: the grid ops at +0x40 and `other`).
pub fn giant_penetration_detect(mem: &mut Mem, host: &mut impl DetectorHost, this: u32, other: u32, entry: u32) {
    const LOCALS: u32 = 0xda8;
    const PAIR_HI: u32 = 0x10;
    const PAIR_LO: u32 = 0x18;
    const HI: u32 = 0x20;
    const LO: u32 = 0x30;
    const OPS: u32 = 0x40;
    const OLD_OPS: u32 = 0x6f0;
    let b = detector_frame(entry, LOCALS);
    let owner = mem.u32(this + 4);
    if owner == other || host.collision_grid(b - 4, owner) == 0 {
        return;
    }
    let grid = host.collision_grid(b - 4, other);
    if grid_disabled(mem, grid) || host.exclusion_test(b - 8, owner, other) == 0 || host.entity_110(b - 4, other) == 0 {
        return;
    }
    let owner = mem.u32(this + 4);
    let other_frame = host.entity_34(b - 4, other);
    let frame = host.entity_34(b - 8, owner);
    let grid = host.collision_grid(b - 0xc, owner);
    host.grid_ops(b - 0x10, b + OLD_OPS, grid, frame, other_frame);
    let owner = mem.u32(this + 4);
    let other_frame = host.entity_30(b - 4, other);
    let frame = host.entity_30(b - 8, owner);
    let grid = host.collision_grid(b - 0xc, owner);
    host.grid_ops(b - 0x10, b + OPS, grid, frame, other_frame);
    host.min_max(b - 0x10, b + OLD_OPS, b + OPS, b + LO, b + HI);
    let hi = store_pair(mem, b + PAIR_HI, b + HI, other);
    let lo = store_pair(mem, b + PAIR_LO, b + LO, other);
    if host.overlaps_box(b - 0xc, other, lo, hi) != 0 {
        let extra = mem.u32(this + 0x3c);
        let pair = store_pair(mem, b + PAIR_LO, b + OPS, other);
        let owner = mem.u32(this + 4);
        host.entity_d0(b - 0x10, owner, pair, other, extra);
    }
    host.ray_clear(b - 4, b + OPS);
    host.ray_clear(b - 4, b + OLD_OPS);
}

/// `E46L1PenDetector::Vf04` (0x525a20) on the detector at `this` (+4 the scale word, +8 a word, +0xc the state) with
/// `other`; `entry` is its entry esp. With `other`'s grid enabled and its slot 0x10c set: its box (+0x30, +0x20), a
/// `CL2PenetrationDetector` at +0x90 on it, notified by the box's cells (+0x80); then for each entity of the state's
/// +0x470 list a `CL2GiganticPenetrationDetector` at +0x40 (owner `other`, the box, the two words) runs on the entity
/// and on `other`'s children. The L2 detector's destructor ends it.
pub fn l1_penetration_detect(mem: &mut Mem, host: &mut impl DetectorHost, this: u32, other: u32, entry: u32) {
    const LOCALS: u32 = 0x788;
    const HI: u32 = 0x20;
    const LO: u32 = 0x30;
    const GIANT: u32 = 0x40;
    const CELLS: u32 = 0x80;
    const L2: u32 = 0x90;
    const L2_OPS: u32 = 0x30;
    let b = detector_frame(entry, LOCALS);
    let grid = host.collision_grid(b - 4, other);
    if grid_disabled(mem, grid) || host.entity_10c(b - 4, other) == 0 {
        return;
    }
    host.entity_box(b - 0xc, other, b + LO, b + HI);
    let (scale, word) = (mem.u32(this + 4), mem.u32(this + 8));
    host.penetration_ctor(b - 0x18, b + L2, other, b + LO, b + HI, scale, word);
    host.cells(b - 0x10, b + LO, b + HI, b + CELLS);
    for k in 0..4 {
        let cell = mem.u32(b + CELLS + 4 * k);
        if cell != 0 {
            host.notify_filtered(b - 8, cell, b + L2);
        }
    }
    let mut node = mem.u32(mem.u32(this + 0xc) + STATE_LIST470_AT);
    while node != 0 {
        let giant = b + GIANT;
        let (scale, word) = (mem.u32(this + 4), mem.u32(this + 8));
        mem.set_u32(giant, GIANT_VTABLE);
        mem.set_u32(giant + 4, other);
        copy_vec4(mem, b + LO, giant + 0x10);
        copy_vec4(mem, b + HI, giant + 0x20);
        mem.set_u32(giant + 0x30, 0);
        mem.set_u32(giant + 0x34, 0);
        mem.set_u32(giant + 0x38, scale);
        mem.set_u32(giant + 0x3c, word);
        host.detect(b - 8, giant, node);
        host.walk(b - 8, other, giant);
        node = mem.u32(node + 0x10);
    }
    mem.set_u32(b + L2, L2_PENETRATION_VTABLE);
    host.ray_clear(b - 4, b + L2 + L2_OPS);
}

/// The stack arguments of `E46_Kernel5bd0`.
#[derive(Clone, Copy, Debug)]
pub struct KernelArgs {
    pub a: u32,
    pub b: u32,
    pub mask: u32,
    pub excluded: u32,
    pub out: u32,
}

/// `E46_Kernel5bd0` (0x525bd0, stdcall) with the vectors `a` and `b`, the flag mask, the excluded entity and the output
/// at `out`; `entry` is its entry esp. The box `a - b`, `a + b` (+0x30, +0x40, paired with the identity at +0x28 and
/// +0x20) and its cells (+0x50); each cell's entities, up to `KERNEL_MAX_HITS`, that pass the mask, the exclusion,
/// their grid (set, its byte +0x68d set) and the overlap (slot 0xcc) go to `out` (+0 the count, +4 the entities, the
/// grid ops of their frame 0x30 from +0x30, 0x6b0 bytes each). Returns whether any did (eax).
pub fn kernel_collect(mem: &mut Mem, host: &mut impl DetectorHost, args: KernelArgs, entry: u32) -> bool {
    let KernelArgs { a, b, mask, excluded, out } = args;
    const LOCALS: u32 = 0x58;
    const PAIR_HI: u32 = 0x20;
    const PAIR_LO: u32 = 0x28;
    const LO: u32 = 0x30;
    const HI: u32 = 0x40;
    const CELLS: u32 = 0x50;
    const OUT_ENTITIES: u32 = 4;
    const OUT_RECORDS: u32 = 0x30;
    const RECORD_SIZE: u32 = 0x6b0;
    if out == 0 {
        return false;
    }
    let f = detector_frame(entry, LOCALS);
    mem.set_u32(out, 0);
    let [va, vb] = [a, b].map(|at| vec3s::<1>(mem, at)[0]);
    let (lo, hi) = kernel_box(va, vb);
    set_vec3s(mem, f + LO, &[lo]);
    set_vec3s(mem, f + HI, &[hi]);
    host.cells(f - 0x10, f + LO, f + HI, f + CELLS);
    store_pair(mem, f + PAIR_LO, f + LO, mem.u32(ROOT_IDENTITY));
    store_pair(mem, f + PAIR_HI, f + HI, mem.u32(ROOT_IDENTITY));
    for k in 0..4 {
        let cell = mem.u32(f + CELLS + 4 * k);
        if cell == 0 {
            continue;
        }
        let mut node = mem.u32(cell);
        while node != 0 && mem.u32(out) < KERNEL_MAX_HITS {
            let pass = match host.kernel_tests(mem, f, node, mask, excluded) {
                Some(KernelTest::Fail { next }) => {
                    node = next;
                    continue;
                }
                Some(KernelTest::Pass) => true,
                None => {
                    let entity = mem.u32(node + CELL_NODE_ENTITY_AT);
                    let flags = mem.u32(entity + ENTITY_FLAGS_AT);
                    !kernel_skips(flags, mask, entity, excluded)
                        && host.collision_grid(f - 4, entity) != 0
                        && mem.u8(host.collision_grid(f - 4, mem.u32(node + CELL_NODE_ENTITY_AT)) + GRID_SOLID_AT) != 0
                        && host.overlaps_box(f - 0xc, mem.u32(node + CELL_NODE_ENTITY_AT), f + PAIR_LO, f + PAIR_HI)
                            != 0
                }
            };
            if pass {
                let n = mem.u32(out);
                mem.set_u32(out + OUT_ENTITIES + 4 * n, mem.u32(node + CELL_NODE_ENTITY_AT));
                mem.set_u32(out, n + 1);
                let record = out + n * RECORD_SIZE + OUT_RECORDS;
                if record != 0 {
                    let entity = mem.u32(node + CELL_NODE_ENTITY_AT);
                    let identity = mem.u32(ROOT_IDENTITY);
                    let frame = host.entity_30(f - 8, entity);
                    let grid = host.collision_grid(f - 0xc, entity);
                    host.grid_ops(f - 0x10, record, grid, frame, identity);
                }
            }
            node = mem.u32(node);
        }
    }
    mem.u32(out) != 0
}

/// What `E46_Kernel5bd0`'s tests read of the entity at a cell node: the next node, the entity, its flags, its vtable's
/// slots 0x118 (the grid getter) and 0xcc (the box test), its body grid (+0x64) and that grid's solid byte, and its body
/// bounds (+0x80); the addresses read are in `reads`. For a cache of the tests (a host's
/// [`DetectorHost::kernel_tests`]).
#[derive(Clone, Copy, Debug)]
pub struct KernelEntity {
    pub next: u32,
    pub entity: u32,
    pub flags: u32,
    pub slot_118: u32,
    pub slot_cc: u32,
    pub grid: u32,
    pub solid: u8,
    pub bounds: Option<Vec4Pair>,
    pub reads: [u32; 10],
}

/// [`KernelEntity`] of the cell node `node`, read from the pages alone (`Mem::paged_u32`): `None` when a word other
/// than the grid's or the bounds' is not there. The grid's byte is read when the grid is set, the bounds as they are
/// there.
pub fn kernel_entity(mem: &Mem, node: u32) -> Option<KernelEntity> {
    use crate::collision::layout::{Vec4, Vec4Pair};
    const BODY_GRID_AT: u32 = 0x64;
    const BODY_BOUNDS_AT: u32 = 0x80;
    let next = mem.paged_u32(node)?;
    let entity = mem.paged_u32(node + CELL_NODE_ENTITY_AT)?;
    let flags = mem.paged_u32(entity + ENTITY_FLAGS_AT)?;
    let vtable = mem.paged_u32(entity)?;
    let slot_118 = mem.paged_u32(vtable + 0x118)?;
    let slot_cc = mem.paged_u32(vtable + 0xcc)?;
    let grid = mem.paged_u32(entity + BODY_GRID_AT)?;
    let solid_at = grid.wrapping_add(GRID_SOLID_AT);
    let solid = match grid {
        0 => 0,
        _ => (mem.paged_u32(solid_at & !3)? >> (8 * (solid_at % 4))) as u8,
    };
    let mut lanes = [0; 8];
    let mapped = (0..8).all(|k| mem.paged_u32(entity + BODY_BOUNDS_AT + 4 * k as u32).map(|w| lanes[k] = w).is_some());
    let bounds = mapped.then_some(lanes).map(|w| {
        let v = |k: usize| Vec4 {
            x: f32::from_bits(w[k]),
            y: f32::from_bits(w[k + 1]),
            z: f32::from_bits(w[k + 2]),
            w: f32::from_bits(w[k + 3]),
        };
        Vec4Pair { lower: v(0), upper: v(4) }
    });
    let reads = [
        node,
        node + CELL_NODE_ENTITY_AT,
        entity,
        entity + ENTITY_FLAGS_AT,
        entity + BODY_GRID_AT,
        vtable + 0x118,
        vtable + 0xcc,
        if grid == 0 { entity } else { solid_at },
        entity + BODY_BOUNDS_AT,
        entity + BODY_BOUNDS_AT + 0x1f,
    ];
    Some(KernelEntity { next, entity, flags, slot_118, slot_cc, grid, solid, bounds, reads })
}

/// `CL1CollisionDetector::Vf04` (0x526ad0) on the detector at `this` (+4 a float word, +8 a word) with `other`; `entry`
/// is its entry esp. With `other`'s grid enabled and its slot 0x10c set: the grid ops of its frames (0x34 at +0x730,
/// 0x30 at +0x80), its box (+0x40, +0x50, paired with the identity at +0x70 and +0x78), and an `E46L2Detector` at
/// +0x18 (`other`, the two pairs, the two grid ops, the two words) notified by the box's cells (+0x60) and visited over
/// the physics' +0x470 list.
pub fn l1_collision_detect(mem: &mut Mem, host: &mut impl DetectorHost, this: u32, other: u32, entry: u32) {
    const LOCALS: u32 = 0xde8;
    const L2: u32 = 0x18;
    const LO: u32 = 0x40;
    const HI: u32 = 0x50;
    const CELLS: u32 = 0x60;
    const PAIR_LO: u32 = 0x70;
    const PAIR_HI: u32 = 0x78;
    const OPS: u32 = 0x80;
    const OLD_OPS: u32 = 0x730;
    let b = detector_frame(entry, LOCALS);
    let grid = host.collision_grid(b - 4, other);
    if grid_disabled(mem, grid) || host.entity_10c(b - 4, other) == 0 {
        return;
    }
    let identity = mem.u32(ROOT_IDENTITY);
    let frame = host.entity_34(b - 8, other);
    let grid = host.collision_grid(b - 0xc, other);
    host.grid_ops(b - 0x10, b + OLD_OPS, grid, frame, identity);
    let identity = mem.u32(ROOT_IDENTITY);
    let frame = host.entity_30(b - 8, other);
    let grid = host.collision_grid(b - 0xc, other);
    host.grid_ops(b - 0x10, b + OPS, grid, frame, identity);
    host.entity_box(b - 0xc, other, b + LO, b + HI);
    let (scale, word) = (mem.u32(this + 4), mem.u32(this + 8));
    let hi = store_pair(mem, b + PAIR_HI, b + HI, mem.u32(ROOT_IDENTITY));
    let lo = store_pair(mem, b + PAIR_LO, b + LO, mem.u32(ROOT_IDENTITY));
    let l2 = b + L2;
    mem.set_u32(l2, L2_VTABLE);
    mem.set_u32(l2 + 4, other);
    for (k, pair) in [lo, hi].into_iter().enumerate() {
        let to = l2 + 8 + 8 * k as u32;
        mem.set_u32(to, mem.u32(pair));
        mem.set_u32(to + 4, mem.u32(pair + 4));
    }
    mem.set_u32(l2 + 0x18, b + OLD_OPS);
    mem.set_u32(l2 + 0x1c, b + OPS);
    mem.set_u32(l2 + 0x20, scale);
    mem.set_u32(l2 + 0x24, word);
    host.cells(b - 0x10, b + LO, b + HI, b + CELLS);
    for k in 0..4 {
        let cell = mem.u32(b + CELLS + 4 * k);
        if cell != 0 {
            host.notify_filtered(b - 8, cell, l2);
        }
    }
    host.visit_list470(b - 8, physics(mem), l2);
    host.ray_clear(b - 4, b + OPS);
    host.ray_clear(b - 4, b + OLD_OPS);
}

/// `CL2PenetrationDetector::Vf04` (0x524480) on the detector at `this` (+4 its owner, +0x10 and +0x20 its box corners,
/// +0x6e4 the word the response takes) with `other`; `entry` is its entry esp. The gates (not the owner itself, `other`'s
/// grid not disabled, the owner's exclusion test, the parent gate, `other`'s slot 0x110), `other`'s box (slot 0x120 to
/// +0x40 and +0x30), and the two box tests (the owner's corner less `other`'s high corner, then the low corner less the
/// owner's second corner, all three lanes below zero). Then the owner's grid ops and `other`'s, the response (slot 0xd0)
/// on the owner with the pair of the owner's ops and `other`, and the same from `other`'s side when that answers zero.
pub fn l2_penetration_detect(mem: &mut Mem, host: &mut impl DetectorHost, this: u32, other: u32, entry: u32) {
    const LOCALS: u32 = 0xdb8;
    const CORNER_LO: u32 = 0x40;
    const CORNER_HI: u32 = 0x30;
    const DIFF: u32 = 0x20;
    const PAIR_OWNER: u32 = 0x18;
    const PAIR_OTHER: u32 = 0x20;
    const OPS_OTHER: u32 = 0x50;
    const OPS_OWNER: u32 = 0x700;
    let b = detector_frame(entry, LOCALS);
    let owner = mem.u32(this + 4);
    if owner == other {
        return;
    }
    let grid = host.collision_grid(b - 4, other);
    if grid_disabled(mem, grid) || host.exclusion_test(b - 8, owner, other) == 0 {
        return;
    }
    let parent = mem.u32(other + 0x44);
    if parent != 0 && host.child_gate(b - 4, other) == 0 && host.child_gate(b - 4, parent) != 0 {
        return;
    }
    if host.entity_110(b - 4, other) == 0 {
        return;
    }
    host.entity_box(b - 0xc, other, b + CORNER_LO, b + CORNER_HI);
    let [lo, hi] = [b + CORNER_LO, b + CORNER_HI].map(|at| vec3s::<1>(mem, at)[0]);
    let [near, far] = [this + 0x10, this + 0x20].map(|at| vec3s::<1>(mem, at)[0]);
    let first = near - hi;
    set_vec3s(mem, b + DIFF, &[first]);
    if !all_below_zero(first) {
        return;
    }
    let second = lo - far;
    set_vec3s(mem, b + DIFF, &[second]);
    if !all_below_zero(second) {
        return;
    }
    let pad = mem.u32(this + 4);
    let ignored30 = host.entity_30(b - 4, other);
    let pad30 = host.entity_30(b - 8, pad);
    let pad118 = host.collision_grid(b - 0xc, pad);
    host.grid_ops(b - 0x10, b + OPS_OWNER, pad118, pad30, ignored30);
    let pair = store_pair(mem, b + PAIR_OWNER, b + OPS_OWNER, other);
    let extra = mem.u32(this + 0x6e4);
    if host.entity_d0(b - 0x10, pad, pair, other, extra) == 0 {
        let pad30 = host.entity_30(b - 4, pad);
        let other30 = host.entity_30(b - 8, other);
        let other118 = host.collision_grid(b - 0xc, other);
        host.grid_ops(b - 0x10, b + OPS_OTHER, other118, other30, pad30);
        let pair = store_pair(mem, b + PAIR_OTHER, b + OPS_OTHER, pad);
        host.entity_d0(b - 0x10, other, pair, pad, mem.u32(this + 0x6e4));
        host.ray_clear(b - 4, b + OPS_OTHER);
    }
    host.ray_clear(b - 4, b + OPS_OWNER);
}

/// `E45PhysBody::M4250` (0x524250, stdcall, four arguments), run inside `E46L2Detector::Vf04` on the detector at `this`
/// (+0x20 and +0x24 its words); `entry` is its entry esp. Its pairs (`d` with `b`'s slot 0x30, `c` with `b`'s slot
/// 0x34) are the seven arguments of `a`'s slot 0xd4 with the two words. The pairs stay in the frame the original gave
/// them: the callee gets them by value.
pub fn body_pair_detect(
    mem: &Mem,
    host: &mut impl DetectorHost,
    this: u32,
    a: u32,
    b: u32,
    c: u32,
    d: u32,
    entry: u32,
) {
    let frame = entry - 0x20;
    let r1 = host.entity_30(frame - 4, b);
    let r2 = host.entity_34(frame - 4, b);
    host.entity_d4(frame - 0x20, a, [c, r2, d, r1, b, mem.u32(this + 0x20), mem.u32(this + 0x24)]);
}

/// `E46L2Detector::Vf04` (0x5257e0) on the detector at `this` (+4 its owner, +8 and +0x10 its box corners, +0x18 and
/// +0x1c its grids) with `other`; `entry` is its entry esp. The frame is the original's: the `this` push and the
/// `_chkstk` locals sit above the two saved registers, so `b` is `detector_frame`'s with 0x1ad8 locals. The gates
/// (`other` not the owner, slot 0x110, the exclusion test, the parent gate, `other`'s grid not disabled) and the box
/// test (slot 0xcc on `this` +8 and +0x10). Then, with the owner's and `other`'s grid ops, `body_pair_detect` (the
/// owner as `a`, `other`, the two grid ops); and when `other`'s slot 0x118 answers, the mirrored grid ops and
/// `body_pair_detect` the other way. The ray clears close it.
pub fn e46_l2_detect(mem: &mut Mem, host: &mut impl DetectorHost, this: u32, other: u32, entry: u32) {
    const LOCALS: u32 = 0x1ad8;
    const OPS_A: u32 = 0x10;
    const OPS_B: u32 = 0x6c0;
    const OPS_C: u32 = 0xd70;
    const OPS_D: u32 = 0x1420;
    let b = detector_frame(entry, LOCALS);
    let owner = mem.u32(this + 4);
    if other == 0 || other == owner {
        return;
    }
    if host.entity_110(b - 4, other) == 0 {
        return;
    }
    if host.exclusion_test(b - 8, owner, other) == 0 {
        return;
    }
    let parent = mem.u32(other + 0x44);
    if parent != 0 && host.child_gate(b - 4, other) == 0 && host.child_gate(b - 4, parent) != 0 {
        return;
    }
    let gate = host.collision_grid(b - 4, other);
    if grid_disabled(mem, gate) {
        return;
    }
    if host.overlaps_box(b - 0xc, other, this + 8, this + 0x10) == 0 {
        return;
    }
    let m34 = host.entity_34(b - 4, other);
    host.grid_ops(b - 0x10, b + OPS_D, mem.u32(this + 0x18), mem.u32(ROOT_IDENTITY), m34);
    let m30 = host.entity_30(b - 4, other);
    host.grid_ops(b - 0x10, b + OPS_B, mem.u32(this + 0x1c), mem.u32(ROOT_IDENTITY), m30);
    let owner = mem.u32(this + 4);
    body_pair_detect(mem, host, this, owner, other, b + OPS_D, b + OPS_B, b - 0x14);
    if host.collision_grid(b - 4, other) != 0 {
        let a = host.entity_34(b - 4, owner);
        let inner = host.entity_34(b - 8, other);
        let gate = host.collision_grid(b - 0xc, other);
        host.grid_ops(b - 0x10, b + OPS_A, gate, inner, a);
        let b30 = host.entity_30(b - 4, owner);
        let inner2 = host.entity_30_with(b - 8, other, b30);
        let gate2 = host.collision_grid(b - 0xc, other);
        host.grid_ops(b - 0x10, b + OPS_C, gate2, inner2, b30);
        body_pair_detect(mem, host, this, other, owner, b + OPS_A, b + OPS_C, b - 0x14);
        host.ray_clear(b - 4, b + OPS_C);
        host.ray_clear(b - 4, b + OPS_A);
    }
    host.ray_clear(b - 4, b + OPS_B);
    host.ray_clear(b - 4, b + OPS_D);
}

/// `CPhysics::PenCheck` (0x526cc0) on the physics at `this`; `entry` is its entry esp. An `E46L1PenDetector` at
/// entry-0x10 (scale 1, the solver at the scratch's +0x10, the state) runs on each body of the state and on its children.
pub fn penetration_check(mem: &mut Mem, host: &mut impl DetectorHost, this: u32, entry: u32) {
    let detector = entry - 0x10;
    let state = mem.u32(this + STEP_STATE_AT);
    mem.set_u32(detector, L1_PEN_VTABLE);
    mem.set_f32(detector + 4, 1.0);
    mem.set_u32(detector + 8, mem.u32(this + STEP_SCRATCH_AT) + 0x10);
    mem.set_u32(detector + 0xc, state);
    let mut node = mem.u32(state + STATE_BODIES_AT);
    while node != 0 {
        host.detect(entry - 0x1c, detector, node);
        host.walk(entry - 0x1c, node, detector);
        node = mem.u32(node + STEP_NODE_NEXT_AT);
    }
}

/// The entity flag (byte +0x60) that keeps an entity of the +0x470 list from `CPhysics::Visit470`.
const VISIT_SKIP_FLAG: u8 = 0x40;

/// `CPhysics::Visit470(detector)` (0x526390) on the physics at `this`; `entry` is its entry esp. The detector runs on
/// each entity of the state's +0x470 list without the skip flag, and on its children.
pub fn visit_list470(mem: &mut Mem, host: &mut impl DetectorHost, this: u32, detector: u32, entry: u32) {
    let mut node = mem.u32(mem.u32(this + STEP_STATE_AT) + STATE_LIST470_AT);
    while node != 0 {
        if mem.u8(node + ENTITY_FLAGS_AT) & VISIT_SKIP_FLAG == 0 {
            host.detect(entry - 0x10, detector, node);
            host.walk(entry - 0x10, node, detector);
        }
        node = mem.u32(node + STEP_NODE_NEXT_AT);
    }
}

/// `CL1CollisionDetector`'s vtable.
const L1_COLLISION_VTABLE: u32 = 0x0073_f7b0;

/// `CPhysics::Work6a70(word)` (0x526a70) on the physics at `this`; `entry` is its entry esp. A `CL1CollisionDetector`
/// at entry-0xc (the word, the solver at the scratch's +0x10) runs on each body of the state and on its children.
pub fn run_collision_detectors(mem: &mut Mem, host: &mut impl DetectorHost, this: u32, word: u32, entry: u32) {
    let detector = entry - 0xc;
    let solver = mem.u32(this + STEP_SCRATCH_AT) + 0x10;
    let state = mem.u32(this + STEP_STATE_AT);
    mem.set_u32(detector, L1_COLLISION_VTABLE);
    mem.set_u32(detector + 4, word);
    mem.set_u32(detector + 8, solver);
    let mut node = mem.u32(state + STATE_BODIES_AT);
    while node != 0 {
        host.detect(entry - 0x18, detector, node);
        host.walk(entry - 0x18, node, detector);
        node = mem.u32(node + STEP_NODE_NEXT_AT);
    }
}
