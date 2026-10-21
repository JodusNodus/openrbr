//! `CCar`'s sequence functions: `Vf128x`, `Collide6000`, `Vf54x`, `Test7910`, `Assist8DF0`, `Vf158x`, the retire,
//! the light setters, the grid edge, `Split17C`, the vote probe, the countdown, `Slot97` and the respawn.

use super::car::{
    roll_bar_cell_at, scratch1690, steer_torque_sum_at, Real, PARTIAL_CONTACT, T_OVERRIDE, WALL_CONTACT, WORLD,
};
use super::car_parts::FLOW_OBJECT;
use super::construct::SIGNALS;
use crate::car_body::assist::{self, AssistCalls, RetireCalls};
use crate::car_body::edge::EdgeCalls;
use crate::car_body::scratch_dispatch::ScratchDispatchCalls;
use crate::car_body::store::CarStore;
use crate::car_body::{accessors, edge, scratch_dispatch, steering_ffb};
use crate::engine_driveline::layout::{blend_b, COUNT_AT};
use crate::engine_driveline::store::EngineStore;
use crate::math::quat::{self};
use crate::math::vec::{Vec3, Vec4};
use crate::mem::Mem;
use crate::rigid_body::host::HostUnavailable;
use crate::{fpu, ngp, ntweak, rigid_body};

/// The native callees used by `CCar::Vf128x` (0x4e6100).
pub trait Vf128xCalls {
    fn compose_a(&mut self, sp: u32, this: u32, a: u32, car: u32, out: u32);
    fn collide6000(&mut self, sp: u32, car: u32, m: u32, f1: u32, f2: u32);
    fn e43vf128(&mut self, sp: u32, car: u32, args: [u32; 8]);
}

/// `CCar::Vf128x`: matrix compose, collision probe, then the solid's slot body. Locals `m`, `f1`, and `f2` occupy
/// the same addresses as the original frame, since both calls take their addresses.
pub fn vf128x<S: CarStore>(mem: &mut S, car: u32, args: [u32; 8], frame: u32, calls: &mut impl Vf128xCalls) {
    let [a, b, _, _, e, f, _, _] = args;
    let m = frame - 0x1c;
    let f1 = frame - 0x20;
    let f2 = frame - 0x24;
    mem.set_u32(f1, 0);
    mem.set_u32(f2, 0);
    calls.compose_a(frame - 0x3c, mem.u32(0x7cbb00), a, car, m);
    calls.collide6000(frame - 0x3c, car, m, f1, f2);
    calls.e43vf128(frame - 0x50, car, [a, b, mem.u32(f1), mem.u32(f2), e, f, args[6], args[7]]);
}

/// The native calls `CCar::Collide6000` makes: the car's grid (its vtable slot 0x118, `target`), and
/// `E37GridOps::EdgeEdge` (0x4f8280) on that grid with its six stack arguments in the original's order.
pub trait Collide6000Calls {
    fn grid(&mut self, sp: u32, car: u32, target: u32) -> u32;
    fn edge_edge(&mut self, sp: u32, grid: u32, args: [u32; 6]) -> u32;
}

/// `CCar::Collide6000` (0x4e6000, thiscall, `ret 0xc`): the edge probe along `a`, scaled to length 10 (cleared below
/// 0.001), from the point (0, 0, 0.35) on the grid. On a hit `c` and `b` take the grid's flags at +0x689 and +0x688 and
/// those flags are cleared; otherwise both are zero. The locals are at the original's body esp: the frame is aligned
/// down to 16 bytes, then 0x58 and two saved registers sit below it.
pub fn collide6000<S: CarStore>(
    mem: &mut S,
    car: u32,
    a: u32,
    b: u32,
    c: u32,
    frame: u32,
    calls: &mut impl Collide6000Calls,
) {
    let body = ((frame - 4) & !0xf) - 0x60;
    let v40 = body + 0x30;
    let v50 = body + 0x20;
    let len = Vec3::load(mem, a).length3();
    let scaled = if len <= 0.001 { Vec4::default() } else { Vec4::load(mem, a).scale(10.0 / len) };
    scaled.store(mem, v40);
    (Vec4 { z: 0.35, ..Vec4::default() }).store(mem, v50);
    let target = mem.u32(mem.u32(car) + 0x118);
    // The original pushes the six EdgeEdge arguments before its grid call, so both calls enter at `body - 28`.
    let args = [v40, v50, body + 0x50, body + 0x1c, body + 0x40, body + 0x18];
    let grid = calls.grid(body - 28, car, target);
    if calls.edge_edge(body - 28, grid, args) != 0 {
        let grid = calls.grid(body - 4, car, target);
        mem.set_u32(c, u32::from(mem.u8(grid + 0x689)));
        mem.set_u32(b, u32::from(mem.u8(grid + 0x688)));
        mem.set_u8(grid + 0x688, 0);
        mem.set_u8(grid + 0x689, 0);
    } else {
        mem.set_u32(c, 0);
        mem.set_u32(b, 0);
    }
}

