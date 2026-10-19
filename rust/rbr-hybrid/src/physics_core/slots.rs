//! The small functions: the `CSolid` and `CEntity` slots that forward, the link lists, the material tag, the box
//! cells and edge tests, and the `E46` vector, matrix and force helpers.

use rbr_physics::math::vec::Vec4;
use rbr_physics::mem::Mem;
use rbr_physics::physics_core::detect::{self, EdgeHit, EdgeHost, GridHost};
use rbr_physics::physics_core::math::AccumHost;
use rbr_physics::physics_core::{layout, mounted};

use super::passenger_step::SLOT_24;
use super::{E37_EDGE_EDGE, EMPTY_HOOK, LOOKUP_TWIN, MB560, TEMPS, WALKER};
use crate::entities::VF5C;
use crate::{aligned_frame, Ctx, Decline, Host, Regs, Return};

/// thiscall `()`: `CSolid::E43Vf5C`: `CEntity::E43Vf5C` on the solid, then its slot 24.
pub(crate) fn dispatch_vf5c(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    ctx.host.call(VF5C, r.ecx, &[]);
    let slot = ctx.mem.u32(ctx.mem.u32(r.ecx) + SLOT_24);
    Ok(Return::void(ctx.host.call(slot, r.ecx, &[]).eax, 0))
}

/// thiscall `()`: `CSolid::E43Vf80`, the word at `+0x1dc`.
pub(super) fn vf80(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(ctx.mem.u32(r.ecx + 0x1dc), 0))
}

/// thiscall `()`: `CSolid::E43Vf8C` (slot 35, `S8C`), the word at `+0x1ec`.
pub(super) fn vf8c(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::cache_1ec(&ctx.mem, r.ecx), 0))
}

/// thiscall `()`: `CEntity::E43Vf88` (slot 34), 0.
pub(super) fn entity_vf88(_: &mut Ctx, _: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(0, 0))
}

/// thiscall `()`: `CEntity::Vf00`, the name pointer at +0x3c or the class-name fallback.
pub(super) fn entity_vf00(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::entity_vf00(&ctx.mem, r.ecx), 0))
}

struct EntityVf158<'a> {
    host: &'a dyn Host,
}

impl layout::FilingHost for EntityVf158<'_> {
    fn filing(
        &mut self,
        mem: &Mem,
        object: u32,
    ) -> Result<rbr_physics::physics_core::signal::Filing, rbr_physics::rigid_body::host::HostUnavailable> {
        rbr_physics::rigid_body::layout::solid_filing(mem, object)
    }
}

impl layout::EntityVf158Host for EntityVf158<'_> {
    fn slot158(
        &mut self,
        sp: u32,
        target: u32,
        parent: u32,
    ) -> Result<u32, rbr_physics::rigid_body::host::HostUnavailable> {
        Ok(self.host.call_at(sp, target, parent, &[]).eax)
    }
}

/// thiscall `()`: `CEntity::E43Vf158`, its own WallRel filing or its parent's slot 86.
pub(super) fn entity_vf158(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut host = EntityVf158 { host: ctx.host };
    let eax = layout::entity_vf158(&mut ctx.mem, &mut host, r.ecx, r.esp)
        .map_err(|_| Decline::Unsupported("entity slot-86 filing is not CSolid"))?;
    Ok(Return::void(eax, 0))
}

/// thiscall `()` on an item's link: `E34Link10::UnlinkA` (`layout::unlink_a`).
pub(super) fn unlink_a(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::unlink_a(&mut ctx.mem, r.ecx, r.eax), 0))
}

/// thiscall `(item)` on a list: `E34LinkList10::PushItem` (`layout::push_item`); eax is the item.
pub(super) fn push_item(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let item = r.stack[0];
    layout::push_item(&mut ctx.mem, r.ecx, item);
    Ok(Return::void(item, 4))
}

/// thiscall `(object)` on the state: `CPhysics::CState::RegWall` (`layout::reg_wall`). The object's slots are entered
/// 12 bytes below the entry, as the original's.
pub(super) fn reg_wall(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let object = r.stack[0];
    let host = ctx.host;
    let eax = layout::reg_wall(&mut ctx.mem, r.ecx, object, |mem, slot| {
        let target = mem.u32(mem.u32(object) + slot);
        host.call_at(r.esp - 0xc, target, object, &[]).eax != 0
    });
    Ok(Return::void(eax, 4))
}

/// The stack bytes a binding's temporaries take, below its entry's return slot (the nested calls run 0x1000 below it).
fn temps(r: &Regs) -> u32 {
    (r.esp - TEMPS) & !0xf
}

/// The virtual slot at vtable offset `offset` of the object at `obj`.
pub(super) fn vslot(mem: &Mem, obj: u32, offset: u32) -> u32 {
    mem.u32(mem.u32(obj) + offset)
}

/// thiscall `(tag)` on the manager: `CMaterialManager::Tag520940`, the entry the tag names (eax).
pub(super) fn tag_520940(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = rbr_physics::physics_core::material::tag_entry(&ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(eax, 4))
}

/// fastcall `()` on the mounted body: `E45_M1920`, whether its state is 1 (eax 0 or 1).
pub(super) fn flag_is_one(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(mounted::flag_is_one(&ctx.mem, r.ecx) as u32, 0))
}

