//! The ground probe's ray blocks. After a block's materials are blended (`ngp::tag_blend`), its blended total is compared
//! with its target and the difference limited. The sequence (the reads, the blend and the stores) is `layout::pose_flag`;
//! this module holds the pure part.

/// What limiting a ray block's difference stores: the difference, whether the block is valid (its second word), and
/// whether its two limit words are reset to zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayLimit {
    pub difference: f32,
    pub valid: bool,
    pub reset_limits: bool,
}

/// Limits the difference `d` (target less blended total). A difference up to `limit` marks the block valid and resets its
/// limit words, and is raised to `floor` when it is below it. Above `limit` the block is not valid and the difference is
/// stored as it is. A NaN difference takes the second branch, as the original's unordered compares do.
pub fn limit_ray(d: f32, limit: f32, floor: f32) -> RayLimit {
    if d <= limit {
        let difference = if d < floor { floor } else { d };
        RayLimit { difference, valid: true, reset_limits: true }
    } else {
        RayLimit { difference: d, valid: false, reset_limits: false }
    }
}
