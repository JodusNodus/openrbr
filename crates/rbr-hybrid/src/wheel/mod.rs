//! Bindings of the wheel ports (`rbr_physics::wheel`, exe 0x5390d0-0x53dd30). The ABIs are the original's (thiscall
//! `this` in ecx, the bytes `ret` pops). The bindings call the car's slots and the other callees through the host, at the
//! original's depth where the callee reads its stack. `CWheel::Hook` (0x539a20) runs in its profiler scope
//! (`physics_core::scope`).

use rbr_physics::collision::layout::CtorHost;
use rbr_physics::entities::frame::{Form, Hook, Owner};
use rbr_physics::entities::layout::{construct_entity, EntityArgs};
use rbr_physics::math::vec::Vec4;
use rbr_physics::mem::Mem;
use rbr_physics::physics_core::layout::vtable_target;
use rbr_physics::rigid_body::emit::{BodyEvent, EmitHost};
use rbr_physics::rigid_body::host::{HostUnavailable, SolidCtorHost};
use rbr_physics::rigid_body::layout::Body;
use rbr_physics::wheel::axes::CrtTrig;
use rbr_physics::wheel::blend::BlendCalls;
use rbr_physics::wheel::construct::{self, ConstructCalls};
use rbr_physics::wheel::layout;
use rbr_physics::wheel::layout::DefCopyCalls;
use rbr_physics::wheel::select::SubBlockInit;
use rbr_physics::wheel::slots::{self, CarSlotCalls, EmbeddedCalls, HookCalls, SlotCalls, SolidCalls};
use rbr_physics::wheel::sub::SubCalls;
use rbr_physics::wheel::walk::WalkCalls;
use rbr_physics::Variant;

use crate::{aligned_frame, physics_core, rigid_body, Binding, Ctx, Decline, Host, Regs, Return};
use std::cell::Cell;

mod step;

use crate::entities::MB560;
use crate::pure::TYRE_PARTICLES_EMIT;
pub use step::wheel_step;
use step::{finish, ground_probe, refresh, update_strut};

pub const E43VF5C: u32 = 0x539a90; // CWheel::E43Vf5C: the car's slots 28 and 29, then the clears
pub const E43VF58: u32 = 0x539a20; // CWheel::E43Vf58 (Hook): the wheel's slot 96, in its profiler scope
pub const UNK97A0: u32 = 0x5397a0; // CWheel::Unk97a0: Unk94d0 on the gated sub-objects
pub const COPY_CF30: u32 = 0x53cf30; // E51Wheel::CopyCF30: the CDef copies, with sin and cos of the angle
pub const UNK9660: u32 = 0x539660; // CWheel::Unk9660: the phase pick of this+0x5ac, written to the caller's p
pub const UNK96E0: u32 = 0x5396e0; // CWheel::Unk96e0: the two table lerps of the tyre block, from the pick
pub const M3D1C0: u32 = 0x53d1c0; // E51Wheel::M3D1C0: operator new(0x1420), then the sub-object's constructor
pub const M3B5B0: u32 = 0x53b5b0; // E51Wheel::M3B5B0: CSolid::Vf40 on the wheel, then a tail jump to the embedded object's slot 17
pub const M3D520: u32 = 0x53d520; // E51Wheel::M3D520: the ground probe (three ray casts, then the blend)
pub const M3D460: u32 = 0x53d460; // E51Wheel::M3D460: blend the tags, then set the pose clamp flag
pub const VF28: u32 = 0x53f230; // CWheel::Vf28: the finish of the step (reset, tyre tail, Step77, the emits, M3B880)
/// E51Wheel::M3DD30 (the slot-96 step, vtable 0x180).
pub const M3DD30: u32 = 0x53dd30;
/// The tire-particle signal's Emit: the tyre-noise Emit's template.
pub const E43VF20: u32 = 0x539090; // CWheel::E43Vf20 (vtable slot 8): clears two vectors, Vf40, then the solid's Vf20
pub const BLEND4: u32 = 0x539180; // CWheel::Blend4: the two tyre-scaled blends
pub const UNK9290: u32 = 0x539290; // CWheel::Unk9290: the frame pick into the caller's out
pub const UNK9900: u32 = 0x539900; // CWheel::Unk9900: the material blend of two tags
pub const E43VF6C: u32 = 0x539eb0; // CWheel::E43Vf6C (vtable slot 27): a tail call of CSolid::E43Vf6C
pub const M3A9E0: u32 = 0x53a9e0; // E51Wheel::M3A9E0: the turn axis
pub const M3AB90: u32 = 0x53ab90; // E51Wheel::M3AB90: the state lerp
pub const M3AC50: u32 = 0x53ac50; // E51Wheel::M3AC50: the input setter
pub const M3AEF0: u32 = 0x53aef0; // E51Wheel::M3AEF0: the walk and its gain
pub const M3AFB0: u32 = 0x53afb0; // E51Wheel::M3AFB0: the spin's cross product
pub const M3B900: u32 = 0x53b900; // E51Wheel::M3B900: the pose refresh
pub const M3D030: u32 = 0x53d030; // E51Wheel::M3D030: the car frames, the axes, the strut's blend, M3B900, UpdateAndReset
pub const UNKA860: u32 = 0x53a860; // CWheel::UnkA860: the tyre block, the strut's and the tyre scratch's setters
pub const UNK94D0: u32 = 0x5394d0; // CWheel::Unk94d0: the split of Unk97a0's vector on a sub-object
pub const M3B880: u32 = 0x53b880; // E51Wheel::M3B880: the strut's travel clamp, the end of Vf28
/// `CTyre::CScratch::BlendTable` (thiscall, `ret 0x18`): the two blend coefficients into its two out pointers.
const BLEND_TABLE: u32 = 0x534E70;

