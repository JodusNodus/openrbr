//! `ProfExit` (exe 0x528690): ends a profiler sample. It loads the sample from its handle and tail-jumps to
//! `E46_ProfCore5b0` (0x5285b0), which reads the performance counter and stores the elapsed ticks and seconds.

use crate::fpu::round24;

/// The Windows call the sample makes. Returns the eax the call leaves (`QueryPerformanceCounter`'s BOOL).
pub trait ProfHost {
    /// `QueryPerformanceCounter` (stdcall, the pointer to the 64-bit result) into the word at `at`.
    fn counter(&mut self, at: u32) -> u32;
}

/// The calls `EnterName` makes besides the counter: `CLink::Unlink` (0x47d160, thiscall on the sample) takes a linked
/// sample out of its queue first.
pub trait EnterHost: ProfHost {
    fn unlink(&mut self, sample: u32);
}

/// The seconds for `ticks` at `frequency`, and the reciprocal: the x87 divides at 24 bits.
pub fn timing(ticks: i64, frequency: i64) -> (f64, f64) {
    let seconds = round24(ticks as f64 / frequency as f64);
    let rate = if seconds > 0.0 { round24(1.0 / seconds) } else { 0.0 };
    (seconds, rate)
}

#[cfg(test)]
mod tests {
    use super::timing;

    #[test]
    fn rate_is_zero_without_elapsed_time() {
        assert_eq!(timing(0, 1000), (0.0, 0.0));
        assert_eq!(timing(-5, 1000).1, 0.0);
    }

    #[test]
    fn rate_is_the_reciprocal_of_the_seconds() {
        assert_eq!(timing(500, 1000), (0.5, 2.0));
    }
}