/// `CCar::Vf54x`'s sequence (name ours): run the E47 scratch reset, then dispatch slot 1 on the car's scratch member.
pub fn reset_scratch_and_dispatch<S: CarStore>(
    calls: &mut impl ScratchDispatchCalls,
    mem: &mut S,
    car: u32,
) -> Result<u32, HostUnavailable> {
    rigid_body::layout::scratch_reset_after_entity(mem.mem_mut(), car, calls)?;
    Ok(scratch_dispatch::dispatch_scratch_slot1(calls, scratch1690(car)))
}

/// `CCar::Test7910` (0x4e7910, thiscall, no args): the car's position less the drive line's point at its lane (`BlendB`
/// at the index and fraction of the car's float at +0x17c), with the verdict of `lane_verdict`. `LerpAt` runs in the
/// original too, its result unused, and traps past the table's count: the port returns `None` there. `to_world` is the car's `ToWorld`.
pub fn lane_test<S: CarStore + EngineStore>(mem: &S, car: u32, to_world: impl FnOnce(Vec3) -> Vec3) -> Option<u32> {
    let fv = mem.f32(car + 0x17C);
    let i = fpu::trunc_i32(f64::from(fv));
    let t = fv - i as f32;
    let drive = mem.u32(DRIVE_LINE);
    if i as u32 >= mem.u32(drive + COUNT_AT) {
        return None;
    }
    let point = blend_b(mem, drive, i, t);
    let d = Vec3 {
        x: mem.f32(car + 0x140) - point.x,
        y: mem.f32(car + 0x144) - point.y,
        z: mem.f32(car + 0x148) - point.z,
    };
    Some(assist::lane_verdict(d, to_world))
}

/// `CCar::Assist8DF0(a, b, c)` (0x4e8df0, `ret 12`, thiscall on the car). `entry` is the entry esp: the frame is the
/// original's aligned one (the code word at +0xc, the seed block at +0x20, below it). The probe is cleared into +0x1680,
/// `b` and `c` go to +0x15e4 and +0x1670, and the partial-contact flag (+0x9f0) is `1.0 < +0x9e8` when `a` is nonzero.
/// The code is 1 when the flag is set, else 3 less the lane test's result (2 or 3); its emit is on the outside signal.
pub fn assist_8df0<S: CarStore>(
    calls: &mut impl AssistCalls,
    mem: &mut S,
    car: u32,
    entry: u32,
    a: u32,
    b: u32,
    c: u32,
) {
    let frame = ((entry - 4) & !0xf) - 0x40;
    mem.set_u32(car + 0x15E4, b);
    mem.set_u32(car + T_OVERRIDE, c);
    for k in 0..4 {
        mem.set_u32(car + 0x1680 + 4 * k, 0);
    }
    if a == 0 {
        mem.set_u32(car + 0x9F0, 0);
    } else {
        let partial = u32::from(1.0 < mem.f32(car + PARTIAL_CONTACT));
        mem.set_u32(car + 0x9F0, partial);
        let code = if partial != 0 { 1 } else { 3 - u32::from(calls.lane_test(car) != 0) };
        mem.set_u32(frame + 0xC, code);
        calls.emit_outside(frame + 0xC);
    }
    calls.reset_car(car);
    for word in [0x4, 0x8, 0xC, 0x10, 0x14, 0x18, 0x20] {
        mem.set_u32(frame + 0x20 + word, 0);
    }
    calls.apply_control(car, frame + 0x20);
    calls.set_dirty(car + 0xAE0);
    mem.set_u32(car + 0xAEC, 0);
}

