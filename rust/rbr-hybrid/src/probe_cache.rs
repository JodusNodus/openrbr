//! The probe kernel's entities as last read (`E46_Kernel5bd0`, pure host only). The wheels' probes test each entity
//! in their cells (most of them static scenery) every step: its flags, grid and bounds are read once, kept with the
//! pages they were read from, and used while nothing has written those pages since ([`Pages::written_after`]). The
//! tests' calls write the words the pure host's calls write ([`crate::skip_call`]), so memory ends as the walk leaves
//! it.

use crate::collision::BODY_GRID;
use crate::entities::ENTITY_VF118;
use rbr_physics::collision::body::aabb_pretest;
use rbr_physics::collision::layout::{Vec4, Vec4Pair};
use rbr_physics::mem::{Mem, Pages};
use rbr_physics::physics_core::detectors::{kernel_skips, KernelTest};
use rbr_physics::physics_core::layout::{kernel_entity, KernelEntity};

/// `CBody::Vf118` and `CEntity::Vf118` (the grid getters the pure host reads in place), `CBody::VfCC` (the box test).
pub(crate) const BODY_OVERLAPS: u32 = 0x4e1230;

/// The kernel frame's pairs, the box test's arguments (`kernel_collect`'s `PAIR_LO` and `PAIR_HI`).
const PAIR_LO: u32 = 0x28;
const PAIR_HI: u32 = 0x20;

const SLOTS: usize = 4096;
const PAGE: u32 = 0x1000;

/// How far an entity's tests go on what is known of it.
#[derive(Clone, Copy, Default)]
enum Stage {
    /// A grid getter the pure host runs: only the flag test is known.
    #[default]
    Other,
    /// No grid, or its solid byte clear: the two grid getters run (each writes its return slot), then it fails.
    FailsAtGrid,
    /// Through the grid tests to `CBody::VfCC` on these bounds.
    Overlap(Vec4Pair),
    /// Through the grid tests to another box test.
    OverlapOther,
}

#[derive(Clone, Copy, Default)]
struct Entry {
    /// 0 for an empty slot.
    node: u32,
    next: u32,
    entity: u32,
    flags: u32,
    stage: Stage,
    /// The epoch the reads are of, and the pages they were read from (`pages[..n]`).
    epoch: u64,
    n: u8,
    pages: [u32; PAGES],
}

/// The most pages an entry's reads may come from.
const PAGES: usize = 6;

struct Cache {
    pages: Option<Pages>,
    entries: Box<[Entry]>,
}

thread_local! {
    static CACHE: std::cell::Cell<Option<Box<Cache>>> = const { std::cell::Cell::new(None) };
}

/// One kernel call's use of the cache (taken from the thread's while the call runs): the epoch its reads are of, once
/// it read one.
#[derive(Default)]
pub(crate) struct KernelCall {
    cache: Option<Box<Cache>>,
    epoch: Option<u64>,
}

impl Drop for KernelCall {
    fn drop(&mut self) {
        if let Some(cache) = self.cache.take() {
            CACHE.set(Some(cache));
        }
    }
}

