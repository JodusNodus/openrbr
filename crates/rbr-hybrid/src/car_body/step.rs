//! The car's step: `StepEngine`, the emit, `Vf64x`, the aero step, `Vf6cx` and the update's callees.

use rbr_physics::car_body::drag::DragCalls;
use rbr_physics::car_body::emit::Receive;
use rbr_physics::car_body::step_engine::StepEngineCalls;
use rbr_physics::car_body::transform::FRAME;
use rbr_physics::car_body::update_9af0::{self, Update9AF0Calls};
use rbr_physics::car_body::vf64x::{self, Vf64xCalls};
use rbr_physics::car_body::vf6cx::{self, Vf6cxCalls};
use rbr_physics::car_body::{self as car, layout};
use rbr_physics::math::vec::{Vec3, Vec4};
use rbr_physics::mem::Mem;

use super::car_calls::{
    car_calls, Car, C, DRIVE_STEP, INDEX_LIMIT, SEND, SLOT_RECEIVE, SNAP_9360, STEP_ENGINE_NAME, STEP_ENGINE_SAMPLE,
};
use crate::engine_driveline::SIGNALS;
use crate::entities::{MB560, REMOVE_EXCLUSION, SIM_FD00};
use crate::physics_core::{MOUNTED_VF6C, PASSENGER_STEP};
use crate::rigid_body::{DRIVE, E43VF6C, E46_IMPULSE640};
use crate::vcu::GET_4540;
use crate::{entities, physics_core, vcu, Ctx, Decline, Host, Regs, Return};
use rbr_physics::car_body::store::CarStore;

/// thiscall `(dt)`, `ret 4`: `CCar::StepEngine` (`Hook`), the steering and force-feedback step in its profiler scope. The channel is in the
/// frame at -0x14, and `ProfExit`'s counter word at -0x2c (its entry is at -0x24).
pub(super) fn step_engine(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let dt = f32::from_bits(r.stack[0]);
    let eax = physics_core::scope(ctx, r.esp - 0x14, STEP_ENGINE_SAMPLE, STEP_ENGINE_NAME, r.esp - 0x2c, |ctx| {
        let mut calls = car_calls(ctx, r);
        car::step_engine::step_engine(&mut calls, &mut ctx.mem, r.ecx, dt);
    });
    Ok(Return::void(eax, 4))
}

/// The calls `StepEngine` makes out of its body: the scratch sim, the drive and the entity step, and `Snap9360` through the
/// host; `UpdateFfbTorque` is the bound kernel.
impl StepEngineCalls for Car<'_> {
    fn scratch_step(&mut self, _mem: &mut Mem, car: u32, dt: f32) {
        self.host.call(vcu::STEP, layout::scratch_at(car), &[dt.to_bits()]);
    }

    fn drive_step(&mut self, _mem: &mut Mem, car: u32, dt: f32) {
        self.host.call(DRIVE_STEP, layout::drive_at(car), &[dt.to_bits()]);
    }

    fn entity_vf58(&mut self, _mem: &mut Mem, car: u32, dt: f32) {
        self.host.call(entities::VF58, car, &[dt.to_bits()]);
    }

    fn update_ffb_torque(&mut self, mem: &mut Mem, car: u32) {
        layout::update_ffb_torque(mem, car);
    }

    fn snap_9360(&mut self, _mem: &mut Mem, car: u32) {
        self.host.call(SNAP_9360, car, &[]);
    }
}

/// thiscall `(emitter, signal)`: `CMultiSignal1<T>::Emit` on the car's signal emitter, the receivers answered in place.
/// Declines where the port asserts (the emitter is already emitting, or the index is out of range).
pub(super) fn emit(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let emitter = r.ecx;
    let source = ctx.mem.u32(emitter + 4);
    if source != 0 {
        if ctx.mem.u32(emitter) != 0 {
            return Err(Decline::Unsupported("the emitter is already emitting"));
        }
        if ctx.mem.u32(source) >= INDEX_LIMIT {
            return Err(Decline::Unsupported("the signal index is out of range"));
        }
    }
    let mut sink = Receivers { mem: ctx.mem.clone(), host: ctx.host };
    car::emit::emit(&mut sink, &mut ctx.mem, emitter, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// The receivers' `Receive(signal)`: vtable slot 1 of each list node.
struct Receivers<'a> {
    mem: Mem,
    host: &'a dyn Host,
}

impl Receive for Receivers<'_> {
    fn receive(&mut self, node: u32, signal: u32) {
        let target = self.mem.u32(self.mem.u32(node) + SLOT_RECEIVE);
        self.host.call(target, node, &[signal]);
    }
}

