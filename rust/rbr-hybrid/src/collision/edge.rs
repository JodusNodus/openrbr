//! The grid edge sweeps: `EdgeEdge`, its pair and walk, the edge normal and the gap store.

use rbr_physics::collision::edge_edge::{self, GridData as EdgeGridData};
use rbr_physics::collision::layout::{self};
use rbr_physics::math::vec::Vec3;
use rbr_physics::mem::Mem;

use crate::{Ctx, Decline, Regs, Return};

/// The memory reads EdgeEdge makes from its collision grid. The port asks for each field at the original branch point.
struct EdgeGrid {
    mem: Mem,
    grid: u32,
}

impl EdgeGridData for EdgeGrid {
    fn bounds(&mut self) -> edge_edge::Bounds {
        layout::edge_edge_bounds(&self.mem, self.grid)
    }

    fn face_count(&mut self) -> u32 {
        layout::edge_edge_face_count(&self.mem, self.grid)
    }

    fn face_is_valid(&mut self, index: u32) -> bool {
        layout::edge_edge_face_is_valid(&self.mem, self.grid, index)
    }

    fn face(&mut self, index: u32) -> edge_edge::Face {
        layout::edge_edge_face(&self.mem, self.grid, index)
    }

    fn sample_indices(&mut self, index: u32) -> [u8; 3] {
        layout::edge_edge_sample_indices(&self.mem, self.grid, index)
    }

    fn vertices(&mut self, indices: [u8; 3]) -> [Vec3; 3] {
        layout::edge_edge_vertices(&self.mem, self.grid, indices)
    }

    fn hit_word(&mut self) -> u32 {
        layout::edge_edge_hit_word(&self.mem, self.grid)
    }

    fn sample_dimensions(&mut self) -> Option<(u16, u16)> {
        layout::edge_edge_sample_dimensions(&self.mem, self.grid)
    }

    fn vertex_samples(&mut self, indices: [u8; 3]) -> [[u8; 2]; 3] {
        layout::edge_edge_vertex_samples(&self.mem, self.grid, indices)
    }

    fn sample_bytes(&mut self, first: i32, second: i32) -> [u8; 2] {
        layout::edge_edge_sample_bytes(&self.mem, self.grid, first, second)
    }
}

/// thiscall `(start, end, point, distance, normal, word)`: sweep the segment through the grid's triangle faces.
pub(super) fn edge_edge(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [start_at, end_at, point_at, distance_at, normal_at, word_at, ..] = r.stack;
    let start = layout::Vec4::load(&ctx.mem, start_at);
    let end = Vec3::load(&ctx.mem, end_at);
    let mut grid = EdgeGrid { mem: ctx.mem.clone(), grid: r.ecx };
    let hit = edge_edge::edge_edge(&mut grid, start, end);
    if let Some(hit) = hit {
        layout::store_edge_edge_hit(
            &mut ctx.mem,
            r.ecx,
            start_at,
            end_at,
            point_at,
            distance_at,
            normal_at,
            word_at,
            hit,
        );
    }
    Ok(Return::void(hit.is_some() as u32, 0x18))
}

/// thiscall on the segment's grid (`ecx`) with `(p2, p3, p4)`, ret 0xc: `E37VecSet::Store5570` (0x4f5570), the gap in
/// st0.
pub(super) fn vec_set_gap(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [p2, p3, p4, ..] = r.stack;
    let gap = layout::vec_set_gap(&ctx.mem, r.ecx, p2, p3, p4, crate::ngp::variant());
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
    let t = layout::walk_5ff0(&mut ctx.mem, r.ecx, index, [p3, p4, p5, p6, p7, p8, p9], crate::ngp::variant());
    Ok(Return::float(f64::from(t), 0x20))
}

/// thiscall on the grid with `(p2, p3, p4, p5, hit, t, normal, scratch, grid, 0)`, ret 0x28: `E37GridOps::EdgePair`
/// (0x4f8560), eax 1 on a hit.
pub(super) fn edge_pair(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let args: [u32; 10] = std::array::from_fn(|i| r.stack[i]);
    let eax = layout::edge_pair(&mut ctx.mem, r.ecx, args, crate::ngp::variant());
    Ok(Return::void(eax, 0x28))
}
