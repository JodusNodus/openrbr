//! The C runtime's routines the game's gameplay code depends on, as msvcr71 runs them: `rand()` and `qsort`.

/// The CRT's `rand()` (msvcr71, IAT 0x729540; MSVC: `seed * 214013 + 2531011`, bits 16-30): the thread's linear
/// congruential state, 15-bit results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrtRand(pub u32);

impl CrtRand {
    /// `srand(seed)`. The game seeds it with `time(NULL)` at startup (`CPC_Application::vf04`, 0x44c890, after
    /// `startup.lua`) and again in `InitQuickRaceDamageHandler` (0x567640), after the weather is drawn.
    pub fn srand(seed: u32) -> CrtRand {
        CrtRand(seed)
    }

    pub fn draw(&mut self) -> i32 {
        self.0 = self.0.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.0 >> 16) & 0x7FFF) as i32
    }
}

/// msvcr71's `qsort` (0x7c36695f, retail `Root/msvcr71.dll`) on one-word elements: up to 8 a max-selection
/// (`shortsort` 0x7c3668fb), else a median of three and the partition that keeps track of the middle element, the
/// larger side kept for later. The order of equal elements is this algorithm's.
pub(crate) fn crt_qsort<T: Copy>(v: &mut [T], mut cmp: impl FnMut(T, T) -> i32) {
    if v.len() < 2 {
        return;
    }
    let mut c = |v: &[T], a: isize, b: isize| cmp(v[a as usize], v[b as usize]);
    let swap = |v: &mut [T], a: isize, b: isize| v.swap(a as usize, b as usize);
    let mut stack = Vec::new();
    let (mut lo, mut hi) = (0isize, v.len() as isize - 1);
    loop {
        let size = hi - lo + 1;
        let mut next = None;
        if size <= 8 {
            let mut top = hi;
            while top > lo {
                let mut max = lo;
                for p in lo + 1..=top {
                    if c(v, p, max) > 0 {
                        max = p;
                    }
                }
                swap(v, max, top);
                top -= 1;
            }
        } else {
            let mut mid = lo + size / 2;
            if c(v, lo, mid) > 0 {
                swap(v, lo, mid);
            }
            if c(v, lo, hi) > 0 {
                swap(v, lo, hi);
            }
            if c(v, mid, hi) > 0 {
                swap(v, mid, hi);
            }
            let (mut loguy, mut higuy) = (lo, hi);
            loop {
                if mid > loguy {
                    loguy += 1;
                    while loguy < mid && c(v, loguy, mid) <= 0 {
                        loguy += 1;
                    }
                }
                if mid <= loguy {
                    loguy += 1;
                    while loguy <= hi && c(v, loguy, mid) <= 0 {
                        loguy += 1;
                    }
                }
                higuy -= 1;
                while higuy > mid && c(v, higuy, mid) > 0 {
                    higuy -= 1;
                }
                if higuy < loguy {
                    break;
                }
                swap(v, loguy, higuy);
                if mid == higuy {
                    mid = loguy;
                }
            }
            higuy += 1;
            if mid < higuy {
                higuy -= 1;
                while higuy > mid && c(v, higuy, mid) == 0 {
                    higuy -= 1;
                }
            }
            if mid >= higuy {
                higuy -= 1;
                while higuy > lo && c(v, higuy, mid) == 0 {
                    higuy -= 1;
                }
            }
            if higuy - lo >= hi - loguy {
                if lo < higuy {
                    stack.push((lo, higuy));
                }
                if loguy < hi {
                    next = Some((loguy, hi));
                }
            } else {
                if loguy < hi {
                    stack.push((loguy, hi));
                }
                if lo < higuy {
                    next = Some((lo, higuy));
                }
            }
        }
        match next.or_else(|| stack.pop()) {
            Some(range) => (lo, hi) = range,
            None => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crt_qsort_sorts() {
        let mut r = CrtRand(5);
        for n in 0..60 {
            let mut v: Vec<i32> = (0..n).map(|_| r.draw() % 7).collect();
            let mut want = v.clone();
            want.sort();
            crt_qsort(&mut v, |a, b| (a > b) as i32 - (a < b) as i32);
            assert_eq!(v, want);
        }
    }
}
