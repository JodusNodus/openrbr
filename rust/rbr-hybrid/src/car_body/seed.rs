//! The replay seed and the car's init (`UpdateSeed`, `UpdateA720`, `UpdateMrr`).

use rbr_physics::car_body::apply_seed::{self, SeedCalls};
use rbr_physics::car_body::layout;
use rbr_physics::car_body::links::{self, LinkRef, ListRef};
use rbr_physics::car_body::update_mrr::{self, UpdateMrrCalls};
use rbr_physics::car_body::update_seed::{self, UpdateSeedCalls};
use rbr_physics::mem::Mem;

use super::step::{EMIT_DF0, MOTION_PROBE, SLOT_S8C};
use crate::vcu::READ_PACKET;
use crate::{Ctx, Decline, Host, Regs, Return};
use rbr_physics::car_body::store::CarStore;

/// The calls `UpdateSeed` makes out of its own body (`update_seed.rs`), each at the original's esp: its frame is the
/// 0x28-byte locals and three pushed registers below the entry `esp`, the local seed at `esp - 0x20`, the emit's pair at
/// `esp - 0x28`, and MotionProbe's out word in the argument's own slot (`esp + 4`).
struct Seed<'a> {
    mem: Mem,
    host: &'a dyn Host,
    esp: u32,
}

impl Seed<'_> {
    /// The frame's stack pointer after the three pushes, less the bytes pushed for the call: the callee's entry is 4
    /// below it.
    fn sp(&self, pushed: u32) -> u32 {
        self.esp - 0x34 - pushed - 4
    }

    fn slot(&self, obj: u32, offset: u32) -> u32 {
        let target = self.mem.u32(self.mem.u32(obj) + offset);
        self.host.call_at(self.sp(0), target, obj, &[]).eax
    }
}

impl UpdateSeedCalls for Seed<'_> {
    fn read_packet(&mut self, _mem: &mut Mem, obj: u32, seed: u32) {
        self.host.call_at(self.sp(4), READ_PACKET, obj, &[seed]);
    }

    fn nop_d570(&mut self, _mem: &mut Mem, obj: u32, seed: u32) {
        self.host.call_at(self.sp(4), NOP_D570, obj, &[seed]);
    }

    fn m5970(&mut self, _mem: &mut Mem, obj: u32, seed: u32) {
        self.host.call_at(self.sp(4), M5970, obj, &[seed]);
    }

    fn is_state2(&mut self, obj: u32) -> u32 {
        self.host.call_at(self.sp(0), E19_IS_STATE2, obj, &[]).eax
    }

    fn s8c(&mut self, car: u32) -> u32 {
        self.slot(car, SLOT_S8C)
    }

    fn s158(&mut self, car: u32) {
        self.slot(car, SLOT_S158);
    }

    fn motion_probe(&mut self, car: u32) -> (u32, u32) {
        let out = self.esp + 4;
        let ret = self.host.call_at(self.sp(4), MOTION_PROBE, car, &[out]).eax;
        (ret, self.mem.u32(out))
    }

    fn emit_problem(&mut self, obj: u32, pair: [u32; 2]) {
        let at = self.esp - 0x28;
        self.mem.set_u32(at, pair[0]);
        self.mem.set_u32(at + 4, pair[1]);
        let target = crate::ngp::call_target(&self.mem, EMIT_PROBLEM_SITE);
        self.host.call_at(self.sp(4), target, obj, &[at]);
    }

    fn assist_df0(&mut self, car: u32, words: [u32; 3]) {
        self.host.call_at(self.sp(12), EMIT_DF0, car, &words);
    }
}

const NOP_D570: u32 = 0x50D570;
const M5970: u32 = 0x505970;
const E19_IS_STATE2: u32 = 0x499900;
pub(super) const SLOT_S158: u32 = 0x158;
/// UpdateSeed's call of the problem emit 0x4E8A70, which NGP retargets (to 0x100ed8a0).
const EMIT_PROBLEM_SITE: u32 = 0x4E9306;

/// thiscall `(p)`, `ret 4`: `CCar::UpdateSeed`, the car's per-step seed (the control emit's connection). The original's
/// last load leaves the new step count in eax.
pub(super) fn update_seed(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Seed { mem: ctx.mem.clone(), host: ctx.host, esp: r.esp };
    let steps = update_seed::update_seed(&mut calls, &mut ctx.mem, r.ecx, r.stack[0], r.esp - 0x20);
    Ok(Return::void(steps, 4))
}

