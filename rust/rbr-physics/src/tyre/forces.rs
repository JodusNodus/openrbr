//! `CScratch::SolveForces` (0x532b60): the contact forces of one tyre. It forms the two grip terms from the state's
//! factors (`k40`, `k44`, `k4c`), clamps them through the sign tests of `+0x368`, `+0x36c` and `+0x37c`, and adds the
//! scaled contact vectors (`+0x170`, `+0x180`, `+0x190`) into the two output vectors (`arg1`, `arg2`). The friction
//! term (`Friction`, x87) is added last. The sign and clamp tests follow the asm (`x > 0` and `0 > x`, so a NaN
//! takes the zero branch); the x87 steps round each op to 24 bits.

use super::friction::friction;
use super::layout::{friction_input, scaled_sum, Frame};
use crate::fpu::round_x87;
use crate::math::vec::Vec4;
use crate::math::{abs_asm, sign};
use crate::Variant;

/// The asm's constants: 0.6 (0x72f684), -5 (0x73148c) and 10 (0x72b154).
const K_CLAMP: f32 = 0.6;
const K_SCALE_A: f32 = -5.0;
const K_SCALE_C: f32 = 10.0;

/// `SolveForces` on the frame. `k` is the friction's stack word, and `out2` is the second output vector as the caller
/// holds it (the asm adds into it). Returns the two output vectors (`arg1`, `arg2`).
pub fn solve_forces(f: &mut Frame, k: f32, out2: Vec4, variant: Variant) -> (Vec4, Vec4) {
    let round24 = |v: f64| round_x87(v, variant);
    let (k40, k44, k4c) = (f.state.k40, f.state.k44, f.state.radius);
    let (gate, d3dc, x) = (f.scratch.gate, f.scratch.f3dc, f.scratch.tyre_factor_2);
    let rows = f.scratch.contact_frame;
    let mut a3 = 0.0 - k40 * f.scratch.f394 * f.scratch.scale_3b0;
    let mut a5 = 0.0 - k44 * f.scratch.f398;
    let mut a6 = a5 / k4c;
    // The clamp block runs unless `gate == 0` (the asm's `jnp` skips it only for an equal, ordered zero).
    if gate != 0.0 {
        if sign(a3) == sign(f.scratch.slip_v[0]) {
            let a = abs_asm(f.scratch.slip_ratio_term);
            let v = a * a * K_SCALE_A + 1.0;
            let v = if K_CLAMP > v {
                K_CLAMP
            } else if v > 1.0 {
                1.0
            } else {
                v
            };
            a3 *= v * gate + d3dc;
        }
        if sign(a5) != sign(f.scratch.slip_ratio_term) {
            let a = abs_asm(f.scratch.slip_ratio_term);
            let d = -1.0 - a * K_SCALE_C;
            let r = 1.0 / d;
            let y = 1.0 - (r + 1.0);
            let z = y * gate + d3dc;
            a6 *= z;
            a5 *= z;
        }
        f.scratch.f3b8 *= d3dc;
    }
    // The tail: `+0x170` scaled by `a3` into `out1`, then the scaled vectors added in the asm's order.
    let mut out1 = rows[0].scale(a3);
    out1 = scaled_sum(rows[1], a6, out1);
    f.scratch.f3a0 = a6;
    f.scratch.f39c = a3;
    f.scratch.f3a4 = a5;
    out1 = scaled_sum(rows[2], x, out1);
    let out2 = scaled_sum(rows[0], f.scratch.f3d0 + a5, out2);
    let g = (0.0 - k4c * f.scratch.scale_3b0 * a3) + f.scratch.f3ac * k4c * x;
    let out2 = scaled_sum(rows[1], g, out2);

    // The friction term: `h` is the x87 difference of the two products, and `Friction` adds to it.
    let sgn = sign(f.scratch.slip_v[1]) as f64;
    let b = round24(round24(round24(sgn * f.scratch.f3b8 as f64) * a3 as f64));
    let c = round24(round24(d3dc as f64 * k40 as f64) * a6 as f64);
    let h = round24(b - c) as f32;
    let fr = friction(friction_input(f, k), variant);
    f.state.sv = fr.sv;
    let out2 = scaled_sum(rows[2], round24(fr.value + h as f64) as f32, out2);
    (out1, out2)
}