/// The two calls in `CCar::Vf158x`: slot 0 on the car and the tail-call to the solid's slot 86.
pub trait Vf158xCalls<S = Mem> {
    fn slot0(&mut self, mem: &S, target: u32, sp: u32, car: u32);
    fn solid_vf158(&mut self, mem: &mut S, sp: u32, car: u32) -> u32;
}

const APP_ROOT_FLAG: u32 = 0x0165_9B68;

/// `CCar::Vf158x` (0x4e5ee0): call the car's slot 0, set the app-root flag, then tail-call `CSolid::E43Vf158` with the
/// original entry stack pointer.
pub fn vf158x<S: CarStore>(calls: &mut impl Vf158xCalls<S>, mem: &mut S, car: u32, entry: u32) -> u32 {
    let target = mem.u32(mem.u32(car));
    calls.slot0(mem, target, entry - 8, car);
    mem.set_u32(APP_ROOT_FLAG, 1);
    calls.solid_vf158(mem, entry, car)
}

/// `E34_RetireA` (0x4e8fa0, `__fastcall` on the car, no args): the retire emit with the code 3 (its local, the word at
/// `entry - 4`), then `Assist8DF0` with `(1, 0.5, 4.0)` when the flow holds, else `(0, 30.0, 4.0)`.
pub fn retire_announce<S: CarStore>(calls: &mut impl RetireCalls, mem: &mut S, car: u32, entry: u32) {
    mem.set_u32(entry - 4, 3);
    calls.emit_retire(entry - 4);
    if mem.flow_holds(mem.u32(FLOW_OBJECT)) != 0 {
        calls.assist(car, 1, 0x3F00_0000, 0x4080_0000);
    } else {
        calls.assist(car, 0, 0x41F0_0000, 0x4080_0000);
    }
}

/// The CReal that holds +0x15f4 (dirtied before the store; the value is at +0x15f4).
pub fn speed_sum_at(car: u32) -> u32 {
    car + 0x15E8
}

/// The light state's setters (`CCarLightSystem::Set1e40` 0x4f1e40 and `Set1ed0` 0x4f1ed0, `ret 4`): when the word
/// `state_word` into the light block (`this + 8`) differs from `x`, the tweak `state_word - 0xc` (the `CBase` header of
/// that light value) is made dirty and takes `x`, and the change word `change` of the block at `this + 0x14` is set.
fn set_light<S: CarStore>(mem: &mut S, this: u32, x: u32, state_word: u32, change: u32) -> u32 {
    let block = mem.u32(this + 8);
    if mem.u32(block + state_word) == x {
        return block;
    }
    let value = block + state_word - 0xc;
    ntweak::set_dirty(mem.mem_mut(), value);
    mem.set_u32(value + 0xc, x);
    let changes = mem.u32(this + 0x14);
    mem.set_u32(changes + change, 1);
    changes
}

/// `CCarLightSystem::Set1e40(x)` (0x4f1e40): the brake light. Returns the word `eax` keeps.
pub fn set_brake_light<S: CarStore>(mem: &mut S, this: u32, x: u32) -> u32 {
    set_light(mem, this, x, 0x5c, 0xc)
}

/// `CCarLightSystem::Set1ed0(x)` (0x4f1ed0): the neutral light. Returns the word `eax` keeps.
pub fn set_neutral_light<S: CarStore>(mem: &mut S, this: u32, x: u32) -> u32 {
    set_light(mem, this, x, 0x6c, 0x10)
}

/// `CCar::AddADC(v)` (0x4e6660): the demand cell is dirtied, then `value += v`.
pub fn add_adc<S: CarStore>(mem: &mut S, car: u32, v: f32) {
    let at = steer_torque_sum_at(car);
    accessors::add_steer_torque(Real::load(mem, at), v).store(mem, at);
}

/// `CCar::SetA1C(i, j, v)` (0x4e66a0): the matrix cell is dirtied, then set to `v`.
pub fn set_a1c<S: CarStore>(mem: &mut S, car: u32, i: u32, j: u32, v: f32) {
    let at = roll_bar_cell_at(car, i, j);
    accessors::set_cell(Real::load(mem, at), v).store(mem, at);
}

