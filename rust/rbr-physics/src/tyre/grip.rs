//! `CScratch::SolvePair` (0x5352a0) and `CScratch::ClampGrip` (0x533760): the grip clamp of the contact pair. `SolvePair`
//! runs the clamp when `this+0x3f0 > 0`, on the words `this+0x314`, `+0x1e8`, `+0x1f8`, `+0x394`, `+0x398`, and writes
//! the out words `this+0x3fc` and `+0x400`. `ClampGrip` reads the state's grip fields (`+0x40`, `+0x44`, `+0x4c` and the flag
//! `+0x5c`); the flag is set to 1 when it clamps and to 0 when it does not.

use crate::fpu::round_x87;
use crate::Variant;

use super::layout::{Frame, StateWords};

/// The five float stack words of `ClampGrip`: `s` (`this+0x314`), `a` (`+0x1e8`, the flag's case), `b` (`+0x1f8`),
/// `c` (`+0x394`) and `d` (`+0x398`).
#[derive(Clone, Copy)]
pub struct GripInput {
    pub s: f32,
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
}

/// What `ClampGrip` leaves: the two out words (raw bits, as the asm copies or multiplies them), the state's flag and
/// the two scratch words `this+0x3f4` and `+0x3f8`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grip {
    pub out_a: u32,
    pub out_b: u32,
    pub flag: u32,
    pub f3f4: f32,
    pub f3f8: f32,
}

/// `ClampGrip` (thiscall, seven stack words) on the state's grip factors. With `x = s · (a or b)` (`a` when the state's
/// flag is set), `u = 0 - k40 · c`, `t = ((-1 / k4c) · k44) · d` and `A = t² + u²`, `B = x²`: when `!(A > B)` the outputs
/// are the state's `k40` and `k44` bits, the flag is 0 and `this+0x3f4` and `+0x3f8` are 0. Otherwise the factor `f` is 0
/// unless `A > 1e-5` and `s > 0.001`; then `f` is `P / Q` with `P = s · b` and `Q = sqrt(A)` (each rounded at the x87
/// precision), capped at 1 (`ja`, so a NaN `f` is kept). The outputs are `k40 · f` and `k44 · f`, the flag is 1,
/// and `this+0x3f4` and `+0x3f8` are 2. A NaN compares as the asm's `jbe` (the `!(a > b)` form).
pub fn clamp_grip(state: &StateWords, g: GripInput, variant: Variant) -> Grip {
    let round24 = |v: f64| round_x87(v, variant);
    let (k40_bits, k44_bits) = (state.k40.to_bits(), state.k44.to_bits());
    let (k40, k44, k4c) = (state.k40, state.k44, state.radius);
    let x = g.s * if state.grip_flag != 0 { g.a } else { g.b };
    let u = 0.0 - k40 * g.c;
    let t = -1.0 / k4c * k44 * g.d;
    let a_sq = t * t + u * u;
    let b_sq = x * x;
    if !(a_sq > b_sq) {
        // The raw bits of the state's k40 and k44 are copied (the asm copies the dwords).
        return Grip { out_a: k40_bits, out_b: k44_bits, flag: 0, f3f4: 0.0, f3f8: 0.0 };
    }
    let f = if !(a_sq > 1e-5) || !(g.s > 0.001) {
        0.0
    } else {
        // `P` and `Q` stay on the x87 stack; the `fstp` rounds the quotient to f32.
        let p = round24(g.s as f64 * g.b as f64);
        let q = round24((a_sq as f64).sqrt());
        let r = round24(p / q) as f32;
        if r > 1.0 {
            1.0
        } else {
            r
        }
    };
    Grip { out_a: (k40 * f).to_bits(), out_b: (k44 * f).to_bits(), flag: 1, f3f4: 2.0, f3f8: 2.0 }
}

/// `SolvePair` on the frame: when `this+0x3f0 > 0`, clamps the grip of the pair with the contact's words and writes the
/// out words `this+0x3fc` and `+0x400`. Otherwise both out words are 0 and the state's flag is cleared (the asm's skip path).
pub fn solve_pair(f: &mut Frame, variant: Variant) {
    if !(f.scratch.gate > 0.0) {
        f.scratch.f3fc = 0.0;
        f.scratch.f400 = 0.0;
        f.state.grip_flag = 0;
        return;
    }
    let s = &f.scratch;
    let g = GripInput { s: s.tyre_factor_2, a: s.f_1e8, b: s.factor_1f8, c: s.f394, d: s.f398 };
    let grip = clamp_grip(&f.state, g, variant);
    f.scratch.f3fc = f32::from_bits(grip.out_a);
    f.scratch.f400 = f32::from_bits(grip.out_b);
    f.state.grip_flag = grip.flag;
    f.scratch.f3f4 = grip.f3f4;
    f.scratch.f3f8 = grip.f3f8;
}
