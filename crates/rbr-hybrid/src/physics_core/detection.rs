//! The detectors' `Vf04`s, the collide window and pass (`CPhysics::CollideWork`, `Work6a70`) and the penetration
//! detector's constructor.

use rbr_physics::mem::Mem;
use rbr_physics::physics_core::layout;
use rbr_physics::physics_core::layout::detect_calls::{self as detect, PenetrationHost};
use rbr_physics::physics_core::layout::detector_hosts::{CollideHost, DetectorHost, KernelTest};

use super::slots::vslot;
use crate::collision::{BODY_GRID, E01_VEC4_COPY_FROM, E37_GRID_COPY, E45_CELL_QUERY, RAY_CLEAR, RAY_SEEDS};
use crate::entities::{EXCLUSION_TEST, WALK_PRE_ORDER};
use crate::probe_cache::BODY_OVERLAPS;
use crate::{Ctx, Decline, Host, Regs, Return};

/// The detectors' and the contact loop's calls on the original, at the entry stack pointers the port gives.
struct Detectors<'a> {
    mem: Mem,
    host: &'a dyn Host,
    /// The probe kernel's last direct `CBody::VfCC` (`kernel_collect` writes none of the words its fetches read or
    /// write while it walks the cells), `None` elsewhere.
    vf_cc: Option<Option<crate::collision::VfCcCorners>>,
    /// The probe kernel's entity tests from what is known of the entities (pure host).
    kernel: crate::probe_cache::KernelCall,
}

impl Detectors<'_> {
    fn new<'a>(ctx: &Ctx<'a>) -> Detectors<'a> {
        Detectors { mem: ctx.mem.clone(), host: ctx.host, vf_cc: None, kernel: Default::default() }
    }

    fn slot(&self, sp: u32, entity: u32, offset: u32, args: &[u32]) -> u32 {
        let target = vslot(&self.mem, entity, offset);
        self.host.call_at(sp, target, entity, args).eax
    }
}