const fn bind(addr: u32, name: &'static str, run: crate::BindingFn) -> Binding {
    Binding { addr, name, subsystem: "wheel", run }
}

pub static BINDINGS: [Binding; 30] = [
    bind(M3D520, "E51Wheel::M3D520", ground_probe),
    bind(E43VF5C, "CWheel::E43Vf5C", forward_to_car_and_clear),
    bind(E43VF58, "CWheel::E43Vf58", hook),
    bind(UNK97A0, "CWheel::Unk97a0", update_sub_objects),
    bind(COPY_CF30, "E51Wheel::CopyCF30", copy_cf30),
    bind(UNK9660, "CWheel::Unk9660", uint_select),
    bind(UNK96E0, "CWheel::Unk96e0", table_lerp_pair),
    bind(M3B5B0, "E51Wheel::M3B5B0", embedded_dispatch),
    bind(M3D1C0, "E51Wheel::M3D1C0", new_sub_object),
    bind(M3D460, "E51Wheel::M3D460", pose_flag),
    bind(BLEND_OPS, "E50BlendOps::Blend", blend_ops),
    bind(M3DD30, "E51Wheel::M3DD30", wheel_step),
    bind(VF28, "CWheel::Vf28", finish),
    bind(
        TYRE_PARTICLES_EMIT,
        "CMultiSignal1<CSigTireParticles>::Emit",
        crate::tyre::signal_emit::<TYRE_PARTICLES_EMIT>,
    ),
    bind(E43VF20, "CWheel::E43Vf20", clear_and_forward),
    bind(BLEND4, "CWheel::Blend4", blend4),
    bind(UNK9290, "CWheel::Unk9290", pick_contact),
    bind(UNK9900, "CWheel::Unk9900", blend_tags),
    bind(E43VF6C, "CWheel::E43Vf6C", forward_vf6c),
    bind(M3A9E0, "E51Wheel::M3A9E0", turn_axis),
    bind(M3AB90, "E51Wheel::M3AB90", state_lerp),
    bind(M3AC50, "E51Wheel::M3AC50", set_input),
    bind(M3AEF0, "E51Wheel::M3AEF0", walk_result),
    bind(M3AFB0, "E51Wheel::M3AFB0", spin_cross),
    bind(M3B900, "E51Wheel::M3B900", refresh),
    bind(M3D030, "E51Wheel::M3D030", update_strut),
    bind(UNKA860, "CWheel::UnkA860", tyre_tail_setup),
    bind(UNK94D0, "CWheel::Unk94d0", split_on_sub),
    bind(M3B880, "E51Wheel::M3B880", refresh_travel),
    bind(SUB_OBJECT_CTOR, "E51Wheel::Ctor", wheel_ctor),
];

