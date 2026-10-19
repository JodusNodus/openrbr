//! `E90Group`'s build (0x601270-0x602fe0): the segment, particle, spring, pin and bend pools.

use rbr_physics::entities::layout;

use crate::{Ctx, Decline, Host, Regs, Return};

struct E90BuildHost<'a>(&'a dyn Host);

impl rbr_physics::entities::layout::E90BuildCalls for E90BuildHost<'_> {
    fn allocate(&mut self, sp: u32, size: u32) -> u32 {
        self.0.call_at(sp, 0x70bec2, 0, &[size]).eax
    }

    fn init(&mut self, sp: u32, pool: u32, count: u32, mem: u32) -> u32 {
        self.0.call_at(sp, 0x601270, pool, &[count, mem]).eax
    }

    fn build_post(&mut self, sp: u32, handler: u32, segment: u32, dummies: u32, mem: u32) -> u32 {
        self.0.call_at(sp, 0x602fe0, handler, &[segment, dummies, mem]).eax
    }

    fn build_span(&mut self, sp: u32, handler: u32, span: u32, prev: u32, post: u32, dummy: u32, mem: u32) -> u32 {
        self.0.call_at(sp, 0x6029b0, handler, &[span, prev, post, dummy, mem]).eax
    }
}

struct E90SegPoolHost<'a>(&'a dyn Host);

impl rbr_physics::entities::layout::E90SegPoolCalls for E90SegPoolHost<'_> {
    fn allocate(&mut self, sp: u32, size: u32) -> u32 {
        self.0.call_at(sp, 0x70bec2, 0, &[size]).eax
    }

    fn construct(&mut self, sp: u32, segment: u32) {
        self.0.call_at(sp, 0x600ff0, segment, &[]);
    }
}

struct E90SegmentHost<'a>(&'a dyn Host);

impl rbr_physics::entities::layout::E90SegmentCalls for E90SegmentHost<'_> {
    fn init(&mut self, sp: u32, segment: u32, dummy: u32) {
        self.0.call_at(sp, 0x605b60, segment, &[dummy]);
    }

    fn parts_init(&mut self, sp: u32, segment: u32, count: u32, mem: u32) -> u32 {
        self.0.call_at(sp, 0x6026a0, segment, &[count, mem]).eax
    }

    fn springs_init(&mut self, sp: u32, segment: u32, count: u32, mem: u32) -> u32 {
        self.0.call_at(sp, 0x6027a0, segment, &[count, mem]).eax
    }

    fn bends_init(&mut self, sp: u32, segment: u32, count: u32, mem: u32) -> u32 {
        self.0.call_at(sp, 0x6028b0, segment, &[count, mem]).eax
    }

    fn pins_init(&mut self, sp: u32, segment: u32, count: u32, mem: u32) -> u32 {
        self.0.call_at(sp, 0x602930, segment, &[count, mem]).eax
    }

    fn add_particle(&mut self, sp: u32, segment: u32, pos: u32, inv_mass: f32) -> u32 {
        self.0.call_at(sp, 0x605f30, segment, &[pos, inv_mass.to_bits()]).eax
    }

    fn add_pin(&mut self, sp: u32, segment: u32, particle: u32) {
        self.0.call_at(sp, 0x606090, segment, &[particle]);
    }

    fn add_spring(&mut self, sp: u32, segment: u32, a: u32, b: u32, kind: u32, stiffness: f32) -> u32 {
        self.0.call_at(sp, 0x605fb0, segment, &[a, b, kind, stiffness.to_bits()]).eax
    }

    fn add_fixed(&mut self, sp: u32, segment: u32, spring: u32) {
        self.0.call_at(sp, 0x606070, segment, &[spring]);
    }

    fn add_bend(&mut self, sp: u32, segment: u32, spring: u32, particle: u32) {
        self.0.call_at(sp, 0x6060b0, segment, &[spring, particle]);
    }
}

/// thiscall, no stack arguments: `E90Group::Build` (0x601470). Native allocation keeps the original placement; the
/// group loop and its segment/dummy links run through the typed layout port.
pub(super) fn e90_build_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = E90BuildHost(ctx.host);
    layout::build_e90_group(&mut ctx.mem, r.ecx, r.esp, &mut calls);
    Ok(Return::void(0, 0))
}

/// thiscall `(count, block)`: `E90SegPool::Init` (0x601270), with native allocation kept at its source position.
pub(super) fn e90_seg_pool_init_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [count, block, ..] = r.stack;
    let mut calls = E90SegPoolHost(ctx.host);
    let eax = layout::init_e90_seg_pool(&mut ctx.mem, r.ecx, count, block, r.esp, &mut calls);
    Ok(Return::void(eax, 8))
}

/// thiscall `(post, dummy, block)`: `CFenceHandler::BuildPost` (0x602fe0).
pub(super) fn e90_build_post_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [post, dummy, block, ..] = r.stack;
    let mut calls = E90SegmentHost(ctx.host);
    let eax = layout::build_e90_post(&mut ctx.mem, r.ecx, post, dummy, block, r.esp, &mut calls);
    Ok(Return::void(eax, 12))
}

