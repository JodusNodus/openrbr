//! `CBody`'s own slots and links: the grid, the corners (`VfCC`), `Vf40`, the bounds and the filtered notify.

use rbr_physics::collision::body::{self, ClearGridHost, Vf40Host, VfCcHost};
use rbr_physics::collision::layout::{self, Body, Vec4};
use rbr_physics::collision::links::{self, NotifyFilteredHost};
use rbr_physics::collision::state::StateHost;
use rbr_physics::mem::Mem;

use super::{Callees, CSTATE_UNKA44, E01_VEC4_COPY_FROM, E37_WALL_SYNC, FETCH_X, NAME_WALK};
use crate::{Ctx, Decline, Host, Regs, Return};

/// The original callees of `CBody::CState::Unk44`: the owner's slot 12 (thiscall, `+0x30` of its vtable), and the wall
/// sync (E37, `CColGrid::SyncGrid`). `mem` reads the owner's vtable.
struct StateCallees<'a> {
    host: &'a dyn Host,
    mem: Mem,
}

impl StateHost for StateCallees<'_> {
    fn owner_vf30(&mut self, owner: u32) -> u32 {
        let slot = self.mem.u32(self.mem.u32(owner) + 0x30);
        self.host.call(slot, owner, &[]).eax
    }

    fn wall_sync(&mut self, wall: u32, body: u32, vec_a: u32, vec_b: u32) -> u32 {
        self.host.call(E37_WALL_SYNC, wall, &[body, vec_a, vec_b]).eax
    }
}

/// thiscall on the state (`ecx`), no arguments: `CBody::CState::Unk44` (0x4e33e0), the wall reset of its two vectors.
/// Eax is the owner's slot-12 result, or the wall sync's when the owner has a wall.
pub(super) fn state_unka44(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut host = StateCallees { host: ctx.host, mem: ctx.mem.clone() };
    let eax = layout::state_unka44(&mut host, &mut ctx.mem, r.ecx);
    Ok(Return::void(eax, 0))
}

/// CBodyLink::Unlink (0x4e0020): thiscall on the link; eax is the pointer the original read last (see
/// `layout::unlink`).
pub(super) fn body_link_unlink(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::unlink(&mut ctx.mem, r.ecx), 0))
}

/// cdecl `(link, cell)`: a body's cell link moved to `cell` (`layout::move_to_cell`); eax is the cell.
pub(super) fn move_to_cell(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [link, cell, ..] = r.stack;
    layout::move_to_cell(&mut ctx.mem, link, cell);
    Ok(Return::void(cell, 0))
}

pub(super) fn lookup_twin(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [x, y, ..] = r.stack;
    let link = layout::lookup_twin(&mut Callees(ctx.host), &mut ctx.mem, r.ecx, f32::from_bits(x), f32::from_bits(y));
    Ok(Return::void(link, 8))
}

/// CBody::Vf40 (0x4e1120): thiscall on the body with no arguments; eax is the base walk's result.
pub(super) fn body_vf40(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::reset_state_and_base(&mut Vf40Callees { host: ctx.host, sp: r.esp }, r.ecx), 0))
}

/// CBody::ClearGrid (0x4e11a0): thiscall on the body with the source and two bound vectors; eax is the grid's wall
/// sync result, or `u32::MAX` when no grid is attached.
pub(super) fn body_clear_grid(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [source, lower, upper, ..] = r.stack;
    let flag = layout::clear_grid(&mut BodyCallees(ctx.host), &ctx.mem, r.ecx, source, lower, upper);
    Ok(Return::void(flag, 12))
}

/// CBody::Vf118 (0x4e11f0): thiscall on the body; eax is its grid pointer (+0x64).
pub(super) fn body_grid(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(Body::new(r.ecx).grid(&ctx.mem), 0))
}

/// CBody::VfCC (0x4e1230): thiscall on the body with two corner matrices; eax is the flag. The corners go to the
/// frame below the call (as `collect`), where the original's own frame would hold them.
pub(super) fn body_vf_cc(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [first, second, ..] = r.stack;
    let flag = layout::aabb_pretest(&mut BodyCallees(ctx.host), &mut ctx.mem, r.ecx, first, second, r.esp - 0x800);
    Ok(Return::void(flag, 8))
}