/// The original callees a wheel binding makes: the vtable calls on an object (`mem` shares the emulator's memory).
struct Calls<'a> {
    mem: Mem,
    host: &'a dyn Host,
}

impl Calls<'_> {
    /// Calls the object's vtable slot `offset` (a thiscall with one stack word, `p`).
    fn slot(&self, obj: u32, offset: u32, p: u32) -> u32 {
        let target = self.mem.u32(self.mem.u32(obj) + offset);
        self.host.call(target, obj, &[p]).eax
    }
}

impl CarSlotCalls for Calls<'_> {
    fn car_vf70(&mut self, car: u32, p: u32) {
        self.slot(car, 0x70, p);
    }

    fn car_vf74(&mut self, car: u32, p: u32) {
        self.slot(car, 0x74, p);
    }
}

/// `Unk94d0` as the asm calls it: ecx is the wheel, the stack is the sub-object, then the vector (written at `Unk97a0`'s
/// local, its aligned frame's -0x10), `a`, `b` and `c`; the call enters at the frame's -0x58, as the original's.
struct Sub<'a> {
    mem: Mem,
    host: &'a dyn Host,
    frame: u32,
}

impl SubCalls for Sub<'_> {
    fn sub_unk94d0(&mut self, wheel: u32, sub: u32, v: Vec4, a: u32, b: u32, c: u32) {
        let v_at = self.frame - 0x10;
        v.store(&mut self.mem, v_at);
        self.host.call_at(self.frame - 0x58, UNK94D0, wheel, &[sub, v_at, a, b, c]);
    }
}

/// thiscall `()`: the car's slots 28 and 29 with `this+0x9f0` and `this+0xa00`, then the two blocks cleared (name
/// ours). The second `E09Vec4::Clear` leaves eax at its `this` argument, `this+0xa00`.
fn forward_to_car_and_clear(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Calls { mem: ctx.mem.clone(), host: ctx.host };
    layout::forward_to_car_and_clear(&mut calls, &mut ctx.mem, r.ecx);
    Ok(Return::void(r.ecx.wrapping_add(0xA00), 0))
}

/// `E50BlendOps::Blend` (0x5381f0, thiscall, five stack words: `a`, `b` and the floats `c`, `d`, `e`; `ret 0x14`), the
/// material blend `CWheel::Unk9900` makes.
pub const BLEND_OPS: u32 = 0x5381F0;

fn blend_ops(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, d, e, ..] = r.stack;
    let (c, d, e) = (f32::from_bits(c), f32::from_bits(d), f32::from_bits(e));
    rbr_physics::wheel::blend::blend_ops(&mut ctx.mem, r.ecx, a, b, c, d, e);
    Ok(Return::void(0, 0x14))
}

/// thiscall `(a, b, c)`, eax: 1 when a sub-object is gated on (name ours).
fn update_sub_objects(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, ..] = r.stack;
    let mut calls = Sub { mem: ctx.mem.clone(), host: ctx.host, frame: aligned_frame(r.esp) };
    let gated = layout::update_sub_objects(&mut calls, &mut ctx.mem, r.ecx, a, b, c);
    Ok(Return::void(u32::from(gated), 0xC))
}

/// The three host calls `CopyCF30` makes (thiscall; the objects are the strut, the scratch and the CDef's base).
struct DefCopy<'a> {
    host: &'a dyn Host,
}

