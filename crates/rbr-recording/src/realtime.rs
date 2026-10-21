//! The real-time live protocol (`rbr-standalone --live --realtime`, ours): the stepping process runs the game on its
//! own clock, one frame per physics step, and the viewer sends its input when it changes instead of one payload per
//! period. All values little-endian.
//!
//! The viewer writes 40-byte messages: a tag, a sequence number and a 32-byte input payload (`crate::input`, its
//! `frames` unused):
//!
//! ```text
//! u8  tag[4]    INPUT: the newest input; PAUSE: the clock stops (the pause menu); RESUME: it goes on from now
//! u32 seq       the input's number (the viewer's count), which the steps that take it echo
//! u8  payload[0x20]
//! ```
//!
//! The first message is an INPUT, answered with the first frame as in the paced protocol; the clock starts at the
//! first RESUME. The process answers every 7 ms of game time with a frame and its records as the paced protocol does
//! (`crate::live`), and after every frame that stepped with a [`Step`] record, so the force and the car's control
//! reach the viewer per step.
//!
//! `--frames-out FILE` logs every frame the clock ran as a [`Logged`] record after [`LOG_MAGIC`], and `--replay-frames
//! FILE` steps them again: the frames are the drive, so the replay is the live drive bit for bit.

use crate::CONTROL_BYTES;

/// The viewer's message tags.
pub const INPUT: [u8; 4] = *b"RTI1";
pub const PAUSE: [u8; 4] = *b"RTP1";
pub const RESUME: [u8; 4] = *b"RTR1";
/// Bytes of a viewer's message.
pub const MESSAGE_BYTES: usize = 8 + CONTROL_BYTES;

/// One message of the viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Message {
    pub tag: [u8; 4],
    pub seq: u32,
    pub payload: [u8; CONTROL_BYTES],
}

impl Message {
    pub fn to_bytes(&self) -> [u8; MESSAGE_BYTES] {
        let mut out = [0; MESSAGE_BYTES];
        out[..4].copy_from_slice(&self.tag);
        out[4..8].copy_from_slice(&self.seq.to_le_bytes());
        out[8..].copy_from_slice(&self.payload);
        out
    }

    /// Reads a message; an error for an unknown tag.
    pub fn read(bytes: &[u8; MESSAGE_BYTES]) -> Result<Message, String> {
        let tag: [u8; 4] = bytes[..4].try_into().unwrap();
        if ![INPUT, PAUSE, RESUME].contains(&tag) {
            return Err(format!("a real-time message tagged {tag:02x?}"));
        }
        Ok(Message {
            tag,
            seq: u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            payload: bytes[8..].try_into().unwrap(),
        })
    }
}

/// The tag of the per-step record, in place of a frame's magic.
pub const STP1: [u8; 4] = *b"STP1";
/// Bytes of a step record after its tag.
pub const STEP_BYTES: usize = 16 + CONTROL_BYTES;
/// [`Step::flags`]: the physics is NGP's (its rounding and dead zone apply to the force, `crate::ffb`).
pub const STEP_NGP: u32 = 1;

/// What a frame that stepped gives the wheel and the crew:
///
/// ```text
/// u8  tag[4]    STP1
/// u32 step      the step after the frame
/// u32 seq       the newest input the frame took (`Message::seq`)
/// f32 force     the steering force after the frame (`crate::ffb`)
/// u32 flags     STEP_NGP
/// u8  control[0x20]  the car's control the frame's last step took (`crate::live::CTL1`)
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Step {
    pub step: u32,
    pub seq: u32,
    pub force: f32,
    pub flags: u32,
    pub control: [u8; CONTROL_BYTES],
}

impl Step {
    /// The record with its tag.
    pub fn to_bytes(&self) -> [u8; 4 + STEP_BYTES] {
        let mut out = [0; 4 + STEP_BYTES];
        out[..4].copy_from_slice(&STP1);
        for (i, word) in [self.step, self.seq, self.force.to_bits(), self.flags].into_iter().enumerate() {
            out[4 + 4 * i..8 + 4 * i].copy_from_slice(&word.to_le_bytes());
        }
        out[20..].copy_from_slice(&self.control);
        out
    }

