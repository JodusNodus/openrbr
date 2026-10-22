//! `E90Group`'s build (0x601270-0x606070): the fence posts and spans, the segment, particle, spring, pin and bend
//! pools.

use crate::entities::fence_group;
use crate::entities::fence_span;
use crate::math::vec::Vec3;
use crate::mem::Mem;

/// Native E90 build calls. Each stack pointer is relative to the original Build entry, before the call instruction.
pub trait E90BuildCalls {
    fn allocate(&mut self, sp: u32, size: u32) -> u32;
    fn init(&mut self, sp: u32, pool: u32, count: u32, mem: u32) -> u32;
    fn build_post(&mut self, sp: u32, handler: u32, segment: u32, dummies: u32, mem: u32) -> u32;
    fn build_span(&mut self, sp: u32, handler: u32, span: u32, prev: u32, post: u32, dummy: u32, mem: u32) -> u32;
}

/// `E90Group::Build` (0x601470): allocate and clear its block, initialize the segment pool, then build and link posts and
/// spans. The fence handler's post and span construction remains in the original exe.
pub fn build_e90_group(mem: &mut Mem, group: u32, frame: u32, calls: &mut impl E90BuildCalls) {
    if mem.u32(group + 0x3c) != 0 {
        return;
    }
    let n = mem.u32(group);
    let type_at = 0x7d81a0u32.wrapping_add(mem.u32(group + 0x2c).wrapping_mul(0x2c));
    let cols = mem.u32(type_at + 8);
    let rows = mem.u32(type_at + 0xc);
    let layers = mem.u32(type_at + 0x24);
    let bytes = fence_group::build_size(n, cols, rows, layers);
    let allocation = calls.allocate(frame - 0x28, bytes);
    mem.set_u32(group + 0x3c, allocation);
    for at in (0..bytes).step_by(4) {
        mem.set_u32(allocation + at, 0);
    }
    for at in (bytes & !3..bytes).step_by(1) {
        mem.set_u8(allocation + at, 0);
    }

    let pool = group + 0x40;
    let mem_end = calls.init(frame - 0x2c, pool, n.wrapping_add(n.wrapping_sub(1)), allocation);
    let (mut prev, next_count) = fence_group::next_segment(mem.u32(pool), mem.u32(pool + 0xc));
    mem.set_u32(pool, next_count);
    let dummies = mem.u32(group + 0xc);
    let mut cursor = calls.build_post(frame - 0x30, 0x167ea90, prev, dummies, mem_end);

    for i in 0..n.wrapping_sub(1) {
        let dummy = dummies.wrapping_add(i.wrapping_mul(0xb4));
        let (span, count) = fence_group::next_segment(mem.u32(pool), mem.u32(pool + 0xc));
        mem.set_u32(pool, count);
        let (post, count) = fence_group::next_segment(mem.u32(pool), mem.u32(pool + 0xc));
        mem.set_u32(pool, count);
        cursor = calls.build_post(frame - 0x34, 0x167ea90, post, mem.u32(dummy + 0x64), cursor);
        cursor = calls.build_span(frame - 0x40, 0x167ea90, span, prev, post, dummy, cursor);
        mem.set_u32(prev + 0x58, span);
        mem.set_u32(span + 0x54, prev);
        mem.set_u32(span + 0x58, post);
        mem.set_u32(post + 0x54, span);
        mem.set_u32(dummy + 0x7c, span);
        mem.set_u32(dummy + 0x80, prev);
        prev = post;
    }
    mem.set_u32(dummies + n.wrapping_sub(1).wrapping_mul(0xb4) + 0x80, prev);
}

/// Calls made by `CFenceHandler::BuildPost` and `BuildSpan`. The stack offsets are relative to the helper's entry
/// ESP; keeping them here lets the binding reproduce the original's call sites.
pub trait E90SegmentCalls {
    fn init(&mut self, sp: u32, segment: u32, dummy: u32);
    fn parts_init(&mut self, sp: u32, segment: u32, count: u32, mem: u32) -> u32;
    fn springs_init(&mut self, sp: u32, segment: u32, count: u32, mem: u32) -> u32;
    fn bends_init(&mut self, sp: u32, segment: u32, count: u32, mem: u32) -> u32;
    fn pins_init(&mut self, sp: u32, segment: u32, count: u32, mem: u32) -> u32;
    fn add_particle(&mut self, sp: u32, segment: u32, pos: u32, inv_mass: f32) -> u32;
    fn add_pin(&mut self, sp: u32, segment: u32, particle: u32);
    fn add_spring(&mut self, sp: u32, segment: u32, a: u32, b: u32, kind: u32, stiffness: f32) -> u32;
    fn add_fixed(&mut self, sp: u32, segment: u32, spring: u32);
    fn add_bend(&mut self, sp: u32, segment: u32, spring: u32, particle: u32);
}

