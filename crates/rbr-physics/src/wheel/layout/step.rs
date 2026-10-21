//! `E51Wheel::M3DD30` (0x53dd30), the wheel step: the ground contact, the strut's spring and the slip.

use super::fields::{
    CAR, DEF, DEF_FLAGS, DEF_INVERSE_MASS, ON_GROUND, STRUT, STRUT_LENGTH, STRUT_TWEAK, STRUT_VEL, WHEEL_INDEX,
    WORLD_DEF,
};
use crate::car_body::layout as car_layout;
use crate::entities::layout::ROOT_IDENTITY;
use crate::fpu::round_x87;
use crate::physics_core::layout::SIGNAL_MANAGER;
use crate::wheel::store::WheelStore;
use crate::Variant;

/// The host calls of `wheel_step`, one method per original callee (`car_slot` is the car's vtable slot 12, called with
/// its arguments left on the stack for `pair_walk`, which pops them as the asm does).
pub trait StepCalls {
    /// `CBase::GetName` (0x52db10) on the strut.
    fn strut_name(&mut self, strut: u32, sp: u32) -> u32;
    /// `CBase::SetDirty` (0x52f450).
    fn set_dirty(&mut self, object: u32, sp: u32);
    /// `E51Wheel::M3B900` (0x53b900) (name ours).
    fn refresh(&mut self, this: u32, travel: f32, sp: u32);
    /// `E51Wheel::M3D520` (0x53d520), the ground probe: 1 when it found a contact.
    fn ground(&mut self, this: u32, out: u32, tmp: u32, sp: u32) -> u32;
    /// The car's vtable slot 12 (`[[car] + 0x30]`) with `args`; they stay on the stack.
    fn car_slot(&mut self, car: u32, args: &[u32], sp: u32) -> u32;
    /// `E43PairWalk::MB560` (0x51b560) on `this`, with the three arguments the car call received.
    fn pair_walk(&mut self, this: u32, a: u32, b: u32, c: u32, sp: u32);
    /// `CCar::Xform6950` (0x4e6950, world compose).
    fn world_compose(&mut self, car: u32, a: u32, b: u32, sp: u32);
    /// `CCar::Xform68E0` (0x4e68e0, to world).
    fn to_world(&mut self, car: u32, a: u32, b: u32, sp: u32);
    /// `CCar::Fwd6DB0` (0x4e6600, through the thunk); its st0 result.
    fn fwd6db0(&mut self, car: u32, a: u32, sp: u32) -> f32;
    /// `CCar::Fwd6DD0` (0x4e6610); its st0 result.
    fn fwd6dd0(&mut self, car: u32, a: u32, sp: u32) -> f32;
    /// `CCar::Fwd43C0` (0x4e6620).
    fn fwd43c0(&mut self, car: u32, i: u32, a: f32, b: f32, sp: u32);
    /// `CCar::GetA48` (0x4e6730); its st0 result.
    fn get_a48(&mut self, car: u32, i: u32, j: u32, sp: u32) -> f32;
    /// `CCar::AddADC` (0x4e6660).
    fn add_adc(&mut self, car: u32, v: f32, sp: u32);
    /// `CCar::SetA1C` (0x4e66a0).
    fn set_a1c(&mut self, car: u32, i: u32, j: u32, v: f32, sp: u32);
    /// `CE36SubBlock::Init` (0x4f3e80) on the block at `this`.
    fn init_sub_block(&mut self, this: u32, sp: u32);
    /// `CSolid::Step76` (0x529c80) on `this`.
    fn step76(&mut self, this: u32, a: u32, b: u32, sp: u32);
    /// `CSpringDamper::Step` (0x52e090) on the strut, with its six arguments.
    fn spring_step(&mut self, strut: u32, args: [u32; 6], sp: u32);
    /// `CTyre::CScratch::SetStateField` (0x532470).
    fn set_state_field(&mut self, scratch: u32, v: u32, sp: u32);
    /// `CTyre::CScratch::SetState48` (0x533490).
    fn set_state48(&mut self, scratch: u32, v: f32, sp: u32);
    /// `CTyre::CScratch::RelaxCount` (0x5358c0).
    fn relax_count(&mut self, scratch: u32, x: f32, sp: u32);
    /// `CTyre::CScratch::Step` (0x5360c0) on the scratch, with its sixteen arguments in callee order.
    fn scratch_step(&mut self, scratch: u32, args: [u32; 16], sp: u32);
    /// `CWheel::Blend4` (0x539180).
    fn blend4(&mut self, this: u32, a: u32, b: u32, c: u32, d: u32, sp: u32);
    /// `CWheel::Unk97a0` (0x5397a0) (name ours).
    fn update_sub_objects(&mut self, this: u32, a: u32, b: u32, c: u32, sp: u32);
    /// `E51Wheel::M3AB90` (0x53ab90) (name ours).
    fn state_lerp(&mut self, this: u32, sp: u32);
    /// `E51Wheel::M3AC50` (0x53ac50) (name ours).
    fn set_input(&mut self, this: u32, v: f32, sp: u32);
    /// `E51Wheel::M3AFB0` (0x53afb0) (name ours).
    fn spin_cross(&mut self, this: u32, p: u32, sp: u32);
    /// `E51MultiTire::Emit` (0x53d0d0) on `multi_tire`.
    fn emit(&mut self, multi_tire: u32, sig: u32, sp: u32);
    /// `E09Vec4::Clear` (0x465d80) on `at`; `sp` is the callee's entry stack pointer, as the asm's pushes leave it.
    fn clear_vec4(&mut self, at: u32, sp: u32);
    /// `E01Vec4::CopyFrom` (0x43c5b0): `dst` takes the vector at `src`.
    fn copy_vec4(&mut self, dst: u32, src: u32, sp: u32);
    /// `E01VecSetA::Cross` (0x43c630): `out` = `a x b`.
    fn cross(&mut self, a: u32, b: u32, out: u32, sp: u32);
    /// `CFloat3::Scale` (0x473cd0): `out` = `factor * src`, four lanes.
    fn scale_vec4(&mut self, src: u32, factor: f32, out: u32, sp: u32);
}