impl DefCopyCalls for DefCopy<'_> {
    fn copy_state_to_def(&mut self, _: &mut Mem, strut: u32, b: u32) {
        self.host.call(0x52DBD0, strut, &[b]);
    }

    fn copy_def_from(&mut self, _: &mut Mem, scratch: u32, c: u32) {
        self.host.call(0x534760, scratch, &[c]);
    }

    fn set_dirty(&mut self, _: &mut Mem, base: u32) {
        self.host.call(0x52F450, base, &[]);
    }
}

/// The CRT sine and cosine (cdecl, the angle on the stack; x87 results). Each call's eax is kept: the function's
/// eax is the last call's.
struct Libm<'a> {
    host: &'a dyn Host,
    eax: Cell<u32>,
}

impl Libm<'_> {
    fn call(&self, addr: u32, angle: f32) -> f64 {
        let called = self.host.call(addr, 0, &[angle.to_bits()]);
        self.eax.set(called.eax);
        called.st0
    }
}

impl CrtTrig for Libm<'_> {
    fn sin(&mut self, angle: f32) -> f64 {
        self.call(0x53F3B0, angle)
    }

    fn cos(&mut self, angle: f32) -> f64 {
        self.call(0x53F3A0, angle)
    }
}

/// thiscall `(a, b, c)`: the CDef copies. The asm leaves the sine in the caller's `b` slot and the angle in `c`'s.
fn copy_cf30(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, ..] = r.stack;
    let host = ctx.host;
    let mut libm = Libm { host, eax: Cell::new(0) };
    let mut calls = DefCopy { host };
    layout::copy_cf30(&mut calls, &mut libm, &mut ctx.mem, r.ecx, a, b, c);
    let def = ctx.mem.u32(r.ecx + layout::DEF);
    ctx.mem.set_u32(r.esp + 8, ctx.mem.u32(def + layout::CF_DEF_SIN));
    ctx.mem.set_u32(r.esp + 0xC, ctx.mem.u32(a + layout::CF_SRC_ANG));
    Ok(Return::void(libm.eax.get(), 0xC))
}

/// `Unk9660` (thiscall, `p` and `n` on the stack, `ret 8`): the pick is written to `p`, and the wrap count is returned
/// (name ours).
fn uint_select(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [p, n, ..] = r.stack;
    let wraps = layout::uint_select(&mut ctx.mem, r.ecx, p, n);
    Ok(Return::void(wraps, 8))
}

/// `CE36SubBlock::Init` (0x4f3e80, thiscall) as the original calls it from `Unk96e0`.
struct Init<'a> {
    host: &'a dyn Host,
}

impl SubBlockInit for Init<'_> {
    fn init_sub_block(&mut self, block: u32) {
        self.host.call(0x4F3E80, block, &[]);
    }
}

/// `Unk96e0` (thiscall, plain `ret`) (name ours). Its local is the free stack below the return address (as `Unk97a0`'s
/// payload). The eax it returns is the tyre block pointer, which the asm leaves there.
fn table_lerp_pair(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut init = Init { host: ctx.host };
    let tuning = match crate::ngp::variant() {
        Variant::Stock102 => layout::WheelTuning::STOCK,
        Variant::Ngp778 => crate::ngp::wheel_tuning(ctx),
    };
    let li = r.esp.wrapping_sub(0x10);
    let block = layout::table_lerp_pair(&mut init, &mut ctx.mem, r.ecx, li, tuning);
    Ok(Return::void(block, 0))
}

/// `M3B5B0`'s scratch: below the return address, as the rigid-body bindings use it for `E46Mat::Mul`'s operands.
const SCRATCH_BELOW_ESP: u32 = 0x800;

/// The calls `M3B5B0` makes, run as the rigid-body ports run them: `Vf16Helper` on the wheel, then the embedded object's
/// slot 17 (`Vf17x`, 0x529a50), whose eax is the matrix at `+0x30` (the asm's last `SetFromQuat` return).
struct Dispatch<'c, 'h> {
    ctx: &'c mut Ctx<'h>,
    scratch: u32,
}