/// The original callees of CBody's own methods, run in the emulator: thiscall on the state and on the body, and
/// `E33Mat34::FetchX` on a corner matrix.
pub(crate) struct BodyCallees<'a>(pub(crate) &'a dyn Host);

impl ClearGridHost for BodyCallees<'_> {
    fn sync_grid(&mut self, grid: u32, source: u32, lower: u32, upper: u32) -> u32 {
        self.0.call(E37_WALL_SYNC, grid, &[source, lower, upper]).eax
    }
}

impl VfCcHost for BodyCallees<'_> {
    fn fetch_x(&mut self, matrix: u32, out: u32, worker: u32) -> u32 {
        self.0.call(FETCH_X, matrix, &[out, worker]).eax
    }
}

/// `CBody::VfCC` (thiscall on `body`, `(first, second)`) entered at `sp`, run here (the probes call it on each body
/// in their cells): the call's words written as the call writes them ([`crate::skip_call`]), then [`body_vf_cc`]'s
/// port, its `FetchX` calls placed where the pure host places a call below the entry and done here when they can
/// ([`crate::entities::fetch_x_direct`]). Returns the flag (eax); `None` (nothing written) on a host that runs the
/// original.
///
/// `last` is the call before it in the same probe kernel, whose corners are the same: while nothing has run since
/// ([`Host::entered`]) and its fetches were direct, this call's fetches would write the words they wrote, so only the
/// call's own words are written and its corners tested.
pub(crate) fn body_vf_cc_direct(
    host: &dyn Host,
    mem: &mut Mem,
    sp: u32,
    body: u32,
    first: u32,
    second: u32,
    last: &mut Option<VfCcCorners>,
) -> Option<u32> {
    if !crate::skip_call(host, mem, sp, &[first, second]) {
        return None;
    }
    let key = (sp, first, second, host.entered());
    if let Some(VfCcCorners { corners: [lo, hi], .. }) = last.filter(|c| c.key == key) {
        return Some(u32::from(body::aabb_pretest(Body::new(body).bounds(mem), lo, hi)));
    }
    let scratch = sp - 0x800;
    let mut calls = DirectVfCcCalls { mem: mem.clone(), host, esp: sp, direct: true };
    let flag = layout::aabb_pretest(&mut calls, mem, body, first, second, scratch);
    *last = calls.direct.then(|| VfCcCorners {
        key: (sp, first, second, host.entered()),
        corners: [Vec4::load(mem, scratch), Vec4::load(mem, scratch + 16)],
    });
    Some(flag)
}

/// The corners a direct `CBody::VfCC` fetched ([`body_vf_cc_direct`]), keyed by its entry sp, its two corner
/// arguments and the host's runs entered after it.
#[derive(Clone, Copy)]
pub(crate) struct VfCcCorners {
    key: (u32, u32, u32, u64),
    corners: [Vec4; 2],
}

impl VfCcCorners {
    /// The corners, for a call at `sp` with the corner arguments `first` and `second` while the host has entered
    /// `entered` runs: what that call's fetches would give.
    pub(crate) fn for_call(&self, sp: u32, first: u32, second: u32, entered: u64) -> Option<[Vec4; 2]> {
        (self.key == (sp, first, second, entered)).then_some(self.corners)
    }
}

/// [`body_vf_cc_direct`]'s `FetchX` calls, at the pure host's placement below the entry `esp`; `direct` until one is
/// run by the host.
struct DirectVfCcCalls<'a> {
    mem: Mem,
    host: &'a dyn Host,
    esp: u32,
    direct: bool,
}

impl VfCcHost for DirectVfCcCalls<'_> {
    fn fetch_x(&mut self, matrix: u32, out: u32, worker: u32) -> u32 {
        let sp = (self.esp - 0x1000) & !0xf;
        match crate::entities::fetch_x_direct(self.host, &mut self.mem, sp, matrix, out, worker) {
            Some(eax) => eax,
            None => {
                self.direct = false;
                self.host.call_at(sp, FETCH_X, matrix, &[out, worker]).eax
            }
        }
    }
}

/// `CBody::Vf40`'s two calls at the original's stack pointers, from its entry `sp`: the state's slot 0x44 below the
/// return address and the saved esi, then the tail jump to `CEntity::Vf40` at the entry itself.
pub(crate) struct Vf40Callees<'a> {
    pub(crate) host: &'a dyn Host,
    pub(crate) sp: u32,
}

