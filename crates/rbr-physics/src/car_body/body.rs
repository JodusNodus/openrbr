//! The car body's two point scatters: one subtracts the body's base from each point, the other adds it (lanes 0 to 2;
//! lane 3 is not written). When the body gate is on, the body's first virtual method runs first. The edge reads the gate
//! and the base (`layout::body_pre_step_gate`, `layout::body_base`) and the points before the call: that method is a getter with
//! no effect, so none of them changes.

use crate::math::vec::{Vec3, Vec4};

/// The call the scatter makes out of its body: the body's first virtual method.
pub trait BodyCalls {
    fn slot0(&mut self, body: u32);
}

/// The scatter's operator.
#[derive(Clone, Copy, PartialEq)]
enum Op {
    Sub,
    Add,
}

/// Subtracts `base` from each point. `base` is None when the original faults (a null owner base): a scatter of no
/// points never reads it.
pub fn subtract_points(calls: &mut impl BodyCalls, gate: bool, body: u32, base: Option<Vec3>, points: &mut [Vec4]) {
    scatter(calls, gate, body, base, points, Op::Sub);
}

/// Adds `base` to each point.
pub fn add_points(calls: &mut impl BodyCalls, gate: bool, body: u32, base: Option<Vec3>, points: &mut [Vec4]) {
    scatter(calls, gate, body, base, points, Op::Add);
}

fn scatter(calls: &mut impl BodyCalls, gate: bool, body: u32, base: Option<Vec3>, points: &mut [Vec4], op: Op) {
    if gate {
        calls.slot0(body);
    }
    if points.is_empty() {
        return;
    }
    let base = base.expect("the body's base is null (the original faults)");
    let apply = |a: f32, b: f32| match op {
        Op::Sub => a - b,
        Op::Add => a + b,
    };
    for p in points.iter_mut() {
        p.x = apply(p.x, base.x);
        p.y = apply(p.y, base.y);
        p.z = apply(p.z, base.z);
    }
}