impl DetectorHost for Detectors<'_> {
    fn collision_grid(&mut self, sp: u32, entity: u32) -> u32 {
        // A body's (the probes ask each body in their cells twice) is read here.
        // An entity's without a grid (`CEntity::Vf118`) is 0.
        let target = vslot(&self.mem, entity, 0x118);
        if matches!(target, BODY_GRID | crate::entities::ENTITY_VF118)
            && crate::skip_call(self.host, &mut self.mem, sp, &[])
        {
            return match target {
                BODY_GRID => rbr_physics::collision::layout::Body::new(entity).grid(&self.mem),
                _ => 0,
            };
        }
        self.slot(sp, entity, 0x118, &[])
    }

    fn entity_110(&mut self, sp: u32, entity: u32) -> u32 {
        if vslot(&self.mem, entity, 0x110) == crate::collision::BODY_VF110
            && crate::skip_call(self.host, &mut self.mem, sp, &[])
        {
            return rbr_physics::collision::layout::body_grid_active(&self.mem, entity) as u32;
        }
        self.slot(sp, entity, 0x110, &[])
    }

    fn entity_10c(&mut self, sp: u32, entity: u32) -> u32 {
        self.slot(sp, entity, 0x10c, &[])
    }

    fn entity_30(&mut self, sp: u32, entity: u32) -> u32 {
        self.slot(sp, entity, 0x30, &[])
    }

    fn entity_34(&mut self, sp: u32, entity: u32) -> u32 {
        self.slot(sp, entity, 0x34, &[])
    }

    fn entity_box(&mut self, sp: u32, entity: u32, lo: u32, hi: u32) {
        // `CEntity::Vf120`'s jump to a body's `CBody::Vf11C` enters at its own esp, with its words.
        if vslot(&self.mem, entity, 0x120) == crate::entities::ENTITY_VF120
            && vslot(&self.mem, entity, 0x11c) == crate::collision::BODY_VF11C
            && crate::collision::body_box_direct(self.host, &mut self.mem, sp, entity, lo, hi)
        {
            return;
        }
        self.slot(sp, entity, 0x120, &[lo, hi]);
    }

    fn overlaps_box(&mut self, sp: u32, entity: u32, lo: u32, hi: u32) -> u32 {
        if vslot(&self.mem, entity, 0xcc) == BODY_OVERLAPS {
            let mut none = None;
            let last = self.vf_cc.as_mut().unwrap_or(&mut none);
            if let Some(flag) = crate::collision::body_vf_cc_direct(self.host, &mut self.mem, sp, entity, lo, hi, last)
            {
                return flag;
            }
        }
        self.slot(sp, entity, 0xcc, &[lo, hi])
    }

    fn entity_d0(&mut self, sp: u32, entity: u32, pair: u32, other: u32, extra: u32) -> u32 {
        self.slot(sp, entity, 0xd0, &[pair, other, extra])
    }

    fn entity_d4(&mut self, sp: u32, entity: u32, args: [u32; 7]) {
        self.slot(sp, entity, 0xd4, &args);
    }

    fn child_gate(&mut self, sp: u32, entity: u32) -> u32 {
        self.slot(sp, entity, 0x80, &[])
    }

    fn entity_30_with(&mut self, sp: u32, entity: u32, arg: u32) -> u32 {
        self.slot(sp, entity, 0x30, &[arg])
    }

    fn exclusion_test(&mut self, sp: u32, entity: u32, other: u32) -> u32 {
        if let Some(eax) = crate::entities::exclusion_test_direct(self.host, &mut self.mem, sp, entity, other) {
            return eax;
        }
        self.host.call_at(sp, EXCLUSION_TEST, entity, &[other]).eax
    }

    fn grid_ops(&mut self, sp: u32, at: u32, grid: u32, frame: u32, other_frame: u32) {
        self.host.call_at(sp, E37_GRID_COPY, at, &[grid, frame, other_frame]);
    }

    fn min_max(&mut self, sp: u32, at: u32, other: u32, lo: u32, hi: u32) {
        self.host.call_at(sp, MIN_MAX, at, &[other, lo, hi]);
    }

    fn ray_clear(&mut self, sp: u32, at: u32) {
        self.host.call_at(sp, RAY_CLEAR, at, &[]);
    }

    fn cells(&mut self, sp: u32, lo: u32, hi: u32, out: u32) {
        self.host.call_at(sp, E45_CELL_QUERY, 0, &[lo, hi, out]);
    }

    fn kernel_tests(&mut self, mem: &mut Mem, frame: u32, node: u32, mask: u32, excluded: u32) -> Option<KernelTest> {
        let last = self.vf_cc.as_ref()?;
        let ret = self.host.call_slot_word()?;
        let host = self.host;
        let corners = || last.and_then(|c| c.for_call(frame - 0xc, frame + 0x28, frame + 0x20, host.entered()));
        self.kernel.tests(mem, ret, frame, node, (mask, excluded), corners)
    }

    fn penetration_ctor(&mut self, sp: u32, at: u32, owner: u32, lo: u32, hi: u32, scale: u32, word: u32) {
        self.host.call_at(sp, PENETRATION_CTOR, at, &[owner, lo, hi, scale, word]);
    }

    fn notify_filtered(&mut self, sp: u32, cell: u32, detector: u32) {
        self.host.call_at(sp, NOTIFY_FILTERED, cell, &[detector]);
    }

    fn detect(&mut self, sp: u32, detector: u32, node: u32) {
        self.slot(sp, detector, 4, &[node]);
    }

    fn walk(&mut self, sp: u32, entity: u32, detector: u32) {
        self.host.call_at(sp, WALK_PRE_ORDER, entity, &[detector]);
    }

    fn visit_list470(&mut self, sp: u32, physics: u32, detector: u32) {
        self.host.call_at(sp, VISIT_470, physics, &[detector]);
    }
}

