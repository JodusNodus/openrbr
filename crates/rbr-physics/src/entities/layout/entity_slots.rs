//! The entities' small slots: the `CEntity` hooks and flags, `CDummyObject`'s box and span test, the exclusion set
//! and `CCar::Clear480`.

use crate::entities::exclusion::{Excluder, Members};
use crate::mem::Mem;

/// A hook that answers 0 (`CEntity` 0x519ac0: `xor eax, eax; ret`).
pub fn zero_hook() -> u32 {
    0
}

/// `CEntity::VfCC` (0x519e90, slot 51, ret 8): the entity answers 1 whatever its two arguments are.
pub fn vf_cc() -> u32 {
    1
}

/// `CEntity::VfD0` (0x519ea0, slot 52, ret 12): the entity answers 0 whatever its three arguments are.
pub fn vf_d0() -> u32 {
    0
}

/// The exclusion set (`+0x2c`, `[count, members...]`) and the parent (`+0x44`) of a CEntity (`ExclusionTest`).
pub(super) const EXCL_SET: u32 = 0x2c;
pub(super) const PARENT: u32 = 0x44;

/// The entity at `at` as the exclusion test sees it.
pub fn load_excluder(mem: &Mem, at: u32) -> Excluder<SetInMem<'_>> {
    let set = mem.u32(at + EXCL_SET);
    let members = (set != 0).then(|| SetInMem { mem, set, count: mem.u32(set) });
    Excluder { handle: at, parent: mem.u32(at + PARENT), members }
}

/// An exclusion set read where it is (`[count, members...]` at `set`) instead of copied: the step tests dozens.
pub struct SetInMem<'a> {
    mem: &'a Mem,
    set: u32,
    count: u32,
}

impl Members for SetInMem<'_> {
    fn count(&self) -> u32 {
        self.count
    }

    fn holds(&self, entity: u32) -> bool {
        (1..=self.count).any(|i| self.mem.u32(self.set + 4 * i) == entity)
    }
}
