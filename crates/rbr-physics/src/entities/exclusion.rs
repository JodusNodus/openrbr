//! The exclusion set of an entity (a count and the members, allocated once). The exclusion test decides whether two
//! entities may meet, from the smaller of their sets; the allocation makes the set.

use super::store::EntityStore;

/// An entity as the exclusion test sees it: its handle (its address), its parent (0 for none) and its set
/// (`None` until the set is allocated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Excluder<M = Vec<u32>> {
    pub handle: u32,
    pub parent: u32,
    pub members: Option<M>,
}

/// An exclusion set as the test reads it: its member count and whether it holds an entity.
pub trait Members {
    fn count(&self) -> u32;
    fn holds(&self, entity: u32) -> bool;
}

impl Members for Vec<u32> {
    fn count(&self) -> u32 {
        self.len() as u32
    }

    fn holds(&self, entity: u32) -> bool {
        self.contains(&entity)
    }
}

/// The one call the test makes on another entity: the parent's own exclusion test.
pub trait ParentTest {
    fn parent_test(&mut self, parent: u32, other: u32) -> u32;
}

/// The exclusion test: 0 when the parent's test is 0, when both are the same entity, or when
/// the smaller of the two sets holds the entity on the other side; else 1. A missing set gives 1.
pub fn exclusion_test<M: Members>(t: &mut impl ParentTest, this: &Excluder<M>, other: &Excluder<M>) -> u32 {
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
    let found = if mine.count() < theirs.count() { mine.holds(other.handle) } else { theirs.holds(this.handle) };
    u32::from(!found)
}

/// Asks the game for `bytes` of memory.
pub trait Heap {
    fn allocate(&mut self, bytes: u32) -> u32;
}

/// The allocation: with no set yet, allocates `n` words for it and returns the array (the
/// caller stores it and the count 0); with a set already, returns `None` and allocates nothing.
pub fn alloc_exclusion(heap: &mut impl Heap, has_set: bool, n: u32) -> Option<u32> {
    (!has_set).then(|| heap.allocate(n * 4))
}

/// Allocates the entity's exclusion set at `at` from `heap` (the array's pointer, then its count 0).
/// Returns 1 when it allocated, 0 when the entity had a set.
pub fn alloc_exclusion_at<S: EntityStore>(store: &mut S, at: u32, n: u32, heap: &mut impl Heap) -> u32 {
    let has_set = store.exclusion_set(at) != 0;
    match alloc_exclusion(heap, has_set, n) {
        Some(array) => {
            store.store_exclusion_set(at, array);
            store.store_exclusion_count(array, 0);
            1
        }
        None => 0,
    }
}

/// Appends `member` to the entity's set at `at` (it does not check the array's size). Returns 1.
pub fn add_exclusion_at<S: EntityStore>(store: &mut S, at: u32, member: u32) -> u32 {
    let set = store.exclusion_set(at);
    let count = store.exclusion_count(set);
    store.store_exclusion_member(set, count, member);
    store.store_exclusion_count(set, count + 1);
    1
}

/// Removes the first occurrence from the inline count-and-members array.
pub fn remove_exclusion_at<S: EntityStore>(store: &mut S, this: u32, value: u32) {
    let array = store.exclusion_set(this);
    if array == 0 {
        return;
    }
    let count = store.exclusion_count(array);
    let mut found = false;
    for i in 0..count {
        if found {
            let next = store.exclusion_member(array, i);
            store.store_exclusion_member(array, i - 1, next);
        }
        if store.exclusion_member(array, i) == value {
            found = true;
        }
    }
    if found {
        store.store_exclusion_member(array, count - 1, 0);
        store.store_exclusion_count(array, count.wrapping_sub(1));
    }
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
