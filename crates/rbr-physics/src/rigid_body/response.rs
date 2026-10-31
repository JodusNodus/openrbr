//! The body's response to a point: the velocity of the body at the point. With no parent it is `w x d + v`, with
//! `d` the point less the body's position, in the order `(w1*d2 + v0) - w2*d1` and so on, lane by lane. With a parent
//! the call takes the parent's response to the point, plus the same term for the point in the parent's frame,
//! transformed by the parent. The parent path's calls go through `ResponseHost`.

use super::layout::{Body, Vec4};

/// The calls the parent path makes outside the body.
pub trait ResponseHost {
    /// The parent's response to the point.
    fn parent_response(&mut self, parent: u32, point: Vec4) -> Vec4;
    /// The point in the parent's frame.
    fn compose(&mut self, parent: u32, point: Vec4) -> Vec4;
    /// The parent's transform of `v`.
    fn parent_transform(&mut self, parent: u32, v: Vec4) -> Vec4;
}

/// `(w x d) + v`: each lane is `(w * d + v) - w * d`, the first product added to `v`.
fn cross_plus(w: Vec4, d: Vec4, v: Vec4) -> Vec4 {
    Vec4 {
        x: (w.y * d.z + v.x) - w.z * d.y,
        y: (w.z * d.x + v.y) - w.x * d.z,
        z: (w.x * d.y + v.z) - w.y * d.x,
        w: 0.0,
    }
}

/// The response to `point`.
pub fn response(body: &Body, point: Vec4, host: &mut impl ResponseHost) -> Vec4 {
    let w = body.angular_velocity;
    let v = body.linear_velocity;
    if body.parent == 0 {
        let d =
            Vec4 { x: point.x - body.position.x, y: point.y - body.position.y, z: point.z - body.position.z, w: 0.0 };
        return cross_plus(w, d, v);
    }
    let base = host.parent_response(body.parent, point);
    let l = host.compose(body.parent, point);
    let out = host.parent_transform(body.parent, cross_plus(w, l, v));
    Vec4 { x: base.x + out.x, y: base.y + out.y, z: base.z + out.z, w: 0.0 }
}