impl EmbeddedCalls for Dispatch<'_, '_> {
    fn solid_vf40(&mut self, wheel: u32) -> Result<(), HostUnavailable> {
        rigid_body::vf16(self.ctx, wheel, self.scratch).map(drop)
    }

    fn embedded_slot17(&mut self, obj: u32) -> Result<u32, HostUnavailable> {
        if self.ctx.mem.u32(self.ctx.mem.u32(obj) + 0x44) != rigid_body::VF17X {
            return Err(HostUnavailable);
        }
        rigid_body::vf17x_state(self.ctx, obj, self.scratch)?;
        Ok(obj + 0x30)
    }
}

/// `M3B5B0` (thiscall, no stack words, plain `ret`) (name ours). The tail's target is checked before anything runs, so
/// a different target declines with no side effect.
fn embedded_dispatch(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let obj = r.ecx + layout::EMBEDDED_OBJ;
    if ctx.mem.u32(ctx.mem.u32(obj) + 0x44) != rigid_body::VF17X {
        return Err(Decline::Unsupported("the embedded object's slot 17 is not Vf17x"));
    }
    let mut calls = Dispatch { ctx, scratch: r.esp - SCRATCH_BELOW_ESP };
    let eax = layout::embedded_dispatch(&mut calls, r.ecx)
        .map_err(|HostUnavailable| Decline::Unsupported("a rigid-body port is unavailable"))?;
    Ok(Return::void(eax, 0))
}

/// The sub-object's constructor (`E51Wheel::Ctor`, 0x53b6e0, thiscall, four stack words).
const SUB_OBJECT_CTOR: u32 = 0x53b6e0;

/// The original callees `M3D1C0` makes, through the host: `operator new` and the constructor (bound below).
struct Construct<'a>(&'a dyn Host);

impl ConstructCalls for Construct<'_> {
    fn allocate(&mut self, size: u32) -> u32 {
        self.0.call(rigid_body::OPERATOR_NEW, 0, &[size]).eax
    }

    fn construct(&mut self, obj: u32, outer: u32, a: u32, b: u32) -> u32 {
        self.0.call(SUB_OBJECT_CTOR, obj, &[outer, a, b, 0]).eax
    }
}

/// thiscall `(a, b)`: `E51Wheel::M3D1C0` (0x53d1c0), `ret 8` (name ours).
fn new_sub_object(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = construct::construct(&mut Construct(ctx.host), r.ecx, r.stack[0], r.stack[1]);
    Ok(Return::void(eax, 8))
}

/// thiscall `(p)`, `ret 4`: `CWheel::E43Vf20` (vtable slot 8), `layout::clear_and_forward` (name ours).
fn clear_and_forward(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Slots { mem: ctx.mem.clone(), host: ctx.host };
    layout::clear_and_forward(&mut calls, &mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// The calls `E43Vf20` makes outside the wheel: its `Vf40` (the vtable's slot 0x40, no stack word) and
/// `CSolid::E43Vf20`.
struct Slots<'a> {
    mem: Mem,
    host: &'a dyn Host,
}

impl SlotCalls for Slots<'_> {
    fn wheel_vf40(&mut self, wheel: u32) {
        let target = self.mem.u32(self.mem.u32(wheel) + 0x40);
        self.host.call(target, wheel, &[]);
    }

    fn solid_vf20(&mut self, wheel: u32, p: u32) {
        self.host.call(rigid_body::VF20_JUMP, wheel, &[p]);
    }
}

/// thiscall `(a, b, c, d)`, `ret 0x10`: `CWheel::Blend4`, `layout::blend_pair`.
fn blend4(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, d, ..] = r.stack;
    layout::blend_pair(&mut ctx.mem, r.ecx, a, b, c, d);
    Ok(Return::void(0, 0x10))
}

