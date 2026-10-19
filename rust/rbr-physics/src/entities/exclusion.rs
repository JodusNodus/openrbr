//! The exclusion set of a CEntity (`+0x2c`: `[count, members...]`, allocated once). `ExclusionTest` decides
//! whether two entities may meet, from the smaller of their sets; `AllocExclusion` allocates the set
//! (src/rbr/RBR_E43_EntityC.cpp:220-282, asm 0x519f80 and 0x51a000).

/// An entity as the exclusion test sees it: its handle (its address), its parent (0 for none) and its set
/// (the members; `None` until the set is allocated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Excluder {
    pub handle: u32,
    pub parent: u32,
    pub members: Option<Vec<u32>>,
}

/// The one call the test makes on another entity: the parent's test (`parent.ExclusionTest(other)`).
pub trait ParentTest {
    fn parent_test(&mut self, parent: u32, other: u32) -> u32;
}

/// `CEntity::E43_ExclF80` (0x519f80): 0 when the parent's test is 0, when both are the same entity, or when
/// the smaller of the two sets holds the entity on the other side; else 1. A missing set gives 1.
pub fn exclusion_test(t: &mut impl ParentTest, this: &Excluder, other: &Excluder) -> u32 {
    if this.parent != 0 && t.parent_test(this.parent, other.handle) == 0 {
        return 0;
    }
    let (Some(mine), Some(theirs)) = (&this.members, &other.members) else {
        return 1;
    };
    if this.handle == other.handle {
        return 0;
    }
    // The smaller set is searched for the entity on the other side; on a tie, other's set is searched for this.
    let found = if mine.len() < theirs.len() { mine.contains(&other.handle) } else { theirs.contains(&this.handle) };
    u32::from(!found)
}

/// Asks the game for `bytes` of memory (`operator new`, the CRT allocator: cdecl, one size argument).
pub trait Heap {
    fn allocate(&mut self, bytes: u32) -> u32;
}

/// `CEntity::E43_ExclA000` (0x51a000): with no set yet, allocates `n` words for it and returns the array (the
/// caller stores it and the count 0); with a set already, returns `None` and allocates nothing.
pub fn alloc_exclusion(heap: &mut impl Heap, has_set: bool, n: u32) -> Option<u32> {
    (!has_set).then(|| heap.allocate(n * 4))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The parent's test with a fixed answer; panics if asked when no parent is set.
    struct Parent(u32);

    impl ParentTest for Parent {
        fn parent_test(&mut self, _parent: u32, _other: u32) -> u32 {
            self.0
        }
    }

    fn excl(handle: u32, parent: u32, members: &[u32]) -> Excluder {
        Excluder { handle, parent, members: Some(members.to_vec()) }
    }

    #[test]
    fn smaller_set_decides_both_ways() {
        // 10 is not in the other's (smaller) set, 40 is not in 10's: free.
        let (this, other) = (excl(10, 0, &[20, 30]), excl(40, 0, &[50]));
        assert_eq!(exclusion_test(&mut Parent(1), &this, &other), 1);
        // On a tie, other's set is searched for this entity: found, so excluded.
        let (this, other) = (excl(10, 0, &[60]), excl(40, 0, &[10]));
        assert_eq!(exclusion_test(&mut Parent(1), &this, &other), 0);
        // The smaller set is this one: other is searched in it.
        let (this, other) = (excl(10, 0, &[40]), excl(40, 0, &[11, 12]));
        assert_eq!(exclusion_test(&mut Parent(1), &this, &other), 0);
    }

    #[test]
    fn parent_refusal_and_missing_sets() {
        let (this, other) = (excl(10, 7, &[60]), excl(40, 0, &[]));
        assert_eq!(exclusion_test(&mut Parent(0), &this, &other), 0);
        let (this, other) = (Excluder { handle: 10, parent: 0, members: None }, excl(40, 0, &[]));
        assert_eq!(exclusion_test(&mut Parent(1), &this, &other), 1);
    }

    #[test]
    fn allocation_only_without_a_set() {
        struct NoHeap;
        impl Heap for NoHeap {
            fn allocate(&mut self, _: u32) -> u32 {
                panic!("a set exists: nothing is allocated")
            }
        }
        assert_eq!(alloc_exclusion(&mut NoHeap, true, 8), None);
    }
}
