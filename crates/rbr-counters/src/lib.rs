//! Work counts for measuring the step and the frame: the calling thread's retired instructions and CPU cycles, and the
//! process's allocations. Unlike wall time, the instruction and allocation counts stay the same when other programs
//! load the machine or the work lands on an efficiency core. Instructions and cycles are macOS only (no root needed)
//! and 0 elsewhere; they include the kernel's work for the thread, so other load only ever adds a little. Allocations
//! count every thread's, when the binary installs [`Counting`]:
//! `#[global_allocator] static ALLOCATOR: rbr_counters::Counting = rbr_counters::Counting;`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::ops::AddAssign;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub instructions: u64,
    pub cycles: u64,
    /// Allocator calls (`alloc`, `alloc_zeroed`, `realloc`) and the bytes they asked for.
    pub allocations: u64,
    pub allocated: u64,
}

impl AddAssign for Counts {
    fn add_assign(&mut self, other: Counts) {
        self.instructions += other.instructions;
        self.cycles += other.cycles;
        self.allocations += other.allocations;
        self.allocated += other.allocated;
    }
}

/// The counts at one point, as [`std::time::Instant`] is for time. The instructions and cycles are the thread's, so
/// [`Mark::elapsed`] belongs on the thread that made the mark.
pub struct Mark {
    cpu: Option<[u64; 2]>,
    allocations: u64,
    allocated: u64,
}

impl Mark {
    pub fn now() -> Mark {
        Mark {
            cpu: cpu(),
            allocations: ALLOCATIONS.load(Ordering::Relaxed),
            allocated: ALLOCATED.load(Ordering::Relaxed),
        }
    }

    /// The counts since the mark; instructions and cycles 0 where they can't be read.
    pub fn elapsed(&self) -> Counts {
        let [instructions, cycles] = match (self.cpu, cpu()) {
            (Some([i0, c0]), Some([i1, c1])) => [i1 - i0, c1 - c0],
            _ => [0, 0],
        };
        Counts {
            instructions,
            cycles,
            allocations: ALLOCATIONS.load(Ordering::Relaxed) - self.allocations,
            allocated: ALLOCATED.load(Ordering::Relaxed) - self.allocated,
        }
    }
}

/// The calling thread's retired instructions and cycles: xnu's `thread_selfcounts`, looked up at run time since it is
/// not public API.
#[cfg(target_os = "macos")]
fn cpu() -> Option<[u64; 2]> {
    type SelfCounts = unsafe extern "C" fn(kind: i32, counts: *mut u64, size: usize) -> i32;
    static SELF_COUNTS: std::sync::OnceLock<Option<SelfCounts>> = std::sync::OnceLock::new();
    let self_counts = SELF_COUNTS
        .get_or_init(|| {
            // SAFETY: a lookup of a NUL-terminated name in the loaded images.
            let symbol = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"thread_selfcounts".as_ptr()) };
            // SAFETY: the symbol is `int thread_selfcounts(int type, void *buf, size_t nbytes)`.
            (!symbol.is_null()).then(|| unsafe { std::mem::transmute::<*mut libc::c_void, SelfCounts>(symbol) })
        })
        .as_ref()?;
    let mut counts = [0u64; 2];
    // SAFETY: kind 1 writes the thread's two fixed counters, instructions then cycles, into the 16 bytes it is given.
    (unsafe { self_counts(1, counts.as_mut_ptr(), size_of_val(&counts)) } == 0).then_some(counts)
}

#[cfg(not(target_os = "macos"))]
fn cpu() -> Option<[u64; 2]> {
    None
}

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED: AtomicU64 = AtomicU64::new(0);

/// The system allocator, counting its calls for [`Mark`].
pub struct Counting;

fn count(bytes: usize) {
    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    ALLOCATED.fetch_add(bytes as u64, Ordering::Relaxed);
}

// SAFETY: every call goes to `System` unchanged.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[global_allocator]
    static ALLOCATOR: Counting = Counting;

    #[test]
    fn counts_between_marks() {
        let mark = Mark::now();
        let sum: u64 = (0..1_000_000u64).map(std::hint::black_box).sum();
        let kept = std::hint::black_box(vec![0u8; 4096]);
        let counts = mark.elapsed();
        assert!(sum > 0 && kept.len() == 4096);
        // The test threads allocate too, so at least this one.
        assert!(counts.allocations >= 1 && counts.allocated >= 4096, "{counts:?}");
        if cfg!(target_os = "macos") {
            assert!(counts.instructions > 1_000_000 && counts.cycles > 0, "{counts:?}");
        }
    }
}