/// The calls `Vf64x` makes out of its own body (`vf64x.rs`): the car's externals, and the walk of +0x380 by its address.
impl Vf64xCalls for Car<'_> {
    fn e5224f0(&mut self, obj: u32) {
        self.host.call(0x5224F0, obj, &[]);
    }

    fn e52cb20(&mut self, _mem: &mut Mem, car: u32) {
        self.host.call(0x52CB20, car, &[]);
    }

    fn walk_380(&mut self, car: u32) -> Vec4 {
        let (out, frame) = (self.scratch + C, self.mem.u32(FRAME));
        self.host.call(MB560, frame, &[car + 0x380, car, out]);
        Vec4::load(&self.mem, out)
    }

    fn e495090(&mut self, obj: u32, first: f32, second: f32) {
        self.host.call(0x495090, obj, &[first.to_bits(), second.to_bits()]);
    }

    fn e521920(&mut self, obj: u32) -> u32 {
        self.host.call(0x521920, obj, &[]).eax
    }

    fn e521740(&mut self, obj: u32) {
        self.host.call(0x521740, obj, &[]);
    }
}

/// thiscall: `CCar::Vf64x`, the car's per-step composite (`vf64x.rs`).
pub(super) fn step_aero(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    if ctx.mem.u32(FRAME) == 0 {
        return Err(Decline::Unsupported("the frame global is unset (the original halts)"));
    }
    let mut calls = car_calls(ctx, r);
    let factor = crate::ngp::drag_factor(ctx);
    vf64x::step_aero(&mut calls, &mut ctx.mem, r.ecx, factor, crate::ngp::variant());
    Ok(Return::void(0, 0))
}

/// The offsets of `Vf6cx`'s buffers in the scratch: the 28-byte brake-wall buffer (`BUF`), the timer's double for the drive
/// thunk (`TIMER`), and a signal payload (`PAYLOAD`: the emit takes its address, and the words follow it at +4).
const BUF: u32 = 0x80;
const TIMER: u32 = 0xA0;
const PAYLOAD: u32 = 0xB0;

/// The globals and callees `Vf6cx` takes from the original (its asm's pushes are the host call's words, in order).
const RAYCAST: u32 = 0x4E4960;
const RAYCAST_MANAGER: u32 = 0x7C63C8;
/// Vf6cx's call of the brake-wall emit (0x4E4490), which NGP retargets (to 0x100ed470).
const BRAKE_WALL_EMIT_SITE: u32 = 0x4EA3B0;
const SCRATCH_NOP_A: u32 = 0x536F70;
const DRIVE_NOP3B0: u32 = 0x5043B0;
const SLOT_S80: u32 = 0x80;
const SLOT_S90: u32 = 0x90;
const BODY_TEST_1920: u32 = 0x521920;
const BODY_TEST_1940: u32 = 0x521940;
const BODY_STEP_BIG: u32 = 0x522100;
const DRIVE_THUNK: u32 = 0x50A4F0;
const TIMEKEEPER: u32 = 0x40;
const TIMEKEEPER_RECORD: u32 = 0x4A0540;
const TIMEKEEPER_UPDATE: u32 = 0x4A0320;
const UPDATE_9AF0: u32 = 0x4E9AF0;
const EMIT_POSITION: u32 = 0x4E8B30;
const EMIT_MRR: u32 = 0x4E8BF0;
/// The emitters' offsets on the signal global (the asm's `add ecx, ...`).
const POSITION_EMITTER: u32 = 0xE220;
const MRR_EMITTER: u32 = 0xE650;
const BRAKE_WALL_EMITTER: u32 = 0xA780;

