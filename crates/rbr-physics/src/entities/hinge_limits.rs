//! The hinge's limit springs and its rotation. A hinged body's definition has two limit springs (`Spring0*`, `Spring1*`:
//! type, position, stiffness, damping, maximum displacement and span); each active one gives a torque from how far the
//! hinge angle is from the spring, and drags the spring's position along when the displacement passes the maximum. The
//! torques of the active springs are summed for the hinge solve. The rotation about the hinge axis is the matrix of the
//! hinge angle.
//!
//! Precision: every value is a single-precision float, except the torque, which the original computes with x87 at the
//! thread's precision (`notes/physics.md`, "x87 and float rules"); the model applies [`round_x87`] at the same steps.
//! Comparisons are written as the original takes them (`!(a > b)` where its branch also takes NaN), so a NaN follows the
//! original's branch.

use crate::fpu::round_x87;
use crate::math::trig::{x87_cos, x87_sin};

/// A limit spring is active when its type is this value.
pub const ACTIVE_TYPE: u32 = 2;

/// One limit spring of the definition (`Spring0*` or `Spring1*`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimitSpring {
    /// `Type`: [`ACTIVE_TYPE`] makes the spring count in [`limit_torque_sum`].
    pub kind: u32,
    /// `Stiff`: multiplies the displacement in the torque.
    pub stiffness: f32,
    /// `Damp`: multiplies the hinge rate in the torque.
    pub damping: f32,
    /// `MaxDisp`: the clamp on the displacement.
    pub max_displacement: f32,
    /// `Span`: the distance the spring's position is measured from.
    pub span: f32,
}

impl LimitSpring {
    pub fn active(&self) -> bool {
        self.kind == ACTIVE_TYPE
    }
}

/// The hinge state the limit springs read and write: each spring's position (it starts at the definition's `Pos` and moves
/// when the displacement is clamped), the hinge angle (rad) and the hinge rate (rad/s).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HingeState {
    pub spring_position: [f32; 2],
    pub angle: f32,
    pub rate: f32,
}

/// Which branch [`limit_torque`] took (the record's coverage is reported by it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitBranch {
    /// The position is above the span and the displacement is within the maximum.
    Above,
    /// The position is above the span and the displacement is clamped to the maximum; the position moves.
    AboveClamped,
    /// The position is at or past the angle (`!(angle > t)`): torque 0, no move.
    Settled,
    /// The position is below the span and the displacement is within the maximum.
    Below,
    /// The position is below the span and the displacement is clamped to the maximum; the position moves.
    BelowClamped,
}

/// One spring's torque and, when the displacement was clamped, the spring's new position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimitStep {
    /// At the thread's x87 precision ([`round_x87`]).
    pub torque: f64,
    pub new_position: Option<f32>,
    pub branch: LimitBranch,
}

/// The torque of `spring` at `position`, for the hinge `angle` and `rate`: `-(f * stiffness) - rate * damping`, or 0 when
/// the displacement `f` is 0.
pub fn limit_torque(spring: &LimitSpring, position: f32, angle: f32, rate: f32) -> LimitStep {
    let v = position - spring.span;
    let (f, new_position, branch) = if v > angle {
        let f = angle - v;
        let w = 0.0 - spring.max_displacement;
        if w > f {
            // (span + angle) - w: the position moves to the far side
            let t = spring.span + angle;
            (w, Some(t - w), LimitBranch::AboveClamped)
        } else {
            (f, None, LimitBranch::Above)
        }
    } else {
        let t = spring.span + position;
        // `jbe` in the original: NaN takes the settled branch too
        if !(angle > t) {
            return LimitStep { torque: 0.0, new_position: None, branch: LimitBranch::Settled };
        }
        let f = angle - t;
        if f > spring.max_displacement {
            // (angle - span) - w: the position moves to the near side
            let w = spring.max_displacement;
            let t2 = angle - spring.span;
            (w, Some(t2 - w), LimitBranch::BelowClamped)
        } else {
            (f, None, LimitBranch::Below)
        }
    };
    let torque = if f != 0.0 {
        // x87 order: (f * stiffness), negated, then minus (rate * damping)
        let x87 = round_x87;
        let stop = x87(f64::from(f) * f64::from(spring.stiffness));
        let damp = x87(f64::from(rate) * f64::from(spring.damping));
        x87(-stop - damp)
    } else {
        0.0
    };
    LimitStep { torque, new_position, branch }
}

/// The torques of the active springs, summed in order. The position moves where a spring moves it
/// (`state.spring_position`). The sum is stored as an f32 after each add.
pub fn limit_torque_sum(springs: &[LimitSpring; 2], state: &mut HingeState) -> f32 {
    let mut sum = 0.0_f32;
    for (index, spring) in springs.iter().enumerate() {
        if spring.active() {
            let step = limit_torque(spring, state.spring_position[index], state.angle, state.rate);
            if let Some(next) = step.new_position {
                state.spring_position[index] = next;
            }
            sum = round_x87(step.torque + f64::from(sum)) as f32;
        }
    }
    sum
}

/// The rotation about the hinge axis (X) by the hinge angle, row by row: `[1 0 0; 0 c s; 0 -s c]`, with `s` the sine and
/// `c` the cosine (computed first: `s`, then `c`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotationBlock {
    pub rows: [[f32; 3]; 3],
}

/// The rotation for `angle`. The `-s` entry is `0 - s`, which differs from `-s` only for a zero `s`.
pub fn rotation_block(angle: f32) -> RotationBlock {
    // the x87 values are rounded to f32 where the original stores them
    let s = x87_sin(angle) as f32;
    let c = x87_cos(angle) as f32;
    RotationBlock { rows: [[1.0, 0.0, 0.0], [0.0, c, s], [0.0, 0.0 - s, c]] }
}
