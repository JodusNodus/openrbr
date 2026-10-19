//! The damage handler's functors, impacts and ops (`rbr_physics::car_body::damage`, `layout::dmg_*`).

use rbr_physics::car_body::damage;
use rbr_physics::car_body::layout;
use rbr_physics::mem::Mem;

use super::car_calls::scratch;
use crate::collision::E01_VEC4_COPY_FROM;
use crate::{Ctx, Decline, Host, Regs, Return};

/// thiscall `()`: the windshield gate on `CCarDamageHandler`.
pub(super) fn test_3b20(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(damage::test_3b20(&ctx.mem, r.ecx) as u32, 0))
}

/// thiscall `(a1..a8)`, `ret 0x20`: `CCarDamageHandler::ImpactScale` (slot 2) calls slot 1 with the length scaled by 0.08.
pub(super) fn damage_impact_scale(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = ImpactHost { host: ctx.host, sp: r.esp - 0x24 };
    let eax = layout::impact_scale(&mut calls, &ctx.mem, r.ecx, r.stack[..8].try_into().unwrap());
    Ok(Return::void(eax, 0x20))
}

/// thiscall `(a, b, c, tag)` (or `(a, b, tag)`), `ret 0x10`: a damage functor (`layout::dmg_*`) on the handler (`ecx`).
fn damage_functor(ctx: &mut Ctx, r: &Regs, run: fn(&mut Mem, u32, [u32; 4]) -> u32) -> Result<Return, Decline> {
    let eax = run(&mut ctx.mem, r.ecx, r.stack[..4].try_into().unwrap());
    Ok(Return::void(eax, 0x10))
}

pub(super) fn gearbox_wear(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::gearbox_wear)
}

pub(super) fn grow_cooling_wear(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::grow_cooling_wear)
}

pub(super) fn grow_zone(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::grow_zone)
}

pub(super) fn clear_triple(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::clear_triple)
}

pub(super) fn clear_head_gasket(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::clear_head_gasket)
}

pub(super) fn clear_windshield(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::clear_windshield)
}

pub(super) fn dmg_9a0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::dmg_9a0)
}

pub(super) fn dmg_7b0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::dmg_7b0)
}

pub(super) fn dmg_aa0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::dmg_aa0)
}

pub(super) fn dmg_b20(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::dmg_b20)
}

pub(super) fn drain_word(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::drain_word)
}

pub(super) fn dmg_930(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::dmg_930)
}

pub(super) fn dmg_820(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::dmg_820)
}

pub(super) fn cap_word(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::cap_word)
}

pub(super) fn dmg_bb0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::dmg_bb0)
}

pub(super) fn dmg_cc30(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::dmg_cc30)
}

pub(super) fn fail_suspension_arm(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::fail_suspension_arm)
}

pub(super) fn fail_steering_rod(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::fail_steering_rod)
}

pub(super) fn fail_hub(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::fail_hub)
}

pub(super) fn fail_brake_disc(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::fail_brake_disc)
}

pub(super) fn fail_water_pump(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::fail_water_pump)
}

pub(super) fn grow_intercooler_wear(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::grow_intercooler_wear)
}

pub(super) fn dmg_d020(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::dmg_d020)
}

pub(super) fn set_body_state(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::set_body_state)
}

pub(super) fn zero2f0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::zero2f0)
}

pub(super) fn dmg_d300(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    damage_functor(ctx, r, layout::dmg_d300)
}

/// The handler's slot 1 at the impact's stack: the callee's return slot is the thunk's arguments' end (`sp`).
struct ImpactHost<'a> {
    host: &'a dyn Host,
    sp: u32,
}

impl layout::ImpactCalls for ImpactHost<'_> {
    fn call(&mut self, target: u32, this: u32, args: [u32; 8]) -> u32 {
        self.host.call_at(self.sp, target, this, &args).eax
    }
}

/// The impact path's calls (`layout::DamageHitCalls`), each at the original's stack.
struct DamageHit<'a>(&'a dyn Host);

impl layout::DamageHitCalls for DamageHit<'_> {
    fn functor(&mut self, sp: u32, function: u32, this: u32, args: [u32; 4]) -> u32 {
        self.0.call_at(sp, function, this, &args).eax
    }

    fn copy_vec4(&mut self, sp: u32, dst: u32, src: u32) {
        self.0.call_at(sp, E01_VEC4_COPY_FROM, dst, &[src]);
    }

    fn emit(&mut self, sp: u32, emit: u32, emitter: u32, signal: u32) {
        self.0.call_at(sp, emit, emitter, &[signal]);
    }
}