/// fastcall `()` on the mounted body: `E45_M1940`, whether its state is 2.
pub(super) fn flag_is_two(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(mounted::flag_is_two(&ctx.mem, r.ecx) as u32, 0))
}

/// The cell lookups of the box corners: `CCellGrid::LookupTwin(x, y)` on the grid (thiscall, two float words).
struct Cells<'a>(&'a dyn Host);

impl GridHost for Cells<'_> {
    fn lookup_twin(&mut self, grid: u32, x: f32, y: f32) -> u32 {
        self.0.call(LOOKUP_TWIN, grid, &[x.to_bits(), y.to_bits()]).eax
    }
}

/// cdecl `(a, b, out)`: `E45_M40C0`, the cells of the box between `a` and `b`.
pub(super) fn m40c0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, out, ..] = r.stack;
    detect::box_cells(&mut ctx.mem, &mut Cells(ctx.host), a, b, out);
    Ok(Return::void(0, 0))
}

/// The edge tests of `E45_M46A0`: `E37GridOps::EdgeEdge` on a pair object (thiscall, six stack pointers). Its outputs go
/// to the temporaries below the entry: the point (16 bytes), the distance (+0x10), the normal (+0x20) and the hit word (+0x30).
struct Edges<'a> {
    mem: Mem,
    host: &'a dyn Host,
    temp: u32,
}

impl EdgeHost for Edges<'_> {
    fn edge_edge(&mut self, pair: u32, a: u32, b: u32) -> Option<EdgeHit> {
        let t = self.temp;
        let args = [a, b, t, t + 0x10, t + 0x20, t + 0x30];
        if self.host.call(E37_EDGE_EDGE, pair, &args).eax == 0 {
            return None;
        }
        Some(EdgeHit {
            point: Vec4::load(&self.mem, t),
            dist: self.mem.f32(t + 0x10),
            normal: Vec4::load(&self.mem, t + 0x20),
            hit_word: self.mem.u32(t + 0x30),
        })
    }
}

/// fastcall `(self, edx, a, b, list, out)`: `E45_M46A0`, the best edge hit's id (eax).
pub(super) fn m46a0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, list, out, ..] = r.stack;
    let mut edges = Edges { mem: ctx.mem.clone(), host: ctx.host, temp: temps(r) };
    let id = detect::e45_m46a0(&mut ctx.mem, &mut edges, a, b, list, out);
    Ok(Return::void(id, 16))
}

/// thiscall `(dst, n)`: `E46Vec::Add`, adds the vector to the `n` rows at `dst` (eax is `dst`).
pub(super) fn vec_add(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [dst, n, ..] = r.stack;
    layout::vec_add(&mut ctx.mem, r.ecx, dst, n);
    Ok(Return::void(dst, 8))
}

/// thiscall `(out, b)`: `E46Mat::Mul`, the rows at `b` times the matrix into `out` (eax is `out`).
pub(super) fn mat_mul(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [out, b, ..] = r.stack;
    layout::mat_mul(&mut ctx.mem, r.ecx, out, b);
    Ok(Return::void(out, 8))
}

/// thiscall `(dst, n)`: `E46Mat::ScaleRows`, scales the `n` rows at `dst` (eax is `dst`).
pub(super) fn scale_rows(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [dst, n, ..] = r.stack;
    layout::mat_scale_rows(&mut ctx.mem, r.ecx, dst, n);
    Ok(Return::void(dst, 8))
}

/// The body's calls of `E46_Accum250`: the hook and the point's `S30` through its vtable, the base and the walk (on the
/// walker, the global at `WALKER`), and the accumulate `S70`.
struct Accum<'a> {
    mem: Mem,
    host: &'a dyn Host,
}

impl AccumHost for Accum<'_> {
    fn hook(&mut self, body: u32) {
        self.host.call(vslot(&self.mem, body, 0), body, &[]);
    }

    fn base(&mut self, body: u32) {
        self.host.call(EMPTY_HOOK, body, &[]);
    }

    fn point(&mut self, body: u32, out: u32) -> u32 {
        self.host.call(vslot(&self.mem, body, 0x30), body, &[out]).eax
    }

    fn walk(&mut self, copy: u32, point: u32, out: u32) {
        let walker = self.mem.u32(WALKER);
        self.host.call(MB560, walker, &[copy, point, out]);
    }

    fn accumulate(&mut self, body: u32, force: u32) {
        self.host.call(vslot(&self.mem, body, 0x70), body, &[force]);
    }
}

/// thiscall `()`: `CSolid::E43Vf60` (`E46_Accum250`). A gravity pointer that is the sentinel declines before any call.
pub(super) fn accumulate_forces(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let sentinel = "the physics object's gravity pointer is the original's sentinel (its read is address 0)";
    let mut calls = Accum { mem: ctx.mem.clone(), host: ctx.host };
    layout::accumulate_forces(&mut ctx.mem, &mut calls, r.ecx, temps(r)).ok_or(Decline::Unsupported(sentinel))?;
    Ok(Return::void(0, 0))
}

/// thiscall `(point*, out*)`, `ret 8`: `E46_Offset540`, the velocity term of a point on the body.
pub(super) fn offset_540(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [point, out, ..] = r.stack;
    layout::offset_540(&mut ctx.mem, r.ecx, point, out);
    // eax: the last cross product's output, the original's local at its aligned frame - 0x20.
    Ok(Return::void(aligned_frame(r.esp) - 0x20, 8))
}
