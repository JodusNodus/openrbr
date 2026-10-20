//! Bindings of the debug graphs' pushes (`rbr_physics::graph`): thiscall on the graph, the samples' words on the stack.

use rbr_physics::graph::layout;

use crate::{Binding, Ctx, Decline, Regs, Return};

/// A period of 0: the original's `div` faults.
const ZERO_PERIOD: Decline = Decline::Unsupported("the graph's period is 0 (the original's div faults)");

/// thiscall `(value)`, `ret 4`: `CGraph1D::Push` (0x494cf0).
fn push_1d(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = layout::push_1d(&mut ctx.mem, r.ecx, r.stack[0]).ok_or(ZERO_PERIOD)?;
    Ok(Return::void(eax, 4))
}

/// thiscall `(x, y)`, `ret 8`: `CGraph2D::Push` (0x495090).
fn push_2d(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = layout::push_2d(&mut ctx.mem, r.ecx, r.stack[0], r.stack[1]).ok_or(ZERO_PERIOD)?;
    Ok(Return::void(eax, 8))
}

pub static BINDINGS: [Binding; 2] = [
    Binding { addr: 0x494cf0, name: "CGraph1D::Push", subsystem: "graph", run: push_1d },
    Binding { addr: 0x495090, name: "CGraph2D::Push", subsystem: "graph", run: push_2d },
];
