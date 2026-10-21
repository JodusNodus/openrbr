//! The passenger's step (`CPassenger::Step`, 0x523160) and the two steps it calls, the spring (`E45PSmalls::M29E0`)
//! and the pull (`E45PSmalls::M2BC0`); the step's walk (`E43PairWalk::MB560`) runs through the host.

use rbr_physics::mem::Mem;
use rbr_physics::physics_core::layout::{self, passenger_ctor};

use super::{HOST_AT, MB560, PULL, SPRING, SPRING_EAX_AT, TEMPS, WALKER};
use crate::{Ctx, Decline, Host, Regs, Return};

/// thiscall `(dt)`: `E45PSmalls::M29E0`.
pub(super) fn spring_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    passenger_ctor::spring(&mut ctx.mem, r.ecx, f32::from_bits(r.stack[0]));
    let host = ctx.mem.u32(r.ecx + HOST_AT);
    Ok(Return::void(host + SPRING_EAX_AT, 4))
}

/// thiscall `(dt)`: `E45PSmalls::M2BC0`.
pub(super) fn pull_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = layout::passenger_pull(&mut ctx.mem, r.ecx, f32::from_bits(r.stack[0]));
    Ok(Return::void(eax, 4))
}

/// thiscall `(dt)`: `CPassenger::Step`.
pub(super) fn step_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = StepCalls { mem: ctx.mem.clone(), host: ctx.host, temp: (r.esp - TEMPS) & !0xf };
    let sentinel = "the physics object's gravity pointer is the original's sentinel (its read is address 0)";
    let eax = layout::passenger_step(&mut ctx.mem, &mut calls, r.ecx, f32::from_bits(r.stack[0]))
        .ok_or(Decline::Unsupported(sentinel))?;
    Ok(Return::void(eax, 4))
}

/// The step's calls on the original: the walk's point is written to a temporary the walk reads through its pointer.
struct StepCalls<'a> {
    mem: Mem,
    host: &'a dyn Host,
    temp: u32,
}

impl passenger_ctor::StepHost for StepCalls<'_> {
    fn walk(&mut self, point: [f32; 3], host: u32, out: u32) {
        for (k, value) in point.into_iter().enumerate() {
            self.mem.set_f32(self.temp + 4 * k as u32, value);
        }
        let walker = self.mem.u32(WALKER);
        self.host.call(MB560, walker, &[self.temp, host, out]);
    }

    fn spring(&mut self, this: u32, dt: f32) {
        self.host.call(SPRING, this, &[dt.to_bits()]);
    }

    fn pull(&mut self, this: u32, dt: f32) -> u32 {
        self.host.call(PULL, this, &[dt.to_bits()]).eax
    }
}

/// `CSolid`'s slot 24, the call `E43Vf5C` ends in.
pub(super) const SLOT_24: u32 = 0x60;
