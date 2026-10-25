//! The grid edge sweeps: `EdgeEdge`, its pair and walk, the edge normal and the gap store.

use rbr_physics::collision::grid_edge;
use rbr_physics::collision::layout::{self};

use crate::{Ctx, Decline, Regs, Return};

/// thiscall `(start, end, point, distance, normal, word)`: sweep the segment through the grid's triangle faces.
pub(super) fn edge_edge(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [start_at, end_at, point_at, distance_at, normal_at, word_at, ..] = r.stack;
    let hit = grid_edge::edge_edge(&mut ctx.mem, r.ecx, [start_at, end_at, point_at, distance_at, normal_at, word_at]);
    Ok(Return::void(u32::from(hit), 0x18))
}

/// thiscall on the segment's grid (`ecx`) with `(p2, p3, p4)`, ret 0xc: `E37VecSet::Store5570` (0x4f5570), the gap in
/// st0.
pub(super) fn vec_set_gap(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [p2, p3, p4, ..] = r.stack;
    let gap = layout::vec_set_gap(&ctx.mem, r.ecx, p2, p3, p4);
    Ok(Return::float(gap, 0xc))
}

/// thiscall on the grid with `(idx, p3, p4, out)`, ret 0x10: `E37GridOps::M74A0` (0x4f74a0), the row's edge normal to
/// `out`; st0 is 1.0.
pub(super) fn edge_normal(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [index, p3, p4, out, ..] = r.stack;
    let one = layout::edge_normal(&mut ctx.mem, r.ecx, index, p3, p4, out);
    Ok(Return::float(f64::from(one), 0x10))
}

/// thiscall on the grid with `(idx, p3, p4, p5, p6, p7, p8, p9)`, ret 0x20: `E37GridOps::Walk5FF0` (0x4f5ff0), t in
/// st0.
pub(super) fn walk_5ff0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [index, p3, p4, p5, p6, p7, p8, p9, ..] = r.stack;
    let t = layout::walk_5ff0(&mut ctx.mem, r.ecx, index, [p3, p4, p5, p6, p7, p8, p9]);
    Ok(Return::float(f64::from(t), 0x20))
}

/// thiscall on the grid with `(p2, p3, p4, p5, hit, t, normal, scratch, grid, 0)`, ret 0x28: `E37GridOps::EdgePair`
/// (0x4f8560), eax 1 on a hit.
pub(super) fn edge_pair(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let args: [u32; 10] = std::array::from_fn(|i| r.stack[i]);
    let eax = layout::edge_pair(&mut ctx.mem, r.ecx, args);
    Ok(Return::void(eax, 0x28))
}