/// The CRT calls `E90SegPool::Init` makes when it owns its array: `operator new`, the `eh vector` constructor and
/// destructor iterators over the segments (0x78 bytes each, with `E90Seg::E90Seg` and `E90Seg::~E90Seg`), and
/// `operator delete`. `sp` is the callee's entry esp.
pub trait E90SegPoolCalls {
    fn allocate(&mut self, sp: u32, size: u32) -> u32;
    fn construct_all(&mut self, sp: u32, segments: u32, count: u32);
    fn destroy_all(&mut self, sp: u32, segments: u32, count: u32);
    fn release(&mut self, sp: u32, block: u32);
}

/// A segment's size in the pool (`E90Seg`).
const SEGMENT_SIZE: u32 = 0x78;

/// `E90SegPool::Init` (0x601270): `count` segments from `block` when it is given (the cursor after them), else an owned
/// `new[]` array (its count in the word before the segments, each constructed; the cursor is 0). An owned array from
/// before is destroyed and deleted first. `frame` is the entry esp.
pub fn init_e90_seg_pool(
    mem: &mut Mem,
    pool: u32,
    count: u32,
    block: u32,
    frame: u32,
    calls: &mut impl E90SegPoolCalls,
) -> u32 {
    mem.set_u32(pool, 0);
    mem.set_u32(pool + 4, 0);
    let owned = mem.u32(pool + 0xc);
    if mem.u8(pool + 8) == 0 && owned != 0 {
        calls.destroy_all(frame - 0x30, owned, mem.u32(owned - 4));
        calls.release(frame - 0x24, owned - 4);
        mem.set_u32(pool + 0xc, 0);
    }
    if count == 0 {
        return block;
    }
    mem.set_u32(pool, 0);
    mem.set_u32(pool + 4, count);
    if block != 0 {
        mem.set_u32(pool + 0xc, block);
        mem.set_u8(pool + 8, 1);
        return block.wrapping_add(count.wrapping_mul(SEGMENT_SIZE));
    }
    mem.set_u8(pool + 8, 0);
    let raw = calls.allocate(frame - 0x24, count.wrapping_mul(SEGMENT_SIZE).wrapping_add(4));
    let data = if raw != 0 {
        mem.set_u32(raw, count);
        calls.construct_all(frame - 0x34, raw + 4, count);
        raw + 4
    } else {
        0
    };
    mem.set_u32(pool + 0xc, data);
    block
}

/// The five pools of an `E90Seg` (`+0` particles, `+0x10` the pointer table, `+0x20`, `+0x30`, `+0x40`): count and
/// capacity words, the external-storage flag byte (+8) and the storage (+0xc); the table has four words.
const SEG_POOLS: [u32; 3] = [0x40, 0x30, 0x20];

/// `E90Seg::E90Seg` (0x600ff0, thiscall, no args): every pool's words and flag cleared, in the asm's order (the table's
/// four words between the first and second pools). Returns `seg`.
pub fn construct_e90_seg(mem: &mut Mem, seg: u32) -> u32 {
    for pool in [0, 0x20, 0x30, 0x40] {
        mem.set_u32(seg + pool, 0);
        mem.set_u32(seg + pool + 4, 0);
        mem.set_u8(seg + pool + 8, 0);
        mem.set_u32(seg + pool + 0xc, 0);
        if pool == 0 {
            for at in [0x10, 0x14, 0x18, 0x1c] {
                mem.set_u32(seg + at, 0);
            }
        }
    }
    seg
}

/// `E90Seg::~E90Seg` (0x601030, thiscall, no args): the pools at +0x40, +0x30 and +0x20 emptied (their owned storage
/// deleted), the table's words cleared (its storage is not deleted), then the particle pool emptied. `frame` is the
/// entry esp; `release` is `operator delete`.
pub fn destroy_e90_seg(mem: &mut Mem, seg: u32, frame: u32, calls: &mut impl E90SegPoolCalls) {
    for pool in SEG_POOLS {
        empty_seg_pool(mem, seg + pool, frame - 0x20, calls);
    }
    for at in [0x10, 0x14, 0x18, 0x1c] {
        mem.set_u32(seg + at, 0);
    }
    empty_seg_pool(mem, seg, frame - 0x20, calls);
}

