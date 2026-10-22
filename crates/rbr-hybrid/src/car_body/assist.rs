//! The driver assists and the retire paths (`rbr_physics::car_body::assist`).

use rbr_physics::car_body::assist::{AssistCalls, RetireCalls};
use rbr_physics::car_body::probe::ProbeCalls;
use rbr_physics::car_body::{self as car};
use rbr_physics::mem::Mem;

use super::car_calls::{car_calls, Car};
use super::seed::SLOT_S158;
use super::step::EMIT_DF0;
use crate::car_body::UPDATE_SEED;
use crate::engine_driveline::SIGNALS;
use crate::pure::OUTSIDE_ASSISTANCE_EMIT;
use crate::vcu::SET_DIRTY;
use crate::{Ctx, Decline, Host, Regs, Return};

/// The calls `Assist8DF0` makes out of its own body (`assist.rs`): the lane test, the outside emit with its code word in
/// scratch, S158 through the car's vtable, `UpdateSeed` on the seed block, and the state's `SetDirty`.
impl AssistCalls for Car<'_> {
    fn lane_test(&mut self, car: u32) -> u32 {
        self.host.call(LANE_TEST, car, &[]).eax
    }

    fn emit_outside(&mut self, code_at: u32) {
        let emitter = self.mem.u32(SIGNALS) + OUTSIDE_EMITTER;
        self.host.call(OUTSIDE_ASSISTANCE_EMIT, emitter, &[code_at]);
    }

    fn reset_car(&mut self, car: u32) {
        self.slot(car, SLOT_S158, &[]);
    }

    fn apply_control(&mut self, car: u32, block: u32) {
        self.host.call(UPDATE_SEED, car, &[block]);
    }

    fn set_dirty(&mut self, state: u32) {
        self.host.call(SET_DIRTY, state, &[]);
    }
}

/// The calls `E34_RetireA` makes: the retire emit with its code word's address, and `Assist8DF0` on the car.
impl RetireCalls for Car<'_> {
    fn emit_retire(&mut self, code_at: u32) {
        let emitter = self.mem.u32(SIGNALS) + RETIRE_EMITTER;
        let target = crate::ngp::Call::Retire.address();
        self.host.call(target, emitter, &[code_at]);
    }

    fn assist(&mut self, car: u32, a: u32, b: u32, c: u32) {
        self.host.call(EMIT_DF0, car, &[a, b, c]);
    }
}

const LANE_TEST: u32 = 0x4E7910;
/// The retire and outside-assistance emitters' offsets on the signal global (the asm's `add ecx, ...`).
const RETIRE_EMITTER: u32 = 0xABB0;
const OUTSIDE_EMITTER: u32 = 0xD378;

/// thiscall: `CCar::Test7910`, the lane test (`layout::lane_test`); eax is the verdict. The original's `LerpAt` traps past
/// the table's count, so the port declines there and the original runs.
pub(super) fn lane_test(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let car = r.ecx;
    let mut calls = car_calls(ctx, r);
    let verdict = car::layout::lane_test(&ctx.mem, car, |v| ProbeCalls::to_world(&mut calls, car, v));
    match verdict {
        Some(eax) => Ok(Return::void(eax, 0)),
        None => Err(Decline::Unsupported("LerpAt's count check (the original traps past the count)")),
    }
}

/// thiscall `(a, b, c)`, `ret 12`: `CCar::Assist8DF0`, the outside-assistance worker (`layout::assist_8df0`).
pub(super) fn assist_8df0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, ..] = r.stack;
    let mut calls = car_calls(ctx, r);
    car::layout::assist_8df0(&mut calls, &mut ctx.mem, r.ecx, r.esp, a, b, c);
    Ok(Return::void(0, 12))
}

/// thiscall `()`: `CCar::Vf158x` (slot 86), the car's slot 0 then the tail-call into `CSolid::E43Vf158`.
pub(super) fn vf158x(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    struct Calls<'a> {
        host: &'a dyn Host,
    }
    impl car::layout::Vf158xCalls for Calls<'_> {
        fn slot0(&mut self, _mem: &Mem, target: u32, sp: u32, car: u32) {
            self.host.call_at(sp, target, car, &[]);
        }

        fn solid_vf158(&mut self, _mem: &mut Mem, sp: u32, car: u32) -> u32 {
            self.host.call_at(sp, 0x52c030, car, &[]).eax
        }
    }
    let mut calls = Calls { host: ctx.host };
    let eax = car::layout::vf158x(&mut calls, &mut ctx.mem, r.ecx, r.esp);
    Ok(Return::void(eax, 0))
}

/// fastcall on the car (ecx), no args, `ret`: `E34_RetireA`, the retire announcer (`layout::retire_announce`).
pub(super) fn retire_a(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = car_calls(ctx, r);
    car::layout::retire_announce(&mut calls, &mut ctx.mem, r.ecx, r.esp);
    Ok(Return::void(0, 0))
}
