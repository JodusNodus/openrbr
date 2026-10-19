//! `CScratch::WriteOutputs` (0x533080): builds the output block from the contact vectors and the scalars. It scales
//! `+0x180` by `+0x374` and adds the `+0x170` and `+0x190` terms into `out+0x50`; the `+0x404` clamp and the two
//! vectors at `out+0x10` and `+0x20` depend on the sign-free test of `+0x364`; the friction term (`+0x3d8`) and the
//! copies into `+0x490..+0x4b8` close it. The compares follow the asm, so a NaN takes the other branch, and the raw
//! dwords are copied as they are.

use super::layout::{scaled_sum, Frame, OutputBlock};
use crate::math::abs_asm;

/// The asm's constants: 0.3 (0x72afcc), -0.5 (0x72d160), 1000 (0x737b70), 0.5 (0x795e18) and 0.001 (0x795e94).
const K_SPEED_TERM: f32 = 0.3;
const K_HALF_NEG: f32 = -0.5;
const K_KILO: f32 = 1000.0;
const K_HALF: f32 = 0.5;
const K_MIN_SPEED: f32 = 0.001;

/// `0` below zero, `1` above one, else `v` (the asm's two compares, so a NaN is kept).
pub fn clamp01(v: f32) -> f32 {
    if 0.0 > v {
        0.0
    } else if v > 1.0 {
        1.0
    } else {
        v
    }
}

/// `WriteOutputs` on the frame. The output block is returned for the caller to store; the copies at `+0x490..+0x4b8`
/// are written into the frame.
pub fn write_outputs(f: &mut Frame) -> OutputBlock {
    let s = &f.scratch;
    let def_ac = f.setup.fac;
    let (k160, frame0, frame1, frame2) = (s.k_vec, s.contact_frame[0], s.contact_frame[1], s.contact_frame[2]);
    let (slip0, slip1, long_vel) = (s.slip_v[0], s.slip_v[1], s.long_contact_vel);
    let v50 = frame1.scale(long_vel);
    let v50 = scaled_sum(frame0, slip0, v50);
    let v50 = scaled_sum(frame2, K_SPEED_TERM, v50);
    let f88 = s.force_a * K_KILO;
    let u90 = s.contact_p[5].to_bits();
    let u94 = s.f_e2.to_bits();
    // Both flags are the clamp of `+0x404` when `+0x340` is an ordered zero, and 0 otherwise (a NaN gives 0).
    let flag = if s.contact_p[5] == 0.0 { clamp01(s.flow) } else { 0.0 };
    let (v10, v20) = if s.speed_sq > K_MIN_SPEED {
        let inv = 1.0 / s.speed;
        let p1 = slip0 * inv;
        let v = slip1 * inv * def_ac * K_HALF;
        let t = frame0.scale(v);
        let u = s.tyre_factor * p1 * K_HALF_NEG;
        let t = scaled_sum(frame1, u, t);
        (scaled_sum(t, 1.0, k160), scaled_sum(t, -1.0, k160))
    } else {
        let w = def_ac * K_HALF;
        (scaled_sum(frame0, w, k160), scaled_sum(frame0, 0.0 - w, k160))
    };
    // `+0x98` is `speed * 0` first, then the friction term when `+0x3d8` is not an ordered zero.
    let mut f98 = s.speed * 0.0;
    if s.arg_d != 0.0 {
        let sum = s.tyre_factor * abs_asm(slip0) + abs_asm(slip1) * def_ac;
        f98 = sum * s.arg_d;
    }
    f.scratch.copy_490 = k160;
    f.scratch.copy_4a0 = v50;
    f.scratch.copy_4b0 = f88.to_bits();
    f.scratch.copy_4b8 = slip1.to_bits();
    f.scratch.copy_4b4 = long_vel.to_bits();
    OutputBlock { v10, v20, v50, f80: flag, f84: flag, f88, u90, u94, f98, flag_ac: 1 }
}