/// One pool of `~E90Seg`: count and capacity cleared, owned storage deleted.
fn empty_seg_pool(mem: &mut Mem, pool: u32, sp: u32, calls: &mut impl E90SegPoolCalls) {
    mem.set_u32(pool, 0);
    mem.set_u32(pool + 4, 0);
    let storage = mem.u32(pool + 0xc);
    if mem.u8(pool + 8) == 0 && storage != 0 {
        calls.release(sp, storage);
        mem.set_u32(pool + 0xc, 0);
    }
}

/// The original's `E90Pool::Free` prefix: reset the live count and capacity; storage is retained when it is
/// externally owned. Group builds pass an external block, so the owned-storage delete path is not reached there.
fn reset_e90_pool(mem: &mut Mem, pool: u32) {
    mem.set_u32(pool, 0);
    mem.set_u32(pool + 4, 0);
    if mem.u8(pool + 8) == 0 && mem.u32(pool + 0xc) != 0 {
        mem.set_u32(pool + 0xc, 0);
    }
}

/// `CFenceHandler::BuildPost` (0x602fe0). The pool operations and particle/spring builders remain calls to their
/// original helpers; this ports the order, arguments, frame positions and pointer setup around those calls.
pub fn build_e90_post(
    mem: &mut Mem,
    handler: u32,
    post: u32,
    dummy: u32,
    mut block: u32,
    frame: u32,
    calls: &mut impl E90SegmentCalls,
) -> u32 {
    let group = mem.u32(dummy + 0xb0);
    let type_at = 0x7d81a0u32.wrapping_add(mem.u32(group + 0x30).wrapping_mul(0x2c));
    let rows = mem.u32(type_at + 0xc);
    let height = mem.f32(type_at + 0x10);
    let layers = mem.u32(type_at + 0x24);
    let layer_step = mem.f32(type_at + 0x28);

    calls.init(frame - 0x3c, post, dummy);
    block = calls.parts_init(frame - 0x40, post, layers.wrapping_mul(rows).wrapping_add(3), block);
    block = calls.springs_init(frame - 0x40, post + 0x10, 2, block);

    reset_e90_pool(mem, post + 0x20);
    mem.set_u32(post + 0x20, 0);
    mem.set_u32(post + 0x24, 1);
    if block != 0 {
        mem.set_u32(post + 0x2c, block);
        mem.set_u8(post + 0x28, 1);
        block = block.wrapping_add(4);
    } else {
        // This path asks the native allocator for storage. The entry remains deliberately unbound when the pure
        // runtime has no allocator stand-in; group-build snapshots always provide the shared block.
        mem.set_u8(post + 0x28, 0);
        mem.set_u32(post + 0x2c, 0);
    }
    block = calls.bends_init(frame - 0x40, post + 0x30, layers.wrapping_mul(rows), block);

    reset_e90_pool(mem, post + 0x40);
    mem.set_u32(post + 0x40, 0);
    mem.set_u32(post + 0x44, 1);
    if block != 0 {
        mem.set_u32(post + 0x4c, block);
        mem.set_u8(post + 0x48, 1);
        block = block.wrapping_add(4);
    } else {
        mem.set_u8(post + 0x48, 0);
        mem.set_u32(post + 0x4c, 0);
    }

    let row_step = height / rows.wrapping_sub(1) as f32;
    let p = frame - 0x18;
    for layer in 0..layers {
        for row in 0..rows {
            mem.set_f32(p, mem.f32(dummy + 0x6c));
            let y = (1.0f32 - layer_step * layer as f32) - row as f32 * row_step;
            mem.set_f32(p + 4, y + mem.f32(dummy + 0x70));
            mem.set_f32(p + 8, mem.f32(dummy + 0x74));
            calls.add_particle(frame - 0x40, post, p, f32::from_bits(0x3dcccccd));
        }
    }
    mem.set_f32(p, mem.f32(dummy + 0x6c));
    mem.set_f32(p + 4, mem.f32(dummy + 0x70) + 1.1f32);
    mem.set_f32(p + 8, mem.f32(dummy + 0x74));
    let top = calls.add_particle(frame - 0x40, post, p, f32::from_bits(0x3dcccccd));
    calls.add_pin(frame - 0x3c, post, top);
    let fixed_a = calls.add_particle(frame - 0x40, post, p, 0.0);
    let fixed_b = calls.add_particle(frame - 0x40, post, dummy + 0x6c, 0.0);
    calls.add_spring(frame - 0x48, post, top, fixed_a, 1, f32::from_bits(0x3dcccccd));
    let spring = calls.add_spring(frame - 0x48, post, top, fixed_b, 0, 1.0);
    calls.add_fixed(frame - 0x3c, post, spring);
    for layer in 0..layers {
        let parts = mem.u32(post + 0xc).wrapping_add(layer.wrapping_mul(rows).wrapping_mul(0x28));
        for row in 0..rows {
            calls.add_bend(frame - 0x40, post, spring, parts.wrapping_add(row.wrapping_mul(0x28)));
        }
    }
    let _ = handler;
    block
}