impl CollideHost for Detectors<'_> {
    fn chan_enter(&mut self, sp: u32, chan: u32, sample: u32, name: u32) {
        self.host.call_at(sp, 0x528880, chan, &[sample, name, 0]);
    }

    fn prof_exit(&mut self, sp: u32, chan: u32) {
        self.host.call_at(sp, 0x528690, chan, &[]);
    }

    fn run_collision_detectors(&mut self, sp: u32, physics: u32, word: u32) {
        self.host.call_at(sp, 0x526a70, physics, &[word]);
    }

    fn telemetry_70(&mut self, sp: u32, receiver: u32, events: u32) {
        self.host.call_at(sp, 0x499e70, receiver, &[events]);
    }

    fn telemetry_90(&mut self, sp: u32, receiver: u32) {
        self.host.call_at(sp, 0x499e90, receiver, &[]);
    }

    fn solver_sweep_events(&mut self, sp: u32, solver: u32) {
        self.host.call_at(sp, 0x4fbc20, solver, &[]);
    }

    fn solver_fraction(&mut self, sp: u32, solver: u32) -> f64 {
        self.host.call_at(sp, 0x4fc500, solver, &[]).st0
    }

    fn solver_hit_count(&mut self, sp: u32, solver: u32) -> u32 {
        self.host.call_at(sp, 0x4fbca0, solver, &[]).eax
    }

    fn solver_pass(&mut self, sp: u32, solver: u32) {
        self.host.call_at(sp, 0x4fca20, solver, &[]);
    }

    fn solver_post(&mut self, sp: u32, target: u32, solver: u32) {
        self.host.call_at(sp, target, solver, &[]);
    }

    fn advance_contact_bodies(&mut self, sp: u32, physics: u32, fraction: f32) {
        self.host.call_at(sp, 0x525640, physics, &[fraction.to_bits()]);
    }

    fn advance_contact_bodies_remainder(&mut self, sp: u32, physics: u32, window: f32) {
        self.host.call_at(sp, 0x525760, physics, &[window.to_bits()]);
    }

    fn entity_48(&mut self, sp: u32, target: u32, node: u32) {
        self.host.call_at(sp, target, node, &[]);
    }

    fn entity_54(&mut self, sp: u32, target: u32, node: u32) {
        self.host.call_at(sp, target, node, &[]);
    }
}

const MIN_MAX: u32 = 0x4e0ae0;
pub(super) const PENETRATION_CTOR: u32 = 0x5235b0;
const NOTIFY_FILTERED: u32 = 0x4e0080;
pub(super) const VISIT_470: u32 = 0x526390;

/// thiscall `(other)`: `CL2GiganticPenetrationDetector::Vf04`.
pub(super) fn giant_vf04(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Detectors::new(ctx);
    layout::giant_penetration_detect(&mut ctx.mem, &mut calls, r.ecx, r.stack[0], r.esp);
    Ok(Return::void(0, 4))
}

/// The binding of a detector's `Vf04` at `target` that a caller may run in place of its call (it declines nothing
/// and reads only `ecx`, `esp` and its argument): `CL2GiganticPenetrationDetector`'s, `CL2PenetrationDetector`'s and
/// `E46L2Detector`'s, which `CBodyLink::NotifyFiltered` calls on each body of a cell.
pub(crate) fn detector_vf04(target: u32) -> Option<crate::BindingFn> {
    match target {
        0x5242c0 => Some(giant_vf04),
        0x524480 => Some(penetration_vf04),
        0x5257e0 => Some(e46_l2_vf04),
        _ => None,
    }
}

/// thiscall `(other)`: `CL2PenetrationDetector::Vf04`.
pub(super) fn penetration_vf04(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Detectors::new(ctx);
    layout::l2_penetration_detect(&mut ctx.mem, &mut calls, r.ecx, r.stack[0], r.esp);
    Ok(Return::void(0, 4))
}

