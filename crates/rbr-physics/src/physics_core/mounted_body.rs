//! The mounted body (`CMountedRigidBody`, `src/rbr/RBR_E45_MountedB.cpp`): `CtorBig` (0x521c30) and the def's `Vf50`
//! (0x521d20), which allocates the body (`BODY_SIZE` bytes) and runs `CtorBig` on it. The solid base, the name, the
//! scratch base and `E43_ExclA000` are the caller's (`BodyHost`); the state is `CState::Ctor` (`mounted_ctor`), also
//! the caller's.

use super::construct::Heap;
use super::layout;
use crate::mem::Mem;

/// `CMountedRigidBody`'s size (the block `Vf50` allocates).
pub const BODY_SIZE: u32 = 0x590;
/// The body's vtable (0x73e62c), at +0x00, and its scratch's (0x73e61c), at `SCRATCH_AT`.
const BODY_VTABLE: u32 = 0x0073_e62c;
const SCRATCH_VTABLE: u32 = 0x0073_e61c;
/// The state (`CState`), the scratch, the owner and a word, at these offsets of the body.
const STATE_AT: u32 = 0x3d4;
const OWNER_AT: u32 = 0x3d0;
const SCRATCH_AT: u32 = 0x410;
const WORD_AT: u32 = 0x584;
/// The owner's flags (+0x60 of the owner): the owner counts only when bit 3 is set.
const OWNER_FLAGS_AT: u32 = 0x60;
const OWNER_FLAG: u8 = 0x8;

/// The calls `CtorBig` makes into code outside this port, in the original's order. Each gets the address of the object
/// it builds, as the original passes it.
pub trait BodyHost {
    /// `CSolid::CSolid(def, a, b, flags)` (0x52d480) on the body at `at`: the solid base.
    fn solid_base(&mut self, mem: &mut Mem, at: u32, def: u32, a: u32, b: u32, flags: u32);
    /// `NTweak::CBase::GetNameOrNull(base)` (0x52f0b0): the base's name, or 0.
    fn base_name(&mut self, mem: &mut Mem, base: u32) -> u32;
    /// `CState::Ctor(state, name, c)` (0x5215a0) on the state at `at`, for the body `this`.
    fn state_ctor(&mut self, mem: &mut Mem, at: u32, this: u32, name: u32, c: u32);
    /// `E47_ScratchBaseInit` (0x52b930) on the scratch at `at`.
    fn scratch_base(&mut self, mem: &mut Mem, at: u32);
    /// `CEntity::E43_ExclA000(n)` (0x51a000) on the body at `at`.
    fn exclusion(&mut self, mem: &mut Mem, at: u32, n: u32);
}

/// `CMountedRigidBody::CtorBig(a, b, c, d)` (0x521c30) on the body at `this`: the solid base (with `d` and 0x800),
/// the vptr, the name of `a` (when `a` is set), the state, the scratch base and its vptr, a zero word, the owner (`c`
/// when it has the flag, else 0), and the exclusion `E43_ExclA000(2)`.
pub fn construct_body(mem: &mut Mem, host: &mut impl BodyHost, this: u32, a: u32, b: u32, c: u32, d: u32) {
    host.solid_base(mem, this, a, b, c, d | 0x800);
    layout::set_word(mem, this, BODY_VTABLE);
    let name = if a != 0 { host.base_name(mem, a) } else { 0 };
    host.state_ctor(mem, this + STATE_AT, this, name, c);
    host.scratch_base(mem, this + SCRATCH_AT);
    layout::set_word(mem, this + SCRATCH_AT, SCRATCH_VTABLE);
    layout::set_word(mem, this + WORD_AT, 0);
    let owner = if c != 0 && layout::byte(mem, c + OWNER_FLAGS_AT) & OWNER_FLAG != 0 { c } else { 0 };
    layout::set_word(mem, this + OWNER_AT, owner);
    host.exclusion(mem, this, 2);
}

/// `CMountedRigidBody::CDef::Vf50(a, b)` (0x521d20) on the def `def`: a body of `BODY_SIZE` bytes, built as
/// `CtorBig(def, a, b, 0)`. Returns the body, or 0 when the allocation fails.
pub fn new_body(mem: &mut Mem, host: &mut (impl Heap + BodyHost), def: u32, a: u32, b: u32) -> u32 {
    let at = host.alloc(mem, BODY_SIZE);
    if at != 0 {
        construct_body(mem, host, at, def, a, b, 0);
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host that makes no calls of its own (their bytes are the caller's), names every base 0x77, and allocates
    /// `block` (0 for none).
    struct Silent {
        block: u32,
    }

    impl Heap for Silent {
        fn alloc(&mut self, _: &mut Mem, _: u32) -> u32 {
            self.block
        }
    }

    impl BodyHost for Silent {
        fn solid_base(&mut self, _: &mut Mem, _: u32, _: u32, _: u32, _: u32, _: u32) {}

        fn base_name(&mut self, _: &mut Mem, _: u32) -> u32 {
            0x77
        }

        fn state_ctor(&mut self, _: &mut Mem, _: u32, _: u32, _: u32, _: u32) {}

        fn scratch_base(&mut self, _: &mut Mem, _: u32) {}

        fn exclusion(&mut self, _: &mut Mem, _: u32, _: u32) {}
    }

    const BODY: u32 = 0x1000;
    const OWNER: u32 = 0x3000;

    /// The owner word of a body built on `c`, whose flags byte (+0x60) is `flags`.
    fn owner_of(c: u32, flags: u8) -> u32 {
        let mut mem = Mem::new();
        mem.map(BODY, &[0; BODY_SIZE as usize]);
        let mut owner = vec![0; 0x64];
        owner[OWNER_FLAGS_AT as usize] = flags;
        mem.map(OWNER, &owner);
        construct_body(&mut mem, &mut Silent { block: 0 }, BODY, 0x2000, 0, c, 0);
        mem.u32(BODY + OWNER_AT)
    }

    #[test]
    fn owner_counts_with_flag_eight() {
        assert_eq!(owner_of(OWNER, OWNER_FLAG), OWNER);
    }

    #[test]
    fn owner_without_flag_eight_is_zero() {
        assert_eq!(owner_of(OWNER, 0x7), 0);
    }

    #[test]
    fn no_owner_is_zero() {
        assert_eq!(owner_of(0, OWNER_FLAG), 0);
    }

    #[test]
    fn new_body_without_a_block_returns_zero() {
        let mut mem = Mem::new();
        assert_eq!(new_body(&mut mem, &mut Silent { block: 0 }, BODY, 0, 0), 0);
    }
}
