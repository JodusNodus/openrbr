//! The broad queries: `E36Collect` and the sphere/box tests, the rays and segment sweeps, the grid ops and samples.

use rbr_physics::collision::e36::{self, CollectHost};
use rbr_physics::collision::e38::{self, CellQueryHost, RayHost, SampleHost};
use rbr_physics::collision::grid::{AllocHost, GridOpsHost};
use rbr_physics::collision::layout::{self, Body, GridSyncHost};
use rbr_physics::math::vec::clear_vec4;
use rbr_physics::mem::Mem;

use super::{
    Callees, BODY_GRID, E01_VEC4_COPY_FROM, E09_VEC4_CLEAR, E45_CELL_QUERY, ENTITY_VF118, FLOAT3_LENGTH, NODE_OF,
    RAY_SEEDS, SAMPLE_INDEXED, SAMPLE_TABLE,
};
use crate::entities::{COPY_BLOCKS, COPY_BLOCKS_THREE_FOUR, MB560};
use crate::rigid_body::OPERATOR_NEW;
use crate::{Ctx, Decline, Host, Regs, Return};

/// thiscall `(a, b)`, ret 8: `CCollisionMeshOpen::CopyBounds` (0x4fb070), the mesh's lower bound to `a` and upper bound
/// to `b`. Eax is `b`, the second copy's result.
pub(super) fn mesh_copy_bounds(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, ..] = r.stack;
    Ok(Return::void(layout::mesh_copy_bounds(&mut ctx.mem, r.ecx, a, b), 8))
}

/// thiscall, no arguments: `CCollisionMeshOpen::IsActive` (0x4fb0b0), eax 1.
pub(super) fn mesh_is_active(_ctx: &mut Ctx, _r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(e38::mesh_is_active(), 0))
}

/// thiscall on the E36 object (`ecx`) with `(sphere*, index*)`; eax is the 0 or 1 result.
pub(super) fn sphere_box(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [sphere, index, ..] = r.stack;
    let eax = rbr_physics::collision::layout::sphere_box(&mut ctx.mem, r.ecx, sphere, index);
    Ok(Return::void(eax, 8))
}

/// thiscall on the E36 object (`ecx`) with `(body, list**, count*)`; eax is the 0 or 1 result.
pub(super) fn collect(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [body, list_out, count_out, ..] = r.stack;
    // The original's frame holds the collect's temporaries, and its callees run below it. The original's body esp:
    // after `push ebp; and esp, -0x10; sub esp, 0xe4` and its three pushes (0x4f3950-0x4f3964).
    let frame = ((r.esp - 4) & !0xF) - 0xF0;
    let eax = e36::collect(&mut ctx.mem, frame, r.ecx, body, list_out, count_out, &mut Callees(ctx.host));
    Ok(Return::void(eax, 12))
}

/// thiscall on the SAT box (`ecx`) with `(other_center*, other_extent*)`; eax is 1 when the boxes overlap.
pub(super) fn world_axes_overlap(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [center, extent, ..] = r.stack;
    Ok(Return::void(layout::world_axes_overlap(&ctx.mem, r.ecx, center, extent), 8))
}

/// thiscall on the SAT box (`ecx`) with `(other_center*, other_extent*)`; eax is 1 when the boxes overlap.
pub(super) fn box_sat_overlap(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [center, extent, ..] = r.stack;
    Ok(Return::void(layout::box_sat_overlap(&ctx.mem, r.ecx, center, extent), 8))
}

/// thiscall (`ecx` unused) with `(origin*, direction*, v0*, v1*, v2*, output*)`; eax is 1 on a hit.
pub(super) fn ray_triangle(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [origin, direction, v0, v1, v2, output, ..] = r.stack;
    Ok(Return::void(layout::ray_triangle(&mut ctx.mem, origin, direction, v0, v1, v2, output), 24))
}

/// E38RaySeeds (0x4f6550): thiscall on the ray object; eax is the object (its `E09Vec4::Clear` calls return it).
pub(super) fn ray_seeds(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = RaySeedCalls { mem: ctx.mem.clone(), host: ctx.host, esp: r.esp };
    Ok(Return::void(layout::ray_seeds(&mut calls, &mut ctx.mem, r.ecx), 0))
}

/// `E38RaySeeds`' `E09Vec4::Clear` calls, at the pure host's placement below its entry `esp`, run here there (the
/// binding `vec4_clear` makes no call).
struct RaySeedCalls<'a> {
    mem: Mem,
    host: &'a dyn Host,
    esp: u32,
}

