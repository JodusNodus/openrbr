//! `E33Mat34::CollideInit` (0x51bd30) and the grid copy it uses.

use rbr_physics::entities::layout;
use rbr_physics::entities::layout::CollideHost;
use rbr_physics::mem::Mem;
use rbr_physics::physics_core::layout::ZERO_VEC4;

use crate::collision::{
    CE36_SUB_BLOCK_INIT, COLGRID_SAMPLE_A, COLGRID_SAMPLE_B, COLL_EVENT_INIT_FULL, E01_VEC4_COPY_FROM,
    E01_VEC4_NORMALIZE, E36_ALLOC_EVENT, E37_GRID_COPY, FLOAT3_LENGTH, RAY_CLEAR,
};
use crate::entities::COPY16_WALK;
use crate::{Ctx, Decline, Host, Regs, Return};

const SEGMENT_GATE: u32 = 0x4ff070;
const RAY_QUERY: u32 = 0x4fd760; // E38Ops::M_D760X
const FALLBACK_QUERY: u32 = 0x4febc0; // E38Ops::M_EBC0
const VEC_LERP: u32 = 0x473c90; // E11_VecLerp

/// thiscall `(from)`, ret 4: `E37GridOps::Copy6770` (0x4f6770), the grid at `from` copied to `this` (nothing when it
/// is 0); eax is `this`.
pub(super) fn copy_grid_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::copy_grid(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(r.ecx, 4))
}

/// `E33Mat34::CollideInit`'s calls at the original's stack pointers; `mem` reads bodyA's vtable.
struct CollideCalls<'a> {
    host: &'a dyn Host,
    mem: Mem,
}

impl CollideCalls<'_> {
    fn slot(&self, sp: u32, entity: u32, slot: u32, args: &[u32]) -> u32 {
        let target = layout::vtable_target(&self.mem, entity, slot);
        self.host.call_at(sp, target, entity, args).eax
    }
}

impl CollideHost for CollideCalls<'_> {
    fn init_block(&mut self, sp: u32, block: u32) {
        self.host.call_at(sp, CE36_SUB_BLOCK_INIT, block, &[]);
    }

    fn gate_boxes(&mut self, sp: u32, body: u32, a: u32, b: u32) {
        self.slot(sp, body, 0x120, &[a, b]);
    }

    fn segment_gate(&mut self, sp: u32, host: u32, a: u32, b: u32) -> u32 {
        self.host.call_at(sp, SEGMENT_GATE, host, &[a, b, 0]).eax
    }

    fn transform(&mut self, sp: u32, body: u32) -> u32 {
        self.slot(sp, body, 0x30, &[])
    }

    fn grid_source(&mut self, sp: u32, body: u32) -> u32 {
        self.slot(sp, body, 0x118, &[])
    }

    fn build_grid(&mut self, sp: u32, grid: u32, source: u32, transform: u32, identity: u32) {
        self.host.call_at(sp, E37_GRID_COPY, grid, &[source, transform, identity]);
    }

    fn copy_from(&mut self, sp: u32, to: u32, from: u32) {
        self.host.call_at(sp, E01_VEC4_COPY_FROM, to, &[from]);
    }

    fn ray_query(&mut self, sp: u32, host: u32, a: u32, b: u32, out: u32) -> u32 {
        self.host.call_at(sp, RAY_QUERY, host, &[a, b, out, 0]).eax
    }

    fn fallback_query(&mut self, sp: u32, host: u32, a: u32, out: u32) -> u32 {
        self.host.call_at(sp, FALLBACK_QUERY, host, &[a, out, 0]).eax
    }

    fn ray_clear(&mut self, sp: u32, grid: u32) {
        self.host.call_at(sp, RAY_CLEAR, grid, &[]);
    }

    fn compose(&mut self, sp: u32, identity: u32, point: u32, transform: u32, out: u32) {
        self.host.call_at(sp, COPY16_WALK, identity, &[point, transform, out]);
    }

    fn alloc_event(&mut self, sp: u32, size: u32) -> u32 {
        self.host.call_at(sp, E36_ALLOC_EVENT, 0, &[size]).eax
    }

    fn init_event(&mut self, sp: u32, event: u32, kind: u32, body: u32, contact: u32, point: u32) -> u32 {
        let args = [kind, body, contact, ZERO_VEC4, point, 0];
        self.host.call_at(sp, COLL_EVENT_INIT_FULL, event, &args).eax
    }

    fn sample_b(&mut self, sp: u32, grid: u32, index: u32, out_a: u32, out_b: u32) {
        self.host.call_at(sp, COLGRID_SAMPLE_B, grid, &[index, out_a, out_b]);
    }

    fn sample_a(&mut self, sp: u32, grid: u32, index: u32, t: f32, out_a: u32, out_b: u32) {
        let args = [index, t.to_bits(), out_a, out_b];
        self.host.call_at(sp, COLGRID_SAMPLE_A, grid, &args);
    }

    fn report(&mut self, sp: u32, sink: u32, event: u32) {
        self.slot(sp, sink, 4, &[event]);
    }

    fn lerp(&mut self, sp: u32, a: u32, b: u32, t: f32, out: u32) {
        self.host.call_at(sp, VEC_LERP, a, &[b, t.to_bits(), out]);
    }

    fn normalize(&mut self, sp: u32, vector: u32) {
        self.host.call_at(sp, E01_VEC4_NORMALIZE, vector, &[]);
    }

    fn length(&mut self, sp: u32, vector: u32) -> f32 {
        self.host.call_at(sp, FLOAT3_LENGTH, vector, &[]).st0 as f32
    }
}

/// E33Mat34::CollideInit (0x51bd30): thiscall on bodyB with `(a, b, c)`, ret 0xc; eax is 1 when a contact was reported.
pub(super) fn collide_init_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [body, _, sink, ..] = r.stack;
    let mut calls = CollideCalls { host: ctx.host, mem: ctx.mem.clone() };
    let eax = layout::collide_init(&mut calls, &mut ctx.mem, r.ecx, body, sink, r.esp);
    Ok(Return::void(eax, 12))
}
