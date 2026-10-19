//! The wheel's step (`E51Wheel::M3DD30`) and the calls around it: the pose refresh, the ground probe, `CWheel::Vf28`'s
//! finish and the strut update.

use rbr_physics::math::vec::Vec4;
use rbr_physics::mem::Mem;
use rbr_physics::wheel::layout;
use rbr_physics::wheel::update::StrutCalls;
use rbr_physics::wheel::xform::CarXform;
use rbr_physics::Variant;

use super::{Libm, M3B900, M3D520, MB560, SCRATCH_BELOW_ESP, UNK96E0, UNK97A0};
use crate::entities::BODY_COMPOSE;
use crate::vcu::SET_DIRTY;
use crate::{Ctx, Decline, Host, Regs, Return};
use std::cell::Cell;

/// `E51Wheel::M3B900` (0x53b900), `ret 4`: the wheel's pose refresh for the strut travel `x` (name ours). The
/// prologue's two calls (the lerp `M3AB90` and the turn axis `M3A9E0`, each stored) run here; the rest is
/// `layout::refresh`.
pub(super) fn refresh(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let x = f32::from_bits(r.stack[0]);
    layout::state_lerp(&mut ctx.mem, r.ecx);
    let mut libm = Libm { host: ctx.host, eax: Cell::new(0) };
    layout::turn_axis(&mut libm, &mut ctx.mem, r.ecx);
    let mut calls = Refresh { mem: ctx.mem.clone(), host: ctx.host };
    let eax = layout::refresh(&mut ctx.mem, &mut calls, r.ecx, x, crate::ngp::variant());
    Ok(Return::void(eax, 4))
}

/// The original calls `M3B900` makes, through the host (`mem` shares the emulator's memory).
struct Refresh<'a> {
    mem: Mem,
    host: &'a dyn Host,
}

impl layout::RefreshCalls for Refresh<'_> {
    fn set_dirty(&mut self, object: u32) {
        self.host.call(0x52F450, object, &[]);
    }

    fn tyre_tail(&mut self, container: u32) -> u32 {
        self.host.call(0x5326B0, container, &[]).eax
    }

    fn vf40(&mut self, wheel: u32) -> u32 {
        let target = self.mem.u32(self.mem.u32(wheel) + 0x40);
        self.host.call(target, wheel, &[]).eax
    }
}

/// `E51Wheel::M3D520` (0x53d520), `ret 8`: the ground probe (name ours). Its scratch is the frame of the body's
/// prologue (`S`, see `layout::PROBE_PROLOGUE`), and its callees run below it.
pub(super) fn ground_probe(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [out, tmp, ..] = r.stack;
    let frame = ((r.esp - 4) & !0xF) - layout::PROBE_PROLOGUE;
    let mut calls = Ground { mem: ctx.mem.clone(), host: ctx.host, sp: frame - 0x20 };
    let tuning = match crate::ngp::variant() {
        Variant::Stock102 => layout::ProbeTuning::stock(&ctx.mem),
        Variant::Ngp778 => crate::ngp::probe_tuning(ctx),
    };
    let eax = layout::ground_probe(&mut ctx.mem, &mut calls, r.ecx, frame, out, tmp, tuning);
    Ok(Return::void(eax, 8))
}

/// The original callees of the ground probe. `sp` is the first callee's return slot, below the probe's scratch.
struct Ground<'a> {
    mem: Mem,
    host: &'a dyn Host,
    sp: u32,
}

impl Ground<'_> {
    fn call(&self, addr: u32, ecx: u32, args: &[u32]) -> u32 {
        self.host.call_at(self.sp, addr, ecx, args).eax
    }
}