/// `CFenceHandler::BuildSpan` (0x6029b0). Its helper calls stay at the original entry ESP offsets; the span's
/// particle grid and spring creation order follow the native loop.
#[allow(clippy::assign_op_pattern)] // preserve the original floating-point operand order
pub fn build_e90_span(
    mem: &mut Mem,
    _handler: u32,
    span: u32,
    prev: u32,
    next: u32,
    dummy: u32,
    mut block: u32,
    frame: u32,
    calls: &mut impl E90SegmentCalls,
) -> u32 {
    let group = mem.u32(dummy + 0xb0);
    let type_at = 0x7d81a0u32.wrapping_add(mem.u32(group + 0x2c).wrapping_mul(0x2c));
    let cols = mem.u32(type_at + 8);
    let rows = mem.u32(type_at + 0xc);
    let layers = mem.u32(type_at + 0x24);
    let height = mem.f32(type_at + 0x10);
    let layer_step = mem.f32(type_at + 0x28);
    let inner = cols.wrapping_sub(2);
    let stiffness = mem.u32(type_at + 4);

    calls.init(frame - 0xa4, span, dummy);
    block = calls.parts_init(frame - 0xa8, span, inner.wrapping_mul(layers).wrapping_mul(rows), block);
    let spring_count = (rows
        .wrapping_sub(4)
        .wrapping_add(rows.wrapping_sub(1).wrapping_mul(cols.wrapping_sub(1)).wrapping_mul(4))
        .wrapping_add(cols))
    .wrapping_mul(layers);
    block = calls.springs_init(frame - 0xa8, span + 0x10, spring_count, block);
    reset_e90_pool(mem, span + 0x20);
    reset_e90_pool(mem, span + 0x30);
    block = calls.pins_init(frame - 0xa8, span + 0x40, inner.wrapping_mul(layers).wrapping_mul(rows), block);

    let step = 1.0f32 / (cols as f32 - 1.0f32);
    let dy = (mem.f32(mem.u32(dummy + 0x64) + 0x70) - mem.f32(dummy + 0x70)) * step;
    let dx = mem.f32(mem.u32(dummy + 0x64) + 0x6c) - mem.f32(dummy + 0x6c);
    let dz = mem.f32(mem.u32(dummy + 0x64) + 0x74) - mem.f32(dummy + 0x74);
    let row_step = 0.0f32 - height / rows.wrapping_sub(1) as f32;
    let p = frame - 0x18;
    let mut last = 0u32;
    for layer in 0..layers {
        let mut x = mem.f32(dummy + 0x6c);
        let mut z = mem.f32(dummy + 0x74);
        let mut y = (1.0f32 - layer as f32 * layer_step) + mem.f32(dummy + 0x70) + dy;
        for _column in 1..cols.wrapping_sub(1) {
            z = z + dz * step;
            x = x + step * dx;
            for row in 0..rows {
                mem.set_f32(p, x);
                mem.set_f32(p + 4, row_step * row as f32 + y);
                mem.set_f32(p + 8, z);
                last = calls.add_particle(frame - 0xa8, span, p, 1.0);
                calls.add_pin(frame - 0xa4, span, last);
            }
            y = dy + y;
        }
        let base = mem.u32(span + 0xc).wrapping_add(layer.wrapping_mul(inner).wrapping_mul(rows).wrapping_mul(0x28));
        let mut q = base;
        for _column in 1..inner {
            if rows != 1 && rows.wrapping_sub(1) < 0x8000_0000 {
                for _row in 0..rows.wrapping_sub(1) {
                    calls.add_spring(frame - 0xb0, span, q, q + 0x28, stiffness, 1.0);
                    calls.add_spring(frame - 0xb0, span, q, q + rows * 0x28, 0, 1.0);
                    calls.add_spring(frame - 0xb0, span, q, q + (rows + 1) * 0x28, 0, 1.0);
                    calls.add_spring(frame - 0xb0, span, q + 0x28, q + rows * 0x28, 0, 1.0);
                    q = q.wrapping_add(0x28);
                }
            }
            q = q.wrapping_add(0x28);
        }
        q = base.wrapping_add(rows.wrapping_sub(1).wrapping_mul(0x28));
        for _column in 1..inner {
            calls.add_spring(frame - 0xb0, span, q, q + rows * 0x28, 0, 1.0);
            q = q.wrapping_add(rows.wrapping_mul(0x28));
        }
        q = base.wrapping_add(inner.wrapping_sub(1).wrapping_mul(rows).wrapping_mul(0x28));
        for _row in 0..rows.wrapping_sub(1) {
            calls.add_spring(frame - 0xb0, span, q, q + 0x28, 0, 1.0);
            q = q.wrapping_add(0x28);
        }
        let prev_parts = mem.u32(prev + 0xc).wrapping_add(layer.wrapping_mul(rows).wrapping_mul(0x28));
        let next_parts = mem.u32(next + 0xc).wrapping_add(layer.wrapping_mul(rows).wrapping_mul(0x28));
        let last_row = last.wrapping_sub(rows.wrapping_sub(1).wrapping_mul(0x28));
        for row in 0..rows.wrapping_sub(1) {
            let p0 = prev_parts.wrapping_add(row.wrapping_mul(0x28));
            let p1 = p0 + 0x28;
            let q0 = base.wrapping_add(row.wrapping_mul(0x28));
            let q1 = q0 + 0x28;
            let n0 = next_parts.wrapping_add(row.wrapping_mul(0x28));
            let n1 = n0 + 0x28;
            let l0 = last_row.wrapping_add(row.wrapping_mul(0x28));
            let l1 = l0 + 0x28;
            calls.add_spring(frame - 0xb0, span, p0, q0, 0, 1.0);
            calls.add_spring(frame - 0xb0, span, p0, q1, 0, 1.0);
            calls.add_spring(frame - 0xb0, span, p1, q0, 0, 1.0);
            calls.add_spring(frame - 0xb0, span, n0, l0, 0, 1.0);
            calls.add_spring(frame - 0xb0, span, n0, l1, 0, 1.0);
            calls.add_spring(frame - 0xb0, span, n1, l0, 0, 1.0);
        }
        let last_prev = prev_parts.wrapping_add(rows.wrapping_sub(1).wrapping_mul(0x28));
        let last_base = base.wrapping_add(rows.wrapping_sub(1).wrapping_mul(0x28));
        let last_next = next_parts.wrapping_add(rows.wrapping_sub(1).wrapping_mul(0x28));
        let last_edge = last_row.wrapping_add(rows.wrapping_sub(1).wrapping_mul(0x28));
        calls.add_spring(frame - 0xb0, span, last_prev, last_base, 0, 1.0);
        calls.add_spring(frame - 0xb0, span, last_next, last_edge, 0, 1.0);
    }
    mem.set_u32(span + 0x58, next);
    mem.set_u32(span + 0x54, prev);
    mem.set_u32(prev + 0x58, span);
    mem.set_u32(next + 0x54, span);
    block
}