/// Floats the asm reads from fixed addresses (their bits as the exe holds them).
const STEP_DAMP: f32 = f32::from_bits(0x3D4C_CCCD); // 0x72dd78, 0.05
const STEP_SLACK: f32 = f32::from_bits(0x3CA3_D70A); // 0x72fcb4, 0.02
const STEP_ONE: f32 = f32::from_bits(0x3F80_0000); // 0x795e14, 1.0
const STEP_LOW: f32 = f32::from_bits(0xC2C8_0000); // 0x7353b0, -100
const STEP_HIGH: f32 = f32::from_bits(0x42C8_0000); // 0x72bf10, 100
const STEP_WRAP: f32 = f32::from_bits(0x427B_53D2); // 0x740bd4, 62.83185
const STEP_SCALE: f32 = f32::from_bits(0xBF33_3333); // 0x72f43c, -0.7 (the fmul before the Fwd6DD0 result is stored)
const STEP_MINUS_ONE: f32 = f32::from_bits(0xBF80_0000); // -1.0, the factor of the Scale into +0xa80

/// The multi-tire emitter's offset in the signal manager (`SIGNAL_MANAGER`); the car calls take `[ROOT_IDENTITY]`.
const STEP_TIRES_EMIT: u32 = 0x4518;
/// The tyre's pointer in the wheel (`this+0x7f0`).
const TYRE: u32 = 0x7F0;
/// The world's flags (`WORLD+0x24`, then `+0x160cc`): the ground probe's bit, and the brake bit.
const FLAG_PROBE: u32 = 0x10000;
const FLAG_BRAKE: u32 = 0x80000;