impl Car<'_> {
    /// Writes the bytes at `at`.
    fn put_bytes(&mut self, at: u32, bytes: &[u8]) {
        for (k, byte) in bytes.iter().enumerate() {
            self.mem.set_u8(at + k as u32, *byte);
        }
    }

    /// Reads `out.len()` bytes at `at` back.
    fn get_bytes(&self, at: u32, out: &mut [u8]) {
        for (k, byte) in out.iter_mut().enumerate() {
            *byte = self.mem.u8(at + k as u32);
        }
    }

    /// The payload's words at the emitter's pointer: the asm writes them from `P + 4`, and the pointer is `P`.
    fn payload(&mut self, words: &[u32]) -> u32 {
        let at = self.scratch + PAYLOAD;
        for (k, word) in words.iter().enumerate() {
            self.mem.set_u32(at + 4 + 4 * k as u32, *word);
        }
        at
    }
}

/// The calls `Vf6cx` makes out of its own body (`vf6cx.rs`). Each is the original's call with its pushes in order; the
/// buffers of the brake wall go through the scratch, since the original's callees write them in memory.
impl Vf6cxCalls for Car<'_> {
    fn passenger_step(&mut self, _mem: &mut Mem, passenger: u32, dt: f32) {
        self.host.call(PASSENGER_STEP, passenger, &[dt.to_bits()]);
    }

    fn scratch_nop_a(&mut self, _mem: &mut Mem, scratch: u32, dt: f32) {
        self.host.call(SCRATCH_NOP_A, scratch, &[dt.to_bits()]);
    }

    fn drive_nop3b0(&mut self, _mem: &mut Mem, drive: u32, dt: f32) {
        self.host.call(DRIVE_NOP3B0, drive, &[dt.to_bits()]);
    }

    fn solid_vf6c(&mut self, _mem: &mut Mem, car: u32, dt: f32) {
        self.host.call(E43VF6C, car, &[dt.to_bits()]);
    }

    fn raycast(&mut self, _mem: &mut Mem, car: u32, out: &mut [u8; 28]) -> u32 {
        let at = self.scratch + BUF;
        self.put_bytes(at, out);
        let manager = self.mem.u32(RAYCAST_MANAGER);
        let hit = self.host.call(RAYCAST, manager, &[car + 0x140, at]).eax;
        self.get_bytes(at, out);
        hit
    }

    /// The pushes are `dt`, the word at +0x10, the word at +0x14 and the buffer, so the callee reads `f` and `p` as the
    /// port takes them from the buffer.
    fn impulse(&mut self, _mem: &mut Mem, car: u32, out: &mut [u8; 28], f: f32, p: u32, dt: f32) {
        let at = self.scratch + BUF;
        self.put_bytes(at, out);
        self.host.call(E46_IMPULSE640, car, &[at, f.to_bits(), p, dt.to_bits()]);
        self.get_bytes(at, out);
    }

    fn emit_brake_wall(&mut self, mem: &mut Mem, out: &[u8; 28]) {
        let at = self.scratch + BUF;
        self.put_bytes(at, out);
        let emitter = mem.u32(SIGNALS) + BRAKE_WALL_EMITTER;
        let target = crate::ngp::call_target(mem, BRAKE_WALL_EMIT_SITE);
        self.host.call(target, emitter, &[at]);
    }

    fn pair_walk(&mut self, a: Vec3, car: u32) -> Vec3 {
        DragCalls::pair_walk(self, a, car)
    }

    fn wheel_s80(&mut self, _mem: &mut Mem, wheel: u32) -> u32 {
        self.slot(wheel, SLOT_S80, &[]).eax
    }

    fn wheel_s90(&mut self, _mem: &mut Mem, wheel: u32) {
        self.slot(wheel, SLOT_S90, &[]);
    }

    fn wheel_simfd00(&mut self, _mem: &mut Mem, wheel: u32) -> u32 {
        self.host.call(SIM_FD00, wheel, &[]).eax
    }

    fn body_test_1920(&mut self, _mem: &mut Mem, body: u32) -> u32 {
        self.host.call(BODY_TEST_1920, body, &[]).eax
    }

    fn body_vf6c(&mut self, _mem: &mut Mem, body: u32, dt: f32) {
        self.host.call(MOUNTED_VF6C, body, &[dt.to_bits()]);
    }

    fn body_test_1940(&mut self, _mem: &mut Mem, body: u32) -> u32 {
        self.host.call(BODY_TEST_1940, body, &[]).eax
    }

    fn body_step_big(&mut self, _mem: &mut Mem, body: u32) -> u32 {
        self.host.call(BODY_STEP_BIG, body, &[]).eax
    }

    fn body_excl(&mut self, _mem: &mut Mem, body: u32, car: u32) {
        self.host.call(REMOVE_EXCLUSION, body, &[car]);
    }

    /// The thunk takes the car's word at +0x17c and a pointer to the timer's double (a copy in the scratch).
    fn drive_thunk(&mut self, mem: &mut Mem, car: u32, timer: f64) -> f32 {
        let at = self.scratch + TIMER;
        self.put_bytes(at, &timer.to_bits().to_le_bytes());
        let word = mem.drive_line_word(car);
        let drive = self.mem.u32(DRIVE);
        self.host.call(DRIVE_THUNK, drive, &[word, at]).st0 as f32
    }

    /// `Record` takes the timer by address: the copy the drive thunk call already wrote in the scratch.
    fn timekeeper_record(&mut self, mem: &mut Mem, f8: f32, _timer: f64) {
        let keeper = mem.u32(mem.u32(layout::WORLD) + TIMEKEEPER);
        self.host.call(TIMEKEEPER_RECORD, keeper, &[f8.to_bits(), self.scratch + TIMER]);
    }

    fn timekeeper_update(&mut self, mem: &mut Mem) -> u32 {
        let keeper = mem.u32(mem.u32(layout::WORLD) + TIMEKEEPER);
        self.host.call(TIMEKEEPER_UPDATE, keeper, &[]).eax
    }

    fn drive_get4540(&mut self, _mem: &mut Mem, drive: u32) -> f32 {
        self.host.call(GET_4540, drive, &[]).st0 as f32
    }

    fn emit_signals(&mut self, _mem: &mut Mem, car: u32, v: u32) {
        self.host.call(UPDATE_9AF0, car, &[v]);
    }

    fn emit_position(&mut self, mem: &mut Mem, payload: [u32; 5]) {
        let at = self.payload(&payload);
        let emitter = mem.u32(SIGNALS) + POSITION_EMITTER;
        self.host.call(EMIT_POSITION, emitter, &[at]);
    }

    fn emit_mrr(&mut self, mem: &mut Mem, payload: [u32; 3]) {
        let at = self.payload(&payload);
        let emitter = mem.u32(SIGNALS) + MRR_EMITTER;
        self.host.call(EMIT_MRR, emitter, &[at]);
    }
}

