//! The hinged body's limit and rotation sims (`src/rbr/RBR_E44_SimA.cpp`): `CHingedRigidBody::SimC960` (the rotation
//! block of the hinge angle), `SimCF10` (one hinge limit's residual, with the limit state it moves) and `SimEA80` (the sum
//! of the active limits' residuals). The asm (`notes/physics.md` "Jointed bodies", the disassembly of 0x51c960, 0x51cf10,
//! 0x51ea80) is the reference for the operation order; the C++ agrees on everything the asm checks.
//!
//! Precision: every value is a single-precision float. The asm's x87 chains are at 24-bit precision (`notes/physics.md` "x87 and float rules"),
//! so each step is one f32 operation and the port uses f32 operators in the asm's order. Comparisons are written as the asm
//! takes them (`!(a > b)` where the asm's `jbe` also takes NaN), so a NaN follows the original's branch.

/// The limit is active when the definition's mode word is this value.
use crate::fpu::round_x87;
use crate::math::trig::{x87_cos, x87_sin};
use crate::Variant;

pub const LIMIT_ACTIVE: u32 = 2;

/// One hinge limit: the definition's block for the limit (`CHingedRigidBody::CDef`, the block at +0x1f0 + 0x60 i). Working
/// names from the field use in `SimCF10` (rbr-recomp `notes/physics/jointed-bodies.md`, 2a; the meanings are inferred, see README).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HingeLimit {
    /// The mode word; `LIMIT_ACTIVE` makes the limit count in `limit_sum`.
    pub mode: u32,
    /// Multiplies the limit's penetration in the residual (`limit_stop_coef`).
    pub stop_coef: f32,
    /// Multiplies the hinge rate in the residual (`limit_damp_coef`).
    pub damp_coef: f32,
    /// The clamp on the per-step angle correction (`limit_max_step`).
    pub max_step: f32,
    /// The reference angle the limit is measured from (`limit_angle`).
    pub ref_angle: f32,
}

impl HingeLimit {
    pub fn active(&self) -> bool {
        self.mode == LIMIT_ACTIVE
    }
}

/// The hinge state the limit sims read and write: each limit's state (`this+0x3e8`, `+0x3ec`), the hinge angle (`+0x3fc`)
/// and the hinge rate (`+0x40c`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HingeState {
    pub limit_state: [f32; 2],
    pub hinge_angle: f32,
    pub hinge_rate: f32,
}

/// Which branch `limit_residual` took (the record's coverage is reported by it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitBranch {
    /// The state is above the limit, and the correction is within `max_step`.
    Above,
    /// The state is above the limit and the correction is clamped to `max_step`; the state moves.
    AboveClamped,
    /// The state is at or past the limit (`!(hinge_angle > t)`): residual 0, no move.
    Settled,
    /// The state is below the limit, and the correction is within `max_step`.
    Below,
    /// The state is below the limit and the correction is clamped to `max_step`; the state moves.
    BelowClamped,
}

/// One `SimCF10`: the residual and, when the limit moved the state, the new state of that limit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimitStep {
    /// The st0 result, at the control word's precision (`round_x87`).
    pub residual: f64,
    pub new_state: Option<f32>,
    pub branch: LimitBranch,
}

/// `CHingedRigidBody::SimCF10(idx)` on `limit` (the definition's block `idx`), its state `limit_state` and the hinge
/// angle and rate. The residual is `-(f * stop_coef) - hinge_rate * damp_coef`, or 0 when the correction `f` is 0.
pub fn limit_residual(
    limit: &HingeLimit,
    limit_state: f32,
    hinge_angle: f32,
    hinge_rate: f32,
    variant: Variant,
) -> LimitStep {
    let lim = hinge_angle;
    let v = limit_state - limit.ref_angle;
    let (f, new_state, branch) = if v > lim {
        let f = lim - v;
        let w = 0.0 - limit.max_step;
        if w > f {
            // (ref + lim) - w: the state moves to the limit's far side
            let t = limit.ref_angle + lim;
            (w, Some(t - w), LimitBranch::AboveClamped)
        } else {
            (f, None, LimitBranch::Above)
        }
    } else {
        let t = limit.ref_angle + limit_state;
        // asm `jbe`: NaN takes the settled branch too
        if !(lim > t) {
            return LimitStep { residual: 0.0, new_state: None, branch: LimitBranch::Settled };
        }
        let f = lim - t;
        if f > limit.max_step {
            // (lim - ref) - w: the state moves to the limit's near side
            let w = limit.max_step;
            let t2 = lim - limit.ref_angle;
            (w, Some(t2 - w), LimitBranch::BelowClamped)
        } else {
            (f, None, LimitBranch::Below)
        }
    };
    let residual = if f != 0.0 {
        // x87 order: (f * stop_coef), negated, then minus (hinge_rate * damp_coef)
        let x87 = |v: f64| round_x87(v, variant);
        let stop = x87(f64::from(f) * f64::from(limit.stop_coef));
        let damp = x87(f64::from(hinge_rate) * f64::from(limit.damp_coef));
        x87(-stop - damp)
    } else {
        0.0
    };
    LimitStep { residual, new_state, branch }
}

/// `CHingedRigidBody::SimEA80`: the residuals of the active limits, summed in order. The state moves where a limit moves
/// it (`state.limit_state`). The sum is stored as an f32 after each `fadd`.
pub fn limit_sum(limits: &[HingeLimit; 2], state: &mut HingeState, variant: Variant) -> f32 {
    let mut sum = 0.0_f32;
    for (idx, limit) in limits.iter().enumerate() {
        if limit.active() {
            let step = limit_residual(limit, state.limit_state[idx], state.hinge_angle, state.hinge_rate, variant);
            if let Some(next) = step.new_state {
                state.limit_state[idx] = next;
            }
            sum = round_x87(step.residual + f64::from(sum), variant) as f32;
        }
    }
    sum
}

/// The rotation block `CHingedRigidBody::SimC960` writes at +0x598..+0x5b8, row by row: `[1 0 0; 0 c s; 0 -s c]` for the
/// hinge angle, with `s` the sine and `c` the cosine (computed first: `s`, then `c`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotationBlock {
    pub rows: [[f32; 3]; 3],
}

/// `SimC960` on `angle`. The `-s` entry is `0 - s` as the asm computes it (it differs from `-s` only for a zero `s`).
pub fn rotation_block(angle: f32) -> RotationBlock {
    // the asm stores s and c as f32 (fstps / fsts), so the x87 values are rounded there
    let s = x87_sin(angle) as f32;
    let c = x87_cos(angle) as f32;
    RotationBlock { rows: [[1.0, 0.0, 0.0], [0.0, c, s], [0.0, 0.0 - s, c]] }
}