impl layout::GroundCalls for Ground<'_> {
    fn tyre_tail14(&mut self, container: u32) -> u32 {
        self.call(0x5326C0, container, &[])
    }

    fn table_lerp_pair(&mut self, wheel: u32) {
        self.call(UNK96E0, wheel, &[]);
    }

    fn seg_test170(&mut self, object: u32, a: u32, b: u32) {
        self.call(0x4FF170, object, &[a, b, 0]);
    }

    fn ray_blocks_init(&mut self, rays: u32) {
        self.call(0x70C408, 0, &[rays, 0x6B0, 8, 0x4F6550, 0x4F6740]);
    }

    fn ray_blocks_fini(&mut self, rays: u32) {
        self.call(0x70C4DA, 0, &[rays, 0x6B0, 8, 0x4F6740]);
    }

    fn kernel5bd0(&mut self, world: u32, a: u32, b: u32, out: u32) {
        self.call(0x525BD0, world, &[a, b, 8, 0, out]);
    }

    fn m_d760x(&mut self, object: u32, b: u32, c: u32, d: u32) -> u32 {
        self.call(0x4FD760, object, &[b, c, d, 0])
    }

    fn m46a0(&mut self, world: u32, a: u32, b: u32, list: u32, out: u32) -> u32 {
        self.call(0x5246A0, world, &[a, b, list, out])
    }

    fn hit_flag(&mut self, hit: u32) -> u32 {
        let target = self.mem.u32(self.mem.u32(hit) + 0x154);
        self.call(target, hit, &[])
    }

    fn pose_flag(&mut self, wheel: u32, block: u32, b: f32, c: f32) {
        self.call(0x53D460, wheel, &[block, b.to_bits(), c.to_bits()]);
    }

    fn segment_sweep(&mut self, object: u32, a: u32, b: u32, c: u32, d: u32) -> u32 {
        self.call(0x4FAD90, object, &[a, b, c, d])
    }

    fn pick(&mut self, wheel: u32, a: u32, b: u32, out: u32) {
        self.call(0x539290, wheel, &[a, b, out]);
    }

    fn blend_tags(&mut self, wheel: u32, x: u32, y: u32, z: f32) {
        self.call(0x539900, wheel, &[x, y, z.to_bits()]);
    }

    fn load_binary(&mut self, tmp: u32, data: u32) {
        self.call(0x4F3D20, tmp, &[data]);
    }
}

pub fn wheel_step(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    // The body's frame: `push ebp; mov ebp, esp; and esp, -0x10; sub esp, 0x284; push ebx, esi, edi` (0x290 bytes below the
    // aligned `ebp`, which is the entry's `esp - 4`).
    let frame = ((r.esp - 4) & !0xF) - 0x290;
    let dt = f32::from_bits(r.stack[0]);
    let mut calls = Step { mem: ctx.mem.clone(), host: ctx.host, wheel: r.ecx };
    let slip = crate::ngp::step_slip(ctx);
    layout::wheel_step(&mut ctx.mem, &mut calls, r.ecx, dt, frame, slip);
    Ok(Return::void(0, 4))
}

/// The host calls of `wheel_step`: each one is the original callee at its address, with `sp` below the frame.
struct Step<'a> {
    mem: Mem,
    host: &'a dyn Host,
    /// The wheel, in `esi` through the body.
    wheel: u32,
}

impl Step<'_> {
    fn on(&self, sp: u32, addr: u32, ecx: u32, args: &[u32]) -> u32 {
        self.host.call_at(sp, addr, ecx, args).eax
    }

    fn float(&self, sp: u32, addr: u32, ecx: u32, args: &[u32]) -> f32 {
        self.host.call_at(sp, addr, ecx, args).st0 as f32
    }

    /// The target of the call at `site`: NGP retargets some of the step's calls to its wrappers.
    fn target(&self, site: u32) -> u32 {
        crate::ngp::call_target(&self.mem, site)
    }
}