/// thiscall `(dt)`, `ret 4`: `CCar::Vf6cx`, the car's per-step composite (`vf6cx.rs`).
pub(super) fn step_car(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    if ctx.mem.u32(FRAME) == 0 {
        return Err(Decline::Unsupported("the frame global is unset (the original halts)"));
    }
    let dt = f32::from_bits(r.stack[0]);
    let mut calls = car_calls(ctx, r);
    vf6cx::step_car(&mut calls, &mut ctx.mem, r.ecx, dt, crate::ngp::variant());
    Ok(Return::void(0, 4))
}

/// thiscall, no args, `ret 0`: `CCar::Vf98x` (0x4e6dd0, the car's slot 38), the countdown worker (`layout::run_countdown`).
pub(super) fn run_countdown(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = car_calls(ctx, r);
    layout::run_countdown(&mut ctx.mem, &mut calls, r.ecx, r.esp).ok_or(DRIVE_LINE_TRAP)?;
    Ok(Return::void(0, 0))
}

/// The drive line index past its count: `CDriveLine::LerpAt` traps in the original.
const DRIVE_LINE_TRAP: Decline = Decline::Unsupported("drive line index past its count (LerpAt traps)");

/// thiscall `(a, b)`, `ret 8`: `CCar::Split17C` (0x4e5530). Eax is `b`, the normalized vector (`Normalize` returns it).
pub(super) fn split_17c(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, ..] = r.stack;
    layout::split_17c(&mut ctx.mem, r.ecx, a, b).ok_or(DRIVE_LINE_TRAP)?;
    Ok(Return::void(b, 8))
}

