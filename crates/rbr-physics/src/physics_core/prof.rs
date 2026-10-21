//! Profiler sample timing: the seconds an interval of performance-counter ticks makes and its reciprocal.

use crate::fpu::round24;

/// The seconds for `ticks` at `frequency` (ticks per second), and the reciprocal (per second): the x87 divides at 24
/// bits. The reciprocal is 0 without elapsed time.
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
