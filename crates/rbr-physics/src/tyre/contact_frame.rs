//! The frame of the contact plane, built from the surface normal and the wheel's axle.

use super::layout::Frame;
use crate::math::vec::Vec4;

/// Builds the contact frame from the surface `normal` and the `axle` direction. The rolling axis is `normal × axle`;
/// when its length is above 0.1 it is normalised (all four lanes, so the row's stale w lane is scaled too), the
/// lateral axis is `rolling × normal`, and the normal axis is `normal`. Also stores the sine of the inclination,
/// `-(normal · axle)`, and its cosine, `lateral · axle`. Returns whether it built the frame; the rolling row is
/// written either way.
pub fn build(f: &mut Frame, normal: Vec4, axle: Vec4) -> bool {
    let frame = &mut f.scratch.contact_frame;
    let rolling = normal.cross3(axle);
    frame.rolling.x = rolling.x;
    frame.rolling.y = rolling.y;
    frame.rolling.z = rolling.z;
    let length = rolling.length3();
    if !(length > 0.1) {
        return false;
    }
    let inv = 1.0 / length;
    let rolling = Vec4 { w: frame.rolling.w, ..rolling }.scale(inv);
    frame.rolling = rolling;
    let lateral = rolling.cross3(normal);
    frame.lateral.x = lateral.x;
    frame.lateral.y = lateral.y;
    frame.lateral.z = lateral.z;
    frame.normal = normal;
    f.scratch.inclination_sin = 0.0 - (normal.y * axle.y + normal.z * axle.z + normal.x * axle.x);
    f.scratch.inclination_cos = lateral.y * axle.y + lateral.z * axle.z + lateral.x * axle.x;
    true
}