/// `E51Wheel::M3DD30` (0x53dd30, thiscall, `dt` on the stack): the slot-96 step. `frame` is the body's `esp`: the asm's
/// `[esp+k]` is `frame+k`, and the host calls get the frame's addresses, so the lanes the asm never writes keep the
/// original's stack contents. `slip` is the tyre load thresholds and slope of the step's `tyre+8` flag: NGP's per-car `10172f58`, `10172f5c` and
/// `10172f60` (`0x53ea21`, `0x53ea2d`, `0x53ea45`). The x87 steps run at 53 bits, and the host calls go where NGP's patched calls go (the host's concern).
pub fn wheel_step<S: WheelStore>(
    mem: &mut S,
    calls: &mut impl StepCalls,
    this: u32,
    dt: f32,
    frame: u32,
    slip: [f32; 3],
) {
    let f = frame;
    let def = mem.u32(this + DEF);
    let car = mem.u32(this + CAR);

    // The travel: the strut's two names bound it between their limits (the travel at +0x5bc).
    calls.clear_vec4(f + 0x50, f - 4);
    calls.clear_vec4(f + 0x40, f - 4);
    mem.set_u32(f + 0xBC, def);
    mem.set_f32(f + 0x20, 0.0);
    mem.set_f32(f + 0x2C, 0.0);
    let strut = this + STRUT;
    let n1 = calls.strut_name(strut, f - 4);
    let l224 = mem.f32(n1 + 0x28) - STEP_DAMP;
    mem.set_f32(f + 0x7C, l224);
    let n2 = calls.strut_name(strut, f - 4);
    let travel_old = mem.f32(this + STRUT_LENGTH);
    let l284 = mem.f32(n2 + 0x38) + STEP_SLACK;
    let travel = if l224 <= travel_old {
        if travel_old > l284 {
            l284
        } else {
            travel_old
        }
    } else {
        l224
    };
    mem.set_f32(f + 0x1C, travel);
    calls.set_dirty(this + STRUT_TWEAK, f - 4);
    mem.set_f32(this + STRUT_LENGTH, travel);
    calls.refresh(this, mem.f32(this + STRUT_LENGTH), f - 8);

    // Two vectors from the wheel, each loaded into the frame and copied to a frame slot. Lane 3 of each copy takes the
    // frame's old `+0x6c`, as the asm leaves it.
    mem.set_u32(f + 0x60, mem.u32(this + 0x128));
    mem.set_u32(f + 0x64, mem.u32(this + 0x12C));
    mem.set_u32(f + 0x68, mem.u32(this + 0x130));
    mem.set_u32(f + 0xA0, mem.u32(f + 0x60));
    mem.set_u32(f + 0xA4, mem.u32(f + 0x64));
    mem.set_u32(f + 0xA8, mem.u32(f + 0x68));
    mem.set_u32(f + 0xAC, mem.u32(f + 0x6C));
    calls.copy_vec4(f + 0x160, f + 0xA0, f - 8);
    mem.set_u32(f + 0x60, mem.u32(this + 0x11C));
    mem.set_u32(f + 0x64, mem.u32(this + 0x120));
    mem.set_u32(f + 0x68, mem.u32(this + 0x124));
    mem.set_u32(f + 0xA0, mem.u32(f + 0x60));
    mem.set_u32(f + 0xA4, mem.u32(f + 0x64));
    mem.set_u32(f + 0xA8, mem.u32(f + 0x68));
    mem.set_u32(f + 0xAC, mem.u32(f + 0x6C));
    calls.copy_vec4(f + 0x120, f + 0xA0, f - 8);
    calls.world_compose(car, this + 0x140, f + 0x80, f - 0xC);
    let g = mem.u32(ROOT_IDENTITY);
    car_walk(calls, car, f + 0x160, g, f + 0x150, f - 0x10);
    car_walk(calls, car, this + 0x9E0, g, this + 0xA70, f - 0x10);
    calls.blend4(this, this + 0xA70, f + 0x80, f + 0x190, f + 0x170, f - 0x14);
    calls.scale_vec4(f + 0x150, STEP_MINUS_ONE, this + 0xA80, f - 0xC);
    calls.copy_vec4(this + 0xA30, f + 0x80, f - 8);
    let wheel_index = mem.u32(this + WHEEL_INDEX);
    let fwd = calls.fwd6db0(car, wheel_index, f - 8);
    mem.set_f32(this + 0xA50, fwd);

    // The ground probe, when the world's flag is set and it finds a contact.
    let world = mem.u32(car_layout::WORLD);
    let probe_flags = mem.u32(world + WORLD_DEF);
    mem.set_f32(f + 0x7C, 0.0);
    let ground = if mem.u32(probe_flags + DEF_FLAGS) & FLAG_PROBE != 0 {
        calls.ground(this, f + 0x130, f + 0x140, f - 0xC)
    } else {
        0
    };
    if ground != 0 {
        ground_contact(mem, calls, this, dt, f, car);
    } else {
        mem.set_u32(this + ON_GROUND, 0);
        mem.set_f32(f + 0x34, 0.0);
        calls.clear_vec4(f + 0xD0, f - 4);
        calls.clear_vec4(f + 0x90, f - 4);
        calls.set_state48(this + 0xF00, 0.0, f - 8);
        calls.set_state_field(this + 0xF00, 0, f - 8);
    }
    spring_and_slip(mem, calls, this, dt, f, car, def, slip);
}

