//! Load-time work spread over the machine's threads.

use std::sync::atomic::{AtomicUsize, Ordering};

/// `f` of each of `items`, in order, over the machine's threads. The items go out in small parts, each to the next free
/// thread, so that a run of heavy ones (a stage's large meshes) does not hold one thread up.
pub(crate) fn in_parallel<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    in_parallel_with(items, || (), |(), item| f(item))
}

/// [`in_parallel`] with a state each thread makes with `state` and passes to `f` (its own file handles).
pub(crate) fn in_parallel_with<T: Sync, S, R: Send>(
    items: &[T],
    state: impl Fn() -> S + Sync,
    f: impl Fn(&mut S, &T) -> R + Sync,
) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let parts: Vec<&[T]> = items.chunks(items.len().div_ceil(threads * 16).max(1)).collect();
    let next = AtomicUsize::new(0);
    let mut done: Vec<(usize, Vec<R>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads.min(parts.len()))
            .map(|_| {
                scope.spawn(|| {
                    let (mut own, mut mine) = (state(), Vec::new());
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(part) = parts.get(i) else { break mine };
                        mine.push((i, part.iter().map(|item| f(&mut own, item)).collect::<Vec<_>>()));
                    }
                })
            })
            .collect();
        workers.into_iter().flat_map(|worker| worker.join().expect("a load thread")).collect()
    });
    done.sort_unstable_by_key(|&(i, _)| i);
    done.into_iter().flat_map(|(_, results)| results).collect()
}