/// thiscall `(a, b, out)`, `ret 0xC`: `CWheel::Unk9290`, `layout::pick_contact` (name ours).
fn pick_contact(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, out, ..] = r.stack;
    layout::pick_contact(&mut ctx.mem, r.ecx, a, b, out, crate::ngp::variant());
    Ok(Return::void(0, 0xC))
}

/// thiscall `(x, y, z)`, `ret 0xC`: `CWheel::Unk9900`, `layout::blend_tags` (name ours). Its `BlendTable` writes two
/// words below the frame, and the caller reads them there.
fn blend_tags(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [x, y, z, ..] = r.stack;
    let mut calls = Blends { mem: ctx.mem.clone(), host: ctx.host, out: r.esp - SCRATCH_BELOW_ESP };
    layout::blend_tags(&mut calls, &mut ctx.mem, r.ecx, x, y, f32::from_bits(z));
    Ok(Return::void(0, 0xC))
}

/// The calls `Unk9900` makes outside the wheel: `BlendTable` on the tyre scratch (its out pointers are `out` and
/// `out+4`) and `E50BlendOps::Blend`.
struct Blends<'a> {
    mem: Mem,
    host: &'a dyn Host,
    out: u32,
}

impl BlendCalls for Blends<'_> {
    fn blend_coefficients(&mut self, scratch: u32, i: u32, j: u32, a: f32, b: f32) -> (f32, f32) {
        let (o1, o2) = (self.out, self.out + 4);
        self.host.call(BLEND_TABLE, scratch, &[i, j, a.to_bits(), b.to_bits(), o1, o2]);
        (self.mem.f32(o1), self.mem.f32(o2))
    }

    fn blend_materials(&mut self, ops: u32, s1: u32, s2: u32, a: f32, b: f32, z: f32) {
        self.host.call(BLEND_OPS, ops, &[s1, s2, a.to_bits(), b.to_bits(), z.to_bits()]);
    }
}

/// The hook's profiler sample (`g_hookChannel`) and its name ("CWheel::Hook").
const HOOK_SAMPLE: u32 = 0x165f268;
const HOOK_NAME: u32 = 0x742a54;

/// thiscall `(p)`, `ret 4`: `CWheel::E43Vf58` (Hook, 0x539a20, vtable slot 22): `slots::hook` in its profiler scope
/// (name ours). The channel is in the frame at -0x14, and `ProfExit`'s counter word at -0x24 (its entry is at -0x1c).
fn hook(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = physics_core::scope(ctx, r.esp - 0x14, HOOK_SAMPLE, HOOK_NAME, r.esp - 0x24, |ctx| {
        let mut calls = Slot96 { mem: ctx.mem.clone(), host: ctx.host };
        slots::hook(&mut calls, r.ecx, r.stack[0]);
    });
    Ok(Return::void(eax, 4))
}

/// The call `Hook` makes on the wheel: its vtable slot 96 (0x180), the step (`M3DD30`).
struct Slot96<'a> {
    mem: Mem,
    host: &'a dyn Host,
}

impl HookCalls for Slot96<'_> {
    fn wheel_vf180(&mut self, wheel: u32, p: u32) {
        let target = self.mem.u32(self.mem.u32(wheel) + 0x180);
        self.host.call(target, wheel, &[p]);
    }
}

/// thiscall `(f)`, `ret 4`: `CWheel::E43Vf6C`, `slots::forward_vf6c` (name ours).
fn forward_vf6c(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    slots::forward_vf6c(&mut Solid(ctx.host), r.ecx, f32::from_bits(r.stack[0]));
    Ok(Return::void(0, 4))
}

/// The tail call of `E43Vf6C`: `CSolid::E43Vf6C` (void) on the wheel. Its eax is not kept.
struct Solid<'a>(&'a dyn Host);

impl SolidCalls for Solid<'_> {
    fn solid_vf6c(&mut self, wheel: u32, f: f32) {
        self.0.call(rigid_body::E43VF6C, wheel, &[f.to_bits()]);
    }
}