    /// Reads the bytes after the tag.
    pub fn from_body(body: &[u8; STEP_BYTES]) -> Step {
        let word = |i: usize| u32::from_le_bytes(body[4 * i..4 * i + 4].try_into().unwrap());
        Step {
            step: word(0),
            seq: word(1),
            force: f32::from_bits(word(2)),
            flags: word(3),
            control: body[16..].try_into().unwrap(),
        }
    }

    /// The control's steering (word 4), -1 (left) to 1 (right).
    pub fn steering(&self) -> f32 {
        f32::from_le_bytes(self.control[16..20].try_into().unwrap())
    }

    pub fn ngp(&self) -> bool {
        self.flags & STEP_NGP != 0
    }
}

/// The first bytes of a `--frames-out` log.
pub const LOG_MAGIC: [u8; 4] = *b"RTF1";
/// Bytes of a logged frame.
pub const LOGGED_BYTES: usize = CONTROL_BYTES + 16;

/// One frame of a real-time drive: the payload it ran (an input payload with the periods it stood for, or a recorded
/// payload with them for `--pilot`), the substeps it ran and a digest of the car and wheels after it (the FRM1 body).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Logged {
    pub payload: [u8; CONTROL_BYTES],
    pub periods: u32,
    pub substeps: u32,
    pub digest: u64,
}

impl Logged {
    pub fn to_bytes(&self) -> [u8; LOGGED_BYTES] {
        let mut out = [0; LOGGED_BYTES];
        out[..CONTROL_BYTES].copy_from_slice(&self.payload);
        out[CONTROL_BYTES..CONTROL_BYTES + 4].copy_from_slice(&self.periods.to_le_bytes());
        out[CONTROL_BYTES + 4..CONTROL_BYTES + 8].copy_from_slice(&self.substeps.to_le_bytes());
        out[CONTROL_BYTES + 8..].copy_from_slice(&self.digest.to_le_bytes());
        out
    }

    /// The frames of a log; an error for a bad magic or a cut record.
    pub fn read_log(bytes: &[u8]) -> Result<Vec<Logged>, String> {
        let body = bytes.strip_prefix(&LOG_MAGIC).ok_or("not a real-time frame log (RTF1)")?;
        if body.len() % LOGGED_BYTES != 0 {
            return Err(format!("a frame log of {} bytes cut in a record", bytes.len()));
        }
        Ok(body
            .chunks_exact(LOGGED_BYTES)
            .map(|r| Logged {
                payload: r[..CONTROL_BYTES].try_into().unwrap(),
                periods: u32::from_le_bytes(r[CONTROL_BYTES..CONTROL_BYTES + 4].try_into().unwrap()),
                substeps: u32::from_le_bytes(r[CONTROL_BYTES + 4..CONTROL_BYTES + 8].try_into().unwrap()),
                digest: u64::from_le_bytes(r[CONTROL_BYTES + 8..].try_into().unwrap()),
            })
            .collect())
    }
}

/// FNV-1a over `bytes`: the log's digest.
pub fn digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_records_round_trip() {
        let message = Message { tag: INPUT, seq: 7, payload: [3; CONTROL_BYTES] };
        assert_eq!(Message::read(&message.to_bytes()), Ok(message));
        assert!(Message::read(&[0; MESSAGE_BYTES]).is_err());
        let mut control = [0; CONTROL_BYTES];
        control[16..20].copy_from_slice(&(-0.25f32).to_le_bytes());
        let step = Step { step: 9, seq: 7, force: 0.5, flags: STEP_NGP, control };
        let bytes = step.to_bytes();
        let read = Step::from_body(bytes[4..].try_into().unwrap());
        assert_eq!((bytes[..4] == STP1, read, read.steering(), read.ngp()), (true, step, -0.25, true));
        let logged = Logged { payload: [1; CONTROL_BYTES], periods: 2, substeps: 3, digest: 4 };
        let log = [&LOG_MAGIC[..], &logged.to_bytes(), &logged.to_bytes()].concat();
        assert_eq!(Logged::read_log(&log), Ok(vec![logged, logged]));
        assert!(Logged::read_log(&log[..log.len() - 1]).is_err());
    }
}