/// The D3DX normalise `E90Seg::Init` calls (`_D3DXVec3Normalize@8`, 0x627235, stdcall): `out = normalize(v)`.
pub trait E90SegInitCalls {
    fn normalize(&mut self, sp: u32, out: u32, v: u32);
}

/// The up vector `E90Seg::Init` uses (0x16fc524), and the bit 0 of 0x16fc530 that says it is stored.
const E90_UP: u32 = 0x16fc524;
const E90_UP_GUARD: u32 = 0x16fc530;

/// The position of the dummy's neighbour linked at `link` (+0x64 next, +0x68 prev), or of the dummy when it has none.
fn neighbour_position(mem: &Mem, dummy: u32, link: u32) -> u32 {
    match mem.u32(dummy + link) {
        0 => dummy + 0x6c,
        neighbour => neighbour + 0x6c,
    }
}

/// `E90Seg::Init` (0x605b60, thiscall on the segment with `dummy`, ret 4): the owner dummy and cleared links, then the
/// plane through the dummy along the fence line. The direction and side are normalised in the frame, through the D3DX
/// calls at the original's stack positions (their vectors are in memory, as the original leaves them).
pub fn e90_seg_init(mem: &mut Mem, seg: u32, dummy: u32, frame: u32, calls: &mut impl E90SegInitCalls) {
    mem.set_u32(seg + 0x50, dummy);
    mem.set_u32(seg + 0x54, 0);
    mem.set_u32(seg + 0x58, 0);
    mem.set_u32(seg + 0x5c, 0);
    if mem.u32(E90_UP_GUARD) & 1 == 0 {
        mem.set_u32(E90_UP_GUARD, mem.u32(E90_UP_GUARD) | 1);
        (Vec3 { x: 0.0, y: 1.0, z: 0.0 }).store(mem, E90_UP);
    }
    let up = Vec3::load(mem, E90_UP);
    let a = neighbour_position(mem, dummy, 0x68);
    let b = neighbour_position(mem, dummy, 0x64);
    let dir_at = frame - 0x1c;
    let side_at = frame - 0x28;
    let sp = frame - 0x3c;

    (Vec3::load(mem, b) - Vec3::load(mem, a)).store(mem, dir_at);
    calls.normalize(sp, dir_at, dir_at);
    let dir = Vec3::load(mem, dir_at);
    dir.cross3(up).store(mem, side_at);
    calls.normalize(sp, side_at, side_at);
    let side = Vec3::load(mem, side_at);
    let plane = fence_span::plane(side.cross3(dir), Vec3::load(mem, dummy + 0x6c));
    for (k, v) in plane.into_iter().enumerate() {
        mem.set_f32(seg + 0x68 + 4 * k as u32, v);
    }
}