impl RayHost for RaySeedCalls<'_> {
    fn vec4_clear(&mut self, at: u32) -> u32 {
        let sp = (self.esp - 0x1000) & !0xf;
        if crate::skip_call(self.host, &mut self.mem, sp, &[]) {
            clear_vec4().store(&mut self.mem, at);
            return at;
        }
        self.host.call(E09_VEC4_CLEAR, at, &[]).eax
    }
}

/// CColGrid::SyncGrid (0x4f7090): thiscall on the grid with three stack words (`a`, which the transform takes as its
/// ecx, then `b` and `c`); eax is 1 when the grid has edges. The host's copy and transform scratch is below the call's
/// frame, `count` vectors (at most 127).
pub(super) fn grid_sync(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, ..] = r.stack;
    let count = u32::from(ctx.mem.u8(r.ecx + 0x68d));
    if count > 127 {
        return Err(Decline::Unsupported("edge count above the scratch window"));
    }
    let scratch = r.esp - 0x800 - 16 * count;
    let flag = layout::sync_grid(&mut Callees(ctx.host), &mut ctx.mem, r.ecx, a, b, c, scratch);
    Ok(Return::void(flag, 12))
}

/// M_D760X (0x4fd760): thiscall on the grid with four stack words (the ray's start and end, the output record, and the
/// record list, or null for the grid's own list); eax is 1 on a hit.
pub(super) fn grid_ray(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [start, end, out, list, ..] = r.stack;
    let flag = layout::mesh_ray_query(&mut Callees(ctx.host), &mut ctx.mem, r.ecx, start, end, out, list);
    Ok(Return::void(flag, 16))
}

/// thiscall `(origin, end, out)`, ret 0xc: `CCollisionSystem::Vf00` (0x4fe810), the segment query on the system
/// (`layout::system_vf00`); eax is 1 on a hit.
pub(super) fn system_vf00(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [origin, end, out, ..] = r.stack;
    let frame = layout::system_frame(r.esp);
    let mut calls = SystemCallsAt { host: ctx.host, frame };
    let eax = layout::system_vf00(&mut calls, &mut ctx.mem, r.ecx, origin, end, out, frame);
    Ok(Return::void(eax, 0xc))
}

/// The lookups of `CCollisionSystem::Vf00` in the emulator, each at its entry stack pointer in the frame.
struct SystemCallsAt<'a> {
    host: &'a dyn Host,
    frame: layout::SystemFrame,
}

impl SampleHost for SystemCallsAt<'_> {
    fn sample(&mut self, byte: u32, low: f32, high: f32) -> u32 {
        self.host
            .call_at(self.frame.sample_sp, SAMPLE_INDEXED, SAMPLE_TABLE, &[byte, low.to_bits(), high.to_bits()])
            .eax
    }
}

/// thiscall `(origin, direction, root, out)`, ret 0x10: `E38Ops::M_DE40X` (0x4fde40), the mesh ray over the tree
/// (`layout::mesh_ray_worklist`). Its result is the record at `out`; eax is not modelled.
pub(super) fn mesh_ray_worklist(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [origin, direction, root, out, ..] = r.stack;
    layout::mesh_ray_worklist(&mut ctx.mem, r.ecx, origin, direction, root, out);
    Ok(Return::void(0, 0x10))
}

/// E38NodeWalk (0x4fdd60): stdcall with three stack words (the query box, the root, and the output); eax is not
/// modelled.
pub(super) fn node_walk(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [query, root, out, ..] = r.stack;
    layout::node_walk(&mut ctx.mem, query, root, out);
    Ok(Return::void(0, 12))
}

/// E38SegmentSweep (0x4fad90): stdcall with four stack words (the segment's start and end, the hit tags, and the
/// distance). The query's scratch is below the caller's frame, as in the collect's.
pub(super) fn segment_sweep(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [start, end, out, distance, ..] = r.stack;
    layout::segment_sweep(&mut Callees(ctx.host), &mut ctx.mem, r.esp - 0x800, start, end, out, distance);
    Ok(Return::void(0, 16))
}

/// E38RayClear (0x4f6740): thiscall on the ray object; leaves eax as the caller had it, which the port does not model.
pub(super) fn ray_clear(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::ray_clear(&mut ctx.mem, r.ecx);
    Ok(Return::void(0, 0))
}

