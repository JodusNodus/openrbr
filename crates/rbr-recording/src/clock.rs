//! The step clock record `rbr-standalone --live` writes after each frame's DMG1 record (a tagged record: tag, u32
//! length, body): what one physics step of the frame header's counters is in game time, and how many steps the load's
//! settle spent before the race (state 1). 1.02 steps 7 ms ([`crate::STEP_SECONDS`]) and settles 160 steps; NGP
//! 7.4.778 steps 1/720 s and settles 1520 (`notes/ngp.md`, "Stepping"). A reader without the record (`harness/live.py`,
//! a recording) takes 1.02's ([`StepClock::default`]). Little-endian body:
//!
//! ```text
//! f32 step     the image's `MaxTimeStep`, seconds per `StepForward(1)`
//! u32 settle   the load settle's steps (`StepForward(40)` frames until the load countdown is spent)
//! ```

use std::io;

/// The tag.
pub const CLK1: [u8; 4] = *b"CLK1";
/// The body's size.
pub const BODY_BYTES: usize = 8;

/// One step clock record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepClock {
    pub step: f32,
    pub settle: u32,
}

impl Default for StepClock {
    /// 1.02's: 7 ms steps after the four settle calls of 40.
    fn default() -> Self {
        StepClock { step: crate::STEP_SECONDS, settle: 160 }
    }
}

impl StepClock {
    /// Game seconds of `steps` physics steps.
    pub fn seconds(self, steps: u64) -> f64 {
        steps as f64 * f64::from(self.step)
    }

    /// The tagged record.
    pub fn record(&self) -> Vec<u8> {
        let mut body = [0; BODY_BYTES];
        body[..4].copy_from_slice(&self.step.to_le_bytes());
        body[4..].copy_from_slice(&self.settle.to_le_bytes());
        crate::live::record(CLK1, &body)
    }

    /// The record's body (the bytes after the tag and length).
    pub fn read_body(body: &[u8]) -> io::Result<StepClock> {
        let body: &[u8; BODY_BYTES] = crate::sized(body, format!("a CLK1 body of {} bytes", body.len()))?;
        Ok(StepClock { step: f32::from_bits(crate::word(body, 0)), settle: crate::word(body, 1) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_record_round_trips() {
        let clock = StepClock { step: f32::from_bits(0x3ab6_0b61), settle: 1520 };
        let bytes = clock.record();
        assert_eq!(&bytes[..4], b"CLK1");
        assert_eq!(bytes[4..8], 8u32.to_le_bytes());
        assert_eq!(StepClock::read_body(&bytes[8..]).unwrap(), clock);
        assert!(StepClock::read_body(&bytes[8..12]).is_err());
        assert_eq!(StepClock::default().seconds(1000), 1000.0 * f64::from(crate::STEP_SECONDS));
    }
}