/// thiscall, plain `ret`: `E51Wheel::M3A9E0`, `layout::turn_axis` (name ours). eax is the last sine or cosine call's.
fn turn_axis(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut libm = Libm { host: ctx.host, eax: Cell::new(0) };
    layout::turn_axis(&mut libm, &mut ctx.mem, r.ecx);
    Ok(Return::void(libm.eax.get(), 0))
}

/// thiscall, plain `ret`: `E51Wheel::M3AB90`, `layout::state_lerp` (name ours).
fn state_lerp(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::state_lerp(&mut ctx.mem, r.ecx);
    Ok(Return::void(0, 0))
}

/// thiscall `(v)`, `ret 4`: `E51Wheel::M3AC50`, `layout::set_input` (name ours). eax is the last sine or cosine call's.
fn set_input(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut libm = Libm { host: ctx.host, eax: Cell::new(0) };
    layout::set_input(&mut libm, &mut ctx.mem, r.ecx, f32::from_bits(r.stack[0]));
    Ok(Return::void(libm.eax.get(), 4))
}

/// The walk `E43PairWalk::MB560` makes for `M3AEF0` and `M3AFB0`: `a` is copied to the scratch below the frame, and the
/// walk's output `c` is the scratch's next 16 bytes.
struct Walk<'a> {
    mem: Mem,
    host: &'a dyn Host,
    scratch: u32,
}

impl WalkCalls for Walk<'_> {
    fn pair_walk(&mut self, walker: u32, a: Option<Vec4>, car: u32) -> Vec4 {
        let a = match a {
            Some(v) => {
                v.store(&mut self.mem, self.scratch);
                self.scratch
            }
            None => 0,
        };
        let c = self.scratch + 0x10;
        self.host.call(MB560, walker, &[a, car, c]);
        Vec4::load(&self.mem, c)
    }
}

/// thiscall, plain `ret`: `E51Wheel::M3AEF0`, `layout::walk_result` (name ours).
fn walk_result(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut walk = Walk { mem: ctx.mem.clone(), host: ctx.host, scratch: r.esp - SCRATCH_BELOW_ESP };
    layout::walk_result(&mut walk, &mut ctx.mem, r.ecx, crate::ngp::variant());
    Ok(Return::void(0, 0))
}

/// thiscall `(p)`, `ret 4`: `E51Wheel::M3AFB0`, `layout::spin_cross` (name ours).
fn spin_cross(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut walk = Walk { mem: ctx.mem.clone(), host: ctx.host, scratch: r.esp - SCRATCH_BELOW_ESP };
    layout::spin_cross(&mut walk, &mut ctx.mem, r.ecx, r.stack[0], crate::ngp::variant());
    Ok(Return::void(0, 4))
}

/// thiscall `(blk, b, c)`, `ret 0xC`: `E51Wheel::M3D460`, `layout::pose_flag` (name ours).
fn pose_flag(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::ngp::only()?;
    let [blk, b, c, ..] = r.stack;
    layout::pose_flag(&mut ctx.mem, blk, f32::from_bits(b), f32::from_bits(c));
    Ok(Return::void(0, 0xC))
}

/// thiscall `(a, b, c)`, `ret 0xc`: `CWheel::UnkA860`, the tyre block and the two setters (`layout::tyre_tail_setup`)
/// (name ours).
fn tyre_tail_setup(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, ..] = r.stack;
    layout::tyre_tail_setup(&mut ctx.mem, r.ecx, a, b, c);
    Ok(Return::void(0, 0xC))
}

/// thiscall `(sub, v, acc, b, c)`, `ret 0x14`: `CWheel::Unk94d0`, `layout::split_on_sub` (name ours). eax is the copy's
/// destination.
fn split_on_sub(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [sub, v, acc, ..] = r.stack;
    let eax = layout::split_on_sub(&mut ctx.mem, r.ecx, sub, v, acc, r.esp);
    Ok(Return::void(eax, 0x14))
}

/// thiscall, plain `ret`: `E51Wheel::M3B880`, `layout::refresh_travel` (name ours).
fn refresh_travel(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::refresh_travel(&mut ctx.mem, r.ecx), 0))
}