/// The car's vtable call with `a, b, c`, then `pair_walk` on its result with the same three (the asm leaves them on the
/// stack for `MB560` to pop).
fn car_walk(calls: &mut impl StepCalls, car: u32, a: u32, b: u32, c: u32, sp: u32) {
    let r = calls.car_slot(car, &[a, b, c], sp);
    calls.pair_walk(r, a, b, c, sp);
}

/// The branch of `wheel_step` with a ground contact (`M3D520` returned 1): the contact's frame and its force.
fn ground_contact<S: WheelStore>(mem: &mut S, calls: &mut impl StepCalls, this: u32, dt: f32, f: u32, car: u32) {
    mem.set_u32(this + ON_GROUND, 1);
    calls.to_world(car, this + 0x990, f + 0x1C0, f - 0xC);
    let g = mem.u32(ROOT_IDENTITY);
    car_walk(calls, car, this + 0x140, g, f + 0xA0, f - 0x10);
    calls.cross(car + 0x190, f + 0xA0, f + 0xE0, f - 0xC);
    // `+0x1c0` of the car, unless the car is the null object (`car == -0x1b0`).
    let pf = if car.wrapping_add(0x1B0) == 0 { 0 } else { car + 0x1C0 };
    for k in 0..3 {
        let v = mem.f32(pf + 4 * k) + mem.f32(f + 0xE0 + 4 * k);
        mem.set_f32(f + 0xE0 + 4 * k, v);
    }
    let s = mem.f32(this + STRUT_VEL);
    mem.set_f32(f + 0x1C, s);
    for k in 0..4 {
        let v = mem.f32(f + 0x1C0 + 4 * k) * s + mem.f32(f + 0xE0 + 4 * k);
        mem.set_f32(f + 0xE0 + 4 * k, v);
    }
    calls.copy_vec4(this + 0xA60, f + 0xE0, f - 8);
    calls.clear_vec4(f + 0x1B0, f - 4);
    calls.clear_vec4(f + 0x1A0, f - 4);
    calls.init_sub_block(f + 0x280, f - 4);
    calls.clear_vec4(f + 0x230, f - 4);
    calls.clear_vec4(f + 0x1F0, f - 4);
    calls.clear_vec4(f + 0x200, f - 4);

    mem.set_u32(f + 0x218, mem.u32(f + 0x88));
    mem.set_u32(f + 0x210, mem.u32(f + 0x80));
    mem.set_u32(f + 0x214, mem.u32(f + 0x84));
    mem.set_u32(f + 0x21C, mem.u32(f + 0x8C));
    mem.set_f32(f + 0x268, 0.0);
    mem.set_f32(f + 0x260, 0.0);
    mem.set_u32(f + 0x28C, 0);
    mem.set_f32(f + 0x274, 0.0);
    mem.set_f32(f + 0x264, 0.0);
    mem.set_f32(f + 0x278, 0.0);
    mem.set_f32(f + 0x270, 0.0);
    let d7c = mem.u32(this + 0xD7C);
    if d7c != 0 {
        calls.step76(d7c, f + 0x130, f + 0x1D0, f - 0xC);
        for k in 0..3 {
            let v = mem.f32(this + 0xA60 + 4 * k) - mem.f32(f + 0x1D0 + 4 * k);
            mem.set_f32(f + 0x180 + 4 * k, v);
        }
    } else {
        calls.copy_vec4(f + 0x180, this + 0xA60, f - 8);
    }

    let args = [
        mem.u32(this + 0xA50),
        dt.to_bits(),
        mem.u32(this + 0xAA0),
        mem.u32(this + 0xD78),
        this + 0xAA4,
        this + 0xC04,
        f + 0x140,
        f + 0x80,
        this + 0xA70,
        f + 0x180,
        f + 0x130,
        f + 0x1B0,
        f + 0x1A0,
        f + 0x20,
        f + 0x2C,
        f + 0x1E0,
    ];
    calls.scratch_step(this + 0xF00, args, f - 0x44);
    calls.update_sub_objects(this, f + 0x1B0, f + 0x1A0, f + 0x20, f - 0x10);
    mem.set_u32(f + 0x1E0, mem.u32(this + WHEEL_INDEX));
    calls.copy_vec4(f + 0x220, f + 0x140, f - 8);
    let c04 = mem.u32(this + 0xC04);
    let c08 = mem.u32(this + 0xC08);
    let c0c = mem.u32(this + 0xC0C);
    mem.set_u32(f + 0x280, c04);
    mem.set_u32(f + 0x284, c08);
    mem.set_u32(f + 0x288, c0c);
    calls.emit(mem.u32(SIGNAL_MANAGER) + STEP_TIRES_EMIT, f + 0x1E0, f - 8);

    let r5 = calls.car_slot(car, &[f + 0xD0], f - 8);
    calls.pair_walk(mem.u32(ROOT_IDENTITY), f + 0x1B0, r5, f + 0xD0, f - 0x10);
    let r4 = calls.car_slot(car, &[f + 0x90], f - 8);
    calls.pair_walk(mem.u32(ROOT_IDENTITY), f + 0x1A0, r4, f + 0x90, f - 0x10);

    let t = mem.f32(f + 0x98) * mem.f32(f + 0x128)
        + mem.f32(f + 0x94) * mem.f32(f + 0x124)
        + mem.f32(f + 0x90) * mem.f32(f + 0x120);
    mem.set_f32(f + 0x7C, t);
    let v990 = this + 0x990;
    let x1 = mem.f32(this + 0x998) * mem.f32(f + 0xD8);
    let x2 = mem.f32(v990 + 4) * mem.f32(f + 0xD4) + x1;
    let x3 = x2 + mem.f32(f + 0xD0) * mem.f32(v990);
    mem.set_u32(f + 0x3C, v990);
    mem.set_f32(f + 0x34, x3);
    mem.set_u32(f + 0x38, f + 0x60);
    let neg = 0.0 - x3;
    mem.set_f32(f + 0x18, neg);
    mem.set_u32(f + 0x1C, f + 0xD0);
    for k in 0..4 {
        let v = mem.f32(v990 + 4 * k) * neg + mem.f32(f + 0xD0 + 4 * k);
        mem.set_f32(f + 0x60 + 4 * k, v);
    }
    let v = mem.f32(f + 0x60) + mem.f32(f + 0x50);
    mem.set_f32(f + 0x50, v);
    let v = mem.f32(f + 0x64) + mem.f32(f + 0x54);
    mem.set_f32(f + 0x54, v);
    let v = mem.f32(f + 0x68) + mem.f32(f + 0x58);
    mem.set_f32(f + 0x58, v);
    let v = mem.f32(f + 0x40) + mem.f32(f + 0x90);
    mem.set_f32(f + 0x40, v);
    let v = mem.f32(f + 0x44) + mem.f32(f + 0x94);
    mem.set_f32(f + 0x44, v);
    let v = mem.f32(f + 0x48) + mem.f32(f + 0x98);
    mem.set_f32(f + 0x48, v);
    calls.set_state48(this + 0xF00, mem.f32(this + 0xAA0), f - 8);
}

