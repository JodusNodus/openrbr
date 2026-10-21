//! `CBody`'s contact slots (`VfD0`, `VfD8`, `VfDC`, `VfE8`, `VfF4`; `rbr_physics::collision::layout::{body_slots, collide_hosts}`).

use rbr_physics::collision::grid_contact::ContactKind;
use rbr_physics::collision::layout::body_slots::{self, D8Calls, E8Calls, F4Calls};
use rbr_physics::collision::layout::collide_hosts::{VfD0Host, VfDcHost};
use rbr_physics::collision::layout::{self, Body};
use rbr_physics::mem::Mem;
use rbr_physics::physics_core::layout::ZERO_VEC4;

use super::RAY_SEEDS;
use super::{
    COLGRID_SAMPLE_A, COLGRID_SAMPLE_B, COMPOSE_B, E01_VEC4_NORMALIZE, E09_VEC4_SUB, E21_VEC3_ADD, E33_MAKE_EVENT,
    E37_EDGE_EDGE, E37_EDGE_PAIR, E37_GRID_COPY, FETCH_X, MAT_COLLIDE_INIT, PAIR_COMBINE_TWIN,
};
use super::{COLL_EVENT_INIT_FULL, E36_ALLOC_EVENT};
use crate::collision::{CE36_SUB_BLOCK_INIT, RAY_CLEAR};
use crate::entities::{COPY16_WALK, FETCH_Y, PAIR_COMBINE};
use crate::{Ctx, Decline, Host, Regs, Return};

/// CBody::VfD8 (0x4e3070): thiscall on the body `this` with `(a, b, c, d, e)`, where `c` is the other body and `e` the
/// event list; eax is the hit flag. The frame goes below the entry esp (`layout::record_edge_hits_frame`).
pub(crate) fn body_vf_d8(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, other, _, list, ..] = r.stack;
    let frame = layout::record_edge_hits_frame(r.esp, a, b);
    let mut calls = D8CallsAt { mem: &ctx.mem, host: ctx.host, this: r.ecx, other, list };
    let hit = body_slots::record_edge_hits(&mut calls, &frame);
    Ok(Return::void(hit, 0x14))
}

/// The calls of CBody::VfD8 in the emulator: the original callees at their own stack pointers (the host places them
/// below the frame), and the other body's and this body's virtuals through their vtables.
struct D8CallsAt<'a> {
    mem: &'a Mem,
    host: &'a dyn Host,
    this: u32,
    other: u32,
    list: u32,
}

const VF30_SLOT: u32 = 0x30;
const VF34_SLOT: u32 = 0x34;
const VF_E8_SLOT: u32 = 0xe8;
const APPEND_SLOT: u32 = 0x4;

impl D8CallsAt<'_> {
    fn virtual_call(&mut self, object: u32, slot: u32, args: &[u32]) -> u32 {
        let target = layout::vtable_slot(self.mem, object, slot);
        self.host.call(target, object, args).eax
    }
}

impl D8Calls for D8CallsAt<'_> {
    fn ray_seeds(&mut self, ray: u32) {
        self.host.call(RAY_SEEDS, ray, &[]);
    }

    fn fetch_y(&mut self, matrix: u32, out: u32, worker: u32) {
        self.host.call(FETCH_Y, matrix, &[out, worker]);
    }

    fn other_vf34(&mut self) -> u32 {
        self.virtual_call(self.other, VF34_SLOT, &[])
    }

    fn other_vf30(&mut self) -> u32 {
        self.virtual_call(self.other, VF30_SLOT, &[])
    }

    fn this_vf30(&mut self) -> u32 {
        self.virtual_call(self.this, VF30_SLOT, &[])
    }

    fn combine(&mut self, out: u32, src: u32, worker: u32) {
        self.host.call(PAIR_COMBINE, out, &[src, worker]);
    }

    fn sub_block_init(&mut self, block: u32) {
        self.host.call(CE36_SUB_BLOCK_INIT, block, &[]);
    }

    fn grid_count(&mut self) -> u32 {
        layout::grid_edge_count(self.mem, self.this)
    }

    fn grid_hit(&mut self, twin: u32, desc: u32, payload: u32) -> bool {
        self.virtual_call(self.other, VF_E8_SLOT, &[twin, desc, 0x8000_0001, payload, 1]) != 0
    }

    fn compose_a(&mut self, out: u32, payload: u32, worker: u32) {
        let identity = self.mem.u32(layout::IDENTITY_WORKER_SLOT);
        self.host.call(COPY16_WALK, identity, &[payload, worker, out]);
    }

    fn alloc_event(&mut self) -> u32 {
        self.host.call(E36_ALLOC_EVENT, 0, &[layout::EVENT_SIZE]).eax
    }

    fn init_full(&mut self, event: u32, aux: u32, payload: u32) -> u32 {
        self.host.call(COLL_EVENT_INIT_FULL, event, &[0, self.this, aux, ZERO_VEC4, payload, 0]).eax
    }

    fn sample_b(&mut self, index: u32, event: u32) {
        let grid = Body::new(self.this).grid(self.mem);
        self.host.call(COLGRID_SAMPLE_B, grid, &[index, event + 0xa4, event + 0xa8]);
    }

    fn append(&mut self, event: u32) {
        let target = layout::vtable_slot(self.mem, self.list, APPEND_SLOT);
        self.host.call(target, self.list, &[event]);
    }

    fn ray_clear(&mut self, ray: u32) {
        self.host.call(RAY_CLEAR, ray, &[]);
    }
}
/// CBody::VfD0's calls at the stack pointers the port gives; `mem` reads the entities' vtables.
struct VfD0Calls<'a> {
    host: &'a dyn Host,
    mem: Mem,
}

