//! `CBitField` name tables and the DefManager registry, read from the
//! original 1.02 executable's `.data`.
//!
//! The bit counts come from the `GetBitCount` overrides (see
//! `notes/formats.md` "Physics scripts"); every name and address below is read
//! from the binary. Generated (run `python3 -I -B scripts/gen-lsp-tables.py`
//! from the repo root to regenerate).

/// `Flags` bit names (`CPhysics::CDef::CFlags 0x523bb0`, table `0x7cbc10`, 20 bits).
pub const BITFIELD_FLAGS: &[&str] = &[
    "FACE_COLL",
    "EDGE_COLL",
    "FACE_PENETR",
    "EDGE_PENETR",
    "CHECK_COLL",
    "CHECK_PENETR",
    "GROUND_COLL",
    "REJECT_BAD_MESHES",
    "SETI",
    "TYRE_REACTION",
    "STUFF_ROTATION",
    "COLLISIONSOLVER_TEST",
    "SUB_STEPPING",
    "BLOCKMAP_NEW_GETBLOCK",
    "SIGCG",
    "ENABLE_SINGLESTEPPING",
    "CHECK_WHEEL_COLL",
    "INTERNAL_SINGLESTEP",
    "FORCE_MATERIAL",
    "ENABLE_DAMAGE",
];

/// `DbgFlags` bit names (`CPhysics::CDef::CDbgFlags 0x523be0`, table `0x7cbcb0`, 27 bits).
pub const BITFIELD_DBGFLAGS: &[&str] = &[
    "RENDER_FORCES",
    "RENDER_COLLISION_MESHES",
    "RENDER_SUSPENSION_GEOMETRY",
    "RENDER_CAR_COG",
    "RENDER_GG_DIAG",
    "RENDER_MATERIAL",
    "RENDER_ENGINE_PRESSURES",
    "RENDER_TURBO",
    "RENDER_DIFF_INFO",
    "RENDER_ENGINE_TORQUE",
    "RENDER_SUSPENSION_TRAVEL",
    "RENDER_BAD_COLLISION_MESHES",
    "RENDER_BRAKEWALL_DATA",
    "RENDER_PENETRATION_EVENTS",
    "RENDER_COLLISION_EVENTS",
    "RENDER_NEARBY_COLLISIONDATA",
    "RENDER_CS_VERSION",
    "RENDER_COLLSOLVER_INFO",
    "RENDER_SOLID_IMPULSES",
    "RENDER_ENGINE_COND",
    "RENDER_DRIVE_COND",
    "RENDER_CAR_COND",
    "RENDER_HINGED_BODY",
    "RENDER_TYRE_THERMO",
    "RENDER_DAMAGE_INFO",
    "RENDER_LINEMESH_ZONECHECK",
    "RENDER_EDGE_ZONECHECK",
];

/// `LogFlags` bit names (`CPhysics::CDef::CLogFlags 0x523c10`, table `0x7cbd90`, 20 bits).
pub const BITFIELD_LOGFLAGS: &[&str] = &[
    "COLLISIONMESH",
    "TRINODE_TRACKING",
    "SEGMENT",
    "CHECK_SEGMENT",
    "SIRAP",
    "INVALID_MATERIAL_ALERT",
    "COLLISIONEVENT",
    "COLLISIONSOLVER",
    "STUFF",
    "CHECK_INTERSECTION",
    "OUTPUT",
    "FORWARD_PROJECT_STATE",
    "FRAME",
    "EFF_MASS",
    "WHEEL_FORCES",
    "INTEGRATOR",
    "ALLOC",
    "CAR_CONTROL",
    "COLLISIONMESH_DETECT",
    "COLLISIONMESH_BUILD",
];

/// `Flags` bit names (`CMaterial::CFlags 0x520860`, table `0x7caf18`, 4 bits).
pub const BITFIELD_MATERIAL: &[&str] = &["VALID", "SOFT", "HARD", "BENDABLE"];

/// One DefManager registry record (table `0x%x`, stride 20): the
/// block name, the class dict address, and the factory function.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RegistryEntry {
    pub name: &'static str,
    pub dict: u32,
    pub factory: u32,
}

/// Every class record in the registry (order matches the exe).
pub const REGISTRY: &[RegistryEntry] = &[
    RegistryEntry { name: "Ground", dict: 0x165926c, factory: 0x51baa0 },
    RegistryEntry { name: "Wheel", dict: 0x165f2cc, factory: 0x53b610 },
    RegistryEntry { name: "Solid", dict: 0x165f118, factory: 0x52b8b0 },
    RegistryEntry { name: "Car", dict: 0x8ef73c, factory: 0x4e6f20 },
    RegistryEntry { name: "CarBody", dict: 0x8ef774, factory: 0x4eba80 },
    RegistryEntry { name: "HingedBody", dict: 0x1659284, factory: 0x51f880 },
    RegistryEntry { name: "MountedBody", dict: 0x1659b20, factory: 0x521b50 },
];

/// Bit names for a bitfield key (`None` when the key is not a
/// bitfield, or its name table is not proven). `Flags` is the
/// `CBitField` child of [`crate::lsp_keys::Class::PhysDef`] and
/// [`crate::lsp_keys::Class::Material`]; other `CBitField` members (such as
/// the solid `+0x14c` flags) have no proven name table.
pub fn bit_names(class: crate::lsp_keys::Class, key: &str) -> Option<&'static [&'static str]> {
    match (class, key) {
        (crate::lsp_keys::Class::PhysDef, "Flags") => Some(BITFIELD_FLAGS),
        (crate::lsp_keys::Class::PhysDef, "DbgFlags") => Some(BITFIELD_DBGFLAGS),
        (crate::lsp_keys::Class::PhysDef, "LogFlags") => Some(BITFIELD_LOGFLAGS),
        (crate::lsp_keys::Class::Material, "Flags") => Some(BITFIELD_MATERIAL),
        _ => None,
    }
}
