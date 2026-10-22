//! Slip in the contact plane: the contact point's velocity in the contact frame, the slip directions, and the weights
//! of the sticking and the sliding model.

use crate::fpu::round_x87;

use super::layout::Frame;

/// The axes of the contact frame, each as three components.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactAxes {
    pub lateral: [f32; 3],
    pub rolling: [f32; 3],
    pub normal: [f32; 3],
}

/// The contact point's slip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slip {
    /// The velocity in the contact frame: lateral, longitudinal, normal, m/s.
    pub velocity: [f32; 3],
    /// `lateral² + longitudinal²`.
    pub speed_sq: f32,
    /// `sqrt(speed_sq)`, m/s, rounded to 24 bits as the x87 does.
    pub speed: f32,
    /// `R·ω + longitudinal`: the slip velocity along the rolling axis, m/s.
    pub long_slip_velocity: f32,
    /// `-(1/R)·longitudinal - ω`: the spin the contact's longitudinal velocity does not explain, rad/s.
    pub spin_slip: f32,
}

/// The slip of the contact point moving at `velocity` (world frame) against a wheel of effective `radius` spinning at
/// `spin`. Each component is a dot product with an axis, summed as `(y + z) + x`, as the original adds the terms.
pub fn compute_slip(axes: &ContactAxes, radius: f32, velocity: [f32; 3], spin: f32) -> Slip {
    let round24 = round_x87;
    let dot = |a: [f32; 3]| (a[1] * velocity[1] + a[2] * velocity[2]) + a[0] * velocity[0];
    let v = [dot(axes.lateral), dot(axes.rolling), dot(axes.normal)];
    let speed_sq = v[1] * v[1] + v[0] * v[0];
    Slip {
        velocity: v,
        speed_sq,
        speed: round24((speed_sq as f64).sqrt()) as f32,
        long_slip_velocity: radius * spin + v[1],
        spin_slip: -1.0 / radius * v[1] - spin,
    }
}

/// [`compute_slip`] on the frame, for the contact point moving at `velocity`.
pub fn update_slip(f: &mut Frame, velocity: [f32; 3]) {
    let row = |v: crate::math::vec::Vec4| [v.x, v.y, v.z];
    let frame = f.scratch.contact_frame;
    let axes = ContactAxes { lateral: row(frame.lateral), rolling: row(frame.rolling), normal: row(frame.normal) };
    let slip = compute_slip(&axes, f.state.effective_radius, velocity, f.scratch.spin);
    let s = &mut f.scratch;
    s.contact_velocity = slip.velocity;
    s.speed_sq = slip.speed_sq;
    s.speed = slip.speed;
    s.long_slip_velocity = slip.long_slip_velocity;
    s.spin_slip = slip.spin_slip;
}

/// Splits the slip speed into the weights of the sticking and the sliding model: above 2 m/s the sliding model has
/// weight 1; from 1 to 2 m/s the sliding weight is `speed - 1` and the sticking weight the rest; at 1 m/s or below the
/// sticking weight is 1, and the two low-speed words are cleared. The comparisons are `!(a > b)`, so a NaN speed takes
/// the last branch.
pub fn weigh_models(f: &mut Frame) {
    let speed = f.scratch.speed;
    let (sliding, sticking) = if !(2.0 > speed) {
        (1.0, 0.0)
    } else if !(1.0 > speed) {
        let sliding = speed - 1.0;
        (sliding, 1.0 - sliding)
    } else {
        f.scratch.low_speed_a = 0.0;
        f.scratch.low_speed_b = 0.0;
        (0.0, 1.0)
    };
    f.scratch.sliding_weight = sliding;
    f.scratch.sticking_weight = sticking;
}

/// The absolute direction cosines of the slip and their sum, from the spin, the lateral and longitudinal velocities
/// and the effective radius `R`. The longitudinal slip velocity is `c1 = ω·R + v` (x87, each op rounded to the
/// precision control); the slip length is `sqrt(lateral² + c1²)`, where the square of `c1` is the x87 value times its
/// f32 store (they differ at 53 bits, under NGP). Below 0.001 the result is `(1, 1, 2)`.
pub fn slip_directions(spin: f32, lateral: f32, longitudinal: f32, radius: f32) -> [f32; 3] {
    let round24 = round_x87;
    const LIMIT: f32 = 0.001;
    let st = round24(round24(spin as f64 * radius as f64) + longitudinal as f64);
    let c1 = st as f32;
    let e = round24(round24(round24(st * c1 as f64) + round24(lateral as f64 * lateral as f64)).sqrt()) as f32;
    if e > LIMIT {
        let inv = 1.0f32 / e;
        let bp = inv * lateral;
        let first = if bp > 0.0 { bp } else { 0.0 - bp };
        let cp = inv * c1;
        let second = if cp > 0.0 { cp } else { 0.0 - cp };
        [first, second, first + second]
    } else {
        [1.0, 1.0, 2.0]
    }
}