impl Vf40Host for Vf40Callees<'_> {
    fn state_unka44(&mut self, state: u32) -> u32 {
        self.host.call_at(self.sp - 8, CSTATE_UNKA44, state, &[]).eax
    }

    fn entity_vf40(&mut self, body: u32) -> u32 {
        self.host.call_at(self.sp, NAME_WALK, body, &[]).eax
    }
}

/// The visitor's slot 1 (`Vf04`, thiscall with the body) that `CBodyLink::NotifyFiltered` calls, at its entry stack
/// pointer: below the return address, the saved esi and edi and the pushed body.
struct Notify<'a> {
    host: &'a dyn Host,
    sp: u32,
}

impl NotifyFilteredHost for Notify<'_> {
    fn vf04(&mut self, mem: &mut Mem, visitor: u32, body: u32) -> u32 {
        let target = mem.u32(mem.u32(visitor) + 4);
        // A detector's is run here on the pure host, as the host would run it: the call's words, then the binding
        // entered at `sp` (the calls it makes are all placed from its entry).
        if let Some(run) = crate::physics_core::detector_vf04(target) {
            if crate::skip_call(self.host, mem, self.sp, &[body]) {
                self.host.ran_direct();
                let mut stack = [0; crate::STACK_ARGS];
                stack[0] = body;
                let regs = crate::Regs { ecx: visitor, esp: self.sp, stack, ..crate::Regs::default() };
                let mut ctx = Ctx { mem: mem.clone(), host: self.host };
                return run(&mut ctx, &regs).map_or_else(|_| unreachable!("a detector declines nothing"), |r| r.eax);
            }
        }
        self.host.call_at(self.sp, target, visitor, &[body]).eax
    }
}

/// CBodyLink::NotifyFiltered (0x4e0080): thiscall on the list head with the visitor, ret 4. Its callers (the bound
/// detectors) do not read eax.
pub(super) fn notify_filtered(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Notify { host: ctx.host, sp: r.esp - 0x10 };
    links::notify_filtered(&mut ctx.mem, r.ecx, r.stack[0], &mut calls);
    Ok(Return::void(0, 4))
}

/// CBody::Vf10C (0x4e11b0) and its twin Vf114 (0x4e11e0): thiscall on the body; eax is 1 when it has a grid.
pub(super) fn body_has_grid(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::body_has_grid(&ctx.mem, r.ecx) as u32, 0))
}

/// CBody::Vf110 (0x4e11c0): thiscall on the body; eax is 1 when its grid is active.
pub(super) fn body_grid_active(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::body_grid_active(&ctx.mem, r.ecx) as u32, 0))
}

pub(crate) const BODY_VF110: u32 = 0x4e11c0;
pub(crate) const BODY_VF11C: u32 = 0x4e1200;

/// `CBody::Vf11C` (`body_vf11c`) on `body` with `(lo, hi)` entered at `sp`, run here on the pure host, its two copies
/// too (the calls' words written as the calls write them); false (nothing written) on a host that runs the original.
pub(crate) fn body_box_direct(host: &dyn Host, mem: &mut Mem, sp: u32, body: u32, lo: u32, hi: u32) -> bool {
    if !crate::skip_call(host, mem, sp, &[lo, hi]) {
        return false;
    }
    let (lower, upper) = Body::new(body).bounds_at();
    for (to, from) in [(lo, lower), (hi, upper)] {
        crate::skip_call(host, mem, sp - 0xc, &[from]);
        layout::vec4_copy_from(mem, to, from);
    }
    true
}

/// CBody::Vf11C (0x4e1200): thiscall on the body with `(lo, hi)`, ret 8: `E01Vec4::CopyFrom` of its lower bound to `lo`
/// and its upper bound to `hi`, each entering below the saved esi and the pushed source; eax is the second copy's.
pub(super) fn body_vf11c(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [lo, hi, ..] = r.stack;
    let (lower, upper) = Body::new(r.ecx).bounds_at();
    ctx.host.call_at(r.esp - 0xc, E01_VEC4_COPY_FROM, lo, &[lower]);
    let eax = ctx.host.call_at(r.esp - 0xc, E01_VEC4_COPY_FROM, hi, &[upper]).eax;
    Ok(Return::void(eax, 8))
}