/// The allocator and constructor calls of the E90 pool inits, at their stack positions: `operator new[]` (0x70bf84),
/// `operator delete[]` (0x70c27e), and the element constructor (0x602220) that `NLB_ApplyAll` calls on each element.
pub trait E90PoolCalls {
    fn allocate(&mut self, sp: u32, size: u32) -> u32;
    fn release(&mut self, sp: u32, block: u32);
    fn construct(&mut self, sp: u32, element: u32);
}

/// `E90Pool<T>::Free`, the prefix of each pool init: the count and capacity reset, and owned storage goes to
/// `delete[]`.
fn free_pool(mem: &mut Mem, pool: u32, sp: u32, calls: &mut impl E90PoolCalls) {
    mem.set_u32(pool, 0);
    mem.set_u32(pool + 4, 0);
    if mem.u8(pool + 8) == 0 && mem.u32(pool + 0xc) != 0 {
        calls.release(sp, mem.u32(pool + 0xc));
        mem.set_u32(pool + 0xc, 0);
    }
}

/// `E90Pool<T>::Init` for the pools of plain elements (the bend and pin pools, 0x6028b0 and 0x602930): `n` elements of
/// `size` bytes from `block` when it is given (the cursor after them), else from `new[]` (the cursor is 0).
fn init_pod_pool(
    mem: &mut Mem,
    pool: u32,
    n: u32,
    block: u32,
    size: u32,
    sps: (u32, u32),
    calls: &mut impl E90PoolCalls,
) -> u32 {
    free_pool(mem, pool, sps.0, calls);
    if n == 0 {
        return block;
    }
    mem.set_u32(pool, 0);
    mem.set_u32(pool + 4, n);
    if block != 0 {
        mem.set_u32(pool + 0xc, block);
        mem.set_u8(pool + 8, 1);
        return block.wrapping_add(n.wrapping_mul(size));
    }
    mem.set_u8(pool + 8, 0);
    let data = calls.allocate(sps.1, n.wrapping_mul(size));
    mem.set_u32(pool + 0xc, data);
    block
}

/// `E90BendPool::Init` (0x6028b0): see [`init_pod_pool`], 12-byte bends.
pub fn init_e90_bend_pool(
    mem: &mut Mem,
    pool: u32,
    n: u32,
    block: u32,
    frame: u32,
    calls: &mut impl E90PoolCalls,
) -> u32 {
    init_pod_pool(mem, pool, n, block, 0xc, (frame - 0xc, frame - 0x10), calls)
}