impl VfD0Calls<'_> {
    fn slot(&self, sp: u32, entity: u32, slot: u32, args: &[u32]) -> u32 {
        let target = self.mem.u32(self.mem.u32(entity) + slot);
        self.host.call_at(sp, target, entity, args).eax
    }
}

impl VfD0Host for VfD0Calls<'_> {
    fn ray_seeds(&mut self, sp: u32, grid: u32) {
        self.host.call_at(sp, RAY_SEEDS, grid, &[]);
    }

    fn ray_clear(&mut self, sp: u32, grid: u32) {
        self.host.call_at(sp, RAY_CLEAR, grid, &[]);
    }

    fn collide_init(&mut self, sp: u32, other: u32, body: u32, list: u32) -> u32 {
        self.host.call_at(sp, MAT_COLLIDE_INIT, other, &[body, 0, list]).eax
    }

    fn transform(&mut self, sp: u32, entity: u32) -> u32 {
        self.slot(sp, entity, 0x30, &[])
    }

    fn grid_active(&mut self, sp: u32, entity: u32) -> u32 {
        self.slot(sp, entity, 0x110, &[])
    }

    fn grid(&mut self, sp: u32, entity: u32) -> u32 {
        self.slot(sp, entity, 0x118, &[])
    }

    fn vf80(&mut self, sp: u32, entity: u32) -> u32 {
        self.slot(sp, entity, 0x80, &[])
    }

    fn vf_f0(&mut self, sp: u32, entity: u32, args: [u32; 8]) -> u32 {
        self.slot(sp, entity, 0xf0, &args)
    }

    fn append(&mut self, sp: u32, list: u32, event: u32) {
        self.slot(sp, list, 4, &[event]);
    }

    fn fetch_y(&mut self, sp: u32, matrix: u32, out: u32, transform: u32) {
        self.host.call_at(sp, FETCH_Y, matrix, &[out, transform]);
    }

    fn fetch_x(&mut self, sp: u32, matrix: u32, out: u32, transform: u32) {
        self.host.call_at(sp, FETCH_X, matrix, &[out, transform]);
    }

    fn combine(&mut self, sp: u32, desc: u32, point: u32, transform: u32) {
        self.host.call_at(sp, PAIR_COMBINE, desc, &[point, transform]);
    }

    fn combine_twin(&mut self, sp: u32, desc: u32, point: u32, transform: u32) {
        self.host.call_at(sp, PAIR_COMBINE_TWIN, desc, &[point, transform]);
    }

    fn compose_a(&mut self, sp: u32, transform: u32, point: u32, other: u32, out: u32) {
        self.host.call_at(sp, COPY16_WALK, transform, &[point, other, out]);
    }

    fn compose_b(&mut self, sp: u32, transform: u32, point: u32, other: u32, out: u32) {
        self.host.call_at(sp, COMPOSE_B, transform, &[point, other, out]);
    }

    fn grid_copy(&mut self, sp: u32, grid: u32, src: u32, a: u32, b: u32) {
        self.host.call_at(sp, E37_GRID_COPY, grid, &[src, a, b]);
    }

    fn edge_edge(
        &mut self,
        sp: u32,
        grid: u32,
        from: u32,
        to: u32,
        hit: u32,
        t: u32,
        normal: u32,
        material: u32,
    ) -> u32 {
        let args = [from, to, hit, t, normal, material];
        self.host.call_at(sp, E37_EDGE_EDGE, grid, &args).eax
    }

    fn normalize(&mut self, sp: u32, vector: u32) {
        self.host.call_at(sp, E01_VEC4_NORMALIZE, vector, &[]);
    }

    fn add(&mut self, sp: u32, a: u32, out: u32, b: u32) -> u32 {
        self.host.call_at(sp, E21_VEC3_ADD, a, &[out, b]).eax
    }

    fn sub(&mut self, sp: u32, a: u32, out: u32, b: u32) -> u32 {
        self.host.call_at(sp, E09_VEC4_SUB, a, &[out, b]).eax
    }

    fn make_event(&mut self, sp: u32, kind: ContactKind, hit: u32, body: u32, other: u32, at: u32, normal: u32) -> u32 {
        let args = [kind as u32, hit, 0, body, other, at, ZERO_VEC4, normal, 0];
        self.host.call_at(sp, E33_MAKE_EVENT, 0, &args).eax
    }

    fn sample_b(&mut self, sp: u32, grid: u32, index: u32, out_a: u32, out_b: u32) {
        self.host.call_at(sp, COLGRID_SAMPLE_B, grid, &[index, out_a, out_b]);
    }

    fn sample_a(&mut self, sp: u32, grid: u32, index: u32, t: f32, out_a: u32, out_b: u32) {
        let args = [index, t.to_bits(), out_a, out_b];
        self.host.call_at(sp, COLGRID_SAMPLE_A, grid, &args);
    }
}