impl layout::StepCalls for Step<'_> {
    fn strut_name(&mut self, strut: u32, sp: u32) -> u32 {
        self.on(sp, 0x52DB10, strut, &[])
    }

    fn set_dirty(&mut self, object: u32, sp: u32) {
        self.on(sp, 0x52F450, object, &[]);
    }

    fn refresh(&mut self, this: u32, travel: f32, sp: u32) {
        self.on(sp, 0x53B900, this, &[travel.to_bits()]);
    }

    fn ground(&mut self, this: u32, out: u32, tmp: u32, sp: u32) -> u32 {
        self.on(sp, M3D520, this, &[out, tmp])
    }

    fn clear_vec4(&mut self, at: u32, sp: u32) {
        self.on(sp, 0x465D80, at, &[]);
    }

    fn copy_vec4(&mut self, dst: u32, src: u32, sp: u32) {
        self.on(sp, 0x43C5B0, dst, &[src]);
    }

    fn cross(&mut self, a: u32, b: u32, out: u32, sp: u32) {
        self.on(sp, 0x43C630, a, &[b, out]);
    }

    fn scale_vec4(&mut self, src: u32, factor: f32, out: u32, sp: u32) {
        self.on(sp, 0x473CD0, src, &[factor.to_bits(), out]);
    }

    fn car_slot(&mut self, car: u32, args: &[u32], sp: u32) -> u32 {
        // The car's vtable slot 12: `call [[car] + 0x30]`; the arguments stay on the stack for `pair_walk`.
        let vtable = self.mem.u32(car);
        let slot = self.mem.u32(vtable + 0x30);
        self.on(sp, slot, car, args)
    }

    fn pair_walk(&mut self, this: u32, a: u32, b: u32, c: u32, sp: u32) {
        self.on(sp, MB560, this, &[a, b, c]);
    }

    fn world_compose(&mut self, car: u32, a: u32, b: u32, sp: u32) {
        self.on(sp, 0x4E6950, car, &[a, b]);
    }

    fn to_world(&mut self, car: u32, a: u32, b: u32, sp: u32) {
        self.on(sp, 0x4E68E0, car, &[a, b]);
    }

    fn fwd6db0(&mut self, car: u32, a: u32, sp: u32) -> f32 {
        self.float(sp, 0x4E6600, car, &[a])
    }

    fn fwd6dd0(&mut self, car: u32, a: u32, sp: u32) -> f32 {
        self.float(sp, 0x4E6610, car, &[a])
    }

    fn fwd43c0(&mut self, car: u32, i: u32, a: f32, b: f32, sp: u32) {
        self.on(sp, 0x4E6620, car, &[i, a.to_bits(), b.to_bits()]);
    }

    fn get_a48(&mut self, car: u32, i: u32, j: u32, sp: u32) -> f32 {
        self.float(sp, 0x4E6730, car, &[i, j])
    }

    fn add_adc(&mut self, car: u32, v: f32, sp: u32) {
        self.on(sp, 0x4E6660, car, &[v.to_bits()]);
    }

    fn set_a1c(&mut self, car: u32, i: u32, j: u32, v: f32, sp: u32) {
        self.on(sp, 0x4E66A0, car, &[i, j, v.to_bits()]);
    }

    fn init_sub_block(&mut self, this: u32, sp: u32) {
        self.on(sp, 0x4F3E80, this, &[]);
    }

    fn step76(&mut self, this: u32, a: u32, b: u32, sp: u32) {
        self.on(sp, 0x529C80, this, &[a, b]);
    }

    fn spring_step(&mut self, strut: u32, args: [u32; 6], sp: u32) {
        // 0x52E090, or NGP's 0x10059f70
        self.on(sp, self.target(0x53E547), strut, &args);
    }

    fn set_state_field(&mut self, scratch: u32, v: u32, sp: u32) {
        self.on(sp, 0x532470, scratch, &[v]);
    }

    fn set_state48(&mut self, scratch: u32, v: f32, sp: u32) {
        self.on(sp, 0x533490, scratch, &[v.to_bits()]);
    }

    fn relax_count(&mut self, scratch: u32, x: f32, sp: u32) {
        self.on(sp, 0x5358C0, scratch, &[x.to_bits()]);
    }

    fn scratch_step(&mut self, scratch: u32, args: [u32; 16], sp: u32) {
        // 0x5360C0, or NGP's wrapper 0x1005b660
        self.on(sp, self.target(0x53E2A3), scratch, &args);
    }

    fn blend4(&mut self, this: u32, a: u32, b: u32, c: u32, d: u32, sp: u32) {
        self.on(sp, 0x539180, this, &[a, b, c, d]);
    }

    fn update_sub_objects(&mut self, this: u32, a: u32, b: u32, c: u32, sp: u32) {
        self.on(sp, UNK97A0, this, &[a, b, c]);
    }

    fn state_lerp(&mut self, this: u32, sp: u32) {
        self.on(sp, 0x53AB90, this, &[]);
    }

    fn set_input(&mut self, this: u32, v: f32, sp: u32) {
        self.on(sp, 0x53AC50, this, &[v.to_bits()]);
    }

    fn spin_cross(&mut self, this: u32, p: u32, sp: u32) {
        self.on(sp, 0x53AFB0, this, &[p]);
    }

    fn emit(&mut self, multi_tire: u32, sig: u32, sp: u32) {
        // 0x53D0D0, or NGP's 0x1005f8a0, which reads the wheel from esi
        self.host.call_at_esi(sp, self.target(0x53E316), multi_tire, self.wheel, &[sig]);
    }
}

