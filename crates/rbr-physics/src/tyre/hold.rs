//! The low-speed hold: a unit direction in the contact plane that the tyre keeps against the lateral axis, and the
//! torque about the normal axis that its deviation produces. The torque is added where the sticking model acts (by
//! use: `solve_forces` calls it with the sticking weight, and it leaves at once when that weight is zero).
//!
//! The branches follow the original: each comparison is `!(a > b)`, so a NaN takes the same path. The length and scale
//! are the shared `crate::math::vec` helpers.

use crate::fpu::round_x87;
use crate::math::vec::Vec4;

/// The torque per unit of the direction's lateral component, N·m.
const HOLD_STIFFNESS: f64 = -900.0;
/// The grip limit per newton of the normal force times the kinetic friction, and its cap.
const GRIP_PER_FORCE: f32 = 2.2222222e-5;
const MAX_GRIP: f32 = 0.3;
/// The slip speed is clamped to this range before it moves the direction, m/s.
const MIN_SPEED: f32 = 0.3;
const MAX_SPEED: f32 = 100.0;
/// Below this component along the normal the direction is not projected into the plane.
const PROJECT_MIN: f32 = 0.005;

/// What the hold reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HoldInput {
    /// The weight of the sticking model. Only zero leaves at once (the comparison is the original's `ucomiss` with
    /// `jnp`), so a negative or NaN weight runs the whole function.
    pub sticking_weight: f32,
    /// The tyre state's active flag: a zero leaves at once.
    pub active: u32,
    /// The slip speed, m/s. Its absolute value is clamped to `[0.3, 100]`.
    pub speed: f32,
    /// The normal force, N, and the kinetic friction coefficient: the grip limit is their product.
    pub normal_force: f32,
    pub kinetic_friction: f32,
    /// The contact frame's lateral axis (the direction whose component produces the torque), its rolling axis (the
    /// direction the hold takes on an early exit, and scales by `sqrt(1 - local²)` otherwise) and its normal axis (the
    /// direction the hold is projected off, and moves along).
    pub lateral: Vec4,
    pub rolling: Vec4,
    pub normal: Vec4,
    /// The hold direction.
    pub direction: Vec4,
    /// The time step, s: scales how far the speed moves the direction.
    pub dt: f32,
}

/// What the hold leaves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hold {
    /// The torque about the normal axis, N·m (an x87 value).
    pub torque: f64,
    /// The new hold direction.
    pub direction: Vec4,
}

/// The hold.
pub fn hold(x: HoldInput) -> Hold {
    let round24 = round_x87;
    // The early exit: the direction becomes the rolling axis, and the torque is 0.
    let leave = Hold { torque: 0.0, direction: x.rolling };
    if x.sticking_weight == 0.0 || x.active == 0 {
        return leave;
    }
    let dot = ((x.normal.z * x.direction.z) + (x.normal.y * x.direction.y)) + x.direction.x * x.normal.x;
    let abs_dot = if dot > 0.0 { dot } else { 0.0 - dot };
    let mut direction = x.direction;
    if abs_dot > PROJECT_MIN {
        // Take the normal component off, then normalise. A length not above 0.1 leaves at once.
        direction = Vec4 {
            x: x.normal.x * (0.0 - dot) + direction.x,
            y: x.normal.y * (0.0 - dot) + direction.y,
            z: x.normal.z * (0.0 - dot) + direction.z,
            w: x.normal.w * (0.0 - dot) + direction.w,
        };
        let len = direction.length3();
        if !(len > 0.1) {
            return leave;
        }
        direction = direction.scale(1.0 / len);
    }
    let mut local = ((x.lateral.z * direction.z) + (x.lateral.y * direction.y)) + x.lateral.x * direction.x;
    // The grip: the normal force times the kinetic friction, scaled and clamped to [0, 0.3], times the weight.
    let g = (x.normal_force * x.kinetic_friction) * GRIP_PER_FORCE;
    let g = if 0.0 > g {
        0.0
    } else if g > MAX_GRIP {
        MAX_GRIP
    } else {
        g
    };
    let grip = g * x.sticking_weight;
    // The clamped speed beyond 0.3 m/s, times the time step.
    let abs_speed = if x.speed > 0.0 { x.speed } else { 0.0 - x.speed };
    let clamped_speed = if MIN_SPEED > abs_speed {
        MIN_SPEED
    } else if abs_speed > MAX_SPEED {
        MAX_SPEED
    } else {
        abs_speed
    };
    let step = (clamped_speed - MIN_SPEED) * x.dt;
    let mut moved = false;
    if step > 0.0 && local > 0.0 {
        // The lateral component moves towards 0 by `step` (positive here, so its sign is +1).
        moved = true;
        local = if step > local { 0.0 } else { local - step };
    }
    let abs_local = if local > 0.0 { local } else { 0.0 - local };
    let rotate = if abs_local > grip {
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
        moved
    };
    if !rotate {
        return Hold { torque: round24(local as f64 * HOLD_STIFFNESS), direction };
    }
    // sqrt(1 - local²) in x87 steps, each rounded to 24 bits, then stored as f32.
    let root = round24(round24(1.0 - round24(local as f64 * local as f64)).sqrt()) as f32;
    Hold {
        torque: round24(local as f64 * HOLD_STIFFNESS),
        direction: Vec4 {
            x: x.lateral.x * local + root * x.rolling.x,
            y: x.lateral.y * local + root * x.rolling.y,
            z: x.lateral.z * local + root * x.rolling.z,
            w: x.lateral.w * local + root * x.rolling.w,
        },
    }
}