impl VfDcHost for VfD0Calls<'_> {
    fn vf34(&mut self, sp: u32, entity: u32) -> u32 {
        self.slot(sp, entity, 0x34, &[])
    }

    fn edge_pair(&mut self, sp: u32, grid: u32, args: [u32; 10]) -> u32 {
        self.host.call_at(sp, E37_EDGE_PAIR, grid, &args).eax
    }

    fn pair_event(&mut self, sp: u32, hit: u32, t: u32, body: u32, other: u32, at: u32, normal: u32) -> u32 {
        let args = [1, hit, t, body, other, at, ZERO_VEC4, normal, 0];
        self.host.call_at(sp, E33_MAKE_EVENT, 0, &args).eax
    }
}

/// CBody::VfDC (0x4e2850): thiscall on the body with `(a, b, other, d, list)`, ret 0x14 (`d` is not read); eax is 1
/// when it appended an event.
pub(super) fn body_vf_dc(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, other, _, list, ..] = r.stack;
    let mut calls = VfD0Calls { host: ctx.host, mem: ctx.mem.clone() };
    let eax = layout::body_vf_dc(&mut calls, &mut ctx.mem, r.ecx, a, b, other, list, r.esp);
    Ok(Return::void(eax, 0x14))
}

/// CBody::VfD0 (0x4e1eb0): thiscall on the body with `(matrix, other, list)`, ret 0xc; eax is 1 when it appended a
/// contact (or `CollideInit`'s eax).
pub(super) fn body_vf_d0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [matrix, other, list, ..] = r.stack;
    let mut calls = VfD0Calls { host: ctx.host, mem: ctx.mem.clone() };
    let eax = layout::body_vf_d0(&mut calls, &mut ctx.mem, r.ecx, matrix, other, list, r.esp);
    Ok(Return::void(eax, 12))
}

/// thiscall `(a, b, c, d, e)`, ret 0x14: `CBody::VfE8` (0x4e2f50), the other side's box test. The original calls its
/// own slot 0xf4 with the record's words and, on a hit, copies the grid's flags into the record.
pub(crate) fn body_vf_e8(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, _, record, last, ..] = r.stack;
    let mut calls = E8CallsAt { mem: &mut ctx.mem, host: ctx.host, this: r.ecx, sp: r.esp };
    let hit = body_slots::grid_hit(&mut calls, a, b, record, last);
    Ok(Return::void(hit, 0x14))
}