/// `CWheel::Vf28` (0x53f230, thiscall, plain `ret`): `layout::finish`, with the frame of the body (`push ebp; mov ebp,
/// esp; and esp, -0x10; sub esp, 0x94; push ebx, esi, edi`: 0xa0 bytes below the aligned `ebp`) (name ours).
pub(super) fn finish(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let frame = ((r.esp - 4) & !0xF) - 0xA0;
    let mut calls = Finish { host: ctx.host };
    let eax = layout::finish(&mut ctx.mem, &mut calls, r.ecx, frame);
    Ok(Return::void(eax, 0))
}

/// The original callees `Vf28` makes, each at its entry stack pointer (`sp`).
struct Finish<'a> {
    host: &'a dyn Host,
}

impl layout::FinishCalls for Finish<'_> {
    fn reset_outputs(&mut self, strut: u32, sp: u32) {
        self.host.call_at(sp, 0x52E010, strut, &[]);
    }

    fn tyre_tail(&mut self, scratch: u32, sp: u32) {
        self.host.call_at(sp, 0x5326B0, scratch, &[]);
    }

    fn step77(&mut self, this: u32, a: u32, b: u32, sp: u32) {
        self.host.call_at(sp, 0x529D30, this, &[a, b]);
    }

    fn emit(&mut self, signal: u32, payload: u32, sp: u32) {
        self.host.call_at(sp, 0x53D0D0, signal, &[payload]);
    }

    fn emit_noise(&mut self, scratch: u32, sp: u32) {
        self.host.call_at(sp, 0x535890, scratch, &[]);
    }

    fn refresh_travel(&mut self, this: u32, sp: u32) -> u32 {
        self.host.call_at(sp, 0x53B880, this, &[]).eax
    }
}

/// `CCar::BodyToWorldPair` and `CCar::BodyCompose` (thiscall `(a, b)`, `ret 8`) and the strut's callees, as `M3D030`
/// makes them. A transform's source goes through the scratch (null when the definition's guard is zero), its output is
/// read back from the scratch in four lanes.
struct Strut<'a> {
    mem: Mem,
    host: &'a dyn Host,
    scratch: u32,
}

/// The car's transforms, and the strut's callees: `GetName`, `SetDirty`, `M3B900` and `UpdateAndReset`.
const BODY_TO_WORLD_PAIR: u32 = 0x4E69C0;
const GET_NAME: u32 = 0x52DB10;
const UPDATE_AND_RESET: u32 = 0x52E060;

impl Strut<'_> {
    fn transform(&mut self, callee: u32, car: u32, a: Option<Vec4>) -> Vec4 {
        let (a_at, out) = (self.scratch, self.scratch + 0x10);
        let src = match a {
            Some(v) => {
                v.store(&mut self.mem, a_at);
                a_at
            }
            None => 0,
        };
        self.host.call(callee, car, &[src, out]);
        Vec4::load(&self.mem, out)
    }
}

impl CarXform for Strut<'_> {
    fn body_to_world_pair(&mut self, car: u32, a: Option<Vec4>) -> Vec4 {
        self.transform(BODY_TO_WORLD_PAIR, car, a)
    }

    fn body_compose(&mut self, car: u32, a: Option<Vec4>) -> Vec4 {
        self.transform(BODY_COMPOSE, car, a)
    }
}

impl StrutCalls for Strut<'_> {
    fn strut_name(&mut self, strut: u32) -> u32 {
        self.host.call(GET_NAME, strut, &[]).eax
    }

    fn mark_dirty(&mut self, object: u32) {
        self.host.call(SET_DIRTY, object, &[]);
    }

    fn apply_dirty_sum(&mut self, wheel: u32, sum: f32) {
        self.host.call(M3B900, wheel, &[sum.to_bits()]);
    }

    fn update_and_reset(&mut self, strut: u32) {
        self.host.call(UPDATE_AND_RESET, strut, &[]);
    }
}

/// thiscall `()`: `E51Wheel::M3D030`, the strut's update after the car's frames (`layout::update_strut`) (name ours).
pub(super) fn update_strut(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let host = ctx.host;
    let mut calls = Strut { mem: ctx.mem.clone(), host, scratch: r.esp - SCRATCH_BELOW_ESP };
    let mut libm = Libm { host, eax: Cell::new(0) };
    layout::update_strut(&mut calls, &mut libm, &mut ctx.mem, r.ecx);
    Ok(Return::void(0, 0))
}