/// `E90PinPool::Init` (0x602930): see [`init_pod_pool`], 4-byte particle pointers.
pub fn init_e90_pin_pool(
    mem: &mut Mem,
    pool: u32,
    n: u32,
    block: u32,
    frame: u32,
    calls: &mut impl E90PoolCalls,
) -> u32 {
    init_pod_pool(mem, pool, n, block, 4, (frame - 0xc, frame - 0x10), calls)
}

/// `E90PartPool::Init` (0x6026a0): `n` particles of 0x28 bytes, from `block` (the cursor after them) or from `new[]`,
/// each element constructed. The original runs the constructor loop in `NLB_ApplyAll`; the loop is taken here, and the
/// constructor is called at the stack position `NLB_ApplyAll` gives it.
pub fn init_e90_part_pool(
    mem: &mut Mem,
    pool: u32,
    n: u32,
    block: u32,
    frame: u32,
    calls: &mut impl E90PoolCalls,
) -> u32 {
    free_pool(mem, pool, frame - 0x24, calls);
    if n == 0 {
        return block;
    }
    mem.set_u32(pool, 0);
    mem.set_u32(pool + 4, n);
    if block != 0 {
        mem.set_u32(pool + 0xc, block);
        mem.set_u8(pool + 8, 1);
        return block.wrapping_add(n.wrapping_mul(0x28));
    }
    mem.set_u8(pool + 8, 0);
    let data = calls.allocate(frame - 0x24, n.wrapping_mul(0x28));
    if data != 0 {
        for i in 0..n {
            calls.construct(frame - 0x44, data.wrapping_add(i.wrapping_mul(0x28)));
        }
    }
    mem.set_u32(pool + 0xc, data);
    block
}

/// `E90PtrTab::Init` (0x6027a0): `n` blocks of 0x20 bytes from `block`, and the `n` pointers to them after the blocks
/// (the cursor after both), or both from `new[]` (the cursor is 0). The pointer table takes no `Free` first.
pub fn init_e90_ptr_tab(mem: &mut Mem, tab: u32, n: u32, block: u32, frame: u32, calls: &mut impl E90PoolCalls) -> u32 {
    mem.set_u32(tab, 0);
    mem.set_u32(tab + 4, 0);
    mem.set_u32(tab + 8, 0);
    mem.set_u32(tab + 0xc, 0);
    if n == 0 {
        return block;
    }
    mem.set_u32(tab + 4, n);
    let (blocks, ptrs, cursor) = if block != 0 {
        let ptrs = block.wrapping_add(n.wrapping_mul(0x20));
        (block, ptrs, ptrs.wrapping_add(n.wrapping_mul(4)))
    } else {
        let blocks = calls.allocate(frame - 0x18, n.wrapping_mul(0x20));
        mem.set_u32(tab + 8, blocks);
        (blocks, calls.allocate(frame - 0x1c, n.wrapping_mul(4)), 0)
    };
    mem.set_u32(tab + 8, blocks);
    mem.set_u32(tab + 0xc, ptrs);
    for i in 0..n {
        mem.set_u32(ptrs.wrapping_add(i.wrapping_mul(4)), blocks.wrapping_add(i.wrapping_mul(0x20)));
    }
    cursor
}

/// `_D3DXVec3Normalize@8` (0x627235, stdcall, ret 8): `out = normalize(v)`, returning `out`. The thunk jumps to the SSE
/// variant the game's dispatch picked (`Vec3::normalize_d3dx`).
pub fn d3dx_vec3_normalize(mem: &mut Mem, out: u32, v: u32) -> u32 {
    (Vec3::load(mem, v).normalize_d3dx()).store(mem, out);
    out
}

// The E90 segment's pools as (count, data, element size): `Next()` raises the count and returns the slot of the old
// count. Particles are 0x28 bytes, bends 0xc; the spring, fixed and pin pools hold pointers.
const SEG_PARTS: (u32, u32, u32) = (0x00, 0x0c, 0x28);
const SEG_SPRINGS: (u32, u32, u32) = (0x10, 0x1c, 4);
const SEG_FIXED: (u32, u32, u32) = (0x20, 0x2c, 4);
const SEG_BENDS: (u32, u32, u32) = (0x30, 0x3c, 0xc);
const SEG_PINS: (u32, u32, u32) = (0x40, 0x4c, 4);

/// The next slot of the pool `(count, data, size)` at `seg`: the count rises by one and the slot is the old count's.
fn next_slot(mem: &mut Mem, seg: u32, pool: (u32, u32, u32)) -> u32 {
    let (count, data, size) = pool;
    let n = mem.u32(seg + count);
    mem.set_u32(seg + count, n + 1);
    mem.u32(seg + data) + n * size
}

