//! The wheel's axis vectors and their inputs: `M3A9E0` (0x53a9e0, `turn_axis`), `M3AB00` (0x53ab00, `axis_from_angle`),
//! `M3AB90` (0x53ab90, `state_lerp`) and the setter `M3AC50` (0x53ac50, `set_input`). The ports take the values
//! `layout::load_*` read and return the values `layout::store_*` write. Operation order follows the asm (x87 products are
//! f32 products at 24-bit precision).

use super::layout::{AngleIn, AxisPair, InputSet, SetInputIn, StateLerpIn, TurnIn};
use crate::math::trig::{x87_cos, x87_sin};
use crate::math::vec::{Vec3, Vec4};

/// The CRT sine and cosine the axis functions take (`__CIsin` 0x53f3b0, `__CIcos` 0x53f3a0, x87 `fsin`/`fcos`), as the
/// f64 the x87 leaves in st0 (`M3A9E0` multiplies them unrounded). The port answers them with `Crt` (`math::trig`); the
/// tests can answer with original results to retain the hardware's last-bit approximation (`notes/physics.md` "x87 and float rules").
pub trait CrtTrig {
    fn sin(&mut self, angle: f32) -> f64;
    fn cos(&mut self, angle: f32) -> f64;
}

/// The CRT functions as `math::trig` computes them.
pub struct Crt;

impl CrtTrig for Crt {
    fn sin(&mut self, angle: f32) -> f64 {
        x87_sin(angle)
    }

    fn cos(&mut self, angle: f32) -> f64 {
        x87_cos(angle)
    }
}

/// Clamps to [-1, 1] as the asm's `comiss` pairs do: a NaN passes through.
fn clamp_unit(x: f32) -> f32 {
    if x < -1.0 {
        -1.0
    } else if x > 1.0 {
        1.0
    } else {
        x
    }
}

/// `M3A9E0` (0x53a9e0, `turn_axis`). With the tyre block's input `w` nonzero (NaN too), `c = 2w` is clamped to [-1, 1] as
/// `cut` and scaled by 0.07 into `k`. The angle gives `s = sin` and `co = cos`. The axis is the base plus the turn vector
/// scaled by `co*k` (x, z only), with `-(s*k)` as its y, normalised (lane 3 is kept from the old value, then scaled). With
/// `w` zero, the axis is the base.
pub fn turn_axis(trig: &mut impl CrtTrig, input: &TurnIn) -> Vec4 {
    let w = input.input;
    if w == 0.0 {
        return input.base;
    }
    let cut = clamp_unit(w * 2.0);
    // The C++ scales `cut` by 0.07f before both products.
    let k = cut * 0.07;
    // The x87 keeps the sine and cosine unrounded: each product is rounded once, when it is stored (the asm's `fmuls`).
    let s = trig.sin(input.angle);
    let co = trig.cos(input.angle);
    let turn = input.turn.scale((co * f64::from(k)) as f32);
    let out = Vec4 {
        x: input.base.x + turn.x,
        y: 0.0 - ((s * f64::from(k)) as f32),
        z: input.base.z + turn.z,
        w: input.old_w,
    };
    out.normalize()
}

/// `M3AB00` (0x53ab00, `axis_from_angle`). The axis angle is `a = input * scale + offset` (x87: the product, then the sum,
/// each at 24-bit). The base takes `(cos a, 0, sin a)` and the turn `(-sin a, 0, cos a)`.
pub fn axis_from_angle(trig: &mut impl CrtTrig, input: &AngleIn) -> AxisPair {
    let a = input.input * input.scale + input.offset;
    let s = trig.sin(a) as f32;
    let c = trig.cos(a) as f32;
    AxisPair { base: Vec3 { x: c, y: 0.0, z: s }, turn: Vec3 { x: -s, y: 0.0, z: c } }
}

/// `M3AB90` (0x53ab90, `state_lerp`). The lerp is `lerp_a * (scale_a * acc) + lerp_b`, then `lerp_c * (scale_b * block_b)`
/// plus that (each lane: the product, then the sum, as `mulps` and `addps`).
pub fn state_lerp(input: &StateLerpIn) -> Vec4 {
    let k = input.scale_a * input.acc;
    let lerp = input.lerp_a.scale(k) + input.lerp_b;
    let k2 = input.scale_b * input.block_b;
    input.lerp_c.scale(k2) + lerp
}

/// `M3AC50` (0x53ac50, `set_input`, one float argument `v`). When `v` is above the CDef's lower bound, the input moves by
/// `-(v - lo) / (hi - lo)` (f32 steps, as the asm's SSE), is clamped to [-1, 1], and the axes are rebuilt from it. Below
/// the bound nothing changes and the result is `None`.
pub fn set_input(trig: &mut impl CrtTrig, input: &SetInputIn) -> Option<InputSet> {
    if !(input.v > input.lo) {
        return None;
    }
    let step = (input.v - input.lo) / (input.hi - input.lo);
    let new_input = clamp_unit((0.0 - step) + input.input);
    let axis = axis_from_angle(trig, &AngleIn { input: new_input, ..input.angle });
    Some(InputSet { input: new_input, axis })
}
