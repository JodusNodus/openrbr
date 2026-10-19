//! The contact solvers (`CScratch::SolveA` 0x533ae0, `SolveB` 0x533bf0, `SolveC` 0x533d40, `SolveD` 0x533e80)
//! and their dispatcher `SolveDispatch` (0x5351a0). The solvers differ in the pre-scale, the flag polarity and
//! the terminal stores. A solver either ends or recurses through `solve_dispatch` while `idx <= 3` (the
//! solvers C and D use `idx < 4`, the same thing for unsigned idx).
//!
//! Operation order and precision follow the asm (rbr-recomp `notes/rust-physics.md` section 2): each x87 step is
//! `round24` on f64, and the SSE steps are f32. `T` is the f32 quotient, `d` the x87 chain `T·(-0.5) - sqrt(...)`.

use crate::fpu::round_x87;
use crate::Variant;

/// The constants the asm loads: the x87 `-0.5`, `0.25` and `-1.0`, and the flag gates `±0.00001`.
const HALF_NEG: f64 = -0.5;
const QUARTER: f64 = 0.25;
const MINUS_ONE: f64 = -1.0;
const GATE: f32 = 0.00001;

/// The values one solve reads and writes: `p` (7 words, `p[5]` and `p[6]` are outputs), `qb` (3 words, `qb[1]`
/// and `qb[2]` are outputs), `r` (8 words, read only), the contact offset `f96` (`CScratch+0x258`) and the two
/// state words the solve ends with (`m_state+0x70` and `+0x74`, meaning not named yet).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub f96: f32,
    pub p: [f32; 7],
    pub qb: [f32; 3],
    pub r: [f32; 8],
    pub state_70: u32,
    pub state_74: u32,
}

/// The `sqrt(T²/4 - (-1/r0)·y)` part shared by the four solvers, with the x87 rounding of each step.
/// `t_sq` is `T·T` (before the `·0.25`), `y` the product term; returns `d = T·(-0.5) - sqrt(...)` as f64.
fn chain_root(t: f32, r0: f32, y: f64, variant: Variant) -> f64 {
    let round24 = |v: f64| round_x87(v, variant);
    let quarter = round24(round24(t as f64 * t as f64) * QUARTER);
    let neg_inv = round24(MINUS_ONE / r0 as f64);
    let prod = round24(neg_inv * y);
    let root = round24(round24(quarter - prod).sqrt());
    round24(round24(t as f64 * HALF_NEG) - root)
}

/// Writes the terminal values of a solve that ends without recursing: `qb[1] = qb[2]`, `p[6] = 0`, the flags.
fn finish_copy(c: &mut Contact, flag_70: u32, flag_74: u32) {
    c.p[6] = 0.0;
    c.qb[1] = c.qb[2];
    c.state_70 = flag_70;
    c.state_74 = flag_74;
}

/// Writes the terminal values of a solve that ends with `qb[1] = (r0·d + r1)·d` (SolveB and SolveD).
fn finish_scaled(c: &mut Contact, d: f32, flag_70: u32, flag_74: u32) {
    let t = (c.r[0] * d + c.r[1]) * d;
    c.qb[1] = t;
    c.state_70 = flag_70;
    c.state_74 = flag_74;
}

/// 0x5351a0 `SolveDispatch`: routes to a solver by the two flags, with `idx = a6 + 1`. The solver is B when
/// both flags are set, A when only `f1`, D when only `f2`, and C when neither.
pub fn solve_dispatch(c: &mut Contact, f1: bool, f2: bool, a6: i32, variant: Variant) {
    let idx = a6.wrapping_add(1);
    match (f1, f2) {
        (true, true) => solve_b(c, idx, variant),
        (true, false) => solve_a(c, idx, variant),
        (false, true) => solve_d(c, idx, variant),
        (false, false) => solve_c(c, idx, variant),
    }
}

