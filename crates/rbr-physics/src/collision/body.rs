//! CBody accessors shared by the collision and contact paths.

use super::layout::{Vec4, Vec4Pair};

/// The test CBody::VfCC (0x4e1230) makes on the bounds: the first corner against the upper bound, the second against
/// the lower bound (the layout edge fetches the corners and reads the bounds).
pub fn aabb_pretest(bounds: Vec4Pair, first: Vec4, second: Vec4) -> bool {
    first.x - bounds.upper.x < 0.0
        && first.y - bounds.upper.y < 0.0
        && first.z - bounds.upper.z < 0.0
        && bounds.lower.x - second.x < 0.0
        && bounds.lower.y - second.y < 0.0
        && bounds.lower.z - second.z < 0.0
}

/// Calls outside CBody::VfCC (0x4e1230): `E33Mat34::FetchX` (0x51b6c0) on a corner matrix, which writes the corner to
/// `out` (the caller's frame) and returns `out`. The worker is the identity object.
pub trait VfCcHost {
    fn fetch_x(&mut self, matrix: u32, out: u32, worker: u32) -> u32;
}

/// Calls outside CBody::Vf40 (0x4e1120): its E33 state reset and E43 base walk. The host owns the memory it touches.
pub trait Vf40Host {
    /// `CBody::CState::Unk44` (0x4e33e0) on the state at `state`.
    fn state_unka44(&mut self, state: u32) -> u32;
    /// `CEntity::Vf40` (0x51a4b0) on the body.
    fn entity_vf40(&mut self, body: u32) -> u32;
}

/// Calls outside CBody::Vf44 (0x4e1140): grid selection (E36SelectTable) and the base entity walk. The host owns the
/// memory it touches.
pub trait Vf44Host {
    /// `E36SelectTable` (0x4f5ca0) on the object: the grid it selects.
    fn select_table(&mut self, object: u32) -> u32;
    /// `CEntity::Vf44` (0x51a4b0, slot 17) on the body and its object.
    fn entity_vf44(&mut self, body: u32, object: u32) -> u32;
}

/// Calls outside CBody::ClearGrid (0x4e11a0): E38's grid-bound synchronization.
/// Calls outside CBody::ClearGrid (0x4e11a0): the grid's wall sync.
pub trait ClearGridHost {
    fn sync_grid(&mut self, grid: u32, source: u32, lower: u32, upper: u32) -> u32;
}

/// CBody::Vf10C (0x4e11b0): reports whether a grid has been attached (`grid` is the CBody's grid pointer).
pub fn has_grid(grid: u32) -> bool {
    grid != 0
}

/// CBody::Vf110 (0x4e11c0): reports whether the attached grid is active. `active` is the grid's status byte, or `None`
/// when no grid is attached (the byte is not read then).
pub fn grid_active(active: Option<u8>) -> bool {
    active.is_some_and(|flag| flag != 0)
}
