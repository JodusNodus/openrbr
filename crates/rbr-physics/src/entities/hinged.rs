//! CHingedRigidBody (`src/rbr/RBR_E44_*`): the def's constant slots, the unit triple of `E44_FillOnes`,
//! and the setter of the pointer at +0x650. The componentwise product `E44Shell::M51C8C0` is `Vec4::mul_lanes`.

/// `CHingedRigidBody::CDef::Vf4c` (0x51cc30): the def's class name, "DefHingedRigidBody".
pub const DEF_NAME: u32 = 0x73d20c; // string "DefHingedRigidBody"

/// `CHingedRigidBody::CDef::Unk20` (0x51cc40): the CHingedRigidBody::CDef tweak-key dictionary.
pub const TWEAK_DICT: u32 = 0x01659284; // dict of NPhysics::CHingedRigidBody::CDef, 30 records

pub fn def_name() -> u32 {
    DEF_NAME
}

pub fn tweak_dict() -> u32 {
    TWEAK_DICT
}

/// `E44_FillOnes` (0x51cd60): the three words it fills, each 1. Their meaning is unknown (see README).
pub fn unit_triple() -> [u32; 3] {
    [1; 3]
}