impl GridSyncHost for Callees<'_> {
    fn copy_transform(&mut self, _: &mut Mem, a: u32, edges: u32, worker: u32, out: u32, count: u32) {
        self.0.call(COPY_BLOCKS, a, &[edges, worker, out, count]);
    }

    fn copy_vec4(&mut self, _: &mut Mem, dst: u32, src: u32) {
        self.0.call(E01_VEC4_COPY_FROM, dst, &[src]);
    }
}

impl RayHost for Callees<'_> {
    fn vec4_clear(&mut self, at: u32) -> u32 {
        self.0.call(E09_VEC4_CLEAR, at, &[]).eax
    }
}

impl CellQueryHost for Callees<'_> {
    fn query_cells(&mut self, _mem: &mut Mem, lo: u32, hi: u32, cells: u32) {
        self.0.call(E45_CELL_QUERY, 0, &[lo, hi, cells]);
    }
}

impl SampleHost for Callees<'_> {
    fn sample(&mut self, byte: u32, low: f32, high: f32) -> u32 {
        self.0.call(SAMPLE_INDEXED, SAMPLE_TABLE, &[byte, low.to_bits(), high.to_bits()]).eax
    }
}

impl AllocHost for Callees<'_> {
    fn allocate(&mut self, bytes: u32) -> u32 {
        self.0.call(OPERATOR_NEW, 0, &[bytes]).eax
    }
}

impl CollectHost for Callees<'_> {
    fn virtual_call(&mut self, mem: &mut Mem, sp: u32, target: u32, args: &[u32]) -> u32 {
        // The getters every body in the cells is asked (`CBody::Vf118`, `E43PairWalk::MFc00`) are read here on the
        // pure host.
        if matches!(target, BODY_GRID | ENTITY_VF118 | NODE_OF) && crate::skip_call(self.0, mem, sp, &args[1..]) {
            return match target {
                BODY_GRID => Body::new(args[0]).grid(mem),
                ENTITY_VF118 => 0,
                _ => rbr_physics::entities::layout::node_of(args[0]),
            };
        }
        self.0.call_at(sp, target, args[0], &args[1..]).eax
    }

    fn transform(&mut self, mem: &mut Mem, sp: u32, target: u32, this: u32, a: u32, b: u32, c: u32) {
        debug_assert!(target == MB560 || target != 0);
        let host = self.0;
        let run = |mem: &mut Mem| crate::entities::transform_direct(host, mem, sp, target, this, [a, b, c]);
        if crate::call_memo::call(host, mem, crate::call_memo::Key([sp, target, this, a, b, c]), run) {
            return;
        }
        self.0.call_at(sp, target, this, &[a, b, c]);
    }

    fn query_cells(&mut self, _mem: &mut Mem, sp: u32, lo: u32, hi: u32, cells: u32) {
        self.0.call_at(sp, E45_CELL_QUERY, 0, &[lo, hi, cells]);
    }

    fn length(&mut self, _mem: &Mem, sp: u32, vector: u32) -> f32 {
        self.0.call_at(sp, FLOAT3_LENGTH, vector, &[]).st0 as f32
    }
}

/// `E39_SegTest170` (0x4ff170): fastcall on the collider (`ecx`) with `(a, b, out)`; eax is whether the walk found
/// anything. The query box is in the original's frame, 0x18 bytes below the entry esp.
pub(super) fn seg_test(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, out, ..] = r.stack;
    let eax = layout::seg_test(&mut ctx.mem, r.ecx, a, b, out, r.esp - 0x18);
    Ok(Return::void(eax, 12))
}

/// `E39_SegTest70` (0x4ff070): fastcall on the collider (`ecx`) with `(a, b, out)`; eax is whether the walk found
/// anything. The query box is in the original's frame, 0x18 bytes below the entry esp.
pub(super) fn segment_test(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, out, ..] = r.stack;
    let eax = layout::segment_test(&mut ctx.mem, r.ecx, a, b, out, r.esp - 0x18);
    Ok(Return::void(eax, 12))
}

/// `E38Ops::M_E2D0` (0x4fe2d0): thiscall with `(point, v0, v1, v2, out)`, `ret 0x14`; eax is 1 on a hit.
pub(super) fn point_triangle(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [point, v0, v1, v2, out, ..] = r.stack;
    let eax = layout::point_triangle(&mut ctx.mem, point, v0, v1, v2, out, crate::ngp::variant());
    Ok(Return::void(eax, 20))
}