/// thiscall `(a, b, c)`, `ret 0xc`: `CCar::Sampler54A0` (0x4e54a0). The car is not read; eax is the vote's 1 or 0.
pub(super) fn vote_probe(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, ..] = r.stack;
    let mut calls = car_calls(ctx, r);
    let hit = layout::vote_probe(&mut ctx.mem, &mut calls, r.esp, a, b, c);
    Ok(Return::void(hit, 0xc))
}

impl layout::Sampler54Calls for Car<'_> {
    fn vote(&mut self, mem: &mut Mem, sp: u32, probe_a: u32, probe_b: u32, raw: u32) -> u32 {
        let obj = rbr_physics::stage::layout::collision_at_operand(mem, layout::SAMPLER_OPERAND);
        let target = mem.u32(mem.u32(obj));
        self.host.call_at(sp, target, obj, &[probe_a, probe_b, raw]).eax
    }
}

/// thiscall `(v)`, `ret 4`: `CGhostCarManager::SetPlayback` (0x492e90), the respawn signal's receiver of the ghost
/// manager (`[this + 0x96340] = 1`).
pub(super) fn ghost_set_playback(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    ctx.mem.set_u32(r.ecx + 0x9_6340, 1);
    Ok(Return::void(0, 4))
}

/// thiscall `(sel, sample)`, `ret 8`: `CCar::S184` (0x4e5270), the car's slot 97 (`layout::slot97`).
pub(super) fn s184(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [sel, sample, ..] = r.stack;
    let mut calls = car_calls(ctx, r);
    layout::slot97(&mut ctx.mem, &mut calls, r.ecx, sel, sample, r.esp, crate::ngp::variant());
    Ok(Return::void(0, 8))
}

/// thiscall `(a, b)`, `ret 8`: `CCar::Vf180` (0x4e9660), the car's slot 96, the respawn worker (`layout::respawn_car`).
pub(super) fn respawn_car(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, ..] = r.stack;
    let mut calls = car_calls(ctx, r);
    layout::respawn_car(&mut ctx.mem, &mut calls, r.ecx, a, b, r.esp);
    Ok(Return::void(0, 8))
}

impl layout::Slot97Calls for Car<'_> {
    fn respawn_car(&mut self, mem: &mut Mem, sp: u32, car: u32, a: u32, b: u32) {
        let target = mem.u32(mem.u32(car) + 0x180);
        self.host.call_at(sp, target, car, &[a, b]);
    }
}

impl layout::Vf180Calls for Car<'_> {
    fn slot17_e0(&mut self, mem: &mut Mem, sp: u32, car: u32) {
        let obj = car + 0xE0;
        let target = mem.u32(mem.u32(obj) + 0x44);
        self.host.call_at(sp, target, obj, &[]);
    }

    fn xform_8130(&mut self, _mem: &mut Mem, sp: u32, car: u32, a: u32, out: u32) {
        self.host.call_at(sp, 0x4e8130, car, &[a, out]);
    }

    fn clear_acc(&mut self, _mem: &mut Mem, sp: u32, car: u32) {
        self.host.call_at(sp, 0x529e60, car, &[]);
    }

    fn slot36(&mut self, mem: &mut Mem, sp: u32, car: u32) {
        let target = mem.u32(mem.u32(car) + 0x90);
        self.host.call_at(sp, target, car, &[]);
    }

    fn emit_respawn(&mut self, _mem: &mut Mem, sp: u32, emitter: u32, payload: u32) {
        self.host.call_at(sp, 0x4e8830, emitter, &[payload]);
    }
}

