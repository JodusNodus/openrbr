//! The constructors of the mounted rigid body (`src/rbr/RBR_E45_MountedA.cpp`, `RBR_E45_MountedB.cpp`): the def's
//! `CtorA` (0x521480) and its `NewDef` factory (0x521b50), the state's `CState::Ctor` (0x5215a0) and the def's `Vf4c`
//! (0x521500). The solid base and the NTweak members are built by the caller's constructors (`MountedHost`).

use super::construct::Heap;
use crate::mem::Mem;
use crate::physics_core::layout;

/// `CMountedRigidBody::CDef`'s size (the block `NewDef` allocates).
pub const DEF_SIZE: u32 = 0x240;
/// `CMountedRigidBody::CDef`'s vtable (0x73e59c), at +0x00.
const DEF_VTABLE: u32 = 0x0073_e59c;
/// `CMountedRigidBody::CState`'s vtable (0x73e614), at +0x00.
const STATE_VTABLE: u32 = 0x0073_e614;
/// The string `CMountedRigidBody::CDef::Vf4c` returns (0x73e5fc).
const VF4C_RESULT: u32 = 0x0073_e5fc;

/// The calls the mounted constructors make into code outside this port, in the original's order. Each gets the
/// address of the object it builds; the caller's code writes those objects' bytes.
pub trait MountedHost {
    /// `CSolid::CDef::CDef(name)` (0x529840) on the def at `at`: the solid base.
    fn solid_def(&mut self, mem: &mut Mem, at: u32, name: u32);
    /// `NTweak::CUnsigned::CUnsigned` (0x52fec0) on the tweak at `at`.
    fn unsigned(&mut self, mem: &mut Mem, at: u32);
}

/// `CMountedRigidBody::CDef::CtorA(name)` (0x521480) on the def at `at`: the solid base, the vtable, then the tweaks at
/// +0x1f0 (unsigned), +0x200 (vector) and +0x230 (unsigned).
pub fn construct_def(mem: &mut Mem, host: &mut impl MountedHost, at: u32, name: u32) {
    host.solid_def(mem, at, name);
    layout::set_word(mem, at, DEF_VTABLE);
    host.unsigned(mem, at + 0x1f0);
    crate::ntweak::construct_vector(mem, at + 0x200);
    host.unsigned(mem, at + 0x230);
}

/// `CMountedRigidBody::NewDef(name)` (0x521b50): a def of `DEF_SIZE` bytes built with `name`. Returns the def, or 0
/// when the allocation fails.
pub fn new_def(mem: &mut Mem, host: &mut (impl Heap + MountedHost), name: u32) -> u32 {
    let at = host.alloc(mem, DEF_SIZE);
    if at != 0 {
        construct_def(mem, host, at, name);
    }
    at
}

/// `CMountedRigidBody::CState::Ctor(a, b, c)` (0x5215a0) on the state at `at`: the vtable, then three `CReal`s at +4,
/// +0x14 and +0x24. The three arguments are not used.
pub fn construct_state(mem: &mut Mem, at: u32) {
    layout::set_word(mem, at, STATE_VTABLE);
    crate::ntweak::construct_real(mem, at + 0x4);
    crate::ntweak::construct_real(mem, at + 0x14);
    crate::ntweak::construct_real(mem, at + 0x24);
}

/// `CMountedRigidBody::CDef::Vf4c` (0x521500): the constant string's address.
pub fn vf4c() -> u32 {
    VF4C_RESULT
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A heap with no block: the allocation returns 0, and nothing else is called.
    struct NoBlock;

    impl Heap for NoBlock {
        fn alloc(&mut self, _: &mut Mem, _: u32) -> u32 {
            0
        }
    }

    impl MountedHost for NoBlock {
        fn solid_def(&mut self, _: &mut Mem, _: u32, _: u32) {
            panic!("no def is built without a block");
        }

        fn unsigned(&mut self, _: &mut Mem, _: u32) {
            panic!("no tweak is built without a block");
        }
    }

    #[test]
    fn new_def_without_a_block_returns_zero() {
        let mut mem = Mem::new();
        assert_eq!(new_def(&mut mem, &mut NoBlock, 0x1234), 0);
    }
}
