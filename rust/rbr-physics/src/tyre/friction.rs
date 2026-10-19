//! 0x531c70 `CScratch::Friction`: the friction term of the tyre step. It rotates the state vector `sv`
//! (`m_state+0x80`) towards the contact direction and returns a force term from the projection `local`.
//! The branches follow the asm, not `src/rbr/RBR_E48_TyreMenu.cpp`: each comparison is the `jbe` the
//! original takes, so a NaN takes the same path (`!(a > b)`).
//!
//! The length and scale are the shared `crate::math::vec` helpers; the original's other helpers are inlined:
//! `E01Vec4::CopyFrom` (0x43c5b0) and `E16_Sign` (0x48a440). `E16_Sign(local)` is 1 where it is called.

use crate::fpu::round_x87;
use crate::math::vec::Vec4;
use crate::Variant;

/// The inputs of `Friction` (see `layout::load_friction`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrictionInput {
    /// `this+0x3f0`: the gate on `g`. Only zero leaves at once: the asm's `ucomiss` with `jnp` leaves on
    /// equality, so a negative or NaN gate runs the whole function.
    pub gate: f32,
    /// `m_state+0xc`: a zero leaves at once.
    pub state_flag: u32,
    /// `this+0x360`: the speed. Its absolute value is clamped to `[0.3, 100]`.
    pub speed: f32,
    /// `this+0x314` and `this+0x1f8`: the factors of `g`.
    pub factor_314: f32,
    pub factor_1f8: f32,
    /// `this+0x170..0x17c`: the direction that `local` projects `sv` on.
    pub dir: Vec4,
    /// `this+0x180..0x18c`: the vector `sv` takes on an early exit, and is scaled by `sqrt(1 - local²)` otherwise.
    pub rest: Vec4,
    /// `this+0x190..0x19c`: the axis `dot` projects `sv` on, and that `sv` moves along.
    pub axis: Vec4,
    /// `m_state+0x80..0x8c`: the state vector.
    pub sv: Vec4,
    /// The stack argument: scales `f4 - 0.3`.
    pub k: f32,
}

/// What `Friction` returns: `value` is st0, and `sv` is the state vector it leaves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Friction {
    pub value: f64,
    pub sv: Vec4,
}

/// 0x531c70 `CScratch::Friction`.
pub fn friction(x: FrictionInput, variant: Variant) -> Friction {
    let round24 = |v: f64| round_x87(v, variant);
    // The early exit: `sv` becomes `rest` (CopyFrom), and the function returns 0.
    let leave = Friction { value: 0.0, sv: x.rest };
    if x.gate == 0.0 || x.state_flag == 0 {
        return leave;
    }
    let dot = ((x.axis.z * x.sv.z) + (x.axis.y * x.sv.y)) + x.sv.x * x.axis.x;
    let abs_dot = if dot > 0.0 { dot } else { 0.0 - dot };
    let mut sv = x.sv;
    if abs_dot > 0.005 {
        // sv += -dot · axis, then normalised. A length not above 0.1 leaves at once.
        sv = Vec4 {
            x: x.axis.x * (0.0 - dot) + sv.x,
            y: x.axis.y * (0.0 - dot) + sv.y,
            z: x.axis.z * (0.0 - dot) + sv.z,
            w: x.axis.w * (0.0 - dot) + sv.w,
        };
        let len = sv.length3();
        if !(len > 0.1) {
            return leave;
        }
        sv = sv.scale(1.0 / len);
    }
    let mut local = ((x.dir.z * sv.z) + (x.dir.y * sv.y)) + x.dir.x * sv.x;
    // g: clamped to [0, 0.3], then times the gate. The constant 2.2222222e-5 is at 0x740bcc.
    let g = (x.factor_314 * x.factor_1f8) * 2.2222222e-5;
    let g = if 0.0 > g {
        0.0
    } else if g > 0.3 {
        0.3
    } else {
        g
    };
    let grip = g * x.gate;
    // f4: the absolute speed clamped to [0.3, 100]; `scaled` = (f4 - 0.3) · k.
    let abs_speed = if x.speed > 0.0 { x.speed } else { 0.0 - x.speed };
    let f4 = if 0.3 > abs_speed {
        0.3
    } else if abs_speed > 100.0 {
        100.0
    } else {
        abs_speed
    };
    let scaled = (f4 - 0.3) * x.k;
    let mut stepped = false;
    if scaled > 0.0 && local > 0.0 {
        // Stepped: local becomes 0 when `scaled > local`, else local - E16_Sign(local)·scaled (E16_Sign = 1).
        stepped = true;
        local = if scaled > local { 0.0 } else { local - scaled };
    }
    let abs_local = if local > 0.0 { local } else { 0.0 - local };
    let root_path = if abs_local > grip {
        let sign = if local > 0.0 {
            1.0
        } else if 0.0 > local {
            -1.0
        } else {
            0.0
        };
        local = sign * grip;
        true
    } else {
        stepped
    };
    if !root_path {
        return Friction { value: round24(local as f64 * -900.0), sv };
    }
    // sqrt(1 - local²) in x87 steps, each rounded to 24 bits, then stored as f32 (the `fstp dword`).
    let root = round24(round24(1.0 - round24(local as f64 * local as f64)).sqrt()) as f32;
    Friction {
        value: round24(local as f64 * -900.0),
        sv: Vec4 {
            x: x.dir.x * local + root * x.rest.x,
            y: x.dir.y * local + root * x.rest.y,
            z: x.dir.z * local + root * x.rest.z,
            w: x.dir.w * local + root * x.rest.w,
        },
    }
}