/// `CCar::UpdateFfbTorque` (0x4e7ff0): the filtered steering torque (+0xae0) takes the new value, dirtied.
pub fn update_ffb_torque<S: CarStore>(mem: &mut S, car: u32) {
    let cell = mem.ffb_torque(car);
    let v = steering_ffb::filtered_torque(
        mem.override_timer(car),
        mem.speed(car),
        mem.steering_rack_ratio(car),
        mem.steer_torque_sum(car).value,
        cell.value,
    );
    accessors::set_cell(cell, v).store(mem, car + 0xAE0);
}

/// `CCar::PushGridEdge` (0x4e7270) on the car: the flag is set after the edge is added, as the asm orders it.
pub fn push_grid_edge<S: CarStore>(calls: &mut impl EdgeCalls, mem: &mut S, car: u32) {
    let pushed = edge::push_grid_edge(calls, car, mem.grid_pushed(car), mem.grid_object(car));
    mem.set_grid_pushed(car, pushed);
}

/// `CCar::RefreshEdge(a)` (0x4e72d0): `PushGridEdge`, then `Vf20` on the car with `a`; returns the `Vf20` call's eax.
/// The flag is stored before the `Vf20` call, as in the asm.
pub fn refresh_edge<S: CarStore>(calls: &mut impl EdgeCalls, mem: &mut S, car: u32, a: u32) -> u32 {
    push_grid_edge(calls, mem, car);
    calls.entity_refresh(car, a)
}

/// The drive line's global (`g_1659184`): the table `CDriveLine::BlendB`, `LerpAt` and `Last` read.
pub const DRIVE_LINE: u32 = 0x0165_9184;

/// The operand of the sampler's read of its vote object (`*0x7c755c`, a collision system whose slot 0 is `Vf00`;
/// NGP repoints it at its first copy): `stage::layout::collision_at_operand`.
pub const SAMPLER_OPERAND: u32 = 0x004e_54e8;

/// The calls `CCar::Sampler54A0` makes: slot 0 of the object through `SAMPLER_OPERAND`, `Vf00(probe_a, probe_b, raw)` with three
/// stack words, entered at `sp`. Its eax is the vote (nonzero on a hit).
pub trait Sampler54Calls<S = Mem> {
    fn vote(&mut self, mem: &mut S, sp: u32, probe_a: u32, probe_b: u32, raw: u32) -> u32;
}

/// The calls `CCar::Vf98x` makes out of its own body, each entered at `sp` (the original's pushes below its frame):
/// the car's slot 97 (`S184`, 0x4e5270) with the selected block and the sample, `CDrive::M6840` on the drive, and the
/// car's slot 18 (`CSolid::E43Vf48`).
pub trait Vf98xCalls<S = Mem>: Sampler54Calls<S> {
    fn slot97(&mut self, mem: &mut S, sp: u32, car: u32, sel: u32, sample: u32);
    fn m6840(&mut self, mem: &mut S, sp: u32, drive: u32, value: u32);
    fn slot18(&mut self, mem: &mut S, sp: u32, car: u32);
}

/// `CCar::Split17C(a, b)` (0x4e5530, `ret 8`): `a` takes the drive line's blend at the index and fraction of the car's
/// float at +0x17c (`BlendB`), `b` takes its lerp (`LerpAt`), and `b` is normalized. The index is truncated as
/// `cvttss2si` does it. `LerpAt` traps past the table's count, so the port returns `None` there. NGP retargets both
/// calls to its samplers in doubles (`ngp::layout::drive_line`), which write three lanes and don't trap (1.02's `LerpAt` traps
/// past the table's count).
pub fn split_17c<S: CarStore + EngineStore>(mem: &mut S, car: u32, a: u32, b: u32) {
    let fv = mem.f32(car + 0x17C);
    let i = fpu::trunc_i32(f64::from(fv));
    let t = fv - i as f32;
    let drive = mem.u32(DRIVE_LINE);
    let store3 = |mem: &mut S, at: u32, v: [f32; 3]| (0..3).for_each(|k| mem.set_f32(at + 4 * k, v[k as usize]));
    let blend = ngp::layout::drive_line::blend_b_f32(mem.mem(), drive, i, t);
    store3(mem, a, blend);
    let lerp = ngp::layout::drive_line::lerp_at_f32(mem.mem(), drive, i, t);
    store3(mem, b, lerp);
    Vec4::load(mem, b).normalize().store(mem, b);
}