/// `E38Ops::M_EBC0`'s material table samples (0x520240), at its esp for them (0x8c below its entry).
struct FaceQueryCalls<'a> {
    host: &'a dyn Host,
    sp: u32,
}

impl SampleHost for FaceQueryCalls<'_> {
    fn sample(&mut self, byte: u32, low: f32, high: f32) -> u32 {
        self.host.call_at(self.sp, SAMPLE_INDEXED, SAMPLE_TABLE, &[byte, low.to_bits(), high.to_bits()]).eax
    }
}

/// `E38Ops::M_EBC0` (0x4febc0): thiscall on the collider with `(point, out, list)`, `ret 0xc`; eax is 1 on a hit. The
/// face tests (0x4fe2d0) run in Rust.
pub(super) fn face_query(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [point, out, list, ..] = r.stack;
    let mut calls = FaceQueryCalls { host: ctx.host, sp: r.esp - 0x8c };
    let eax = layout::face_query(&mut calls, &mut ctx.mem, r.ecx, point, out, list, crate::ngp::variant());
    Ok(Return::void(eax, 12))
}

/// The calls of `E37GridOps::M6610`, at the entry stack pointers the port gives.
struct GridOps<'a>(&'a dyn Host);

impl GridOpsHost for GridOps<'_> {
    fn ray_seeds(&mut self, sp: u32, grid: u32) {
        self.0.call_at(sp, RAY_SEEDS, grid, &[]);
    }

    fn copy_blocks_one_two(&mut self, sp: u32, a: u32, src: u32, o2: u32, dst: u32, n: u32) {
        self.0.call_at(sp, COPY_BLOCKS, a, &[src, o2, dst, n]);
    }

    fn copy_blocks_three_four(&mut self, sp: u32, a: u32, src: u32, o2: u32, dst: u32, n: u32) {
        self.0.call_at(sp, COPY_BLOCKS_THREE_FOUR, a, &[src, o2, dst, n]);
    }
}

/// E37GridOps::AddEdge (0x4f65b0): thiscall on the grid with `(vec*)`, ret 4. eax is the loop's pointer after a move
/// (`this + 0x48 + 16 n` for `n` points), else the caller's with its low byte the zero count (`mov al`).
pub(super) fn grid_add_edge(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let moved = layout::grid_add_edge(&mut ctx.mem, r.ecx, r.stack[0]);
    let eax = match moved {
        0 => r.eax & !0xff,
        n => r.ecx + 0x48 + 16 * n,
    };
    Ok(Return::void(eax, 4))
}

/// E37GridOps::M6610 (0x4f6610): thiscall on the grid with `(src, a, b)`, ret 0xc; eax is the grid. Declines a source
/// with one point (the asm's bounds loop would not end) and no grid with a source.
pub(super) fn grid_ops_m6610(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [src, a, b, ..] = r.stack;
    let eax = layout::grid_ops_m6610(&mut GridOps(ctx.host), &mut ctx.mem, r.ecx, src, a, b, r.esp)
        .ok_or(Decline::Unsupported("a grid with one point, or none with a source"))?;
    Ok(Return::void(eax, 12))
}

/// thiscall on the grid with `(index, t, out_1, out_2)`, ret 0x10: `CColGrid::SampleA` (0x4e0840). Eax as the original
/// leaves it: `out_1`, or 0 (the grid's data word) when the grid has no data.
pub(super) fn grid_sample_a(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [index, t, out_1, out_2, ..] = r.stack;
    let samples = layout::sample_a(&ctx.mem, r.ecx, index as usize, f32::from_bits(t));
    Ok(Return::void(store_samples(ctx, r.ecx, out_1, out_2, samples), 0x10))
}

/// The samples' stores of SampleA and SampleB; returns their eax.
fn store_samples(ctx: &mut Ctx, grid: u32, out_1: u32, out_2: u32, samples: (u32, u32)) -> u32 {
    let empty = layout::ColGrid::new(grid).data(&ctx.mem) == 0;
    layout::store_samples(&mut ctx.mem, out_1, out_2, samples);
    if empty {
        0
    } else {
        out_1
    }
}

/// thiscall on the grid with `(index, out_1, out_2)`, ret 0xc: `CColGrid::SampleB` (0x4e0950). Eax as SampleA's.
pub(super) fn grid_sample_b(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [index, out_1, out_2, ..] = r.stack;
    let samples = layout::sample_b(&ctx.mem, r.ecx, index as usize);
    Ok(Return::void(store_samples(ctx, r.ecx, out_1, out_2, samples), 0xc))
}
