//! 0x5358f0 `CScratch::ContactPatch` (thiscall, `ret 4`): the contact's sector and geometry, then the solve. The one
//! stack word is `out`, a float pointer; the port returns the value the asm stores there (`Step` never reads
//! that slot, so the caller does not need the address). It returns `Some` when the patch is solved and `None` when
//! the early test `1e-4 > Q2` fails (the writes before that test stay).
//!
//! The asm's frame holds the working values: the eight `r` words that `SolveDispatch` reads, and the x87 chain of
//! the final value. Names are ours. The x87 steps are `round24` on f64 and the SSE steps are f32. Every branch follows
//! the asm's `ja`/`jbe` order, so a NaN takes the same path. The sector is `trunc_i32` (`_ftol2` in the asm).

use super::layout::{apply_contact, contact_input, Frame};
use super::solve::solve_dispatch;
use crate::fpu::{round_x87, trunc_i32};

/// The constants the asm loads: `2π` and `4/π` (the sector), `1e-4` (the gate on `Q2` and `|T|`), `1e-10` (on `G`),
/// `5e6` and `1e7` (the ratios), and `0.25`, `-0.5`, `-1`, `1` and `2` (the x87 and SSE chains).
const TWO_PI: f32 = 62.83185577392578;
const FOUR_OVER_PI: f32 = 1.2732394933700562;
const GATE: f32 = 9.999999747378752e-05;
const GATE_G: f32 = 1.000000013351432e-10;
const RATIO_MAX: f32 = 5000000.0;
const RATIO_FALLBACK: f32 = 10000000.0;
const QUARTER: f64 = 0.25;
const HALF_NEG: f64 = -0.5;
const MINUS_ONE: f64 = -1.0;
const ONE: f32 = 1.0;
const TWO: f32 = 2.0;