/// `CCar::Sampler54A0(a, b, c)` (0x4e54a0, `ret 0xc`) (name ours): votes on the probe `a` lifted and dropped by 10 in its z lane,
/// with `c` as the vote's output. On a hit `b` takes `c`'s first 16 bytes. `entry` is the sampler's entry esp; its
/// locals sit in the original's frame below it. Returns 1 on a hit, else 0.
pub fn vote_probe<S: CarStore>(
    mem: &mut S,
    calls: &mut impl Sampler54Calls<S>,
    entry: u32,
    a: u32,
    b: u32,
    c: u32,
) -> u32 {
    let frame = ((entry - 4) & !0xf) - 0x2c;
    let (probe_a, probe_b) = (frame + 0x1c, frame + 0xc);
    let probe = Vec4::load(mem, a);
    (Vec4 { z: probe.z + 10.0, ..probe }).store(mem, probe_a);
    (Vec4 { z: probe.z - 10.0, ..probe }).store(mem, probe_b);
    if calls.vote(mem, frame - 20, probe_a, probe_b, c) == 0 {
        return 0;
    }
    Vec4::load(mem, c).store(mem, b);
    1
}

/// `CCar::Vf98x` (0x4e6dd0, thiscall, no args) (name ours). With the countdown `+0x15e4` at zero (ordered; NaN is not zero): the
/// drive line's sample (`Split17C`) is negated, the sampler votes on the probe, and slot 97 takes the voted block (the
/// probe on a miss) and the sample; then `M6840(1)` and slot 18. Otherwise the countdown falls by the app's tick and is
/// stored as 0 when it goes negative (NaN stays). Nothing happens for a null car. `entry` is the entry esp; the frame is
/// aligned as the original's.
pub fn run_countdown<S: CarStore + EngineStore>(mem: &mut S, calls: &mut impl Vf98xCalls<S>, car: u32, entry: u32) {
    if car == 0 {
        return;
    }
    let frame = ((entry - 4) & !0xf) - 0x7c;
    let timer = mem.f32(car + 0x15E4);
    if timer == 0.0 {
        let (probe, sample, ans, raw) = (frame + 0x1c, frame + 0xc, frame + 0x2c, frame + 0x3c);
        split_17c(mem, car, probe, sample);
        (Vec4::load(mem, sample).scale(-1.0)).store(mem, sample);
        let hit = vote_probe(mem, calls, frame - 20, probe, ans, raw);
        let sel = if hit != 0 { ans } else { probe };
        calls.slot97(mem, frame - 16, car, sel, sample);
        calls.m6840(mem, frame - 12, car + 0x4B0, 1);
        calls.slot18(mem, frame - 8, car);
        return;
    }
    let tick = mem.f32(mem.u32(mem.u32(WORLD) + 0x24) + 0x1608C);
    let left = timer - tick;
    mem.set_f32(car + 0x15E4, left);
    if left < 0.0 {
        mem.set_f32(car + 0x15E4, 0.0);
    }
}

/// The respawn signal's emitter, from the signal manager's base.
const RESPAWN_EMITTER: u32 = 0xc0a0;

/// `CCar::Slot97`'s scale of the basis in the output vector (`0.4`, asm 0x72f664), and of the sample in the
/// quaternion's input (asm 0x738230).
const BASIS_WEIGHT: f32 = f32::from_bits(0x3ECC_CCCD);
const SAMPLE_SCALE: f32 = f32::from_bits(0xBE86_0A92);

/// The calls `CCar::Slot97` (the car's vtable slot 97, 0x4e5270) makes out of its own body: the car's slot 96
/// (`CCar::Vf180`), entered at `sp`, with the output vector and the quaternion on the stack.
pub trait Slot97Calls<S = Mem> {
    fn respawn_car(&mut self, mem: &mut S, sp: u32, car: u32, a: u32, b: u32);
}