/// The common tail of `wheel_step`: the strut's spring, the force and its slip, the car's side forces, the brake, and
/// the relaxation.
fn spring_and_slip<S: WheelStore>(
    mem: &mut S,
    calls: &mut impl StepCalls,
    this: u32,
    dt: f32,
    f: u32,
    car: u32,
    def: u32,
    slip: [f32; 3],
) {
    let round24 = |v: f64| round_x87(v, Variant::Ngp778);
    let [slip_lo, slip_hi, slip_k] = slip;
    let y = mem.f32(def + 0x41C) + mem.f32(this + STRUT_LENGTH);
    let z = mem.u32(this + STRUT_VEL);
    calls.spring_step(
        this + STRUT,
        [mem.u32(this + STRUT_LENGTH), y.to_bits(), z, f + 0x24, f + 0x28, f + 0x30],
        f - 0x1C,
    );

    let g = calls.get_a48(car, mem.u32(def + 0x43C), mem.u32(def + 0x44C), f - 0xC);
    mem.set_f32(f + 0x38, g);
    let t = mem.f32(f + 0x38) + mem.f32(f + 0x24) + mem.f32(f + 0x28) + mem.f32(f + 0x34);
    let m = mem.f32(def + DEF_INVERSE_MASS);
    let num = t * m * dt + mem.f32(this + STRUT_VEL);
    let den = m * mem.f32(f + 0x30) * dt + STEP_ONE;
    let f6 = num / den;
    let f6 = if STEP_LOW > f6 {
        STEP_LOW
    } else if f6 > STEP_HIGH {
        STEP_HIGH
    } else {
        f6
    };
    mem.set_f32(f + 0x1C, f6);
    let old_z = mem.f32(this + STRUT_VEL);
    let travel = f6 * dt + mem.f32(this + STRUT_LENGTH);
    mem.set_f32(f + 0x3C, old_z);
    mem.set_f32(f + 0x18, travel);
    calls.set_dirty(this + STRUT_TWEAK, f - 4);
    mem.set_f32(this + STRUT_LENGTH, mem.f32(f + 0x18));
    calls.set_dirty(this + 0x5C0, f - 4);

    let q = (f6 - mem.f32(f + 0x3C)) / dt / m;
    mem.set_f32(this + STRUT_VEL, f6);
    let s = mem.f32(f + 0x34) - q;
    mem.set_f32(f + 0x34, s);
    mem.set_u32(f + 0x3C, this + 0x990);
    mem.set_u32(f + 0x18, f + 0x50);
    for k in 0..4 {
        let v = mem.f32(this + 0x990 + 4 * k) * s + mem.f32(f + 0x50 + 4 * k);
        mem.set_f32(f + 0x50 + 4 * k, v);
    }
    let p = f6 * mem.f32(f + 0x30);
    let f24 = mem.f32(f + 0x24);
    let f28 = mem.f32(f + 0x28);
    mem.set_f32(this + 0xE70, mem.f32(f + 0x38));
    mem.set_f32(this + 0xE74, f24);
    mem.set_f32(this + 0xE7C, s);
    mem.set_f32(f + 0x34, p + f24 + f28);
    mem.set_f32(this + 0xE78, p + f28);

    let v0 = mem.f32(this + 0x140) - mem.f32(def + 0x8A0);
    let v1 = mem.f32(this + 0x144) - mem.f32(def + 0x8A4);
    let v2 = mem.f32(this + 0x148) - mem.f32(def + 0x8A8);
    mem.set_f32(f + 0x110, v0);
    mem.set_f32(f + 0x114, v1);
    mem.set_f32(f + 0x118, v2);
    calls.cross(f + 0x110, f + 0x50, f + 0xC0, f - 0xC);
    for k in 0..3 {
        let v = mem.f32(f + 0xC0 + 4 * k) + mem.f32(f + 0x40 + 4 * k);
        mem.set_f32(f + 0xC0 + 4 * k, v);
    }
    if mem.u32(def + 0x21C) != 0 {
        let v = mem.f32(this + 0x9A8) * mem.f32(f + 0xC8)
            + mem.f32(this + 0x9A4) * mem.f32(f + 0xC4)
            + mem.f32(this + 0x9A0) * mem.f32(f + 0xC0);
        let v = v * mem.f32(this + 0x9C0) * mem.f32(this + 0x9C4);
        calls.add_adc(car, v, f - 8);
    }
    calls.cross(this + 0x140, f + 0x50, f + 0x100, f - 0xC);
    let v = mem.f32(f + 0x100) + mem.f32(f + 0x40);
    mem.set_f32(f + 0x40, v);
    let v = mem.f32(f + 0x104) + mem.f32(f + 0x44);
    mem.set_f32(f + 0x44, v);
    let v = mem.f32(f + 0x108) + mem.f32(f + 0x48);
    mem.set_f32(f + 0x48, v);
    let fwd = calls.fwd6dd0(car, mem.u32(this + WHEEL_INDEX), f - 8);
    mem.set_f32(f + 0x38, round24(fwd as f64 * STEP_SCALE as f64) as f32);
    let e9 = this + 0x9E0;
    mem.set_u32(f + 0x3C, e9);
    mem.set_u32(f + 0x18, f + 0x40);
    for k in 0..4 {
        let v = mem.f32(e9 + 4 * k) * mem.f32(f + 0x38) + mem.f32(f + 0x40 + 4 * k);
        mem.set_f32(f + 0x40 + 4 * k, v);
    }
    calls.spin_cross(this, f + 0xF0, f - 8);
    let v = mem.f32(f + 0x50) + mem.f32(this + 0x9F0);
    let d0 = mem.f32(f + 0x40) - mem.f32(f + 0xF0);
    let d1 = mem.f32(f + 0x44) - mem.f32(f + 0xF4);
    let d2 = mem.f32(f + 0x48) - mem.f32(f + 0xF8);
    mem.set_f32(this + 0x9F0, v);
    let v = mem.f32(f + 0x54) + mem.f32(this + 0x9F4);
    mem.set_f32(this + 0x9F4, v);
    let v = mem.f32(f + 0x58) + mem.f32(this + 0x9F8);
    mem.set_f32(this + 0x9F8, v);
    mem.set_f32(f + 0x40, d0);
    mem.set_f32(this + 0xA00, d0 + mem.f32(this + 0xA00));
    mem.set_f32(f + 0x44, d1);
    mem.set_f32(this + 0xA04, d1 + mem.f32(this + 0xA04));
    mem.set_f32(f + 0x48, d2);
    mem.set_f32(this + 0xA08, d2 + mem.f32(this + 0xA08));

    let tyre = mem.u32(this + TYRE);
    let world = mem.u32(car_layout::WORLD);
    let brake = mem.u32(mem.u32(world + WORLD_DEF) + DEF_FLAGS) & FLAG_BRAKE != 0;
    if brake {
        let v = mem.f32(tyre + 0x34) * mem.f32(f + 0x34);
        if v > mem.f32(def + 0x930) {
            let w = (v - mem.f32(def + 0x930)) / (mem.f32(def + 0x940) - mem.f32(def + 0x930));
            let sq = w * w + mem.f32(tyre);
            mem.set_f32(tyre, sq);
            if mem.f32(tyre) > STEP_ONE {
                mem.set_f32(tyre, STEP_ONE);
            }
            calls.state_lerp(this, f - 4);
            mem.set_u32(this + 0xEC0, 1);
        }
        let v = mem.f32(tyre + 0x34) * mem.f32(f + 0x7C);
        calls.set_input(this, v, f - 8);
        let e9 = this + 0x9E0;
        let s = mem.f32(e9 + 8) * mem.f32(f + 0x58) + mem.f32(e9 + 4) * mem.f32(f + 0x54);
        let s = s + mem.f32(f + 0x50) * mem.f32(e9);
        let x = 0.0 - s * mem.f32(tyre + 0x34);
        if x > slip_lo {
            if x > slip_hi {
                mem.set_f32(tyre + 8, STEP_ONE);
            } else {
                let v = (x - slip_lo) * slip_k;
                if v > mem.f32(tyre + 8) {
                    mem.set_f32(tyre + 8, v);
                }
            }
            mem.set_u32(this + 0xEC0, 1);
        }
    }
    calls.fwd43c0(car, mem.u32(this + WHEEL_INDEX), mem.f32(f + 0x20), mem.f32(f + 0x2C), f - 0x10);

    let v = mem.f32(this + STRUT_LENGTH) - mem.f32(tyre) * mem.f32(def + 0x920);
    calls.set_a1c(car, mem.u32(def + 0x43C), mem.u32(def + 0x44C), v, f - 0x10);
    let fwd = calls.fwd6db0(car, mem.u32(this + WHEEL_INDEX), f - 8);
    mem.set_f32(f + 0x18, round24(fwd as f64 * dt as f64) as f32 + mem.f32(this + 0x5AC));
    calls.set_dirty(this + 0x5A0, f - 4);
    mem.set_f32(this + 0x5AC, mem.f32(f + 0x18));
    let x = mem.f32(this + 0x5AC);
    let wrapped = if x > STEP_WRAP {
        Some(x - STEP_WRAP)
    } else if STEP_WRAP > x {
        Some(x + STEP_WRAP)
    } else {
        None
    };
    if let Some(v) = wrapped {
        mem.set_f32(f + 0x18, v);
        calls.set_dirty(this + 0x5A0, f - 4);
        mem.set_f32(this + 0x5AC, mem.f32(f + 0x18));
    }
    calls.relax_count(this + 0xF00, dt, f - 8);
    let fwd = calls.fwd6db0(car, mem.u32(this + WHEEL_INDEX), f - 8);
    mem.set_f32(this + 0xEA0, fwd);
}

// Small wheel objects: constant accessors, the damage reset, and the scratch's `Vf04` (the clears of its vectors).
