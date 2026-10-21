//! Points on the wheel axle: the tyre's centre from the hub, and points either side of the centre (the carcass edges, at
//! half the tyre's `CarcassWidth`).

use crate::math::vec::Vec4;

/// The point `distance` along the unit `axle` from `from`: each lane `axle * distance`, rounded, then `+ from` (the
/// product is not fused).
pub fn along_axle(axle: Vec4, distance: f32, from: Vec4) -> Vec4 {
    axle.scale(distance) + from
}

/// The tyre's centre: the hub position moved along the axle by the tyre's `HubOffset`.
pub fn tyre_centre(axle: Vec4, hub_offset: f32, hub: Vec4) -> Vec4 {
    along_axle(axle, hub_offset, hub)
}