/// thiscall `(other)`: `E46L2Detector::Vf04`.
pub(super) fn e46_l2_vf04(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Detectors::new(ctx);
    layout::e46_l2_detect(&mut ctx.mem, &mut calls, r.ecx, r.stack[0], r.esp);
    Ok(Return::void(0, 4))
}

/// thiscall `(other)`: `E46L1PenDetector::Vf04`.
pub(super) fn l1_pen_vf04(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Detectors::new(ctx);
    layout::l1_penetration_detect(&mut ctx.mem, &mut calls, r.ecx, r.stack[0], r.esp);
    Ok(Return::void(0, 4))
}

/// stdcall `(a, b, mask, excluded, out)`: `E46_Kernel5bd0`, whether it collected any entity (eax).
pub(super) fn kernel_5bd0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, mask, excluded, out, ..] = r.stack;
    let args = layout::KernelArgs { a, b, mask, excluded, out };
    let mut calls = Detectors { vf_cc: Some(None), ..Detectors::new(ctx) };
    let any = layout::kernel_collect(&mut ctx.mem, &mut calls, args, r.esp);
    Ok(Return::void(any as u32, 0x14))
}

/// thiscall `(other)`: `CL1CollisionDetector::Vf04`.
pub(super) fn l1_collision_vf04(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Detectors::new(ctx);
    layout::l1_collision_detect(&mut ctx.mem, &mut calls, r.ecx, r.stack[0], r.esp);
    Ok(Return::void(0, 4))
}

/// thiscall `()`: `CPhysics::PenCheck`.
pub(super) fn penetration_check(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Detectors::new(ctx);
    layout::penetration_check(&mut ctx.mem, &mut calls, r.ecx, r.esp);
    Ok(Return::void(0, 0))
}

/// thiscall `(window)`: `CPhysics::CollideWork`, the pass count (eax).
pub(super) fn collide_window(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Detectors::new(ctx);
    let window = f32::from_bits(r.stack[0]);
    let passes = layout::collide_window(&mut ctx.mem, &mut calls, r.ecx, window, r.esp);
    Ok(Return::void(passes, 4))
}

/// thiscall `(detector)`: `CPhysics::Visit470`.
pub(super) fn visit_list470(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Detectors::new(ctx);
    layout::visit_list470(&mut ctx.mem, &mut calls, r.ecx, r.stack[0], r.esp);
    Ok(Return::void(0, 4))
}

/// thiscall `(word)`: `CPhysics::Work6a70`.
pub(super) fn run_collision_detectors(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Detectors::new(ctx);
    layout::run_collision_detectors(&mut ctx.mem, &mut calls, r.ecx, r.stack[0], r.esp);
    Ok(Return::void(0, 4))
}

/// The constructor's calls, at their entry stack pointers: `E38RaySeeds` below the saved esi, the copies below the
/// pushed source too.
struct Penetration<'a> {
    host: &'a dyn Host,
    entry: u32,
}

impl PenetrationHost for Penetration<'_> {
    fn ray_seeds(&mut self, _mem: &mut Mem, seeds: u32) {
        self.host.call_at(self.entry - 8, RAY_SEEDS, seeds, &[]);
    }

    fn copy_vec4(&mut self, _mem: &mut Mem, to: u32, from: u32) {
        self.host.call_at(self.entry - 0xc, E01_VEC4_COPY_FROM, to, &[from]);
    }
}

/// thiscall `(owner, lo, hi, scale, word)`: `CL2PenetrationDetector::Ctor`, eax the detector.
pub(super) fn penetration_ctor(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [owner, lo, hi, scale, word, ..] = r.stack;
    let mut calls = Penetration { host: ctx.host, entry: r.esp };
    let eax = detect::l2_penetration_ctor(&mut ctx.mem, &mut calls, r.ecx, owner, lo, hi, f32::from_bits(scale), word);
    Ok(Return::void(eax, 0x14))
}