impl layout::Vf98xCalls for Car<'_> {
    fn slot97(&mut self, mem: &mut Mem, sp: u32, car: u32, sel: u32, sample: u32) {
        let target = mem.u32(mem.u32(car) + 0x184);
        self.host.call_at(sp, target, car, &[sel, sample]);
    }

    fn m6840(&mut self, _mem: &mut Mem, sp: u32, drive: u32, value: u32) {
        self.host.call_at(sp, vcu::M6840, drive, &[value]);
    }

    fn slot18(&mut self, mem: &mut Mem, sp: u32, car: u32) {
        let target = mem.u32(mem.u32(car) + 0x48);
        self.host.call_at(sp, target, car, &[]);
    }
}

/// The calls `Update9AF0` makes out of its own body (`update_9af0.rs`): the externals on the car and its objects, each
/// as the asm pushes it. The timer takes the entry's address, the emits take their payloads by address (in scratch).
impl Update9AF0Calls for Car<'_> {
    fn timer_gap(&mut self, obj: u32, at: u32) -> f64 {
        self.host.call(TIMER_GAP, obj, &[at]).st0
    }

    fn flow_check(&mut self, obj: u32) -> u32 {
        self.host.call(FLOW_CHECK, obj, &[]).eax
    }

    fn s8c(&mut self, car: u32) -> u32 {
        self.slot(car, SLOT_S8C, &[]).eax
    }

    fn emit_df0(&mut self, car: u32, words: [u32; 3]) {
        self.host.call(EMIT_DF0, car, &words);
    }

    fn emit_770(&mut self, obj: u32, code: u32) {
        let at = self.scratch + SEND;
        self.mem.set_u32(at, code);
        self.host.call(crate::ngp::call_target(&self.mem, EMIT_770_SITE), obj, &[at]);
    }

    fn test_3b20(&mut self, obj: u32) -> u32 {
        self.host.call(TEST_3B20, obj, &[]).eax
    }

    fn test_4b0(&mut self, obj: u32) -> u32 {
        self.host.call(TEST_4B0, obj, &[]).eax
    }

    fn test_690(&mut self, obj: u32) -> u32 {
        self.host.call(TEST_690, obj, &[]).eax
    }

    fn motion_probe(&mut self, car: u32) -> (u32, u32) {
        let at = self.scratch + SEND;
        let ret = self.host.call(MOTION_PROBE, car, &[at]).eax;
        (ret, self.mem.u32(at))
    }

    fn emit_8f0(&mut self, obj: u32, pair: [u32; 2]) {
        let at = self.scratch + SEND;
        self.mem.set_u32(at, pair[0]);
        self.mem.set_u32(at + 4, pair[1]);
        self.host.call(crate::ngp::call_target(&self.mem, EMIT_8F0_SITE), obj, &[at]);
    }
}

const TIMER_GAP: u32 = 0x4A0290;
const FLOW_CHECK: u32 = 0x566640;
pub(super) const SLOT_S8C: u32 = 0x8C;
pub(super) const EMIT_DF0: u32 = 0x4E8DF0;
/// Update9AF0's calls of the emits 0x4E8770 and 0x4E88F0, which NGP retargets (to 0x100ee6e0 and 0x100eda50).
const EMIT_770_SITE: u32 = 0x4E9BB9;
const TEST_3B20: u32 = 0x4EC0D0;
const TEST_4B0: u32 = 0x504840;
const TEST_690: u32 = 0x50ECF0;
pub(super) const MOTION_PROBE: u32 = 0x4E7730;
const EMIT_8F0_SITE: u32 = 0x4E9C5B;

/// thiscall `(arg)`: `CCar::Update9AF0`, the car's per-step signal emitter.
pub(super) fn emit_signals(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = car_calls(ctx, r);
    update_9af0::emit_signals(&mut calls, &mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}