impl KernelCall {
    /// The kernel's tests of the entity at `node` from what is known of it (`DetectorHost::kernel_tests`), in the
    /// kernel frame `frame`, with the pure host's call slot word `ret` and the box test's corners `corners` when its
    /// first call fetched them (`None` before). The entities after it that are known to fail are tested too: their
    /// tests write the same words, so the words the last of them wrote are written once. `None` when the tests of
    /// `node` must run.
    pub(crate) fn tests(
        &mut self,
        mem: &mut Mem,
        ret: u32,
        frame: u32,
        start: u32,
        (mask, excluded): (u32, u32),
        corners: impl FnOnce() -> Option<[Vec4; 2]>,
    ) -> Option<KernelTest> {
        let mut corners = Some(corners);
        let mut fetched = None;
        // The last word the tests wrote at `frame - 4` (a grid getter's return slot, or the box test's second corner),
        // and whether a box test wrote its first words.
        let mut at_4 = None;
        let mut boxed = false;
        let mut node = start;
        let stop = loop {
            if node == 0 {
                break None;
            }
            let Some(pages) = mem.pages_for(node, 16) else {
                break Some(None);
            };
            let Some(entry) = self.entry(mem, pages, node) else {
                break Some(None);
            };
            let (next, stage) = (entry.next, entry.stage);
            if kernel_skips(entry.flags, mask, entry.entity, excluded) {
                node = next;
                continue;
            }
            match stage {
                Stage::Other | Stage::OverlapOther => break Some(None),
                Stage::FailsAtGrid => at_4 = Some(ret),
                Stage::Overlap(bounds) => {
                    let Some([lo, hi]) = *fetched.get_or_insert_with(|| corners.take().and_then(|c| c())) else {
                        break Some(None);
                    };
                    boxed = true;
                    at_4 = Some(frame + PAIR_HI);
                    if aabb_pretest(bounds, lo, hi) {
                        break Some(Some(KernelTest::Pass));
                    }
                }
            }
            node = next;
        };
        if boxed {
            mem.set_u32(frame - 0xc, ret);
            mem.set_u32(frame - 8, frame + PAIR_LO);
        }
        if let Some(word) = at_4 {
            mem.set_u32(frame - 4, word);
        }
        match stop {
            // The node the walk stopped at is tested on its own (its tests run, or it passes).
            Some(outcome) if node == start => outcome,
            _ => Some(KernelTest::Fail { next: node }),
        }
    }

    /// The entry of `node`, read again when anything wrote its pages since it was read.
    fn entry(&mut self, mem: &Mem, pages: &Pages, node: u32) -> Option<&Entry> {
        let cache = self.cache.get_or_insert_with(|| CACHE.take().unwrap_or_else(new_cache));
        if !cache.pages.as_ref().is_some_and(|p| p.same(pages)) {
            cache.entries.fill(Entry::default());
            cache.pages = Some(pages.clone());
        }
        let k = (node / 16) as usize % SLOTS;
        let slot = &cache.entries[k];
        if slot.node == node && slot.pages[..slot.n as usize].iter().all(|&at| !pages.written_after(at, slot.epoch)) {
            return Some(&cache.entries[k]);
        }
        cache.entries[k].node = 0;
        let read = kernel_entity(mem, node)?;
        if !read.reads.iter().all(|&at| mem.pages_for(at, 1).is_some()) {
            return None;
        }
        let (n, read_pages) = distinct_pages(read.reads)?;
        let epoch = *self.epoch.get_or_insert_with(|| pages.end_epoch());
        cache.entries[k] = Entry {
            node,
            next: read.next,
            entity: read.entity,
            flags: read.flags,
            stage: stage(&read),
            epoch,
            n,
            pages: read_pages,
        };
        Some(&cache.entries[k])
    }
}

fn new_cache() -> Box<Cache> {
    Box::new(Cache { pages: None, entries: vec![Entry::default(); SLOTS].into_boxed_slice() })
}

/// The pages of `reads`, each once; `None` when there are more than an entry keeps.
fn distinct_pages(reads: [u32; 10]) -> Option<(u8, [u32; PAGES])> {
    let mut pages = [0; PAGES];
    let mut n = 0;
    for at in reads.map(|at| at & !(PAGE - 1)) {
        if !pages[..n].contains(&at) {
            *pages.get_mut(n)? = at;
            n += 1;
        }
    }
    Some((n as u8, pages))
}

fn stage(read: &KernelEntity) -> Stage {
    match read.slot_118 {
        BODY_GRID if read.grid == 0 || read.solid == 0 => Stage::FailsAtGrid,
        BODY_GRID => match (read.slot_cc, read.bounds) {
            (BODY_OVERLAPS, Some(bounds)) => Stage::Overlap(bounds),
            _ => Stage::OverlapOther,
        },
        ENTITY_VF118 => Stage::FailsAtGrid,
        _ => Stage::Other,
    }
}