/// `CSolid::E43Vf90` (0x52bf30), the owner's wake the constructor calls (directly, 0x20 below its entry).
const SOLID_WAKE: u32 = 0x52bf30;

/// The calls `E51Wheel::Ctor` makes outside the wheel's own constructors: the `CSolid` base, built by its port, whose
/// `CEntity` frames ask the parent's slots 12 and 13 through the host, and the owner's wake, `E43Vf90`, through the host
/// at the original's depth.
struct WheelCtor<'a> {
    mem: Mem,
    host: &'a dyn Host,
    this: u32,
    wake_sp: u32,
}

impl SolidCtorHost for WheelCtor<'_> {
    fn solid_ctor(
        &mut self,
        mem: &mut Mem,
        solid: u32,
        def: u32,
        a: u32,
        b: u32,
        flags: u32,
    ) -> Result<(), HostUnavailable> {
        rbr_physics::rigid_body::layout::construct_solid(mem, &mut ParentFrames(self.host), solid, def, a, b, flags);
        Ok(())
    }
}

impl EmitHost for WheelCtor<'_> {
    fn slot18(&mut self, _: &mut Body) {
        unreachable!("the wheel's wake is CSolid::E43Vf90's")
    }

    fn entity_notify(&mut self) {
        unreachable!("the wheel's wake is CSolid::E43Vf90's")
    }

    fn emit(&mut self, _: &BodyEvent) -> Result<(), HostUnavailable> {
        unreachable!("the wheel's wake is CSolid::E43Vf90's")
    }

    fn wake(&mut self, body: &mut Body) -> Result<(), HostUnavailable> {
        body.store(&mut self.mem, self.this);
        self.host.call_at(self.wake_sp, SOLID_WAKE, self.this, &[]);
        *body = Body::load(&self.mem, self.this);
        Ok(())
    }
}

/// `CEntity::CEntity` (0x51a640) under the `CSolid` base, its frames hung under the parent's: the parent's slot 12 (the
/// frame) and 13 (the old frame) through the host.
struct ParentFrames<'a>(&'a dyn Host);

impl CtorHost for ParentFrames<'_> {
    fn entity_ctor(&mut self, mem: &mut Mem, body: u32, definition: u32, frame: u32, parent: u32, flags: u32) -> u32 {
        let args = EntityArgs { definition, sort_key: frame, parent, options: flags };
        construct_entity(mem, body, &args, &mut Frames { mem: mem.clone(), host: self.0 })
    }
}

/// The parent's frame slots, as the frame constructors ask them.
struct Frames<'a> {
    mem: Mem,
    host: &'a dyn Host,
}

impl Owner for Frames<'_> {
    fn hook(&mut self, _: u32, _: Hook, _: Form, _: u32, _: u32) -> u32 {
        unreachable!("a frame's constructor calls no hook")
    }

    fn gate(&mut self, _: u32) -> u32 {
        unreachable!("a frame's constructor asks no gate")
    }

    fn frame_of(&mut self, ent: u32) -> u32 {
        self.host.call(vtable_target(&self.mem, ent, 0x30), ent, &[]).eax
    }

    fn old_frame_of(&mut self, ent: u32) -> u32 {
        self.host.call(vtable_target(&self.mem, ent, 0x34), ent, &[]).eax
    }
}

/// thiscall `(def, name, owner, flags)`, `ret 0x10`: `E51Wheel::Ctor` (0x53b6e0), `layout::wheel_ctor`. eax is the wheel.
fn wheel_ctor(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [def, name, owner, flags, ..] = r.stack;
    let mut host = WheelCtor { mem: ctx.mem.clone(), host: ctx.host, this: r.ecx, wake_sp: r.esp - 0x20 };
    let this = layout::wheel_ctor(&mut ctx.mem, &mut host, r.ecx, def, name, owner, flags)
        .expect("the constructor's host makes every call");
    Ok(Return::void(this, 0x10))
}