/// thiscall `(span, prev, next, dummy, block)`: `CFenceHandler::BuildSpan` (0x6029b0).
pub(super) fn e90_build_span_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [span, prev, next, dummy, block, ..] = r.stack;
    let mut calls = E90SegmentHost(ctx.host);
    let eax = layout::build_e90_span(&mut ctx.mem, r.ecx, span, prev, next, dummy, block, r.esp, &mut calls);
    Ok(Return::void(eax, 20))
}

struct E90SegInitHost<'a>(&'a dyn Host);

impl rbr_physics::entities::layout::E90SegInitCalls for E90SegInitHost<'_> {
    fn normalize(&mut self, sp: u32, out: u32, v: u32) {
        self.0.call_at(sp, 0x627235, 0, &[out, v]);
    }
}

struct E90PoolHost<'a>(&'a dyn Host);

impl rbr_physics::entities::layout::E90PoolCalls for E90PoolHost<'_> {
    fn allocate(&mut self, sp: u32, size: u32) -> u32 {
        self.0.call_at(sp, 0x70bf84, 0, &[size]).eax
    }

    fn release(&mut self, sp: u32, block: u32) {
        self.0.call_at(sp, 0x70c27e, 0, &[block]);
    }

    fn construct(&mut self, sp: u32, element: u32) {
        self.0.call_at(sp, 0x602220, element, &[]);
    }
}

/// thiscall `(dummy)`, `ret 4`: `E90Seg::Init` (0x605b60). The two normalisations go through the D3DX thunk.
pub(super) fn e90_seg_init_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = E90SegInitHost(ctx.host);
    layout::e90_seg_init(&mut ctx.mem, r.ecx, r.stack[0], r.esp, &mut calls);
    Ok(Return::void(0, 4))
}

/// thiscall `(count, block)`, `ret 8`: `E90PartPool::Init` (0x6026a0). Eax is the cursor.
pub(super) fn e90_part_pool_init_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [count, block, ..] = r.stack;
    let eax = layout::init_e90_part_pool(&mut ctx.mem, r.ecx, count, block, r.esp, &mut E90PoolHost(ctx.host));
    Ok(Return::void(eax, 8))
}

/// thiscall `(count, block)`, `ret 8`: `E90PtrTab::Init` (0x6027a0). Eax is the cursor.
pub(super) fn e90_ptr_tab_init_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [count, block, ..] = r.stack;
    let eax = layout::init_e90_ptr_tab(&mut ctx.mem, r.ecx, count, block, r.esp, &mut E90PoolHost(ctx.host));
    Ok(Return::void(eax, 8))
}

/// thiscall `(count, block)`, `ret 8`: `E90BendPool::Init` (0x6028b0). Eax is the cursor.
pub(super) fn e90_bend_pool_init_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [count, block, ..] = r.stack;
    let eax = layout::init_e90_bend_pool(&mut ctx.mem, r.ecx, count, block, r.esp, &mut E90PoolHost(ctx.host));
    Ok(Return::void(eax, 8))
}

/// thiscall `(count, block)`, `ret 8`: `E90PinPool::Init` (0x602930). Eax is the cursor.
pub(super) fn e90_pin_pool_init_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [count, block, ..] = r.stack;
    let eax = layout::init_e90_pin_pool(&mut ctx.mem, r.ecx, count, block, r.esp, &mut E90PoolHost(ctx.host));
    Ok(Return::void(eax, 8))
}

/// thiscall, no arguments, `ret 0`: `E90_PartCtor` (0x602220), `mov eax, ecx; ret`.
pub(super) fn e90_part_ctor_run(_ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(r.ecx, 0))
}

/// stdcall `(out, v)`, `ret 8`: `_D3DXVec3Normalize@8` (0x627235). Eax is `out`.
pub(super) fn d3dx_normalize_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [out, v, ..] = r.stack;
    let eax = layout::d3dx_vec3_normalize(&mut ctx.mem, out, v);
    Ok(Return::void(eax, 8))
}

/// thiscall `(pos, inv_mass)`, `ret 8`: `E90Seg::AddParticle` (0x605f30). Eax is the particle.
pub(super) fn e90_add_particle_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [pos, inv_mass, ..] = r.stack;
    let eax = layout::add_particle(&mut ctx.mem, r.ecx, pos, f32::from_bits(inv_mass));
    Ok(Return::void(eax, 8))
}

/// thiscall `(a, b, kind, stiffness)`, `ret 0x10`: `E90Seg::AddSpring` (0x605fb0). Eax is the spring, or 0.
pub(super) fn e90_add_spring_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, kind, stiffness, ..] = r.stack;
    let eax = layout::add_spring(&mut ctx.mem, r.ecx, a, b, kind, f32::from_bits(stiffness));
    Ok(Return::void(eax, 0x10))
}

/// thiscall `(spring)`, `ret 4`: `E90Seg::AddFixed` (0x606070).
pub(super) fn e90_add_fixed_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::add_fixed(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// thiscall `(particle)`, `ret 4`: `E90Seg::AddPin` (0x606090).
pub(super) fn e90_add_pin_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::add_pin(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// thiscall `(spring, particle)`, `ret 8`: `E90Seg::AddBend` (0x6060b0).
pub(super) fn e90_add_bend_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [spring, particle, ..] = r.stack;
    layout::add_bend(&mut ctx.mem, r.ecx, spring, particle);
    Ok(Return::void(0, 8))
}