/// 0x5358f0 `CScratch::ContactPatch` on the frame. Returns the float the asm writes to `out` when the patch is solved
/// (`eax` = 1), and `None` when it is not (`eax` = 0). The writes before the early exit stay in the frame.
pub fn contact_patch(f: &mut Frame) -> Option<f32> {
    let round24 = round_x87;
    // The sector: `trunc((a + 2π) · 4/π) & 7`, where `a` is the word at `+0x5ac` of the record at `this+0x410`.
    let angle = round24(round24(f64::from(f.scratch.contact_angle) + f64::from(TWO_PI)) * f64::from(FOUR_OVER_PI));
    let idx = (trunc_i32(angle) & 7) as u32;
    f.scratch.tyre_index = idx;
    f.state.tyre_index = idx;
    f.scratch.tyre_index_copy = idx;

    // `(1 - table[idx]) · d8c`, where the table is the wear table at `this+0x414`.
    let table = f.scratch.wear[idx as usize];
    let p3bc = (ONE - table) * f.setup.f8c;
    f.scratch.p3bc = p3bc;

    // `this+0x300` is `sqrt(A/B)` with `A = s40²` and `B = dec²` when `B > A`, and 1 otherwise.
    let a1 = f.state.k40 * f.state.k40;
    let b1 = f.setup.fec * f.setup.fec;
    let r300 = if b1 > a1 { round24(round24(f64::from(a1) / f64::from(b1)).sqrt()) as f32 } else { ONE };
    f.scratch.scale_300 = r300;

    // `this+0x304` is `a3a8 / T` while `|T| > 1e-4`, and 1e7 otherwise.
    let a3a8 = f.scratch.arg_c;
    let t = f.scratch.f3ac;
    let abs_t = if t > 0.0 { t } else { 0.0 - t };
    let r304 = if abs_t > GATE { a3a8 / t } else { RATIO_FALLBACK };
    f.scratch.r304 = r304;

    // `Q1 = P3 + d7c` and `Q2 = Q1 - a3a8`. The early exit is `1e-4 > Q2`.
    let d7c = f.setup.f7c;
    let q1 = p3bc + d7c;
    let q2 = q1 - a3a8;
    if GATE > q2 {
        return None;
    }

    // `s8 = sqrt(((2·Q1) - Q2) · Q2)` (x87), and the two values scaled from it.
    let s8 =
        round24(round24(round24(round24(f64::from(q1) + f64::from(q1)) - f64::from(q2)) * f64::from(q2)).sqrt()) as f32;
    f.scratch.tyre_factor = s8 + s8;
    let dcc = f.setup.fcc;
    let y1 = s8 * dcc * TWO;
    let y2 = f.setup.fbc * y1;
    let y3 = y1 - y2;
    f.scratch.f308 = y2;

    // The sector test: `G > 1e-10`, `H > 0` and `5e6 > y1 / G`. It picks the solve path.
    let g = f.scratch.g_228;
    let h = f.scratch.f96;
    let solve_path = g > GATE_G && h > 0.0 && RATIO_MAX > y1 / g;

    // The chain of the solve's inputs: `inv = 1/s8`, the `n` terms, and the three that `SolveDispatch` reads.
    let z5 = f.setup.f2c * f.state.s68;
    let inv = ONE / s8;
    let r1 = inv * q2;
    let r2 = r1 * a3a8;
    let n1 = r2 - s8;
    let n2 = n1 / (q2 * q2);
    let n5 = r2 - n2 * q2 * TWO;
    let n11 = n5 * z5 * dcc * TWO;
    let n12 = n11 + f.state.s50;
    let n10 = n2 * z5 * dcc * TWO;
    let d9c = f.setup.f9c;
    let n15 = f.state.s54 * y2 / (d9c + p3bc);

    if !solve_path {
        // The sector's final value: `V10 = (N10 · V7 + N12) · V7` with `V7 = (-0.5 · u2) - sqrt(V1 - V2·V3)` (x87).
        let u1 = round24(f64::from(n12) + f64::from(n15));
        let u2 = round24(u1 / f64::from(n10));
        let u3 = round24(HALF_NEG * u2);
        let v1 = round24(round24(u2 * u2) * QUARTER);
        let v2 = round24(f64::from(n15) * f64::from(q2));
        let v3 = round24(MINUS_ONE / f64::from(n10));
        let v5 = round24(v1 - round24(v2 * v3));
        let v7 = round24(u3 - round24(v5.sqrt()));
        let v10 = round24(round24(round24(f64::from(n10) * v7) + f64::from(n12)) * v7) as f32;
        f.state.contact_70 = 1;
        f.state.contact_74 = 0;
        f.scratch.tyre_factor_2 = v10;
        f.scratch.f318 = v10;
        f.scratch.f31c = v10;
        f.scratch.contact_p[1] = v7 as f32;
        f.scratch.contact_p[5] = 0.0;
        f.scratch.contact_p[6] = 0.0;
        return Some(v10);
    }

    // The solve: the four `p` words set here, then `SolveDispatch` on `p`, `qb` and `r`. `r` is the frame's
    // `[n10, n12, n15, g, y2, y3, t2, t1]` with `t = 1/g · (y2 | y3)`.
    f.scratch.contact_p[0] = q2;
    f.scratch.contact_p[2] = d7c;
    f.scratch.contact_p[3] = a3a8;
    f.scratch.contact_p[4] = p3bc;
    let inv_g = ONE / g;
    let r = [n10, n12, n15, g, y2, y3, inv_g * y3, inv_g * y2];
    let mut contact = contact_input(f, r);
    let (f1, f2) = (contact.state_70 != 0, contact.state_74 != 0);
    solve_dispatch(&mut contact, f1, f2, 0);
    apply_contact(f, &contact);

    // The two outputs: `L1 = K208·(1-t) + J·t` and `L2 = K218·(1-t) + K1f8·t`, with `t = p[5] / H`.
    let t = f.scratch.contact_p[5] / f.scratch.f96;
    let one_minus = ONE - t;
    let l1 = f.scratch.f_208 * one_minus + f.scratch.f_1e8 * t;
    let l2 = f.scratch.f_218 * one_minus + f.scratch.factor_1f8 * t;
    f.scratch.dirty_1e4 = 1;
    f.scratch.f_1e8 = l1;
    f.scratch.flag_1f4 = 1;
    f.scratch.factor_1f8 = l2;
    let qb1 = f.scratch.f318;
    f.scratch.tyre_factor_2 = qb1;
    Some(qb1)
}
