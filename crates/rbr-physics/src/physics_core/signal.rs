//! The rules of the signal dispatch and of the placement notifications: when an emission traps, which object kind a
//! placement becomes and which collision-mesh signal it sends. The dispatch itself is in `layout`.

/// The number of slots of a multi-signal; a slot count at or above it is out of range.
pub const SLOT_COUNT: u32 = 0x20;

/// Whether an emission traps: a count out of range is logged, then the original breaks into the debugger. It does so
/// only for a signal that has a counter (`has_count`) and is not emitting already (`guard` clear).
pub fn traps(has_count: bool, guard: u32, count: u32) -> bool {
    has_count && guard == 0 && count >= SLOT_COUNT
}

/// The object kind of the cone and of the banner stand, which a placement becomes when its collision grid's name holds
/// the part (`CONE`, `BANNER_STAND`).
const CONE_KIND: u32 = 0x19;
const BANNER_STAND_KIND: u32 = 0x15;

/// The object kind a placement is made as: its own `object_type`, unless the name of its collision grid (`None`
/// without a grid or a name) holds `CONE` or else `BANNER_STAND`.
pub fn stuff_kind(object_type: u32, grid_name: Option<&[u8]>) -> u32 {
    let holds = |name: &[u8], part: &[u8]| name.windows(part.len()).any(|w| w == part);
    match grid_name {
        Some(name) if holds(name, b"CONE") => CONE_KIND,
        Some(name) if holds(name, b"BANNER_STAND") => BANNER_STAND_KIND,
        _ => object_type,
    }
}

/// What a placement sends on the collision-mesh signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshEmit {
    /// A static placement (its flags' bits 0x30 clear, or 0x20 alone): nothing.
    None,
    /// A dynamic one with 0x10: the collision-mesh signal on its descriptor.
    Emit,
    /// A dynamic one with 0x30: first the next free collision id and flag 4 on its descriptor, then the signal.
    NewId,
}

/// What a placement with these `flags` sends.
pub fn mesh_emit(flags: u32) -> MeshEmit {
    match flags & 0x30 {
        0x10 => MeshEmit::Emit,
        0x30 => MeshEmit::NewId,
        _ => MeshEmit::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stuff_kind_takes_cones_and_banner_stands_by_their_grids_name() {
        assert_eq!(stuff_kind(7, None), 7);
        assert_eq!(stuff_kind(7, Some(b"POLE")), 7);
        assert_eq!(stuff_kind(7, Some(b"X_TRAFFIC_CONE_1")), 0x19);
        assert_eq!(stuff_kind(7, Some(b"BANNER_STAND_A")), 0x15);
    }
}