/// fastcall `(replay)`: `E19IsState2`, 1 when the replay is in its state 2.
pub(super) fn replay_is_state2(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(ctx.mem.replay_is_state2(r.ecx), 0))
}

/// thiscall `(a)`: `CCar::AppendA980`, the link `a` goes to the front of the car's list (`Unlink` first when it is
/// linked elsewhere). A null car is not ported (the asm skips the append).
pub(super) fn append_a980(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    if r.ecx == 0 {
        return Err(Decline::Unsupported("the car is null (the asm skips the append)"));
    }
    links::append(&mut ctx.mem, ListRef::new(r.ecx), LinkRef::new(r.stack[0]));
    Ok(Return::void(0, 4))
}

/// The calls `UpdateA720` makes (`apply_seed.rs`): its reads of the seed, the wheel list and the frame object, each
/// made when the asm makes it; the externals and `UpdateMrr` are the original's (a bound one runs in Rust again).
struct Seeded<'a> {
    mem: Mem,
    host: &'a dyn Host,
    car: u32,
    seed: u32,
}

impl SeedCalls for Seeded<'_> {
    fn seed(&mut self) -> layout::Seed {
        layout::Seed::load(&self.mem, self.seed)
    }

    fn wheels(&mut self) -> [u32; 4] {
        self.mem.wheel_pointers(self.car)
    }

    fn frame_object(&mut self) -> u32 {
        self.mem.frame_object(self.car)
    }

    fn call(&mut self, callee: u32, this: u32, args: &[u32]) {
        self.host.call(callee, this, args);
    }

    fn store_word(&mut self, at: u32, word: u32) {
        self.mem.set_u32(at, word);
    }

    fn update_mrr(&mut self, car: u32) {
        self.host.call(UPDATE_MRR, car, &[]);
    }
}

const UPDATE_MRR: u32 = 0x4E9C70;
/// `CMultiSignal1<const CSigMrRCarInit&>::Emit` (0x4e8cb0): the replay manager records the car (not physics).
pub(super) const EMIT_CAR_INIT: u32 = 0x4E8CB0;
/// `UpdateMrr`'s locals below its return address: the seed block `UpdateSeed` takes (0x20 bytes) and the emit's pair.
const MRR_BLOCK: u32 = 0x20;
const MRR_PAIR: u32 = 0x28;

/// thiscall `(seed)`, `ret 4`: `CCar::UpdateA720`, the setups into the car's definitions, then `UpdateMrr`.
pub(super) fn update_a720(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Seeded { mem: ctx.mem.clone(), host: ctx.host, car: r.ecx, seed: r.stack[0] };
    apply_seed::apply_seed(&mut calls, r.ecx);
    // the asm keeps its wheel loop's counter in the argument's slot, which the loop leaves at 0
    ctx.mem.set_u32(r.esp + 4, 0);
    Ok(Return::void(0, 4))
}

/// The calls `UpdateMrr` makes (`update_mrr.rs`): the original's, and the emit with its pair where the original's frame
/// has it.
struct Mrr<'a> {
    mem: Mem,
    host: &'a dyn Host,
    pair: u32,
}

impl UpdateMrrCalls for Mrr<'_> {
    fn call(&mut self, callee: u32, this: u32, args: &[u32]) {
        self.host.call(callee, this, args);
    }

    fn emit(&mut self, obj: u32, pair: [u32; 2]) {
        self.mem.set_u32(self.pair, pair[0]);
        self.mem.set_u32(self.pair + 4, pair[1]);
        self.host.call(EMIT_CAR_INIT, obj, &[self.pair]);
    }
}

/// thiscall `()`: `CCar::UpdateMrr`, the car's wiring to its damage object and its bodies' first frames. The seed block
/// and the pair are at the original's frame offsets, so the block's unset word is the stack's as in the original.
pub(super) fn update_mrr(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Mrr { mem: ctx.mem.clone(), host: ctx.host, pair: r.esp - MRR_PAIR };
    update_mrr::update_mrr(&mut calls, &mut ctx.mem, r.ecx, r.esp - MRR_BLOCK);
    Ok(Return::void(0, 0))
}