/// The calls `CCar::Vf180` (the car's slot 96, 0x4e9660) makes out of its own body, each entered at `sp`: slot 17 of the
/// object at +0xe0 (`0x529a50`), `CCar::Xform8130` (0x4e8130), `CSolid::E43ClearAcc` (0x529e60), the car's slot 36
/// (`0x52bf30`), and the respawn emit (0x4e8830) on the signal manager's emitter with the 16-byte payload's address.
pub trait Vf180Calls<S = Mem> {
    /// Slot 17 (`0x529a50`) of the object at `car + 0xe0`, thiscall on that object.
    fn slot17_e0(&mut self, mem: &mut S, sp: u32, car: u32);
    fn compose_body_into_world(&mut self, mem: &mut S, sp: u32, car: u32, a: u32, out: u32);
    fn clear_acc(&mut self, mem: &mut S, sp: u32, car: u32);
    fn slot36(&mut self, mem: &mut S, sp: u32, car: u32);
    fn emit_respawn(&mut self, mem: &mut S, sp: u32, emitter: u32, payload: u32);
}

/// `CCar::Slot97(sel, sample)` (0x4e5270, thiscall, `ret 8`, the car's slot 97). Builds a frame from the sample `s` and
/// the up vector (0, 0, 1): `a = normalize(s x up)`, `b = a x s`; the rows (`a`, `s`, `b`) become the quaternion `q`
/// (`ToQuat`). The output vector is `sel + 0.4 * b`. With the car's word at +0x9f0 set, `q` is normalized after the
/// product `(0.8 * s, 1) * q`... and +0x9f0 clears, with the output's z raised by 0.4. Then slot 96 takes the output and
/// `q`. `entry` is the entry esp; the locals sit in the original's frame. Nothing happens for a null car. Only `ToQuat`
/// takes the variant; the rest isn't checked under NGP.
pub fn slot97<S: CarStore>(mem: &mut S, calls: &mut impl Slot97Calls<S>, car: u32, sel: u32, sample: u32, entry: u32) {
    if car == 0 {
        return;
    }
    let frame = ((entry - 4) & !0xf) - 0xc0;
    let s = Vec3::load(mem, sample);
    let a = s.cross3(Vec3 { x: 0.0, y: 0.0, z: 1.0 });
    a.store(mem, frame + 0x20);
    let inv = 1.0 / a.length3();
    let a4 = Vec4::load(mem, frame + 0x20).scale(inv);
    a4.store(mem, frame + 0x20);
    let b = Vec3 { x: a4.x, y: a4.y, z: a4.z }.cross3(s);
    b.store(mem, frame + 0x30);
    (Vec3 { x: a4.x, y: a4.y, z: a4.z }).store(mem, frame + 0x8c);
    s.store(mem, frame + 0x98);
    b.store(mem, frame + 0xa4);
    mem.store_quat(frame + 0x50, quat::to_quat(&mem.load_mat3(frame + 0x8c)));

    let out = Vec4::load(mem, sel).add_scaled(Vec4::load(mem, frame + 0x30), BASIS_WEIGHT);
    out.store(mem, frame + 0x60);
    if mem.u32(car + 0x9F0) != 0 {
        let scaled = Vec4 { x: s.x * SAMPLE_SCALE, y: s.y * SAMPLE_SCALE, z: s.z * SAMPLE_SCALE, w: 1.0 };
        scaled.store(mem, frame + 0x70);
        let product = quat::mul(mem.load_quat(frame + 0x70), mem.load_quat(frame + 0x50));
        mem.store_quat(frame + 0x50, quat::normalize(product));
        mem.set_f32(frame + 0x68, mem.f32(frame + 0x68) + BASIS_WEIGHT);
        mem.set_u32(car + 0x9F0, 0);
    }
    calls.respawn_car(mem, frame - 12, car, frame + 0x60, frame + 0x50);
}

