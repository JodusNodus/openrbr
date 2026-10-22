//! The output block the wheel reads after a step: the two edges of the contact patch's mark, the contact's motion, the
//! soil flow, and the slip words (by use: the wheel's tyre-mark and particle emitters read them).
//!
//! The compares are `a > b`, so a NaN takes the other branch, and the raw dwords are copied as they are.

use super::layout::{scaled_sum, Frame};
use crate::math::abs_asm;
use crate::math::vec::Vec4;

/// The share of the normal axis in the motion vector, and the half factors of the patch's edges.
const NORMAL_SHARE: f32 = 0.3;
const HALF_NEGATIVE: f32 = -0.5;
const HALF: f32 = 0.5;
/// The soil flow is scaled by this for the block.
const KILO: f32 = 1000.0;
/// Below this squared slip speed the edges follow the lateral axis.
const MIN_SPEED_SQ: f32 = 0.001;

/// The block, in the order of the caller's struct.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutputBlock {
    /// The two edges of the contact patch's mark: the contact point plus and minus half the tread width across the
    /// slip direction.
    pub edge_a: Vec4,
    pub edge_b: Vec4,
    /// The contact's motion: the rolling axis scaled by the longitudinal slip velocity, plus the lateral velocity
    /// along the lateral axis and a share of the normal axis.
    pub motion: Vec4,
    /// The abrasion rate clamped to [0, 1], when the tyre does not sink into the soil, else 0 (twice).
    pub mark_a: f32,
    pub mark_b: f32,
    /// The soil flow times 1000.
    pub soil_flow: f32,
    /// The sink depth and the slip phase, as raw bits.
    pub sink_depth_bits: u32,
    pub slip_phase_bits: u32,
    /// The slipped area times its scale (0 when the scale is zero).
    pub slip_area: f32,
    /// Always 1.
    pub valid: u32,
}

/// `0` below zero, `1` above one, else `v` (two compares, so a NaN is kept).
pub fn clamp01(v: f32) -> f32 {
    if 0.0 > v {
        0.0
    } else if v > 1.0 {
        1.0
    } else {
        v
    }
}

/// Builds the output block from the frame. The block is returned for the caller to store; the copies the scratch keeps
/// of the contact point, the motion, the soil flow and the velocities are written into the frame.
pub fn write_output_block(f: &mut Frame) -> OutputBlock {
    let s = &f.scratch;
    let tread_width = f.setup.tread_width;
    let (point, frame) = (s.contact_point, s.contact_frame);
    let (lateral_velocity, longitudinal_velocity, long_slip_velocity) =
        (s.contact_velocity[0], s.contact_velocity[1], s.long_slip_velocity);
    let motion = frame.rolling.scale(long_slip_velocity);
    let motion = scaled_sum(frame.lateral, lateral_velocity, motion);
    let motion = scaled_sum(frame.normal, NORMAL_SHARE, motion);
    let soil_flow = s.soil_flow_opposed * KILO;
    let sink_depth_bits = s.indentation.sink_depth.to_bits();
    let slip_phase_bits = s.slip_phase.to_bits();
    // Both marks are the clamp of the abrasion rate when the sink depth is an ordered zero, and 0 otherwise.
    let mark = if s.indentation.sink_depth == 0.0 { clamp01(s.abrasion_rate) } else { 0.0 };
    let (edge_a, edge_b) = if s.speed_sq > MIN_SPEED_SQ {
        let inv = 1.0 / s.speed;
        let along = lateral_velocity * inv;
        let across = longitudinal_velocity * inv * tread_width * HALF;
        let t = frame.lateral.scale(across);
        let u = s.patch_length * along * HALF_NEGATIVE;
        let t = scaled_sum(frame.rolling, u, t);
        (scaled_sum(t, 1.0, point), scaled_sum(t, -1.0, point))
    } else {
        let w = tread_width * HALF;
        (scaled_sum(frame.lateral, w, point), scaled_sum(frame.lateral, 0.0 - w, point))
    };
    // The slipped area is `speed * 0` first (the original multiplies by a zeroed register), then the product when the
    // scale is not an ordered zero.
    let mut slip_area = s.speed * 0.0;
    if s.slip_area_scale != 0.0 {
        let sum = s.patch_length * abs_asm(lateral_velocity) + abs_asm(longitudinal_velocity) * tread_width;
        slip_area = sum * s.slip_area_scale;
    }
    f.scratch.contact_point_copy = point;
    f.scratch.motion_copy = motion;
    f.scratch.soil_flow_copy = soil_flow.to_bits();
    f.scratch.longitudinal_velocity_copy = longitudinal_velocity.to_bits();
    f.scratch.long_slip_velocity_copy = long_slip_velocity.to_bits();
    OutputBlock {
        edge_a,
        edge_b,
        motion,
        mark_a: mark,
        mark_b: mark,
        soil_flow,
        sink_depth_bits,
        slip_phase_bits,
        slip_area,
        valid: 1,
    }
}
