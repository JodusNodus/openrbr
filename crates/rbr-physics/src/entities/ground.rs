//! CGround (`src/rbr/RBR_E43_GroundA.cpp`, `GroundB.cpp`, `GroundC.cpp`, `RBR_E44_Def.cpp`): the def's constant
//! slots (name, tweak-key dictionary), the flags that report the ground present, the def's construction, the
//! factory (`GroundDefNew9`), the ground's make (`CDef::Vf50`) and the box probe (`VfE8`). Their calls are
//! `ProbeCalls` and `Replace` (below); `layout` makes them and stores the results.

/// `CGround::CDef::Vf4c` (0x51ba20): the def's class name, "DefGround".
pub const DEF_NAME: u32 = 0x73cf0c; // string "DefGround"

/// `CGround::CDef::Unk20` (0x51bb00): the CGround::CDef tweak-key dictionary (notes/physics.md "Renames and tweak keys").
pub const TWEAK_DICT: u32 = 0x0165926c; // dict of NPhysics::CGround::CDef, 4 records

pub fn def_name() -> u32 {
    DEF_NAME
}

pub fn tweak_dict() -> u32 {
    TWEAK_DICT
}

/// `CGround::Vf88` (0x51bb60), `Vf110` (0x51bb70), `Vf114` (0x51bb80) and `VfCC` (0x51bc70, which ignores
/// its arguments): the ground is present.
pub fn present() -> bool {
    true
}

pub const DEF_VTABLE: u32 = 0x73CEAC; // CGround::CDef's vtable
pub const GROUND_VTABLE: u32 = 0x73CF1C; // CGround's vtable
pub const DEF_SIZE: u32 = 0x15C; // the def, allocated by the factory
pub const GROUND_SIZE: u32 = 0x64; // the ground, allocated by CDef::Vf50
pub const GROUND_OPTIONS: u32 = 0x80; // the options CDef::Vf50 gives the ground's CEntity
pub const SINGLETON: u32 = 0x0165_9268; // the ground singleton CDef::Vf50 keeps

/// A tweak member of the def, built by its constructor: `NTweak::CInteger` (0x52ff20) or `NTweak::CUnsigned`
/// (0x52fec0). Each writes two vtable words, then zero words (the value at +0xc). Their keys are not known
/// (rbr-recomp `notes/physics/ground.md` 2a, open 2), so they are named by type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TweakKind {
    Integer,
    Unsigned,
}

impl TweakKind {
    /// The two vtable words the constructor stores at +0 and +4 (`NTweak::CInteger`'s and `CUnsigned`'s).
    pub fn vtables(self) -> [u32; 2] {
        match self {
            TweakKind::Integer => [0x740594, 0x7405E0],  // NTweak::CInteger
            TweakKind::Unsigned => [0x740534, 0x740580], // NTweak::CUnsigned
        }
    }
}

/// The four tweak members of the def, by offset in the def (0x11c, 0x12c, 0x13c, 0x14c).
pub const TWEAK_MEMBERS: [(u32, TweakKind); 4] = [
    (0x11C, TweakKind::Integer),
    (0x12C, TweakKind::Integer),
    (0x13C, TweakKind::Unsigned),
    (0x14C, TweakKind::Unsigned),
];

/// The call `CDef::Vf50` makes on the ground it replaces: its deleting destructor (vtable slot 7, with 1).
pub trait Replace {
    fn delete(&mut self, ground: u32);
}

/// The part of the record `CGround::VfE8` writes on a hit that the port stores itself, from the host's 13 floats `out`:
/// words 4..15 (the first four are `E01Vec4::CopyFrom` of `out[0..4]`, the call the port makes). `owner` is the ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contact {
    /// Words 4..8: `out[4..8]`.
    pub second: [f32; 4],
    /// Words 8..11: `out[10]`, `out[11]`, `out[9]` (the order the asm stores them in).
    pub point: [f32; 3],
    /// Word 13: `out[12]`. Words 11 and 12 are cleared.
    pub depth: f32,
    /// Word 14: the ground (`this`).
    pub owner: u32,
}

/// The scatter of `CGround::VfE8` (0x51c6f0) on a hit, with `owner` the ground.
pub fn contact(out: &[f32; 13], owner: u32) -> Contact {
    Contact { second: [out[4], out[5], out[6], out[7]], point: [out[10], out[11], out[9]], depth: out[12], owner }
}

/// The calls `CGround::VfE8` makes: `E33Mat34::FetchX` on the transform at `this` (its own record: the walk), the host's
/// box query `S00` (on `host`, writes the 13 floats to `out`; returns whether it hit), and `E01Vec4::CopyFrom`.
pub trait ProbeCalls {
    fn fetch_x(&mut self, this: u32, out: u32, arg: u32) -> u32;
    fn query(&mut self, host: u32, t60: u32, t70: u32, out: u32) -> u32;
    fn copy_from(&mut self, to: u32, from: u32);
}

/// The scratch `CGround::VfE8` keeps on its stack: the two matrices (`t60`, `t70`, 16 bytes each) and the host's
/// 13-float output (`out`). Their addresses are the original's stack; the port takes them from its caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeScratch {
    pub t60: u32,
    pub t70: u32,
    pub out: u32,
}