/// `CCar::Vf180(a, b)` (0x4e9660, thiscall, `ret 8`, the car's slot 96) (name ours): the respawn worker. `b` is the seed quaternion
/// and `a` the output vector; the car takes the seed at +0x100 and `a` at +0x140, runs its clearing and transform
/// calls, zeroes the vectors at +0x190, +0x1c0, +0xaa0 and +0xac0, and emits the respawn signal with `a` as its
/// payload. `entry` is the entry esp.
pub fn respawn_car<S: CarStore>(mem: &mut S, calls: &mut impl Vf180Calls<S>, car: u32, a: u32, b: u32, entry: u32) {
    if car == 0 {
        return;
    }
    let frame = ((entry - 4) & !0xf) - 0x50;
    Vec4::load(mem, b).store(mem, car + 0x100);
    Vec4::load(mem, a).store(mem, car + 0x140);
    calls.slot17_e0(mem, frame - 4, car);
    mem.zero_vec4(frame + 0x20);
    calls.compose_body_into_world(mem, frame - 12, car, frame + 0x20, frame + 0x10);
    let z = mem.f32(car + 0x148);
    mem.set_f32(car + 0x148, (z - mem.f32(frame + 0x18)) + z);
    calls.clear_acc(mem, frame - 4, car);
    for at in [0x1C0, 0x190, 0xAA0, 0xAC0] {
        mem.zero_vec4(car + at);
    }
    calls.slot36(mem, frame - 4, car);
    Vec4::load(mem, a).store(mem, frame + 0x30);
    let pending = mem.u32(car + 0x274);
    if pending != 0 {
        mem.set_u32(pending, 0);
        mem.set_u32(car + 0x274, 0);
    }
    mem.set_f32(car + WALL_CONTACT, 0.0);
    let emitter = mem.u32(SIGNALS) + RESPAWN_EMITTER;
    calls.emit_respawn(mem, frame - 8, emitter, frame + 0x30);
}

/// The animated-replay state signal's emitter, from the signal manager's base.
const REPLAY_STATE_EMITTER: u32 = 0x9078;

/// The callees of `CCar::Snap9360`, each with the stack pointer the original's pushes give it.
pub trait Snap9360Calls<S = Mem> {
    fn compose_body_into_world(&mut self, mem: &mut S, sp: u32, car: u32, a: u32, out: u32);
    /// `CMultiSignal1<CSigCarStateAnimatedReplay>::Emit` (0x4e85b0) on `emitter`.
    fn emit_replay_state(&mut self, mem: &mut S, sp: u32, emitter: u32, payload: u32);
}

/// `CCar::Snap9360` (0x4e9360, thiscall, no args) (name ours): the animated replay's car state, emitted while the snapshot
/// flag is set. The payload at `frame + 0x40` is the world position of the body origin (`Xform8130` of a cleared vector),
/// the quaternion, the steering input and each present wheel's travel (+0x5bc) and +0x5ac; its first 16 bytes and an
/// absent wheel's pair are not written. The position and quaternion copies (`E01Vec4::CopyFrom`, 16 bytes) are made in
/// the original's order. `entry` is the entry esp.
pub fn snapshot<S: CarStore>(mem: &mut S, calls: &mut impl Snap9360Calls<S>, car: u32, entry: u32) {
    let frame = ((entry - 4) & !0xf) - 0xa0;
    let payload = frame + 0x40;
    Vec4::load(mem, car + 0x140).store(mem, frame + 0x10);
    Vec4::load(mem, car + 0x100).store(mem, frame + 0x30);
    mem.zero_vec4(frame + 0x20);
    calls.compose_body_into_world(mem, frame - 12, car, frame + 0x20, frame + 0x10);
    Vec4::load(mem, frame + 0x10).store(mem, payload + 0x10);
    Vec4::load(mem, frame + 0x30).store(mem, payload + 0x20);
    mem.set_f32(payload + 0x30, mem.steer_input(car));
    for i in 0..4 {
        let wheel = mem.u32(super::wheel_list_at(car, i));
        if wheel != 0 {
            mem.set_f32(payload + 0x34 + 8 * i, mem.f32(wheel + 0x5BC));
            mem.set_f32(payload + 0x38 + 8 * i, mem.f32(wheel + 0x5AC));
        }
    }
    let emitter = mem.u32(SIGNALS) + REPLAY_STATE_EMITTER;
    calls.emit_replay_state(mem, frame - 8, emitter, payload);
}