/// The calls of CBody::VfE8 in the emulator: `VfF4` (slot 0xf4) on this body, entered 0x2c below the entry esp (the
/// original pushes its eight arguments after `esi` and `edi`).
struct E8CallsAt<'a> {
    mem: &'a mut Mem,
    host: &'a dyn Host,
    this: u32,
    sp: u32,
}

/// The entry esp of `VfF4` when the original calls it from CBody::VfE8's frame.
const VF_E8_CALL_SP: u32 = 0x2c;
const VF_F4_SLOT: u32 = 0xf4;

impl E8Calls for E8CallsAt<'_> {
    fn box_edge_hit(&mut self, a: u32, b: u32, record: u32) -> u32 {
        let target = layout::vtable_slot(self.mem, self.this, VF_F4_SLOT);
        let args = layout::grid_hit_args(a, b, record);
        self.host.call_at(self.sp - VF_E8_CALL_SP, target, self.this, &args).eax
    }

    fn take_grid_flags(&mut self) -> (u32, u32) {
        layout::take_grid_flags(self.mem, self.this)
    }

    fn store_hit(&mut self, record: u32, flags: (u32, u32)) {
        layout::store_grid_hit(self.mem, record, flags, self.this);
    }
}

/// thiscall `(a, b, c, d, e, f, g, h)`, ret 0x20: `CBody::VfF4` (0x4e2e90), the box test on the body's grid. Its frame
/// is below the entry esp (`layout::box_edge_hit_frame`); eax is the hit flag.
pub(crate) fn body_vf_f4(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, d, e, f, g, h, ..] = r.stack;
    let frame = layout::box_edge_hit_frame(r.esp, [a, b, c, d, e, f, g, h]);
    let mut calls = F4CallsAt { mem: &mut ctx.mem, host: ctx.host, this: r.ecx, sp: layout::box_edge_hit_sp(r.esp) };
    let hit = body_slots::box_edge_hit(&mut calls, &frame);
    Ok(Return::void(hit, 0x20))
}

/// The calls of CBody::VfF4 in the emulator, each at the entry stack pointer the original gives its callee (`sp` is
/// the saved-register frame's bottom, `layout::box_edge_hit_sp`).
struct F4CallsAt<'a> {
    mem: &'a mut Mem,
    host: &'a dyn Host,
    this: u32,
    sp: u32,
}

impl F4CallsAt<'_> {
    /// A thiscall with no arguments on the body's vtable slot `slot`, entered at `sp`.
    fn virtual_call(&mut self, slot: u32, sp: u32) -> u32 {
        let target = layout::vtable_slot(self.mem, self.this, slot);
        self.host.call_at(sp, target, self.this, &[]).eax
    }
}

impl F4Calls for F4CallsAt<'_> {
    fn vf34(&mut self) -> u32 {
        self.virtual_call(VF34_SLOT, self.sp - 4)
    }

    // The original pushes the compose arguments before the second and third calls, so those run lower; the callee
    // takes none, and nothing it writes is read after the call, so the entry at `sp - 4` is equivalent.
    fn vf30(&mut self) -> u32 {
        self.virtual_call(VF30_SLOT, self.sp - 4)
    }

    fn fetch_x(&mut self, matrix: u32, out: u32, t: u32) {
        self.host.call_at(self.sp - 12, FETCH_X, matrix, &[out, t]);
    }

    fn edge_edge(&mut self, from: u32, to: u32, hit: u32, t: u32, normal: u32, material: u32) -> bool {
        let grid = Body::new(self.this).grid(self.mem);
        let args = [from, to, hit, t, normal, material];
        self.host.call_at(self.sp - 0x1c, E37_EDGE_EDGE, grid, &args).eax != 0
    }

    fn store_hit(&mut self, f: u32, g: u32, h: u32) {
        layout::store_vf_f4_hit(self.mem, f, g, h);
    }

    fn compose_a(&mut self, transform: u32, point: u32, out: u32) {
        let identity = self.mem.u32(layout::IDENTITY_WORKER_SLOT);
        self.host.call_at(self.sp - 0x10, COPY16_WALK, transform, &[point, identity, out]);
    }

    fn compose_b(&mut self, transform: u32, point: u32, out: u32) {
        let identity = self.mem.u32(layout::IDENTITY_WORKER_SLOT);
        self.host.call_at(self.sp - 0x10, COMPOSE_B, transform, &[point, identity, out]);
    }
}
