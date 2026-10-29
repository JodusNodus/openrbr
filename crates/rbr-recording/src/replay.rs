//! A replay (ours): a live drive kept to watch again. The physics is not stored but run again from the drive's
//! frames, which replay it bit for bit (`crate::realtime`), so a replay plays only on the build, install and setup
//! that drove it; each frame's digest tells where it does not. All values little-endian:
//!
//! ```text
//! u8  magic[4]   RRP1
//! u32 count      the viewer's command line that drove (its arguments, without the program)
//! count times:
//!   u32 bytes
//!   u8  text[bytes]   UTF-8
//! then a frame log: RTF1 and its frames (`realtime::Logged`), a restart a frame of no periods (`Logged::RESTART`)
//! ```
//!
//! The viewer writes the header (`viewer --save-replay`), the stepping process appends the log
//! (`rbr-standalone --replay-out`), and `rbr-standalone --replay` runs the frames of the last start.

use crate::realtime::{Logged, LOGGED_BYTES, LOG_MAGIC};

/// The first bytes of a replay.
pub const MAGIC: [u8; 4] = *b"RRP1";

/// The header of a replay of a drive by the viewer's `args`.
pub fn header(args: &[String]) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&(args.len() as u32).to_le_bytes());
    for arg in args {
        out.extend_from_slice(&(arg.len() as u32).to_le_bytes());
        out.extend_from_slice(arg.as_bytes());
    }
    out
}

/// A replay as read.
#[derive(Debug, Clone, PartialEq)]
pub struct Replay {
    /// The viewer's arguments that drove.
    pub args: Vec<String>,
    /// The frames from the drive's last start: those before a restart are not watched.
    pub frames: Vec<Logged>,
}

impl Replay {
    /// Reads a replay; a log cut short (a process that did not end) keeps its whole frames.
    pub fn read(bytes: &[u8]) -> Result<Replay, String> {
        let mut at = 0;
        let mut take = |n: usize| {
            let part = bytes.get(at..at + n).ok_or("a replay cut short in its header")?;
            at += n;
            Ok::<_, String>(part)
        };
        let word = |b: &[u8]| u32::from_le_bytes(b.try_into().unwrap()) as usize;
        if take(4)? != MAGIC {
            return Err("not a replay (RRP1)".to_owned());
        }
        let count = word(take(4)?);
        let args = (0..count)
            .map(|_| {
                let len = word(take(4)?);
                String::from_utf8(take(len)?.to_vec()).map_err(|_| "a replay argument that is not UTF-8".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let log = &bytes[at..];
        if log.get(..4) != Some(&LOG_MAGIC[..]) {
            return Err("a replay without its frames (RTF1)".to_owned());
        }
        let all = log[4..].chunks_exact(LOGGED_BYTES).map(|r| Logged::from_bytes(r.try_into().unwrap()));
        let frames = all.fold(Vec::new(), |mut frames, frame| {
            if frame.is_restart() {
                frames.clear();
            } else {
                frames.push(frame);
            }
            frames
        });
        Ok(Replay { args, frames })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CONTROL_BYTES;

    #[test]
    fn a_replay_keeps_the_last_start() {
        let args = vec!["--stage".to_owned(), "Rally Cote d'Azur".to_owned()];
        let frame = |n| Logged { payload: [n; CONTROL_BYTES], periods: 1, substeps: 1, digest: 9 };
        let mut bytes = header(&args);
        bytes.extend_from_slice(&LOG_MAGIC);
        for logged in [frame(1), frame(2), Logged::RESTART, frame(3), frame(4)] {
            bytes.extend_from_slice(&logged.to_bytes());
        }
        // A frame cut short at the end.
        bytes.extend_from_slice(&frame(5).to_bytes()[..10]);
        assert_eq!(Replay::read(&bytes), Ok(Replay { args, frames: vec![frame(3), frame(4)] }));
        assert!(Replay::read(&bytes[..6]).is_err());
        assert!(Replay::read(b"RTF1").is_err());
    }
}