/// 0x533ae0 `SolveA`. Ends when `f1 && !f2`; otherwise recurses while `idx <= 3`, and ends otherwise.
pub fn solve_a(c: &mut Contact, idx: i32, variant: Variant) {
    let round24 = |v: f64| round_x87(v, variant);
    let f96 = c.f96;
    let r = c.r;
    let nr2 = 0.0 - r[2];
    let t = (r[1] - nr2) / r[0];
    let v1 = (c.p[0] - f96) * r[2];
    c.p[5] = f96;
    let d = chain_root(t, r[0], v1 as f64, variant);
    let p1 = d as f32;
    c.p[1] = p1;
    let w = nr2 * p1 + v1;
    c.p[6] = round24(round24(c.p[2] as f64 - c.p[3] as f64) - d) as f32;
    c.qb[2] = w;
    let f1 = w > -GATE;
    let f2 = c.p[6] > GATE;
    if f1 && !f2 || (idx as u32) > 3 {
        finish_copy(c, 1, 0);
    } else {
        solve_dispatch(c, f1, f2, idx, variant);
    }
}

/// 0x533bf0 `SolveB`. Ends when `f1 && f2`; otherwise recurses while `idx <= 3`, and ends otherwise.
pub fn solve_b(c: &mut Contact, idx: i32, variant: Variant) {
    let round24 = |v: f64| round_x87(v, variant);
    let f96 = c.f96;
    let r = c.r;
    let nr2 = 0.0 - r[2];
    let nr6 = 0.0 - r[6];
    let t = ((r[1] - nr2) - nr6) / r[0];
    let v0 = c.p[2] - c.p[3];
    let x2 = (c.p[0] - f96) * r[2];
    c.p[5] = f96;
    let inner = round24(round24(v0 as f64 * r[6] as f64) + x2 as f64);
    let d = chain_root(t, r[0], inner, variant);
    let df = d as f32;
    let w0 = v0 - df;
    c.p[1] = df;
    c.p[6] = w0;
    let w = df * nr2 + x2;
    c.qb[2] = w;
    let f1 = w > -GATE;
    let f2 = c.p[6] > -GATE;
    if f1 && f2 || (idx as u32) > 3 {
        finish_scaled(c, df, 1, 1);
    } else {
        solve_dispatch(c, f1, f2, idx, variant);
    }
}

/// 0x533d40 `SolveC`. Ends when `!f1 && !f2`; otherwise recurses while `idx < 4`, and ends otherwise.
pub fn solve_c(c: &mut Contact, idx: i32, variant: Variant) {
    let round24 = |v: f64| round_x87(v, variant);
    let f96 = c.f96;
    let r = c.r;
    let w0 = 1.0 / (1.0 / r[7] + 1.0 / r[2]);
    let v1 = c.p[0] * w0;
    let nw0 = 0.0 - w0;
    let t = (r[1] - nw0) / r[0];
    let d = chain_root(t, r[0], v1 as f64, variant);
    let p1 = d as f32;
    c.p[1] = p1;
    let w = nw0 * p1 + v1;
    c.p[6] = round24(round24(c.p[2] as f64 - c.p[3] as f64) - d) as f32;
    c.qb[2] = w;
    let u = c.p[0] - p1 - w / r[2];
    c.p[5] = u;
    let f1 = u - f96 > GATE;
    let f2 = c.p[6] > GATE;
    if !f1 && !f2 || (idx as u32) >= 4 {
        finish_copy(c, 0, 0);
    } else {
        solve_dispatch(c, f1, f2, idx, variant);
    }
}

/// 0x533e80 `SolveD`. Ends when `!f1 && f2`; otherwise recurses while `idx < 4`, and ends otherwise.
pub fn solve_d(c: &mut Contact, idx: i32, variant: Variant) {
    let round24 = |v: f64| round_x87(v, variant);
    let f96 = c.f96;
    let r = c.r;
    let w0 = 1.0 / (1.0 / r[7] + 1.0 / r[2]);
    let v1 = c.p[0] * w0;
    let nw0 = 0.0 - w0;
    let nr6 = 0.0 - r[6];
    let t = ((r[1] - nw0) - nr6) / r[0];
    let v = c.p[2] - c.p[3];
    let inner = round24(round24(v as f64 * r[6] as f64) + v1 as f64);
    let d = chain_root(t, r[0], inner, variant);
    let df = d as f32;
    let w0x = v - df;
    c.p[1] = df;
    c.p[6] = w0x;
    let w = df * nw0 + v1;
    c.qb[2] = w;
    let u = c.p[0] - df - w / r[2];
    c.p[5] = u;
    let f1 = u - f96 > GATE;
    let f2 = c.p[6] > -GATE;
    if !f1 && f2 || (idx as u32) >= 4 {
        finish_scaled(c, df, 0, 1);
    } else {
        solve_dispatch(c, f1, f2, idx, variant);
    }
}
