//! The classes the entity layer builds in the game: the vtables, names, sizes and tweak dictionaries of the defs and
//! objects, and the singleton of the ground. `CHingedRigidBody::CDef` and `CMaterial` give their name and dictionary to
//! the game's reflection calls (`Vf4c`, `Unk20`), which the hybrid answers from here.

pub use crate::ntweak::layout::BASE_VTABLE;

/// `E43EntityDef`'s vtable after construction (0x73cc04); after release the def's vtable is `BASE_VTABLE` (0x72e444).
pub const ENTITY_DEF_VTABLE: u32 = 0x73CC04;

/// `CGround::CDef::Vf4c` (0x51ba20): the def's class name, "DefGround".
pub const GROUND_DEF_NAME: u32 = 0x73cf0c;
/// `CGround::CDef::Unk20` (0x51bb00): the tweak-key dictionary of `NPhysics::CGround::CDef`, 4 records.
pub const GROUND_TWEAK_DICT: u32 = 0x0165926c;
pub const GROUND_DEF_VTABLE: u32 = 0x73CEAC;
pub const GROUND_VTABLE: u32 = 0x73CF1C;
/// The def, allocated by the factory (`CGround::CDef::New9`).
pub const GROUND_DEF_SIZE: u32 = 0x15C;
/// The ground, allocated by `CGround::CDef::Vf50`.
pub const GROUND_SIZE: u32 = 0x64;
/// The options `CGround::CDef::Vf50` gives the ground's `CEntity`.
pub const GROUND_OPTIONS: u32 = 0x80;
/// The ground singleton `CGround::CDef::Vf50` keeps.
pub const GROUND_SINGLETON: u32 = 0x0165_9268;

/// `CHingedRigidBody::CDef::Vf4c` (0x51cc30): the def's class name, "DefHingedRigidBody".
pub const HINGED_DEF_NAME: u32 = 0x73d20c;
/// `CHingedRigidBody::CDef::Unk20` (0x51cc40): the tweak-key dictionary of `NPhysics::CHingedRigidBody::CDef`, 30 records.
pub const HINGED_TWEAK_DICT: u32 = 0x01659284;
/// `CMaterial::Unk20` (0x520820): the tweak-key dictionary of `NPhysics::CMaterial`.
pub const MATERIAL_TWEAK_DICT: u32 = 0x01659aec;

/// A tweak member of the ground's def, built by its constructor: `NTweak::CInteger` (0x52ff20) or `NTweak::CUnsigned`
/// (0x52fec0). Each writes two vtable words, then zero words (the value at +0xc). The keys are `Left`, `Top` (integers)
/// and `Width`, `Height` (unsigned): `data/physics-lsp-fields.md`, "CGround::CDef".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TweakKind {
    Integer,
    Unsigned,
}

impl TweakKind {
    /// The two vtable words the constructor stores at +0 and +4.
    pub fn vtables(self) -> [u32; 2] {
        match self {
            TweakKind::Integer => [0x740594, 0x7405E0],
            TweakKind::Unsigned => [0x740534, 0x740580],
        }
    }
}

/// The four tweak members of the ground's def, by offset in the def: `Left`, `Top`, `Width`, `Height`.
pub const GROUND_TWEAK_MEMBERS: [(u32, TweakKind); 4] = [
    (0x11C, TweakKind::Integer),
    (0x12C, TweakKind::Integer),
    (0x13C, TweakKind::Unsigned),
    (0x14C, TweakKind::Unsigned),
];
