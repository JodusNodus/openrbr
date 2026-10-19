//! The car body's two point scatters, `CCarBody::Vf9Cx` (exe 0x4eb820, slot 39, `ret 8`: subtract) and `CCarBody::VfA0x`
//! (0x4eb8c0, slot 40: add). Each point (its lanes 0 to 2; lane 3 is not written) gets the body's base subtracted or added.
//! When the body gate is on, the body's slot 0 runs first. The asm (not the C++) decides; the slot-0 call is answered from
//! the records. The edge reads the gate and the base (`layout::body_gate`, `layout::body_base`) and the points before the
//! call: the slot-0 call is a getter with no effect, so none of them changes.

use crate::math::vec::{Vec3, Vec4};

/// The call the scatter makes out of its body: the body's vtable slot 0 (`body` is `this`).
pub trait BodyCalls {
    fn slot0(&mut self, body: u32);
}

/// The scatter's operator: `Vf9Cx` subtracts the base, `VfA0x` adds it.
#[derive(Clone, Copy, PartialEq)]
enum Op {
    Sub,
    Add,
}

/// `CCarBody::Vf9Cx(out, n)` (0x4eb820, `ret 8`) (name ours): subtracts `base` from each point. `base` is None when the original
/// faults (a null owner base): a scatter of no points never reads it.
pub fn subtract_points(calls: &mut impl BodyCalls, gate: bool, body: u32, base: Option<Vec3>, points: &mut [Vec4]) {
    scatter(calls, gate, body, base, points, Op::Sub);
}

/// `CCarBody::VfA0x(out, n)` (0x4eb8c0, `ret 8`) (name ours): adds `base` to each point.
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