/// thiscall `(a1, a2, a3, dmg, a5, src_a, src_b, _)`, `ret 0x20`: `E36DamageOps::Slot1`, the impact on a zone.
pub(super) fn damage_slot1(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let args = r.stack[..8].try_into().unwrap();
    let tuning = crate::ngp::damage_tuning(ctx);
    let eax = layout::damage_slot1(&mut DamageHit(ctx.host), &mut ctx.mem, r.ecx, r.esp, args, tuning);
    Ok(Return::void(eax, 0x20))
}

/// thiscall `(idx, v)`, `ret 8`: `ZoneScaled`, 1 when a zone was raised.
pub(super) fn zone_scaled(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [idx, v, ..] = r.stack;
    let eax = layout::zone_scaled(&mut ctx.mem, r.ecx, idx, f32::from_bits(v));
    Ok(Return::void(eax, 8))
}

/// thiscall `(idx, scale, best)`, `ret 0xc`: `ApplyFunctors`, the functors at the original's stack.
pub(super) fn apply_functors(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [idx, scale, best, ..] = r.stack;
    let mut calls = DamageHit(ctx.host);
    let eax = layout::apply_functors(&mut calls, &mut ctx.mem, r.ecx, r.esp, idx, f32::from_bits(scale), best);
    Ok(Return::void(eax, 0xc))
}

/// thiscall `(a, b, c, d)`, `ret 0x10`: `EngineGate`, 5 when the gate opens.
pub(super) fn engine_gate(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, ..] = r.stack;
    let mut calls = DamageHit(ctx.host);
    let eax = layout::engine_gate(&mut calls, &mut ctx.mem, r.ecx, r.esp, f32::from_bits(a), f32::from_bits(b));
    Ok(Return::void(eax, 0x10))
}

/// thiscall `(a, b, c, tag)`, `ret 0x10`: `RingStore`, the speech functor.
pub(super) fn ring_store(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let args = r.stack[..4].try_into().unwrap();
    let eax = layout::ring_store(&mut DamageHit(ctx.host), &mut ctx.mem, r.ecx, r.esp, args);
    Ok(Return::void(eax, 0x10))
}

/// cdecl `(lo, hi, seed)`: `E35_Rand`, the value in st0 and the quotient in eax.
pub(super) fn damage_rand(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [lo, hi, seed, ..] = r.stack;
    let (value, eax) = layout::damage_rand(&mut ctx.mem, f32::from_bits(lo), f32::from_bits(hi), seed);
    Ok(Return::float_with_eax(eax, value as f64, 0))
}

/// The calls `E36DamageOps::InitTables` and `Slot4` make to the original signal and vector helpers.
struct DamageOps<'a>(&'a dyn Host);

impl layout::DamageOpsCalls for DamageOps<'_> {
    fn call(&mut self, _mem: &mut Mem, callee: u32, this: u32, args: &[u32]) {
        self.0.call(callee, this, args);
    }
}

/// thiscall `()`: `E36DamageOps::Ctor`, the fixed handler tables and the E36 zone state.
pub(super) fn damage_ops_ctor(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let result = layout::construct_damage_ops(&mut ctx.mem, r.ecx);
    Ok(Return::void(result, 0))
}

/// thiscall `(index, scale, value, tag)`, `ret 0x10`: append an entry to the selected table.
pub(super) fn add_damage_functor(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [index, scale, value, tag, ..] = r.stack;
    let added = damage::add_functor(&mut ctx.mem, r.ecx, index as i32, scale, value, tag);
    Ok(Return::void(u32::from(added), 0x10))
}

/// thiscall `()`: rebuild the fixed E36 functor tables.
pub(super) fn damage_ops_init_tables(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = DamageOps(ctx.host);
    layout::init_damage_ops_tables(&mut calls, &mut ctx.mem, r.ecx);
    Ok(Return::void(0, 0))
}

/// thiscall `()`: `CCar::UpdateMrr`'s damage slot 4.
pub(super) fn damage_level_fractions(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = DamageOps(ctx.host);
    layout::damage_level_fractions(&mut calls, &mut ctx.mem, r.ecx, scratch(r));
    Ok(Return::void(0, 0))
}
