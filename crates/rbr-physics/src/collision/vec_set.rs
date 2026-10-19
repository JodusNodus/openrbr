//! E37 three-vector operations over collision objects.

use crate::math::vec::Vec3;

/// E37VecSet::DotX (0x4f5530): crosses `this` with `b`, then dots the cross product with `c`. The asm sums the y and z
/// terms first and adds the x term last (x87 order: `(y*y' + z*z') + x*x'`).
pub fn cross_then_dot(this: Vec3, b: Vec3, c: Vec3) -> f32 {
    let cross = this.cross3(b);
    let yz = cross.y * c.y + cross.z * c.z;
    yz + cross.x * c.x
}

/// E37VecSet::Store5630 (0x4f5630): the cross product of `p2 - origin` and `p3 - origin`. The caller stores the result
/// after all three inputs are read, so the output may alias an input.
pub fn cross_from_origin(origin: Vec3, p2: Vec3, p3: Vec3) -> Vec3 {
    (p2 - origin).cross3(p3 - origin)
}
