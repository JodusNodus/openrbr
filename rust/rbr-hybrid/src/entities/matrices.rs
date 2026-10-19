//! The `E33Mat34` transform fetches and copies (`rbr_physics::entities::transform`, 0x51b460-0x51b780), and their
//! direct runs on the pure host.

use rbr_physics::entities::layout;
use rbr_physics::entities::transform;
use rbr_physics::entities::walk::mb560;
use rbr_physics::mem::Mem;

use super::walks::{PairWalk, MB560_CALL};
use super::{COPY16_WALK, GRID_WALK, MB560};
use crate::{Ctx, Decline, Host, Regs, Return};

/// `E33Mat34::FetchX`'s walk: the original pushes two saved registers, then `1`, `out` and `arg`.
const FETCH_X_CALL: u32 = 24;

/// `E33Mat34::FetchX` (thiscall on `matrix`, `(out, arg)`) entered at `sp`, done here when its walk is of a node and
/// itself, as the probes' identity walks are: the call's words and its walk's are written as the calls write them
/// ([`crate::skip_call`]) and the matrix copied; returns `out` (eax). `None` on a host that runs the original, or when
/// the walk would run (the call's words may then be written: the caller's call writes the same).
pub(crate) fn fetch_x_direct(host: &dyn Host, mem: &mut Mem, sp: u32, matrix: u32, out: u32, arg: u32) -> Option<u32> {
    if !crate::skip_call(host, mem, sp, &[out, arg]) {
        return None;
    }
    let t = layout::load_transform(mem, matrix);
    if t.worker != arg {
        return None;
    }
    layout::copy_mat16(mem, out, t.matrix);
    crate::skip_call(host, mem, sp.wrapping_sub(FETCH_X_CALL), &[arg, out, 1]);
    Some(out)
}

/// thiscall `(out, arg)`: `E33Mat34::FetchX` (0x51b6c0), the copy of the matrix to `out` and the walk of the worker
/// against `out`; returns `out`. The walk's callee enters 24 bytes below the entry esp (the original's pushes).
pub(super) fn fetch_x_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [out, arg, ..] = r.stack;
    let t = layout::load_transform(&ctx.mem, r.ecx);
    let mut pairs = PairWalk { mem: &mut ctx.mem, host: ctx.host, esp: r.esp, walk_depth: FETCH_X_CALL };
    let eax = transform::fetch_x(&mut pairs, t, out, arg);
    Ok(Return::void(eax, 8))
}

/// thiscall `(out, arg)`: `E33Mat34::FetchY` (0x51b780), `fetch_x` on a contact grid: the grid copy to `out` and the
/// grid walk of the worker against `out`; returns `out`. The walk enters where `FetchX`'s does.
pub(super) fn fetch_y_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [out, arg, ..] = r.stack;
    let t = layout::load_transform(&ctx.mem, r.ecx);
    let mut calls = GridFetchAt { mem: ctx.mem.clone(), host: ctx.host, sp: r.esp - FETCH_X_CALL };
    let eax = transform::fetch_y(&mut calls, t, out, arg);
    Ok(Return::void(eax, 8))
}

/// `FetchY`'s grid copy, in Rust, and its walk, entered at `sp`.
struct GridFetchAt<'a> {
    mem: Mem,
    host: &'a dyn Host,
    sp: u32,
}

impl transform::GridFetch for GridFetchAt<'_> {
    fn copy_grid(&mut self, to: u32, from: u32) {
        layout::copy_grid(&mut self.mem, to, from);
    }

    fn grid_walk(&mut self, receiver: u32, o2: u32, a: u32, b: u32) {
        self.host.call_at(self.sp, GRID_WALK, receiver, &[o2, a, b]);
    }
}

/// thiscall `(src, o2, out)`: `E33Mat34::ComposeB` (0x51b600), `copy16_walk_run` with the Five-Six walk. Returns `out`.
pub(super) fn compose_b_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [src, o2, out, ..] = r.stack;
    let t = transform::Transform { matrix: src, worker: r.ecx };
    let mut pairs = PairWalk { mem: &mut ctx.mem, host: ctx.host, esp: r.esp, walk_depth: FETCH_X_CALL };
    let eax = transform::compose_b(&mut pairs, t, out, o2);
    Ok(Return::void(eax, 12))
}

/// The copies' walk enters 24 bytes below the entry esp (the original's pushes for the call: `1`, `dst`, `o2`).
const COPY_BLOCKS_CALL: u32 = 24;

/// thiscall `(src, o2, dst, n)`: `E43PairWalk` (0x51b460), the copy of `n` blocks from `src` to `dst`, then the walk
/// of `receiver` against `o2`. Returns `dst`.
pub(super) fn copy_blocks_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [src, o2, dst, n, ..] = r.stack;
    let mut pairs = PairWalk { mem: &mut ctx.mem, host: ctx.host, esp: r.esp, walk_depth: COPY_BLOCKS_CALL };
    let eax = transform::copy_blocks_then_walk(&mut pairs, r.ecx, o2, src, dst, n);
    Ok(Return::void(eax, 16))
}

/// thiscall `(src, o2, dst, n)`: `E43PairWalk` (0x51b500), `copy_blocks_run`'s twin whose walk is `RoutePairThreeFour`.
pub(super) fn copy_blocks_three_four_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [src, o2, dst, n, ..] = r.stack;
    let mut pairs = PairWalk { mem: &mut ctx.mem, host: ctx.host, esp: r.esp, walk_depth: COPY_BLOCKS_CALL };
    let eax = transform::copy_blocks_then_walk_three_four(&mut pairs, r.ecx, o2, src, dst, n);
    Ok(Return::void(eax, 16))
}

/// `E33Mat34::M51B4C0` or `E43PairWalk::MB560` (`target`, thiscall on `this` with `args`) entered at `sp`, run here
/// on the pure host as their bindings (`copy16_walk_run`, `mb560_run`) would, its walks in place
/// ([`PairWalk::walk`]); false (nothing written) for another target or on a host that runs the original.
pub(crate) fn transform_direct(
    host: &dyn Host,
    mem: &mut Mem,
    sp: u32,
    target: u32,
    this: u32,
    args: [u32; 3],
) -> bool {
    if !matches!(target, COPY16_WALK | MB560) || !crate::skip_call(host, mem, sp, &args) {
        return false;
    }
    let [a, b, c] = args;
    if target == MB560 {
        mb560(&mut PairWalk { mem, host, esp: sp, walk_depth: MB560_CALL }, this, a, b, c);
    } else {
        let t = transform::Transform { matrix: a, worker: this };
        transform::fetch_x(&mut PairWalk { mem, host, esp: sp, walk_depth: FETCH_X_CALL }, t, c, b);
    }
    true
}

/// thiscall `(src, o2, out)`: `E33Mat34` (0x51b4c0), `FetchX` on the transform whose matrix is `src`, with `this` as its
/// worker. Returns `out`.
pub(super) fn copy16_walk_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [src, o2, out, ..] = r.stack;
    let t = transform::Transform { matrix: src, worker: r.ecx };
    let mut pairs = PairWalk { mem: &mut ctx.mem, host: ctx.host, esp: r.esp, walk_depth: FETCH_X_CALL };
    let eax = transform::fetch_x(&mut pairs, t, out, o2);
    Ok(Return::void(eax, 12))
}