/// `E90Seg::AddParticle` (0x605f30, thiscall, `ret 8`): the next particle takes `pos` as its position and old position,
/// a zero force and `inv_mass`. Returns the particle.
pub fn add_particle(mem: &mut Mem, seg: u32, pos: u32, inv_mass: f32) -> u32 {
    let p = next_slot(mem, seg, SEG_PARTS);
    for k in 0..3 {
        let lane = mem.u32(pos + 4 * k);
        mem.set_u32(p + 4 * k, lane);
        mem.set_u32(p + 0xc + 4 * k, lane);
        mem.set_f32(p + 0x18 + 4 * k, 0.0);
    }
    mem.set_f32(p + 0x24, inv_mass);
    p
}

/// `E90Seg::AddSpring` (0x605fb0, thiscall, `ret 0x10`): the next spring joins particles `a` and `b`, unless their inverse
/// masses sum to zero (then it returns 0 and adds nothing). The weights, the rest length and its square follow the
/// asm's order; each x87 step is an f32 step at the game's 24-bit precision.
pub fn add_spring(mem: &mut Mem, seg: u32, a: u32, b: u32, kind: u32, stiffness: f32) -> u32 {
    let (inv_a, inv_b) = (mem.f32(a + 0x24), mem.f32(b + 0x24));
    if inv_b + inv_a == 0.0 {
        return 0;
    }
    let slot = next_slot(mem, seg, SEG_SPRINGS);
    let s = mem.u32(slot);
    let w = 1.0 / (inv_b + inv_a);
    mem.set_u32(s, a);
    mem.set_u32(s + 4, b);
    mem.set_f32(s + 8, w * inv_a);
    mem.set_f32(s + 0xc, inv_b * w);
    let dx = mem.f32(b) - mem.f32(a);
    let dy = mem.f32(b + 4) - mem.f32(a + 4);
    let dz = mem.f32(b + 8) - mem.f32(a + 8);
    let len = (((dz * dz) + (dx * dx)) + (dy * dy)).sqrt();
    mem.set_f32(s + 0x10, len);
    mem.set_f32(s + 0x14, len * len);
    mem.set_f32(s + 0x18, stiffness);
    mem.set_u32(s + 0x1c, kind);
    s
}

/// `E90Seg::AddFixed` (0x606070, thiscall, `ret 4`): the next fixed-spring slot takes `spring`.
pub fn add_fixed(mem: &mut Mem, seg: u32, spring: u32) {
    let slot = next_slot(mem, seg, SEG_FIXED);
    mem.set_u32(slot, spring);
}

/// `E90Seg::AddPin` (0x606090, thiscall, `ret 4`): the next pin slot takes `particle`.
pub fn add_pin(mem: &mut Mem, seg: u32, particle: u32) {
    let slot = next_slot(mem, seg, SEG_PINS);
    mem.set_u32(slot, particle);
}

/// `E90Seg::AddBend` (0x6060b0, thiscall, `ret 8`): the next bend holds `spring` and `particle` and the particle's
/// projection on the spring: `(p - a) . (b - a) / |b - a|^2` written as `(p.z - a.z)*inv*(dz*inv)` plus the x and y
/// terms, in that order, with `inv = 1 / |b - a|`.
pub fn add_bend(mem: &mut Mem, seg: u32, spring: u32, particle: u32) {
    let bend = next_slot(mem, seg, SEG_BENDS);
    mem.set_u32(bend, spring);
    mem.set_u32(bend + 4, particle);
    let a = mem.u32(spring);
    let b = mem.u32(spring + 4);
    let dx = mem.f32(b) - mem.f32(a);
    let dy = mem.f32(b + 4) - mem.f32(a + 4);
    let dz = mem.f32(b + 8) - mem.f32(a + 8);
    let px = mem.f32(particle) - mem.f32(a);
    let py = mem.f32(particle + 4) - mem.f32(a + 4);
    let pz = mem.f32(particle + 8) - mem.f32(a + 8);
    let inv = 1.0 / (((dz * dz) + (dx * dx)) + (dy * dy)).sqrt();
    let proj = (((pz * inv) * (dz * inv)) + ((px * inv) * (dx * inv))) + ((py * inv) * (dy * inv));
    mem.set_f32(bend + 8, proj);
}
